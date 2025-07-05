use crate::indexer::{CodeIndexer, FileWatchReceiver};
use crate::protocol::{self, ServerRequest, ServerResponse, FindDefinitionParams, FindDefinitionResponse, StatsResponse, SymbolDefinition};
use crate::web_ui::{WebUIServer, LogSender, LogBroadcaster};
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{info, error, debug, warn};
use std::path::Path;

pub struct CodeIntelServer {
    indexer: Arc<Mutex<CodeIndexer>>,
    project_path: String,
    log_broadcaster: Option<LogBroadcaster>,
}

impl CodeIntelServer {
    pub fn new<P: AsRef<Path>>(project_path: P) -> Self {
        Self {
            indexer: Arc::new(Mutex::new(CodeIndexer::new())),
            project_path: project_path.as_ref().to_string_lossy().to_string(),
            log_broadcaster: None,
        }
    }

    pub fn with_web_ui(mut self, log_sender: LogSender) -> Self {
        self.log_broadcaster = Some(LogBroadcaster::new(log_sender));
        self
    }

    /// サーバーを開始してプロジェクトをインデックス
    pub async fn start(&self, port: u16) -> Result<()> {
        let log_message = format!("Starting code_intel server on port {}", port);
        info!("{}", log_message);
        self.broadcast_log(log_message);
        
        // 初回インデックス
        {
            let mut indexer = self.indexer.lock().await;
            let log_message = format!("Initial indexing of project: {}", self.project_path);
            info!("{}", log_message);
            self.broadcast_log(log_message);
            
            indexer.index_directory(&self.project_path)
                .context("Failed to index project")?;
            
            let stats = indexer.get_stats();
            let log_message = format!("Initial indexing completed: {}", stats);
            info!("{}", log_message);
            self.broadcast_log(log_message);
            
            // Web UIに統計情報を送信
            self.broadcast_stats(&stats);
        }

        // TCPリスナー開始
        let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).await
            .context("Failed to bind TCP listener")?;
        
        let log_message = format!("Server listening on 127.0.0.1:{}", port);
        info!("{}", log_message);
        self.broadcast_log(log_message);

        // ファイル監視を別タスクで開始
        {
            let indexer_clone = Arc::clone(&self.indexer);
            let project_path = self.project_path.clone();
            let log_broadcaster = self.log_broadcaster.clone();
            
            tokio::spawn(async move {
                if let Err(e) = Self::start_file_watcher(indexer_clone, project_path, log_broadcaster).await {
                    error!("File watcher error: {}", e);
                }
            });
        }

        // クライアント接続を受け付け
        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    let log_message = format!("New client connection from: {}", addr);
                    debug!("{}", log_message);
                    self.broadcast_log(log_message);
                    
