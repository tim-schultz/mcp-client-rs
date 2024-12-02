use dotenv::dotenv;
use mcp_client_rs::{ClientError, Protocol};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), ClientError> {
    dotenv().ok();

    let mut envs = HashMap::new();
    envs.insert(
        "GITHUB_PERSONAL_ACCESS_TOKEN".to_string(),
        std::env::var("GITHUB_PERSONAL_ACCESS_TOKEN").unwrap_or_default(),
    );

    let client = Protocol::new(
        "example-client",
        "npx",
        vec!["-y", "@modelcontextprotocol/server-github"],
        envs,
    )
    .await?;

    // List available tools
    println!("\n=== Listing Available Tools ===");
    let tools = client.list_tools().await?;
    println!("Available tools: {:#?}", tools);

    // Call a specific tool
    println!("\n=== Calling search_repositories Tool ===");
    let search_response = client
        .call_tool(
            "search_repositories",
            serde_json::json!({
                "query": "rust language:rust stars:>1000"
            }),
        )
        .await?;
    println!("Search results: {:#?}", search_response);

    Ok(())
}
