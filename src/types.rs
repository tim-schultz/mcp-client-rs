use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
    pub inputSchema: serde_json::Value,
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
pub struct InitializeParams {
    // Changed from protocol_version to protocolVersion to match server requirements
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    pub capabilities: serde_json::Value,
    // Changed from client_info to clientInfo to match server requirements
    #[serde(rename = "clientInfo")]
    pub client_info: ClientInfo,
}

#[derive(Serialize)]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
}

#[derive(Serialize)]
pub struct ToolCallParams {
    pub name: String,
    pub arguments: serde_json::Value,
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
pub enum ResponseContent<T> {
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
    pub fn new(id: u64, method: RequestType, params: T) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            method,
            params,
        }
    }
}
