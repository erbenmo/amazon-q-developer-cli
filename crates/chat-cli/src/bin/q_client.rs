//! Q CLI ACP Client - Simple client to test the Q CLI ACP agent
//!
//! Usage:
//!
//! To test:
//! ```bash
//! cargo build --bin q_agent --bin q_client
//! cargo run --bin q_client -- target/debug/q_agent
//! ```

use agent_client_protocol::{self as acp, Agent as _};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use eyre::Result;

struct QCliTestClient {}

#[async_trait::async_trait(?Send)]
impl acp::Client for QCliTestClient {
    async fn request_permission(
        &self,
        _args: acp::RequestPermissionRequest,
    ) -> acp::Result<acp::RequestPermissionResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn write_text_file(
        &self,
        _args: acp::WriteTextFileRequest,
    ) -> acp::Result<acp::WriteTextFileResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn read_text_file(
        &self,
        _args: acp::ReadTextFileRequest,
    ) -> acp::Result<acp::ReadTextFileResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn create_terminal(
        &self,
        _args: acp::CreateTerminalRequest,
    ) -> Result<acp::CreateTerminalResponse, acp::Error> {
        Err(acp::Error::method_not_found())
    }

    async fn terminal_output(
        &self,
        _args: acp::TerminalOutputRequest,
    ) -> acp::Result<acp::TerminalOutputResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn release_terminal(
        &self,
        _args: acp::ReleaseTerminalRequest,
    ) -> acp::Result<acp::ReleaseTerminalResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn wait_for_terminal_exit(
        &self,
        _args: acp::WaitForTerminalExitRequest,
    ) -> acp::Result<acp::WaitForTerminalExitResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn kill_terminal_command(
        &self,
        _args: acp::KillTerminalCommandRequest,
    ) -> acp::Result<acp::KillTerminalCommandResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn session_notification(
        &self,
        args: acp::SessionNotification,
    ) -> acp::Result<(), acp::Error> {

        match args.update {
            acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk { content, .. }) => {
                let text = match content {
                    acp::ContentBlock::Text(text_content) => text_content.text,
                    acp::ContentBlock::Image(_) => "<image>".into(),
                    acp::ContentBlock::Audio(_) => "<audio>".into(),
                    acp::ContentBlock::ResourceLink(resource_link) => resource_link.uri,
                    acp::ContentBlock::Resource(_) => "<resource>".into(),
                };
                println!("{text}");
            }
            acp::SessionUpdate::UserMessageChunk { .. }
            | acp::SessionUpdate::AgentThoughtChunk { .. }
            | acp::SessionUpdate::ToolCall(_)
            | acp::SessionUpdate::ToolCallUpdate(_)
            | acp::SessionUpdate::Plan(_)
            | acp::SessionUpdate::CurrentModeUpdate { .. }
            | acp::SessionUpdate::AvailableCommandsUpdate { .. } => {}
        }
        Ok(())
    }

    async fn ext_method(&self, _args: acp::ExtRequest) -> acp::Result<acp::ExtResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn ext_notification(&self, _args: acp::ExtNotification) -> acp::Result<()> {
        Err(acp::Error::method_not_found())
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let command = std::env::args().collect::<Vec<_>>();
    let (outgoing, incoming, child) = match command.as_slice() {
        [_, program, args @ ..] => {
            let mut child = tokio::process::Command::new(program)
                .args(args.iter())
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .kill_on_drop(true)
                .spawn()?;
            (
                child.stdin.take().unwrap().compat_write(),
                child.stdout.take().unwrap().compat(),
                child,
            )
        }
        _ => eyre::bail!("Usage: q_client AGENT_PROGRAM AGENT_ARG..."),
    };

    let local_set = tokio::task::LocalSet::new();
    local_set
        .run_until(async move {
            // Set up the client connected to the agent's stdio
            let (conn, handle_io) =
                acp::ClientSideConnection::new(QCliTestClient {}, outgoing, incoming, |fut| {
                    tokio::task::spawn_local(fut);
                });

            println!("1. Going to initialize connection");

            // Handle I/O in the background
            tokio::task::spawn_local(handle_io);

            // Connect to the agent and set up a session
            conn.initialize(acp::InitializeRequest {
                protocol_version: acp::V1,
                client_capabilities: acp::ClientCapabilities::default(),
                client_info: Some(acp::Implementation {
                    name: "q-cli-test-client".to_string(),
                    title: Some("Q CLI Test Client".to_string()),
                    version: "0.1.0".to_string(),
                }),
                meta: None,
            })
            .await?;

            println!("2. Completed initialization of connection & session");
            
            let response = conn
                .new_session(acp::NewSessionRequest {
                    mcp_servers: Vec::new(),
                    cwd: std::env::current_dir()?,
                    meta: None,
                })
                .await?;

            println!("Connected to Q CLI Agent! Session ID: {}", response.session_id.0);
            
    println!(
        r#"
      _     _
     ( \---/ )
      ) . . (
     =\ T _ T /=
       )     (
      /       \
     ( (  ) (  )
      ""   ""
"#
    );
            
            println!("Type messages to send to the agent (Ctrl+C to quit):");

            // Send prompts to the agent until stdin is closed
            let mut rl = rustyline::DefaultEditor::new()?;
            while let Ok(line) = rl.readline("> ") {
                eprintln!("Start of prompt loop");

                if line.trim().is_empty() {
                    continue;
                }
                
                let result = conn
                    .prompt(acp::PromptRequest {
                        session_id: response.session_id.clone(),
                        prompt: vec![line.into()],
                        meta: None,
                    })
                    .await;

                eprintln!("End of prompt loop");
                    
                if let Err(e) = result {
                    eprintln!("Error: {e}");
                }
            }

            drop(child);
            Ok(())
        })
        .await
}
