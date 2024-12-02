use crate::types::{
    CallToolResponse, ClientError, ClientInfo, InitializeParams, InitializeResponse,
    JsonRpcRequest, JsonRpcResponse, ListToolsResponse, Prompt, RequestType, ResourcesListResponse,
    ResourcesReadResponse, ResponseContent, ServerCapabilities, ServerCapability, ToolCallParams,
};
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
