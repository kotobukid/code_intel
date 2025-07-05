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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
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

    /// stdio transport で MCP クライアントを開始（REPLモード）
    pub async fn run_stdio(&self) -> Result<()> {
        // MCP通信中はログを無効化（stdoutをクリーンに保つため）
        
        // デバッグ用: 起動確認をstderrに出力（無効化）
        // eprintln!("[MCP] Starting MCP client on stdin/stdout");
        
        // サーバーが起動しているかチェック（ただし継続して動作）
        let _server_available = self.client.is_server_running().await;

        let stdin = tokio::io::stdin();
        let mut stdout = tokio::io::stdout();
        let mut reader = BufReader::new(stdin).lines();

        // 初回のメッセージを待つ（タイムアウトあり）
        let mut first_message = true;
        
        loop {
            match reader.next_line().await? {
                Some(line) => {
                    if first_message {
                        // eprintln!("[MCP] Received first message");
                        first_message = false;
                    }
                    let trimmed = line.trim();
                    
                    // 終了コマンドチェック
                    if trimmed == "/quit" || trimmed == "/exit" {
                        break;
                    }
                    
                    // 空行スキップ
                    if trimmed.is_empty() {
                        continue;
                    }

                    
                    match self.handle_mcp_message(trimmed).await {
                        Ok(Some(response)) => {
                            // コンパクトなJSON出力（改行や余分なスペースを削除）
                            let response_str = serde_json::to_string(&response)?;
                            stdout.write_all(response_str.as_bytes()).await?;
                            stdout.write_all(b"\n").await?;
                            stdout.flush().await?;
                        }
                        Ok(None) => {
                            // Notification (応答なし)
                        }
                        Err(e) => {
                            // エラーは無視（MCPプロトコル維持のため）
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
                None => {
                    // stdin closed (EOF)
                    // eprintln!("[MCP] stdin closed (EOF), exiting");
                    break;
                }
            }
        }

        // eprintln!("[MCP] MCP client shutting down");
        Ok(())
    }

    async fn handle_mcp_message(&self, message: &str) -> Result<Option<JsonRpcResponse>> {
        let request: JsonRpcRequest = serde_json::from_str(message)
            .context("Failed to parse JSON-RPC request")?;

        // debug!("Handling MCP method: {}", request.method);

        let response = match request.method.as_str() {
            "initialize" => self.handle_initialize(&request).await?,
            "tools/list" => self.handle_tools_list(&request).await?,
            "tools/call" => self.handle_tools_call(&request).await?,
            "resources/list" => self.handle_resources_list(&request).await?,
            method if method.starts_with("notifications/") => {
                // notification処理（応答不要）
                return Ok(None);
            }
            "initialized" => {
                // initialized notification（応答不要）
                return Ok(None);
            }
            "ping" => {
                // ping要求に対してpong応答
                JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    result: Some(json!({})),
                    error: None,
                    id: request.id.clone(),
                }
            }
            _ => {
                // Unknown method - silent ignore
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
        // info!("Initializing MCP client");
        
        let capabilities = json!({
            "tools": {
                "find_definition": {
                    "description": "Find symbol definition by name (functions, structs, enums, traits)",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "symbol_name": {
                                "type": "string",
                                "description": "Name of the symbol to find"
                            },
                            "symbol_type": {
                                "type": "string",
                                "description": "Type of symbol to search for (Function, Struct, Enum, Trait). If not specified, searches all types.",
                                "enum": ["Function", "Struct", "Enum", "Trait"]
                            }
                        },
                        "required": ["symbol_name"]
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
                "description": "Find symbol definition by name (functions, structs, enums, traits)",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "symbol_name": {
                            "type": "string",
                            "description": "Name of the symbol to find"
                        },
                        "symbol_type": {
                            "type": "string",
                            "description": "Type of symbol to search for (Function, Struct, Enum, Trait). If not specified, searches all types.",
                            "enum": ["Function", "Struct", "Enum", "Trait"]
                        }
                    },
                    "required": ["symbol_name"]
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
        // 後方互換性のため、function_nameも受け付ける
        let symbol_name = arguments.get("symbol_name")
            .or_else(|| arguments.get("function_name"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing symbol_name parameter"))?;
        
        // symbol_typeパラメータを取得
        let symbol_type = arguments.get("symbol_type")
            .and_then(|v| v.as_str())
            .and_then(|s| serde_json::from_value::<protocol::SymbolType>(json!(s)).ok());

        // info!("Finding definition for symbol: {} (type: {:?})", symbol_name, symbol_type);

        // サーバーが起動しているかチェック
        if !self.client.is_server_running().await {
            return Ok(JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                result: Some(json!({
                    "content": [{
                        "type": "text",
                        "text": "Error: Code intelligence server is not running. Please start the server with 'code_intel serve' before using this tool."
                    }]
                })),
                error: None,
                id: request_id.clone(),
            });
        }

        // サーバーに問い合わせ
        let server_result = self.client.find_definition_with_type(symbol_name, symbol_type).await?;
        
        // protocol::FindDefinitionResponse をパース
        let find_response: protocol::FindDefinitionResponse = serde_json::from_value(server_result)?;

        let result = if find_response.definitions.is_empty() {
            json!({
                "content": [{
                    "type": "text",
                    "text": format!("No definition found for symbol '{}'", symbol_name)
                }]
            })
        } else {
            let definitions_text = serde_json::to_string_pretty(&find_response.definitions)?;
            json!({
                "content": [{
                    "type": "text",
                    "text": format!("Found {} definition(s) for symbol '{}':\n\n{}", 
                                  find_response.definitions.len(), 
                                  symbol_name,
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