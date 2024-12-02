use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::HashMap,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResponse {
    pub protocol_version: String,
    pub capabilities: ServerCapabilities,
    pub server_info: ServerInfo,
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcesReadResponse {
    pub contents: Vec<ResourceContents>,
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
pub struct ResourceContents {
    pub uri: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcesListResponse {
    pub resources: Vec<Resource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
pub struct Resource {
    pub uri: String,
    #[serde(rename = "type")]
    pub resource_type: String,
}

#[derive(Debug, Deserialize)]
pub struct ListToolsResponse {
    pub tools: Vec<Tool>,
}

#[derive(Debug, Deserialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct CallToolResponse {
    #[serde(rename = "toolResult")]
    pub result: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct Prompt {
    pub id: String,
    pub description: String,
}

#[derive(Debug, Clone)]
pub enum RequestType {
    Initialize,
    CallTool,
    ResourcesUnsubscribe,
    ResourcesSubscribe,
    ResourcesRead,
    ResourcesList,
    LoggingSetLevel,
    PromptsGet,
    PromptsList,
    CompletionComplete,
    Ping,
    ListTools,
    ListResourceTemplates,
    ListRoots,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ServerCapability {
    Experimental,
    Logging,
    Prompts,
    Resources,
    Tools,
    Sampling,
}

impl RequestType {
    pub fn as_str(&self) -> &'static str {
        match self {
            RequestType::Initialize => "initialize",
            RequestType::CallTool => "tools/call",
            RequestType::ResourcesUnsubscribe => "resources/unsubscribe",
            RequestType::ResourcesSubscribe => "resources/subscribe",
            RequestType::ResourcesRead => "resources/read",
            RequestType::ResourcesList => "resources/list",
            RequestType::LoggingSetLevel => "logging/setLevel",
            RequestType::PromptsGet => "prompts/get",
            RequestType::PromptsList => "prompts/list",
            RequestType::CompletionComplete => "completion/complete",
            RequestType::Ping => "ping",
            RequestType::ListTools => "tools/list",
            RequestType::ListResourceTemplates => "resources/templates/list",
            RequestType::ListRoots => "roots/list",
        }
    }
}

impl Protocol {
    pub fn capable(&self, capability: ServerCapability) -> bool {
        if let Some(caps) = &self.capabilities {
            match capability {
                ServerCapability::Experimental => caps.experimental.is_some(),
                ServerCapability::Logging => caps.logging.is_some(),
                ServerCapability::Prompts => caps.prompts.is_some(),
                ServerCapability::Resources => caps.resources.is_some(),
                ServerCapability::Tools => caps.tools.is_some(),
                ServerCapability::Sampling => caps.sampling.is_some(),
            }
        } else {
            false
        }
    }

    fn check_capability(&self, capability: ServerCapability) -> Result<(), ClientError> {
        if self.capable(capability) {
            Ok(())
        } else {
            Err(ClientError::CapabilityError(format!(
                "Server does not support {:?} capability",
                capability
            )))
        }
    }

    pub async fn list_prompts(&self) -> Result<Vec<Prompt>, ClientError> {
        self.check_capability(ServerCapability::Prompts)?;
        let request = JsonRpcRequest::new(self.next_id(), RequestType::PromptsList, json!({}));
        let response = self.send_request(request).await?;
        if let ResponseContent::Success { result } = response.response {
            serde_json::from_value(result).map_err(|e| {
                ClientError::PromptError(format!("Failed to parse prompts list: {}", e))
            })
        } else {
            Err(ClientError::PromptError(
                "Failed to list prompts".to_string(),
            ))
        }
    }

    pub async fn list_resources(&self) -> Result<ResourcesListResponse, ClientError> {
        self.check_capability(ServerCapability::Resources)?;
        let request = JsonRpcRequest::new(self.next_id(), RequestType::ResourcesList, json!({}));
        let response = self.send_request(request).await?;
        if let ResponseContent::Success { result } = response.response {
            serde_json::from_value(result).map_err(|e| {
                ClientError::ResourceError(format!("Failed to parse resources list: {}", e))
            })
        } else {
            Err(ClientError::ResourceError(
                "Failed to list resources".to_string(),
            ))
        }
    }

    pub async fn read_resources(
        &self,
        uris: Vec<String>,
    ) -> Result<ResourcesReadResponse, ClientError> {
        self.check_capability(ServerCapability::Resources)?;
        let request = JsonRpcRequest::new(
            self.next_id(),
            RequestType::ResourcesRead,
            json!({ "uris": uris }),
        );
        let response = self.send_request(request).await?;
        if let ResponseContent::Success { result } = response.response {
            serde_json::from_value(result).map_err(|e| {
                ClientError::ResourceError(format!(
                    "Failed to parse read resources response: {}",
                    e
                ))
            })
        } else {
            Err(ClientError::ResourceError(
                "Failed to read resources".to_string(),
            ))
        }
    }

    pub async fn list_tools(&self) -> Result<ListToolsResponse, ClientError> {
        self.check_capability(ServerCapability::Tools)?;
        let request = JsonRpcRequest::new(self.next_id(), RequestType::ListTools, json!({}));
        let response = self.send_request(request).await?;
        if let ResponseContent::Success { result } = response.response {
            serde_json::from_value(result)
                .map_err(|e| ClientError::ToolError(format!("Failed to parse tools list: {}", e)))
        } else {
            Err(ClientError::ToolError("Failed to list tools".to_string()))
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerCapabilities {
    pub experimental: Option<serde_json::Value>,
    pub logging: Option<LoggingCapability>,
    pub prompts: Option<PromptsCapability>,
    pub resources: Option<ResourcesCapability>,
    pub tools: Option<ToolsCapability>,
    #[serde(default)]
    pub sampling: Option<SamplingCapability>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LoggingCapability {
    pub levels: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PromptsCapability {
    pub supports_custom: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ResourcesCapability {
    pub supports_subscribe: bool,
    pub supports_delta: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ToolsCapability {
    #[serde(default)]
    pub supports_streaming: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SamplingCapability {
    pub max_tokens: Option<u32>,
    pub supported_methods: Vec<String>,
}

#[derive(Debug)]
pub enum ClientError {
    Io(std::io::Error),
    InitializationFailed(String),
    ResourceError(String),
    ToolError(String),
    PromptError(String),
    CapabilityError(String),
    SerializationError(String),
    ProtocolError(String),
}

impl From<std::io::Error> for ClientError {
    fn from(err: std::io::Error) -> Self {
        ClientError::Io(err)
    }
}

impl From<serde_json::Error> for ClientError {
    fn from(err: serde_json::Error) -> Self {
        ClientError::SerializationError(err.to_string())
    }
}

#[derive(Serialize)]
pub struct JsonRpcRequest<T> {
    jsonrpc: String,
    id: u64,
    #[serde(serialize_with = "serialize_request_type")]
    method: RequestType,
    params: T,
}

fn serialize_request_type<S>(request_type: &RequestType, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(request_type.as_str())
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
#[derive(Deserialize, Debug, Clone)]
pub struct JsonRpcResponse<T> {
    pub jsonrpc: String,
    pub id: u64,
    #[serde(flatten)]
    pub response: ResponseContent<T>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
enum ResponseContent<T> {
    Success { result: T },
    Error { error: JsonRpcError },
}

#[derive(Deserialize, Debug, Clone)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
}

// Request builder implementation
impl<T> JsonRpcRequest<T> {
    fn new(id: u64, method: RequestType, params: T) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            method,
            params,
        }
    }
}

pub struct Protocol {
    // Protect stdin/stdout with a mutex for exclusive access
    inner: Arc<Mutex<Client>>,
    // Atomic counter for generating unique request IDs
    next_id: AtomicU64,
    // Server capabilities received during initialization
    capabilities: Option<ServerCapabilities>,
}

// Inner state protected by the mutex
struct Client {
    stdin: tokio::process::ChildStdin,
    stdout: BufReader<tokio::process::ChildStdout>,
    _child: tokio::process::Child,
}

impl Protocol {
    pub async fn new(
        version: &str,
        program: &str,
        args: Vec<&str>,
        envs: HashMap<String, String>,
    ) -> Result<Self, ClientError> {
        let mut child = tokio::process::Command::new(program)
            .args(args)
            .envs(envs)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;

        let stdin = child.stdin.take().expect("Failed to get stdin");
        let stdout = child.stdout.take().expect("Failed to get stdout");

        let inner = Client {
            stdin,
            stdout: BufReader::new(stdout),
            _child: child,
        };

        let mut client = Self {
            inner: Arc::new(Mutex::new(inner)),
            next_id: AtomicU64::new(0),
            capabilities: None,
        };

        client.initialize(version).await?;

        Ok(client)
    }
    pub fn next_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }
    pub async fn initialize(&mut self, version: &str) -> Result<InitializeResponse, ClientError> {
        let init_params = InitializeParams {
            protocol_version: version.to_string(),
            capabilities: serde_json::json!({}),
            client_info: ClientInfo {
                name: "test".to_string(),
                version: "0.1.0".to_string(),
            },
        };

        let init_request =
            JsonRpcRequest::new(self.next_id(), RequestType::Initialize, init_params);
        let response = self.send_request(init_request).await?;

        if let ResponseContent::Success { result } = response.response {
            let init_response: InitializeResponse = serde_json::from_value(result)
                .map_err(|e| ClientError::InitializationFailed(e.to_string()))?;
            self.capabilities = Some(init_response.capabilities.clone());
            Ok(init_response)
        } else {
            Err(ClientError::InitializationFailed(
                "Initialization failed".to_string(),
            ))
        }
    }

    /// Get the current server capabilities if initialized
    pub fn get_capabilities(&self) -> Option<&ServerCapabilities> {
        self.capabilities.as_ref()
    }

    pub async fn send_request<T: Serialize>(
        &self,
        request: JsonRpcRequest<T>,
    ) -> Result<JsonRpcResponse<serde_json::Value>, ClientError> {
        let message = serde_json::to_string(&request)
            .map_err(|e| ClientError::SerializationError(e.to_string()))?;
        let mut inner = self.inner.lock().await;

        inner.stdin.write_all(message.as_bytes()).await?;
        inner.stdin.write_all(b"\n").await?;
        inner.stdin.flush().await?;

        let mut response = String::new();
        inner.stdout.read_line(&mut response).await?;

        serde_json::from_str(&response)
            .map_err(|e| ClientError::ProtocolError(format!("Failed to parse response: {}", e)))
    }

    pub async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<CallToolResponse, ClientError> {
        self.check_capability(ServerCapability::Tools)?;

        let tool_params = ToolCallParams {
            name: name.to_string(),
            arguments,
        };

        let request = JsonRpcRequest::new(self.next_id(), RequestType::CallTool, tool_params);
        let response = self.send_request(request).await?;

        if let ResponseContent::Success { result } = response.response {
            dbg!(&result);
            serde_json::from_value(result).map_err(|e| {
                ClientError::ToolError(format!("Failed to parse tool response: {}", e))
            })
        } else {
            Err(ClientError::ToolError("Failed to call tool".to_string()))
        }
    }
}
