use crate::protocol::{self, ServerRequest, ServerResponse, FindDefinitionParams, SymbolType};
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::net::TcpStream;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

static REQUEST_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

pub struct CodeIntelClient {
    port: u16,
}

impl CodeIntelClient {
    pub fn new(port: u16) -> Self {
        Self { port }
    }

    /// サーバーに接続してリクエストを送信
    async fn send_request_internal(&self, method: &str, params: Value) -> Result<ServerResponse> {
        let mut stream = TcpStream::connect(format!("127.0.0.1:{}", self.port)).await
            .context("Failed to connect to code_intel server")?;

        let request = ServerRequest {
            id: REQUEST_ID_COUNTER.fetch_add(1, Ordering::SeqCst),
            method: method.to_string(),
            params,
        };

        let request_json = serde_json::to_string(&request)?;
        // debug!("Sending request: {}", request_json);

        // リクエスト送信
        stream.write_all(request_json.as_bytes()).await?;
        stream.write_all(b"\n").await?;
        stream.flush().await?;

        // レスポンス受信
        let (reader, _writer) = stream.split();
        let mut reader = BufReader::new(reader);
        let mut response_line = String::new();
        
        reader.read_line(&mut response_line).await
            .context("Failed to read response")?;

        let response: ServerResponse = serde_json::from_str(response_line.trim())
            .context("Failed to parse response")?;

        // debug!("Received response: {:?}", response);

        if let Some(error) = response.error {
            return Err(anyhow::anyhow!("Server error: {}", error));
        }

        Ok(response)
    }
    
    /// サーバーに任意のリクエストを送信（公開API）
    pub async fn send_request(&self, request: ServerRequest) -> Result<ServerResponse> {
        self.send_request_internal(&request.method, request.params).await
    }

    /// シンボル定義を検索（互換性のための旧API）
    pub async fn find_definition(&self, function_name: &str) -> Result<Value> {
        self.find_definition_with_type(function_name, Some(SymbolType::Function)).await
    }
    
    /// シンボル定義を検索（型指定付き）
    pub async fn find_definition_with_type(&self, symbol_name: &str, symbol_type: Option<SymbolType>) -> Result<Value> {
        let params = serde_json::to_value(FindDefinitionParams {
            symbol_name: symbol_name.to_string(),
            symbol_type,
        })?;

        let response = self.send_request_internal(protocol::methods::FIND_DEFINITION, params).await?;
        response.result.ok_or_else(|| anyhow::anyhow!("No result in response"))
    }

    /// サーバー統計を取得
    pub async fn get_stats(&self) -> Result<Value> {
        let response = self.send_request_internal(protocol::methods::GET_STATS, json!({})).await?;
        response.result.ok_or_else(|| anyhow::anyhow!("No result in response"))
    }

    /// ヘルスチェック
    pub async fn health_check(&self) -> Result<Value> {
        let response = self.send_request_internal(protocol::methods::HEALTH_CHECK, json!({})).await?;
        response.result.ok_or_else(|| anyhow::anyhow!("No result in response"))
    }

    /// サーバーが起動しているかチェック
    pub async fn is_server_running(&self) -> bool {
        (self.health_check().await).is_ok()
    }
}