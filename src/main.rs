mod protocol;
use dotenv::dotenv;
use protocol::{ClientError, Protocol};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> Result<(), ClientError> {
    dotenv().ok();
    let mut envs = HashMap::new();
    envs.insert(
        "GITHUB_PERSONAL_ACCESS_TOKEN".to_string(),
        std::env::var("GITHUB_PERSONAL_ACCESS_TOKEN").unwrap_or_default(),
    );

    let client = Arc::new(
        Protocol::new(
            "0",
            "npx",
            ["-y", "@modelcontextprotocol/server-github"].to_vec(),
            envs,
        )
        .await?,
    );

    let client = client.clone();
    let handle = tokio::spawn(async move {
        // Each task makes its own request safely
        let response = client
            .call_tool(
                "search_repositories",
                serde_json::json!({
                    "query": format!("rust")
                }),
            )
            .await;
        println!("Task response: {:#?}", response);
    });

    handle.await.expect("Task failed");

    Ok(())
}
