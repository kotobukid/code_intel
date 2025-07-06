use crate::indexer::{CodeIndexer, FileWatchReceiver};
use crate::protocol::{self, ServerRequest, ServerResponse, FindDefinitionParams, FindDefinitionResponse, StatsResponse, SymbolDefinition, ChangeProjectParams, ChangeProjectResponse};
use crate::web_ui::{WebUIServer, LogSender, LogBroadcaster};
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{info, error, debug, warn};
use std::path::{Path, PathBuf};
use std::collections::HashSet;
use std::time::{Duration, Instant};
use notify::Event;

pub struct CodeIntelServer {
    indexer: Arc<Mutex<CodeIndexer>>,
    project_path: Arc<Mutex<String>>,
    log_broadcaster: Option<LogBroadcaster>,
}

impl CodeIntelServer {
    pub fn new<P: AsRef<Path>>(project_path: P) -> Self {
        Self {
            indexer: Arc::new(Mutex::new(CodeIndexer::new())),
            project_path: Arc::new(Mutex::new(project_path.as_ref().to_string_lossy().to_string())),
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
            let project_path = self.project_path.lock().await.clone();
            let log_message = format!("Initial indexing of project: {}", project_path);
            info!("{}", log_message);
            self.broadcast_log(log_message);
            
            indexer.index_directory(&project_path)
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
            let project_path = self.project_path.lock().await.clone();
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
                    let project_path = Arc::clone(&self.project_path);
                    let log_broadcaster = self.log_broadcaster.clone();
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_client(indexer, project_path, stream, log_broadcaster).await {
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

    async fn handle_client(indexer: Arc<Mutex<CodeIndexer>>, project_path: Arc<Mutex<String>>, mut stream: TcpStream, log_broadcaster: Option<LogBroadcaster>) -> Result<()> {
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
            if let Some(broadcaster) = log_broadcaster.as_ref() {
                broadcaster.log(log_message);
            }

            let response = match Self::handle_request(&indexer, &project_path, trimmed_line).await {
                Ok(response) => {
                    // 成功時に統計情報をブロードキャスト
                    if let Some(broadcaster) = log_broadcaster.as_ref() {
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

    async fn handle_request(indexer: &Arc<Mutex<CodeIndexer>>, project_path: &Arc<Mutex<String>>, request_line: &str) -> Result<ServerResponse> {
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
            protocol::methods::CHANGE_PROJECT => {
                Self::handle_change_project(indexer, project_path, &request.params).await?
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

    async fn handle_change_project(
        indexer: &Arc<Mutex<CodeIndexer>>, 
        project_path: &Arc<Mutex<String>>, 
        params: &Value
    ) -> Result<Value> {
        let params: ChangeProjectParams = serde_json::from_value(params.clone())
            .context("Invalid change_project parameters")?;

        // プロジェクトパスの妥当性チェック
        let new_path = std::path::Path::new(&params.project_path);
        if !new_path.exists() {
            let response = ChangeProjectResponse {
                success: false,
                message: format!("Directory does not exist: {}", params.project_path),
                stats: None,
            };
            return Ok(serde_json::to_value(response)?);
        }

        if !new_path.is_dir() {
            let response = ChangeProjectResponse {
                success: false,
                message: format!("Path is not a directory: {}", params.project_path),
                stats: None,
            };
            return Ok(serde_json::to_value(response)?);
        }

        // プロジェクトパスを更新
        {
            let mut current_path = project_path.lock().await;
            *current_path = params.project_path.clone();
        }

        // インデクサーをリセットして新しいディレクトリをインデックス
        let stats = {
            let mut indexer_guard = indexer.lock().await;
            
            // 既存のウォッチャーを停止
            indexer_guard.stop_watching();
            
            // インデックスをクリア
            *indexer_guard = CodeIndexer::new();
            
            // 新しいディレクトリをインデックス
            indexer_guard.index_directory(&params.project_path)
                .context("Failed to index new project")?;
            
            indexer_guard.get_stats()
        };

        let response = ChangeProjectResponse {
            success: true,
            message: format!("Successfully changed project to: {}", params.project_path),
            stats: Some(stats.into()),
        };
        
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

    /// ファイル監視機能を開始（スロットル機能付き）
    async fn start_file_watcher(
        indexer: Arc<Mutex<CodeIndexer>>,
        project_path: String,
        log_broadcaster: Option<LogBroadcaster>,
    ) -> Result<()> {
        let mut watch_receiver = {
            let mut indexer_guard = indexer.lock().await;
            let receiver = indexer_guard.start_watching(&project_path)?;
            
            let log_message = format!("File watcher started for: {} (Rust files only)", project_path);
            info!("{}", log_message);
            if let Some(broadcaster) = log_broadcaster.as_ref() {
                broadcaster.log(log_message);
            }
            
            receiver
        };

        // スロットル用の共有状態
        let pending_files = Arc::new(Mutex::new(HashSet::<PathBuf>::new()));
        let processing_flag = Arc::new(Mutex::new(false));
        
        // 定期的な処理タスクを起動
        let indexer_clone = Arc::clone(&indexer);
        let pending_files_clone = Arc::clone(&pending_files);
        let processing_flag_clone = Arc::clone(&processing_flag);
        let log_broadcaster_clone = log_broadcaster.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(2));
            loop {
                interval.tick().await;
                
                let files_to_process = {
                    let mut pending = pending_files_clone.lock().await;
                    if pending.is_empty() {
                        continue;
                    }
                    let files = pending.clone();
                    pending.clear();
                    files
                };
                
                let mut processing = processing_flag_clone.lock().await;
                if *processing {
                    continue; // 既に処理中の場合はスキップ
                }
                *processing = true;
                drop(processing);
                
                Self::process_file_changes(
                    &indexer_clone,
                    &files_to_process,
                    &log_broadcaster_clone,
                ).await;
                
                let mut processing = processing_flag_clone.lock().await;
                *processing = false;
            }
        });

        // ファイル監視イベントを処理
        while let Some(event_result) = watch_receiver.recv().await {
            match event_result {
                Ok(event) => {
                    // Rustファイルのみをフィルタリング
                    let rust_files: Vec<PathBuf> = event.paths.into_iter()
                        .filter(|path| Self::is_rust_file(path))
                        .collect();
                    
                    if rust_files.is_empty() {
                        // debug!("Non-Rust file change ignored");
                        continue;
                    }

                    debug!("Rust file change detected: {:?}", rust_files);
                    
                    // 変更されたRustファイルを保留リストに追加
                    {
                        let mut pending = pending_files.lock().await;
                        for path in rust_files {
                            pending.insert(path);
                        }
                    }
                }
                Err(e) => {
                    let log_message = format!("File watch error: {}", e);
                    error!("{}", log_message);
                    if let Some(broadcaster) = log_broadcaster.as_ref() {
                        broadcaster.log(log_message);
                    }
                }
            }
        }

        Ok(())
    }

    /// Rustファイルかどうかを判定
    fn is_rust_file(path: &Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext == "rs")
            .unwrap_or(false)
    }

    /// ファイル変更のバッチ処理
    async fn process_file_changes(
        indexer: &Arc<Mutex<CodeIndexer>>,
        changed_files: &HashSet<PathBuf>,
        log_broadcaster: &Option<LogBroadcaster>,
    ) {
        let mut all_updated_symbols = Vec::new();
        
        {
            let mut indexer_guard = indexer.lock().await;
            
            for path in changed_files {
                // 個別のイベントを作成してhandler関数を呼び出し
                let event = Event {
                    kind: notify::EventKind::Modify(notify::event::ModifyKind::Data(notify::event::DataChange::Content)),
                    paths: vec![path.clone()],
                    attrs: Default::default(),
                };
                
                match indexer_guard.handle_watch_event(event) {
                    Ok(updated_symbols) => {
                        all_updated_symbols.extend(updated_symbols);
                    }
                    Err(e) => {
                        let log_message = format!("Error processing file {}: {}", path.display(), e);
                        error!("{}", log_message);
                        if let Some(broadcaster) = log_broadcaster.as_ref() {
                            broadcaster.log(log_message);
                        }
                    }
                }
            }
        }

        if !all_updated_symbols.is_empty() {
            let log_message = format!(
                "Batch file update completed: {} files processed, {} symbols updated: {}",
                changed_files.len(),
                all_updated_symbols.len(),
                all_updated_symbols.join(", ")
            );
            info!("{}", log_message);
            if let Some(broadcaster) = log_broadcaster.as_ref() {
                broadcaster.log(log_message);
            }

            // 統計情報を更新
            let indexer_guard = indexer.lock().await;
            let stats = indexer_guard.get_stats();
            if let Some(broadcaster) = log_broadcaster.as_ref() {
                broadcaster.send_stats(
                    stats.indexed_files_count,
                    stats.total_symbols,
                    stats.unique_symbol_names,
                    stats.is_watching,
                );
            }
        } else if !changed_files.is_empty() {
            let log_message = format!(
                "Batch file check completed: {} files processed, no symbol changes detected",
                changed_files.len()
            );
            debug!("{}", log_message);
        }
    }
}

/// デフォルトポート
pub const DEFAULT_PORT: u16 = 7777;