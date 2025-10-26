I have the source code of Amazon Q CLI checked out. 

I am working on am working on turning the "backend" of the Q CLI (basically everything except UI) into an Agent that follows the AgentClientProtocol. This way I can integrate Q CLI Agent with other "Client" in the future, like IDE.

The core Backend Chat State Machine is defined in crates/chat-cli/src/cli/chat/mod.rs. i.e. initially waiting for user input, then handle input, talk to backend, calls tool, etc. 

I essentially want a Rust server running this state machine and I should be able to talk to it using ACP protocol through stdio.

Also note that I have completed a refactoring in which I replaced direct call to print something to stdout with "sending a structured event" instead. Take a look at crates/chat-cli-ui/src/conduit.rs.

I think conduit can implement the "Agent" protocol. For example, when it receives a event for LLM response, it should follow the ACP protocol and send that LLM output to the Client.

Feel free to look at the ACP rust-sdk here: /Users/moerben/Documents/Work/2025/Project/UI/rust-sdk. 

I am planning to have just 1 Session for the entire life cycle (I also control the "Client")

Use context7 MCP to gather context for AgentClientProtocol


Completed
1. I have added an example ACP agent and client under crates/chat-cli/src/bin/
2. I need to make sure ChatSession doesn't read prompt from input_source, instead it read from a channel that ACP agent can populate.
3. I completely removed input_source from the code base
4. Completely removed spinner

TODO:
