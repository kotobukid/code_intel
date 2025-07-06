use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;
use tracing::{debug, error, info, warn};

pub type LogSender = broadcast::Sender<String>;
pub type LogReceiver = broadcast::Receiver<String>;

#[derive(Clone)]
struct StatsData {
    indexed_files_count: usize,
    total_symbols: usize,
    total_functions: usize,
    total_structs: usize,
    total_enums: usize,
    total_traits: usize,
    unique_symbol_names: usize,
    is_watching: bool,
}

#[derive(Clone)]
pub struct WebUIState {
    pub log_sender: LogSender,
    pub tcp_port: u16,
}

pub struct WebUIServer {
    state: WebUIState,
}

// グローバルな統計情報を保持
use tokio::sync::RwLock;

lazy_static::lazy_static! {
    static ref CURRENT_STATS: Arc<RwLock<Option<StatsData>>> = Arc::new(RwLock::new(None));
}

impl WebUIServer {
    pub fn new(tcp_port: u16) -> (Self, LogSender) {
        let (log_sender, _) = broadcast::channel(1000);
        let state = WebUIState {
            log_sender: log_sender.clone(),
            tcp_port,
        };
        
        (Self { state }, log_sender)
    }

    pub async fn start(&self, port: u16) -> Result<(), anyhow::Error> {
        info!("Starting Web UI server on port {}", port);
        
        let app = Router::new()
            .route("/", get(dashboard))
            .route("/ws", get(websocket_handler))
            .layer(CorsLayer::permissive())
            .with_state(self.state.clone());

        let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
        info!("Web UI server listening on http://localhost:{}", port);
        
        axum::serve(listener, app).await?;
        Ok(())
    }
}

