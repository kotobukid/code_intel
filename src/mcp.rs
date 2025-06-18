use crate::indexer::CodeIndexer;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader as TokioBufReader};
use tokio::sync::Mutex;
use std::sync::Arc;
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpCapabilities {
    pub tools: Option<Value>,
    pub resources: Option<Value>,
    pub prompts: Option<Value>,
}

pub struct McpServer {
    indexer: Arc<Mutex<CodeIndexer>>,
    server_info: McpServerInfo,
}

impl McpServer {
    pub fn new() -> Self {
        Self {
            indexer: Arc::new(Mutex::new(CodeIndexer::new())),
            server_info: McpServerInfo {
                name: "code-intel".to_string(),
                version: "0.1.0".to_string(),
            },
        }
    }

    /// stdio transport で MCP サーバーを開始
    pub async fn run_stdio(&self) -> Result<()> {
        info!("Starting MCP server with stdio transport");
        
        let stdin = tokio::io::stdin();
        let mut stdout = tokio::io::stdout();
        let mut reader = TokioBufReader::new(stdin).lines();

        while let Some(line) = reader.next_line().await? {
            if line.trim().is_empty() {
                continue;
            }

            debug!("Received: {}", line);

            match self.handle_message(&line).await {
                Ok(Some(response)) => {
                    let response_str = serde_json::to_string(&response)?;
                    debug!("Sending: {}", response_str);
                    
                    stdout.write_all(response_str.as_bytes()).await?;
                    stdout.write_all(b"\n").await?;
                    stdout.flush().await?;
                }
                Ok(None) => {
                    // Notification (応答なし)
                }
                Err(e) => {
                    error!("Error handling message: {}", e);
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

    async fn handle_message(&self, message: &str) -> Result<Option<JsonRpcResponse>> {
        let request: JsonRpcRequest = serde_json::from_str(message)
            .context("Failed to parse JSON-RPC request")?;

        debug!("Handling method: {}", request.method);

        let response = match request.method.as_str() {
            "initialize" => self.handle_initialize(&request).await?,
            "tools/list" => self.handle_tools_list(&request).await?,
            "tools/call" => self.handle_tools_call(&request).await?,
            "resources/list" => self.handle_resources_list(&request).await?,
            method if method.starts_with("notifications/") => {
                // Notifications don't need responses
                debug!("Received notification: {}", method);
                return Ok(None);
            }
            _ => {
                warn!("Unknown method: {}", request.method);
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
        info!("Initializing MCP server");
        
        let capabilities = McpCapabilities {
            tools: Some(json!({
                "find_definition": {
                    "description": "Find function definition by name",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "function_name": {
                                "type": "string",
                                "description": "Name of the function to find"
                            },
                            "project_path": {
                                "type": "string", 
                                "description": "Path to the project root (optional)"
                            }
                        },
                        "required": ["function_name"]
                    }
                }
            })),
            resources: Some(json!({})),
            prompts: Some(json!({})),
        };

        Ok(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: Some(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": capabilities,
                "serverInfo": self.server_info
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
                        },
                        "project_path": {
                            "type": "string",
                            "description": "Path to the project root (optional)"
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
        let params = request.params.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Missing parameters for tools/call")
        })?;

        let tool_name = params.get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing tool name"))?;

        let default_args = json!({});
        let arguments = params.get("arguments")
            .unwrap_or(&default_args);

        match tool_name {
            "find_definition" => self.handle_find_definition(arguments, &request.id).await,
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

    async fn handle_find_definition(&self, arguments: &Value, request_id: &Option<Value>) -> Result<JsonRpcResponse> {
        let function_name = arguments.get("function_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing function_name parameter"))?;

        let project_path = arguments.get("project_path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");

        info!("Finding definition for function: {} in project: {}", function_name, project_path);

        // プロジェクトをインデックス
        {
            let mut indexer = self.indexer.lock().await;
            indexer.index_directory(project_path)
                .context("Failed to index project")?;
        }

        // 関数定義を検索
        let indexer = self.indexer.lock().await;
        let definitions = indexer.find_definition(function_name);

        let result = match definitions {
            Some(funcs) => {
                let definitions: Vec<Value> = funcs.iter().map(|func| {
                    json!({
                        "name": func.name,
                        "file_path": func.file_path,
                        "line": func.line,
                        "column": func.column,
                        "signature": func.signature,
                        "visibility": func.visibility
                    })
                }).collect();

                json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Found {} definition(s) for function '{}':\n\n{}", 
                                      definitions.len(), 
                                      function_name,
                                      serde_json::to_string_pretty(&definitions)?)
                    }]
                })
            }
            None => {
                json!({
                    "content": [{
                        "type": "text", 
                        "text": format!("No definition found for function '{}'", function_name)
                    }]
                })
            }
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