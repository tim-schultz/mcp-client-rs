use serde::{Deserialize, Serialize};
use smol::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use std::{
    collections::HashMap,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
use tokio::sync::Mutex;

#[derive(Debug)]
pub enum ClientError {
    Io(std::io::Error),
    InitializationFailed(String),
}

impl From<std::io::Error> for ClientError {
    fn from(err: std::io::Error) -> Self {
        ClientError::Io(err)
    }
}

#[derive(Serialize)]
pub struct JsonRpcRequest<T> {
    jsonrpc: String,
    id: u64,
    method: String,
    params: T,
}

#[derive(Serialize)]
struct InitializeParams {
    // Changed from protocol_version to protocolVersion to match server requirements
    #[serde(rename = "protocolVersion")]
    protocol_version: String,
    capabilities: serde_json::Value,
    // Changed from client_info to clientInfo to match server requirements
    #[serde(rename = "clientInfo")]
    client_info: ClientInfo,
}

#[derive(Serialize)]
struct ClientInfo {
    name: String,
    version: String,
}

#[derive(Serialize)]
struct ToolCallParams {
    name: String,
    arguments: serde_json::Value,
}

// Response handling structures
#[derive(Deserialize, Debug)]
pub struct JsonRpcResponse<T> {
    pub jsonrpc: String,
    pub id: u64,
    #[serde(flatten)]
    pub response: ResponseContent<T>,
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
enum ResponseContent<T> {
    Success { result: T },
    Error { error: JsonRpcError },
}

#[derive(Deserialize, Debug)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
}

// Request builder implementation
impl<T> JsonRpcRequest<T> {
    fn new(id: u64, method: impl Into<String>, params: T) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.into(),
            params,
        }
    }
}

pub struct Client {
    // Protect stdin/stdout with a mutex for exclusive access
    inner: Arc<Mutex<ClientInner>>,
    // Atomic counter for generating unique request IDs
    next_id: AtomicU64,
}

// Inner state protected by the mutex
struct ClientInner {
    stdin: smol::Async<std::process::ChildStdin>,
    stdout: BufReader<smol::Async<std::process::ChildStdout>>,
    _child: Child,
}

impl Client {
    pub async fn new(
        version: &str,
        program: &str,
        args: Vec<&str>,
        envs: HashMap<String, String>,
    ) -> Result<Self, ClientError> {
        let mut child = Command::new(program)
            .args(args)
            .envs(envs)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;

        let stdin = child.stdin.take().expect("Failed to get stdin");
        let stdout = child.stdout.take().expect("Failed to get stdout");

        let inner = ClientInner {
            stdin: smol::Async::new(stdin)?,
            stdout: BufReader::new(smol::Async::new(stdout)?),
            _child: child,
        };

        let client = Self {
            inner: Arc::new(Mutex::new(inner)),
            next_id: AtomicU64::new(0),
        };

        // Initialize the client
        client.initialize(version).await?;

        Ok(client)
    }
    pub fn next_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }
    pub async fn initialize(&self, version: &str) -> Result<(), ClientError> {
        let init_params = InitializeParams {
            protocol_version: version.to_string(),
            capabilities: serde_json::json!({}),
            client_info: ClientInfo {
                name: "test".to_string(),
                version: "0.1.0".to_string(),
            },
        };

        let init_request = JsonRpcRequest::new(self.next_id(), "initialize", init_params);
        self.send_request(init_request).await?;
        Ok(())
    }

    pub async fn send_request<T: Serialize>(
        &self,
        request: JsonRpcRequest<T>,
    ) -> Result<JsonRpcResponse<serde_json::Value>, ClientError> {
        let message = serde_json::to_string(&request)
            .map_err(|e| ClientError::InitializationFailed(e.to_string()))?;

        // Lock the inner client for the duration of the request
        let mut inner = self.inner.lock().await;

        // Perform the request with exclusive access
        inner.stdin.write_all(message.as_bytes()).await?;
        inner.stdin.write_all(b"\n").await?;
        inner.stdin.flush().await?;

        let mut response = String::new();
        inner.stdout.read_line(&mut response).await?;
        dbg!(&response);

        // Parse response after releasing the lock
        serde_json::from_str(&response)
            .map_err(|e| ClientError::InitializationFailed(format!("Invalid response: {}", e)))
    }

    pub async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<JsonRpcResponse<serde_json::Value>, ClientError> {
        let tool_params = ToolCallParams {
            name: name.to_string(),
            arguments,
        };

        let request = JsonRpcRequest::new(self.next_id(), "tools/call", tool_params);
        self.send_request(request).await
    }
}