async fn dashboard() -> impl IntoResponse {
    Html(r#"
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Code Intel Server Dashboard</title>
    <style>
        body {
            font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif;
            margin: 0;
            padding: 20px;
            background-color: #1e1e1e;
            color: #d4d4d4;
        }
        .header {
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            padding: 20px;
            border-radius: 10px;
            margin-bottom: 20px;
        }
        .header h1 {
            margin: 0;
            color: white;
            font-size: 2em;
        }
        .stats {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
            gap: 15px;
            margin-bottom: 20px;
        }
        .stat-card {
            background: #2d2d30;
            padding: 15px;
            border-radius: 8px;
            border: 1px solid #3e3e42;
        }
        .stat-card h3 {
            margin: 0 0 10px 0;
            color: #569cd6;
        }
        .logs-container {
            background: #0d1117;
            border: 1px solid #30363d;
            border-radius: 8px;
            height: 500px;
            overflow-y: auto;
            padding: 15px;
            font-family: 'Consolas', 'Monaco', monospace;
            font-size: 13px;
        }
        .log-entry {
            margin: 2px 0;
            padding: 2px 5px;
            border-radius: 3px;
        }
        .log-info { color: #7dd3fc; }
        .log-debug { color: #a3a3a3; }
        .log-warn { color: #fbbf24; }
        .log-error { color: #f87171; background: rgba(248, 113, 113, 0.1); }
        .status {
            display: inline-block;
            padding: 4px 8px;
            border-radius: 12px;
            font-size: 12px;
            font-weight: bold;
        }
        .status.connected {
            background: #10b981;
            color: white;
        }
        .status.disconnected {
            background: #ef4444;
            color: white;
        }
        .controls {
            margin-bottom: 15px;
        }
        .btn {
            background: #0969da;
            color: white;
            border: none;
            padding: 8px 16px;
            border-radius: 5px;
            cursor: pointer;
            margin-right: 10px;
        }
        .btn:hover {
            background: #0550ae;
        }
        .change-project {
            background: #2c3e50;
            padding: 20px;
            border-radius: 10px;
            margin-bottom: 20px;
        }
        .change-project h3 {
            margin-top: 0;
            color: #fff;
        }
        .change-project input {
            width: 60%;
            padding: 8px 12px;
            background: #1e1e1e;
            border: 1px solid #444;
            color: #d4d4d4;
            border-radius: 5px;
            margin-right: 10px;
        }
        .current-path {
            color: #888;
            font-size: 14px;
            margin-bottom: 10px;
        }
    </style>
</head>
<body>
    <div class="header">
        <h1>🦀 Code Intel Server Dashboard</h1>
        <span id="status" class="status disconnected">Disconnected</span>
    </div>
    
    <div class="change-project">
        <h3>📂 Change Project Directory</h3>
        <div class="current-path" id="current-path">Current: Loading...</div>
        <input type="text" id="project-path" placeholder="Enter new project path (e.g., /path/to/project)">
        <button class="btn" onclick="changeProject()">Change Directory</button>
        <button class="btn" onclick="selectLocalDirectory()" id="select-dir-btn">📁 Browse Local Directory</button>
        <div id="fs-api-warning" style="display: none; color: #fbbf24; margin-top: 10px; font-size: 14px;">
            ⚠️ File System API is not supported in your browser or requires HTTPS
        </div>
    </div>
    
    <div class="stats">
        <div class="stat-card">
            <h3>📁 Indexed Files</h3>
            <div id="file-count">-</div>
        </div>
        <div class="stat-card">
            <h3>🔍 Total Symbols</h3>
            <div id="function-count">-</div>
        </div>
        <div class="stat-card">
            <h3>📊 Unique Names</h3>
            <div id="unique-count">-</div>
        </div>
        <div class="stat-card">
            <h3>👁️ File Watching</h3>
            <div id="watch-status">-</div>
        </div>
        <div class="stat-card">
            <h3>⏱️ Uptime</h3>
            <div id="uptime">-</div>
        </div>
    </div>
    
    <div class="controls">
        <button class="btn" onclick="clearLogs()">Clear Logs</button>
        <button class="btn" onclick="toggleAutoScroll()">Auto Scroll: <span id="autoscroll-status">ON</span></button>
    </div>
    
    <div class="logs-container" id="logs"></div>

    <script>
        let ws = null;
        let autoScroll = true;
        let startTime = new Date();
        
        function connect() {
            const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
            ws = new WebSocket(`${protocol}//${window.location.host}/ws`);
            
            ws.onopen = function() {
                document.getElementById('status').className = 'status connected';
                document.getElementById('status').textContent = 'Connected';
                console.log('WebSocket connected');
            };
            
            ws.onmessage = function(event) {
                console.log('Received WebSocket message:', event.data);
                try {
                    const data = JSON.parse(event.data);
                    console.log('Parsed data:', data);
                    if (data.type === 'log') {
                        addLogEntry(data.message);
                    } else if (data.type === 'stats') {
                        console.log('Updating stats with:', data);
                        updateStats(data);
                    } else if (data.type === 'change_project_response') {
                        if (data.success) {
                            addLogEntry(`✅ ${data.message}`);
                            if (data.stats) {
                                updateStats(data.stats);
                            }
                        } else {
                            addLogEntry(`❌ Error: ${data.message}`);
                        }
                    } else {
                        console.log('Unknown message type:', data.type);
                        addLogEntry(`Unknown message: ${JSON.stringify(data)}`);
                    }
                } catch (e) {
                    console.error('Parse error:', e, 'Raw data:', event.data);
                    addLogEntry(`Parse error: ${event.data}`);
                }
            };
            
            ws.onclose = function() {
                document.getElementById('status').className = 'status disconnected';
                document.getElementById('status').textContent = 'Disconnected';
                console.log('WebSocket disconnected, reconnecting...');
                setTimeout(connect, 2000);
            };
            
            ws.onerror = function(error) {
                console.error('WebSocket error:', error);
            };
        }
        
        function addLogEntry(message) {
            const logsDiv = document.getElementById('logs');
            const logEntry = document.createElement('div');
            logEntry.className = 'log-entry';
            
            const timestamp = new Date().toLocaleTimeString();
            
            // ログレベルに応じてスタイルを設定
            if (message.includes('ERROR')) {
                logEntry.className += ' log-error';
            } else if (message.includes('WARN')) {
                logEntry.className += ' log-warn';
            } else if (message.includes('INFO')) {
                logEntry.className += ' log-info';
            } else if (message.includes('DEBUG')) {
                logEntry.className += ' log-debug';
            }
            
            logEntry.textContent = `[${timestamp}] ${message}`;
            logsDiv.appendChild(logEntry);
            
            if (autoScroll) {
                logsDiv.scrollTop = logsDiv.scrollHeight;
            }
        }
        
        
        function clearLogs() {
            document.getElementById('logs').innerHTML = '';
        }
        
        function toggleAutoScroll() {
            autoScroll = !autoScroll;
            document.getElementById('autoscroll-status').textContent = autoScroll ? 'ON' : 'OFF';
        }
        
        function updateUptime() {
            const now = new Date();
            const diff = Math.floor((now - startTime) / 1000);
            const hours = Math.floor(diff / 3600);
            const minutes = Math.floor((diff % 3600) / 60);
            const seconds = diff % 60;
            document.getElementById('uptime').textContent = 
                `${hours.toString().padStart(2, '0')}:${minutes.toString().padStart(2, '0')}:${seconds.toString().padStart(2, '0')}`;
        }
        
        let currentProjectPath = '';
        
        function updateStats(data) {
            document.getElementById('file-count').textContent = data.indexed_files_count || '0';
            document.getElementById('function-count').textContent = data.total_symbols || '0';
            document.getElementById('unique-count').textContent = data.unique_symbol_names || '0';
            document.getElementById('watch-status').textContent = data.is_watching ? '✅ Active' : '❌ Inactive';
            
            // プロジェクトパスが含まれている場合は更新
            if (data.project_path) {
                currentProjectPath = data.project_path;
                document.getElementById('current-path').textContent = `Current: ${currentProjectPath}`;
                document.getElementById('project-path').value = currentProjectPath;
            }
        }
        
        async function changeProject() {
            const newPath = document.getElementById('project-path').value.trim();
            if (!newPath) {
                alert('Please enter a valid directory path');
                return;
            }
            
            if (!ws || ws.readyState !== WebSocket.OPEN) {
                addLogEntry('❌ WebSocket is not connected');
                return;
            }
            
            // WebSocket経由でchange_projectリクエストを送信
            const request = {
                type: 'change_project',
                project_path: newPath
            };
            
            ws.send(JSON.stringify(request));
            addLogEntry(`📤 Requesting project change to: ${newPath}`);
        }
        
        async function selectLocalDirectory() {
            // File System Access APIのサポートチェック
            if (!('showDirectoryPicker' in window)) {
                document.getElementById('fs-api-warning').style.display = 'block';
                addLogEntry('❌ File System Access API is not supported in this browser');
                
                // フォールバック: ファイル入力を使用（ディレクトリ選択）
                const input = document.createElement('input');
                input.type = 'file';
                input.webkitdirectory = true;
                input.directory = true;
                
                input.onchange = (e) => {
                    if (e.target.files.length > 0) {
                        // ファイルパスからディレクトリパスを抽出
                        const file = e.target.files[0];
                        const path = file.webkitRelativePath || file.name;
                        const dirPath = path.substring(0, path.lastIndexOf('/'));
                        
                        // 注意: セキュリティ上の理由で、ブラウザは完全なローカルパスを提供しません
                        addLogEntry(`ℹ️ Selected directory: ${dirPath} (Note: Full path is not available due to browser security)`);
                        document.getElementById('project-path').value = dirPath;
                    }
                };
                
                input.click();
                return;
            }
            
            try {
                // File System Access APIを使用してディレクトリを選択
                const dirHandle = await window.showDirectoryPicker({
                    mode: 'read',
                    startIn: 'documents'
                });
                
                // ディレクトリハンドルから情報を取得
                const dirName = dirHandle.name;
                addLogEntry(`✅ Selected directory: ${dirName}`);
                
                // 注意: File System Access APIもセキュリティ上の理由で完全なパスを提供しません
                // しかし、ローカルサーバーの場合は、ディレクトリ名から推測することは可能です
                
                // もしサーバーがローカルで動作している場合の推測パス
                if (window.location.hostname === 'localhost' || window.location.hostname === '127.0.0.1') {
                    // ユーザーに完全なパスを入力してもらうためのヒントを表示
                    const suggestedPath = prompt(
                        `Selected directory: "${dirName}"\n\n` +
                        `Please enter the full path to this directory:\n` +
                        `(e.g., /home/user/projects/${dirName} or C:\\Users\\name\\projects\\${dirName})`,
                        dirName
                    );
                    
                    if (suggestedPath) {
                        document.getElementById('project-path').value = suggestedPath;
                        addLogEntry(`📝 Path set to: ${suggestedPath}`);
                    }
                } else {
                    // リモートサーバーの場合
                    alert(`Selected: ${dirName}\n\nFor remote servers, please enter the full server-side path manually.`);
                    document.getElementById('project-path').value = dirName;
                }
                
            } catch (err) {
                if (err.name === 'AbortError') {
                    addLogEntry('ℹ️ Directory selection cancelled');
                } else {
                    addLogEntry(`❌ Error selecting directory: ${err.message}`);
                    console.error('Directory selection error:', err);
                }
            }
        }
        
        // ページ読み込み時にFile System Access APIのサポートをチェック
        window.addEventListener('DOMContentLoaded', () => {
            if (!('showDirectoryPicker' in window)) {
                // HTTPSでない場合やAPIがサポートされていない場合の警告
                const isSecure = window.location.protocol === 'https:' || window.location.hostname === 'localhost';
                if (!isSecure) {
                    document.getElementById('fs-api-warning').textContent = 
                        '⚠️ File System API requires HTTPS (works on localhost)';
                    document.getElementById('fs-api-warning').style.display = 'block';
                }
            }
        });
        
        // Connect and start timers
        connect();
        setInterval(updateUptime, 1000);
    </script>
</body>
</html>
    "#)
}

async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<WebUIState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| websocket_connection(socket, state))
}

async fn websocket_connection(socket: WebSocket, state: WebUIState) {
    debug!("New WebSocket connection established");
    
    let mut log_receiver = state.log_sender.subscribe();
    let (mut ws_sender, mut ws_receiver) = socket.split();

    // WebSocket接続時に現在の統計情報を送信
    let initial_log_message = json!({
        "type": "log",
        "message": "WebSocket connected to dashboard"
    });
    
    if let Err(e) = ws_sender.send(Message::Text(initial_log_message.to_string())).await {
        warn!("Failed to send initial log message: {}", e);
    }
    
    // 保存されている統計情報があれば送信
    if let Some(stats_data) = CURRENT_STATS.read().await.as_ref() {
        let stats_message = json!({
            "type": "stats",
            "indexed_files_count": stats_data.indexed_files_count,
            "total_symbols": stats_data.total_symbols,
            "unique_symbol_names": stats_data.unique_symbol_names,
            "is_watching": stats_data.is_watching
        });
        
        if let Err(e) = ws_sender.send(Message::Text(stats_message.to_string())).await {
            warn!("Failed to send initial stats: {}", e);
        }
    }

    // ログメッセージをクライアントに転送
    let send_task = tokio::spawn(async move {
        while let Ok(log_message) = log_receiver.recv().await {
            // メッセージがすでにJSONかどうかチェック
            let message = if log_message.starts_with("{") && log_message.contains("\"type\"") {
                // すでに整形されたJSONメッセージ（統計情報など）
                log_message
            } else {
                // 通常のログメッセージ
                json!({
                    "type": "log",
                    "message": log_message
                }).to_string()
            };
            
            if let Err(e) = ws_sender.send(Message::Text(message)).await {
                debug!("WebSocket send error: {}", e);
                break;
            }
        }
    });

    // クライアントからのメッセージを受信
    let tcp_port = state.tcp_port;
    let log_sender_clone = state.log_sender.clone();
    let recv_task = tokio::spawn(async move {
        while let Some(msg) = ws_receiver.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    // クライアントからのメッセージを処理
                    if let Ok(data) = serde_json::from_str::<serde_json::Value>(&text) {
                        if data["type"] == "change_project" {
                            if let Some(project_path) = data["project_path"].as_str() {
                                // TCPクライアントを使用してサーバーにリクエストを送信
                                tokio::spawn(handle_change_project_request(
                                    tcp_port,
                                    project_path.to_string(),
                                    log_sender_clone.clone(),
                                ));
                            }
                        }
                    }
                }
                Ok(Message::Close(_)) => {
                    debug!("WebSocket connection closed by client");
                    break;
                }
                Err(e) => {
                    debug!("WebSocket receive error: {}", e);
                    break;
                }
                _ => {}
            }
        }
    });

    // どちらかのタスクが終了したら接続を閉じる
    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }

    debug!("WebSocket connection closed");
}