                    let indexer = Arc::clone(&self.indexer);
                    let log_broadcaster = self.log_broadcaster.clone();
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_client(indexer, stream, log_broadcaster).await {
                            error!("Error handling client {}: {}", addr, e);
                        }
                    });
                }
                Err(e) => {
                    let log_message = format!("Failed to accept connection: {}", e);
                    error!("{}", log_message);
                    self.broadcast_log(log_message);
                }
            }
        }
    }

    async fn handle_client(indexer: Arc<Mutex<CodeIndexer>>, mut stream: TcpStream, log_broadcaster: Option<LogBroadcaster>) -> Result<()> {
        let (reader, mut writer) = stream.split();
        let mut reader = BufReader::new(reader);
        let mut line = String::new();

        while reader.read_line(&mut line).await? > 0 {
            let trimmed_line = line.trim();
            if trimmed_line.is_empty() {
                line.clear();
                continue;
            }

            let log_message = format!("Received request: {}", trimmed_line);
            debug!("{}", log_message);
            if let Some(ref broadcaster) = log_broadcaster {
                broadcaster.log(log_message);
            }

            let response = match Self::handle_request(&indexer, trimmed_line).await {
                Ok(response) => {
                    // 成功時に統計情報をブロードキャスト
                    if let Some(ref broadcaster) = log_broadcaster {
                        let indexer_guard = indexer.lock().await;
                        let stats = indexer_guard.get_stats();
                        broadcaster.send_stats(
                            stats.indexed_files_count,
                            stats.total_symbols,
                            stats.unique_symbol_names,
                            stats.is_watching,
                        );
                    }
                    response
                }
                Err(e) => {
                    error!("Error handling request: {}", e);
                    ServerResponse {
                        id: 0, // エラー時はID不明
                        result: None,
                        error: Some(format!("Internal error: {}", e)),
                    }
                }
            };

            let response_json = serde_json::to_string(&response)?;
            debug!("Sending response: {}", response_json);

            writer.write_all(response_json.as_bytes()).await?;
            writer.write_all(b"\n").await?;
            writer.flush().await?;

            line.clear();
        }

        Ok(())
    }

    async fn handle_request(indexer: &Arc<Mutex<CodeIndexer>>, request_line: &str) -> Result<ServerResponse> {
        let request: ServerRequest = serde_json::from_str(request_line)
            .context("Failed to parse request")?;

        debug!("Handling method: {}", request.method);

        let result = match request.method.as_str() {
            protocol::methods::FIND_DEFINITION => {
                Self::handle_find_definition(indexer, &request.params).await?
            }
            protocol::methods::GET_STATS => {
                Self::handle_get_stats(indexer).await?
            }
            protocol::methods::HEALTH_CHECK => {
                json!({ "status": "ok", "timestamp": chrono::Utc::now().timestamp() })
            }
            _ => {
                warn!("Unknown method: {}", request.method);
                return Ok(ServerResponse {
                    id: request.id,
                    result: None,
                    error: Some(format!("Unknown method: {}", request.method)),
                });
            }
        };

        Ok(ServerResponse {
            id: request.id,
            result: Some(result),
            error: None,
        })
    }

    async fn handle_find_definition(indexer: &Arc<Mutex<CodeIndexer>>, params: &Value) -> Result<Value> {
        let params: FindDefinitionParams = serde_json::from_value(params.clone())
            .context("Invalid find_definition parameters")?;

        let indexer_guard = indexer.lock().await;
        
        // 後方互換性のため、function_nameもサポート
        let symbol_name = &params.symbol_name;
        
        let definitions = indexer_guard.find_definition(symbol_name, params.symbol_type);

        let response = match definitions {
            Some(symbols) => {
                let definitions: Vec<SymbolDefinition> = symbols
                    .into_iter()
                    .map(|symbol| (*symbol).clone().into())
                    .collect();
                
                FindDefinitionResponse { definitions }
            }
            None => {
                FindDefinitionResponse { definitions: vec![] }
            }
        };

        Ok(serde_json::to_value(response)?)
    }

    async fn handle_get_stats(indexer: &Arc<Mutex<CodeIndexer>>) -> Result<Value> {
        let indexer_guard = indexer.lock().await;
        let stats = indexer_guard.get_stats();
        let response: StatsResponse = stats.into();
        Ok(serde_json::to_value(response)?)
    }

    fn broadcast_log(&self, message: String) {
        if let Some(ref broadcaster) = self.log_broadcaster {
            broadcaster.log(message);
        }
    }

    fn broadcast_stats(&self, stats: &crate::indexer::IndexStats) {
        if let Some(ref broadcaster) = self.log_broadcaster {
            broadcaster.send_stats(
                stats.indexed_files_count,
                stats.total_symbols,
                stats.unique_symbol_names,
                stats.is_watching,
            );
        }
    }

    /// ファイル監視機能を開始
    async fn start_file_watcher(
        indexer: Arc<Mutex<CodeIndexer>>,
        project_path: String,
        log_broadcaster: Option<LogBroadcaster>,
    ) -> Result<()> {
        let mut watch_receiver = {
            let mut indexer_guard = indexer.lock().await;
            let receiver = indexer_guard.start_watching(&project_path)?;
            
            let log_message = format!("File watcher started for: {}", project_path);
            info!("{}", log_message);
            if let Some(ref broadcaster) = log_broadcaster {
                broadcaster.log(log_message);
            }
            
            receiver
        };

        // ファイル監視イベントを処理
        while let Some(event_result) = watch_receiver.recv().await {
            match event_result {
                Ok(event) => {
                    let mut indexer_guard = indexer.lock().await;
                    match indexer_guard.handle_watch_event(event) {
                        Ok(updated_functions) => {
                            if !updated_functions.is_empty() {
                                let log_message = format!(
                                    "File changes detected, updated {} function(s): {}",
                                    updated_functions.len(),
                                    updated_functions.join(", ")
                                );
                                info!("{}", log_message);
                                if let Some(ref broadcaster) = log_broadcaster {
                                    broadcaster.log(log_message);
                                }

                                // 統計情報を更新
                                let stats = indexer_guard.get_stats();
                                if let Some(ref broadcaster) = log_broadcaster {
                                    broadcaster.send_stats(
                                        stats.indexed_files_count,
                                        stats.total_symbols,
                                        stats.unique_symbol_names,
                                        stats.is_watching,
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            let log_message = format!("Error processing file watch event: {}", e);
                            error!("{}", log_message);
                            if let Some(ref broadcaster) = log_broadcaster {
                                broadcaster.log(log_message);
                            }
                        }
                    }
                }
                Err(e) => {
                    let log_message = format!("File watch error: {}", e);
                    error!("{}", log_message);
                    if let Some(ref broadcaster) = log_broadcaster {
                        broadcaster.log(log_message);
                    }
                }
            }
        }

        Ok(())
    }
}

/// デフォルトポート
pub const DEFAULT_PORT: u16 = 7777;