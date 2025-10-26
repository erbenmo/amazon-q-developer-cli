//! Q CLI ACP Agent - Agent Client Protocol server for Q CLI
//!
//! This binary runs the Q CLI backend as an ACP agent, communicating over stdio.
//! 
//! To test:
//! ```bash
//! cargo build --bin q_agent --bin q_client
//! cargo run --bin q_client -- target/debug/q_agent
//! ```

use agent_client_protocol::{self as acp, Client as _};
use chat_cli_ui::protocol::Event;
use tokio::sync::{mpsc, oneshot};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

// This component reads structured events from Conduit and send them to ACP Client as SessionUpdate
struct SessionUpdateSender {
    notification_tx: mpsc::UnboundedSender<acp::SessionNotification>,
}

impl SessionUpdateSender {
    fn new(notification_tx: mpsc::UnboundedSender<acp::SessionNotification>) -> Self {
        Self { notification_tx }
    }

    fn spawn_event_processor(
        &self,
        event_receiver: std::sync::mpsc::Receiver<Event>,
        session_id: acp::SessionId,
    ) {
        let tx = self.notification_tx.clone();
        tokio::task::spawn_blocking(move || {
            while let Ok(event) = event_receiver.recv() {
                if let Some(session_notification) = Self::convert_event_to_session_notification(event, &session_id) {
                    let _ = tx.send(session_notification);
                }
            }
        });
    }

    fn convert_event_to_session_notification(event: Event, session_id: &acp::SessionId) -> Option<acp::SessionNotification> {
        match event {
            Event::TextMessageContent(content) => {
                Some(acp::SessionNotification {
                    session_id: session_id.clone(),
                    update: acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk {
                        content: acp::ContentBlock::Text(acp::TextContent {
                            text: String::from_utf8_lossy(&content.delta).to_string(),
                            annotations: None,
                            meta: None,
                        }),
                        meta: None,
                    }),
                    meta: None,
                })
            },
            _ => None,
        }
    }
}

struct QCliAgent {
    // this is the queue for sending SessionUpdate to client
    session_update_tx: mpsc::UnboundedSender<(acp::SessionNotification, oneshot::Sender<()>)>,
    next_session_id: std::sync::atomic::AtomicU64,
}

impl QCliAgent {
    fn new(
        session_update_tx: mpsc::UnboundedSender<(acp::SessionNotification, oneshot::Sender<()>)>,
    ) -> Self {
        Self {
            session_update_tx,
            next_session_id: std::sync::atomic::AtomicU64::new(0),
        }
    }
}

#[async_trait::async_trait(?Send)]
impl acp::Agent for QCliAgent {
    async fn initialize(
        &self,
        _arguments: acp::InitializeRequest,
    ) -> Result<acp::InitializeResponse, acp::Error> {
        Ok(acp::InitializeResponse {
            protocol_version: acp::V1,
            agent_capabilities: acp::AgentCapabilities {
                load_session: false,
                prompt_capabilities: acp::PromptCapabilities {
                    image: false,
                    audio: false,
                    embedded_context: true,
                    meta: None,
                },
                mcp_capabilities: acp::McpCapabilities {
                    http: false,
                    sse: false,
                    meta: None,
                },
                meta: None,
            },
            auth_methods: Vec::new(),
            agent_info: Some(acp::Implementation {
                name: "q-cli-agent".to_string(),
                title: Some("Amazon Q CLI Agent".to_string()),
                version: env!("CARGO_PKG_VERSION").to_string(),
            }),
            meta: None,
        })
    }

    async fn authenticate(
        &self,
        _arguments: acp::AuthenticateRequest,
    ) -> Result<acp::AuthenticateResponse, acp::Error> {
        Ok(acp::AuthenticateResponse::default())
    }

    async fn new_session(
        &self,
        _arguments: acp::NewSessionRequest,
    ) -> Result<acp::NewSessionResponse, acp::Error> {
        let session_id = self.next_session_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(acp::NewSessionResponse {
            session_id: acp::SessionId(session_id.to_string().into()),
            modes: None,
            meta: None,
        })
    }

    // Not supported for now.
    async fn load_session(
        &self,
        _arguments: acp::LoadSessionRequest,
    ) -> Result<acp::LoadSessionResponse, acp::Error> {
        Err(acp::Error::method_not_found())
    }

    // Process a prompt from Client
    async fn prompt(
        &self,
        arguments: acp::PromptRequest,
    ) -> Result<acp::PromptResponse, acp::Error> {
        // Extract text from prompt content
        let mut prompt_text = String::new();
        for content in arguments.prompt {
            match content {
                acp::ContentBlock::Text(text_content) => {
                    prompt_text.push_str(&text_content.text);
                }
                _ => {
                    // For now, ignore non-text content
                }
            }
        }

        // this is used so we can wait for the backgruond thread to send back the SessionUpdate
        let (tx, rx) = oneshot::channel();

        // Send response back through session updates
        self.session_update_tx
            .send((
                acp::SessionNotification {
                    session_id: arguments.session_id.clone(),
                    update: acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk {
                        content: acp::ContentBlock::Text(acp::TextContent {
                            text: format!("Echo: {}", prompt_text),
                            annotations: None,
                            meta: None,
                        }),
                        meta: None,
                    }),
                    meta: None,
                },
                tx,
            ))
            .map_err(|_| acp::Error::internal_error())?;
        
        rx.await.map_err(|_| acp::Error::internal_error())?;

        Ok(acp::PromptResponse {
            stop_reason: acp::StopReason::EndTurn,
            meta: None,
        })
    }

    async fn cancel(&self, _args: acp::CancelNotification) -> Result<(), acp::Error> {
        Ok(())
    }

    async fn set_session_mode(
        &self,
        _args: acp::SetSessionModeRequest,
    ) -> Result<acp::SetSessionModeResponse, acp::Error> {
        Ok(acp::SetSessionModeResponse::default())
    }

    async fn ext_method(&self, _args: acp::ExtRequest) -> Result<acp::ExtResponse, acp::Error> {
        Err(acp::Error::method_not_found())
    }

    async fn ext_notification(&self, _args: acp::ExtNotification) -> Result<(), acp::Error> {
        Ok(())
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> acp::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    let outgoing = tokio::io::stdout().compat_write();
    let incoming = tokio::io::stdin().compat();

    let local_set = tokio::task::LocalSet::new();
    local_set
        .run_until(async move {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            
            // Create the agent
            let agent = QCliAgent::new(tx);
            
            // Start up the agent connected to stdio
            let (conn, handle_io) =
                acp::AgentSideConnection::new(agent, outgoing, incoming, |fut| {
                    tokio::task::spawn_local(fut);
                });
            
            // Create a background thread that process from the queue and send back session update to Client
            tokio::task::spawn_local(async move {
                while let Some((session_notification, tx)) = rx.recv().await {
                    let result = conn.session_notification(session_notification).await;
                    if let Err(e) = result {
                        eprintln!("Error sending session notification: {e}");
                        break;
                    }
                    tx.send(()).ok();
                }
            });
            
            // Run until stdin/stdout are closed
            handle_io.await
        })
        .await
}