// カスタムログ Subscriber を作成してログをブロードキャスト
#[derive(Clone)]
pub struct LogBroadcaster {
    sender: LogSender,
}

impl LogBroadcaster {
    pub fn new(sender: LogSender) -> Self {
        Self { sender }
    }

    pub fn log(&self, message: String) {
        // ブロードキャストチャンネルが満杯でもエラーにしない
        let _ = self.sender.send(message);
    }

    pub fn send_stats(&self, indexed_files: usize, total_symbols: usize, unique_names: usize, is_watching: bool) {
        // グローバル統計を更新
        let stats_data = StatsData {
            indexed_files_count: indexed_files,
            total_symbols,
            total_functions: 0,  // TODO: 個別の統計を受け取るように改善
            total_structs: 0,
            total_enums: 0,
            total_traits: 0,
            unique_symbol_names: unique_names,
            is_watching,
        };
        
        tokio::spawn(async move {
            let mut stats = CURRENT_STATS.write().await;
            *stats = Some(stats_data.clone());
        });
        
        let stats_message = json!({
            "type": "stats",
            "indexed_files_count": indexed_files,
            "total_symbols": total_symbols,
            "unique_symbol_names": unique_names,
            "is_watching": is_watching
        });
        let _ = self.sender.send(stats_message.to_string());
    }
}

