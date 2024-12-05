# MCP Protocol Client - Very Much a WIP

Rust client implementation for the [Model Context Protocol](https://github.com/modelcontextprotocol) - a structured communication protocol between language models and external tools.

## Features

- Client implementation to be used in rust projects that want to use the MCP protocol
- Async/await support with tokio
- Capability negotiation
- Tool execution
- Resource management
- Prompt handling

## Usage

```rust
use mcp_client_rs::{Protocol, ClientError};

#[tokio::main]
async fn main() -> Result<(), ClientError> {
    let client = Protocol::new(
        "0",  // Protocol version
        "npx", // Command
        vec!["-y", "@modelcontextprotocol/server-github"], // Args
        std::collections::HashMap::new(), // Environment variables
    ).await?;

    // Call tools
    let response = client
        .call_tool(
            "search_repositories",
            serde_json::json!({
                "query": "rust language:rust"
            }),
        )
        .await?;

    println!("{:?}", response);
    Ok(())
}
```

## Installation

Add to your Cargo.toml:
```toml
[dependencies]
mcp-client-rs = { git = "https://github.com/tim-schultz/mcp-client-rs.git" }
```

## License

MIT
