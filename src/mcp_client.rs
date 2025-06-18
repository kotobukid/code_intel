use crate::client::CodeIntelClient;
use crate::protocol;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<Value>,
    pub id: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub result: Option<Value>,
    pub error: Option<JsonRpcError>,
    pub id: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    pub data: Option<Value>,
}

pub struct McpClient {
    client: CodeIntelClient,
    port: u16,
}

impl McpClient {
    pub fn new(port: u16) -> Self {
        Self {
            client: CodeIntelClient::new(port),
            port,
        }
    }

    /// stdio transport で MCP クライアントを開始
    pub async fn run_stdio(&self) -> Result<()> {
        info!("Starting MCP client, connecting to server on port {}", self.port);
        
        // サーバーが起動しているかチェック
        if !self.client.is_server_running().await {
            error!("code_intel server is not running on port {}. Please start it with: code_intel serve", self.port);
            std::process::exit(1);
        }

        let stdin = tokio::io::stdin();
        let mut stdout = tokio::io::stdout();
        let mut reader = BufReader::new(stdin).lines();

        while let Some(line) = reader.next_line().await? {
            if line.trim().is_empty() {
                continue;
            }

            debug!("Received MCP request: {}", line);

            match self.handle_mcp_message(&line).await {
                Ok(Some(response)) => {
                    let response_str = serde_json::to_string(&response)?;
                    debug!("Sending MCP response: {}", response_str);
                    
                    stdout.write_all(response_str.as_bytes()).await?;
                    stdout.write_all(b"\n").await?;
                    stdout.flush().await?;
                }
                Ok(None) => {
                    // Notification (応答なし)
                }
                Err(e) => {
                    error!("Error handling MCP message: {}", e);
                    let error_response = JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32603,
                            message: format!("Internal error: {}", e),
                            data: None,
                        }),
                        id: None,
                    };
                    
                    let response_str = serde_json::to_string(&error_response)?;
                    stdout.write_all(response_str.as_bytes()).await?;
                    stdout.write_all(b"\n").await?;
                    stdout.flush().await?;
                }
            }
        }

        Ok(())
    }

    async fn handle_mcp_message(&self, message: &str) -> Result<Option<JsonRpcResponse>> {
        let request: JsonRpcRequest = serde_json::from_str(message)
            .context("Failed to parse JSON-RPC request")?;

        debug!("Handling MCP method: {}", request.method);

        let response = match request.method.as_str() {
            "initialize" => self.handle_initialize(&request).await?,
            "tools/list" => self.handle_tools_list(&request).await?,
            "tools/call" => self.handle_tools_call(&request).await?,
            "resources/list" => self.handle_resources_list(&request).await?,
            method if method.starts_with("notifications/") => {
                debug!("Received notification: {}", method);
                return Ok(None);
            }
            _ => {
                warn!("Unknown MCP method: {}", request.method);
                JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32601,
                        message: format!("Method not found: {}", request.method),
                        data: None,
                    }),
                    id: request.id,
                }
            }
        };

        Ok(Some(response))
    }

    async fn handle_initialize(&self, request: &JsonRpcRequest) -> Result<JsonRpcResponse> {
        info!("Initializing MCP client");
        
        let capabilities = json!({
            "tools": {
                "find_definition": {
                    "description": "Find function definition by name",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "function_name": {
                                "type": "string",
                                "description": "Name of the function to find"
                            }
                        },
                        "required": ["function_name"]
                    }
                }
            },
            "resources": {},
            "prompts": {}
        });

        Ok(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: Some(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": capabilities,
                "serverInfo": {
                    "name": "code-intel-client",
                    "version": "0.1.0"
                }
            })),
            error: None,
            id: request.id.clone(),
        })
    }

    async fn handle_tools_list(&self, request: &JsonRpcRequest) -> Result<JsonRpcResponse> {
        let tools = json!([
            {
                "name": "find_definition",
                "description": "Find function definition by name",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "function_name": {
                            "type": "string",
                            "description": "Name of the function to find"
                        }
                    },
                    "required": ["function_name"]
                }
            }
        ]);

        Ok(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: Some(json!({ "tools": tools })),
            error: None,
            id: request.id.clone(),
        })
    }

    async fn handle_tools_call(&self, request: &JsonRpcRequest) -> Result<JsonRpcResponse> {
        let params = request.params.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Missing parameters for tools/call"))?;

        let tool_name = params.get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing tool name"))?;

        let default_args = json!({});
        let arguments = params.get("arguments")
            .unwrap_or(&default_args);

        match tool_name {
            "find_definition" => self.handle_find_definition_tool(arguments, &request.id).await,
            _ => Ok(JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                result: None,
                error: Some(JsonRpcError {
                    code: -32601,
                    message: format!("Unknown tool: {}", tool_name),
                    data: None,
                }),
                id: request.id.clone(),
            })
        }
    }

    async fn handle_find_definition_tool(&self, arguments: &Value, request_id: &Option<Value>) -> Result<JsonRpcResponse> {
        let function_name = arguments.get("function_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing function_name parameter"))?;

        info!("Finding definition for function: {}", function_name);

        // サーバーに問い合わせ
        let server_result = self.client.find_definition(function_name).await?;
        
        // protocol::FindDefinitionResponse をパース
        let find_response: protocol::FindDefinitionResponse = serde_json::from_value(server_result)?;

        let result = if find_response.definitions.is_empty() {
            json!({
                "content": [{
                    "type": "text",
                    "text": format!("No definition found for function '{}'", function_name)
                }]
            })
        } else {
            let definitions_text = serde_json::to_string_pretty(&find_response.definitions)?;
            json!({
                "content": [{
                    "type": "text",
                    "text": format!("Found {} definition(s) for function '{}':\n\n{}", 
                                  find_response.definitions.len(), 
                                  function_name,
                                  definitions_text)
                }]
            })
        };

        Ok(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: Some(result),
            error: None,
            id: request_id.clone(),
        })
    }

    async fn handle_resources_list(&self, request: &JsonRpcRequest) -> Result<JsonRpcResponse> {
        Ok(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: Some(json!({ "resources": [] })),
            error: None,
            id: request.id.clone(),
        })
    }
}