use axum::extract::ws::CloseFrame;
use crate::client::CodeIntelClient;
use crate::protocol::{ServerRequest, ChangeProjectParams};
use futures_util::{SinkExt, StreamExt};

async fn handle_change_project_request(tcp_port: u16, project_path: String, log_sender: LogSender) {
    let client = CodeIntelClient::new(tcp_port);
    
    // change_projectリクエストを送信
    let request = ServerRequest {
        id: 1,
        method: "change_project".to_string(),
        params: serde_json::to_value(ChangeProjectParams { project_path: project_path.clone() }).unwrap(),
    };
    
    match client.send_request(request).await {
        Ok(response) => {
            if let Some(result) = response.result {
                // 結果をWebSocketクライアントに送信
                let message = json!({
                    "type": "change_project_response",
                    "success": result["success"].as_bool().unwrap_or(false),
                    "message": result["message"].as_str().unwrap_or("Unknown response"),
                    "stats": result["stats"]
                });
                
                let _ = log_sender.send(message.to_string());
            } else if let Some(error) = response.error {
                let message = json!({
                    "type": "change_project_response",
                    "success": false,
                    "message": error
                });
                
                let _ = log_sender.send(message.to_string());
            }
        }
        Err(e) => {
            let message = json!({
                "type": "change_project_response",
                "success": false,
                "message": format!("Failed to connect to server: {}", e)
            });
            
            let _ = log_sender.send(message.to_string());
        }
    }
}