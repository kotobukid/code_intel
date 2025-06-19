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
pub struct WebUIState {
    pub log_sender: LogSender,
}

pub struct WebUIServer {
    state: WebUIState,
}

// グローバルな統計情報を保持
use tokio::sync::RwLock;

lazy_static::lazy_static! {
    static ref CURRENT_STATS: Arc<RwLock<Option<(usize, usize, usize, bool)>>> = Arc::new(RwLock::new(None));
}

impl WebUIServer {
    pub fn new() -> (Self, LogSender) {
        let (log_sender, _) = broadcast::channel(1000);
        let state = WebUIState {
            log_sender: log_sender.clone(),
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
    </style>
</head>
<body>
    <div class="header">
        <h1>🦀 Code Intel Server Dashboard</h1>
        <span id="status" class="status disconnected">Disconnected</span>
    </div>
    
    <div class="stats">
        <div class="stat-card">
            <h3>📁 Indexed Files</h3>
            <div id="file-count">-</div>
        </div>
        <div class="stat-card">
            <h3>⚙️ Total Functions</h3>
            <div id="function-count">-</div>
        </div>
        <div class="stat-card">
            <h3>🔍 Unique Names</h3>
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
        
        function updateStats(stats) {
            console.log('updateStats called with:', stats);
            
            const fileCountEl = document.getElementById('file-count');
            const functionCountEl = document.getElementById('function-count');
            const uniqueCountEl = document.getElementById('unique-count');
            const watchStatusEl = document.getElementById('watch-status');
            
            console.log('Elements found:', {
                fileCountEl,
                functionCountEl, 
                uniqueCountEl,
                watchStatusEl
            });
            
            if (stats.indexed_files_count !== undefined && fileCountEl) {
                console.log('Updating file-count to:', stats.indexed_files_count);
                fileCountEl.textContent = stats.indexed_files_count;
            }
            if (stats.total_functions !== undefined && functionCountEl) {
                console.log('Updating function-count to:', stats.total_functions);
                functionCountEl.textContent = stats.total_functions;
            }
            if (stats.unique_function_names !== undefined && uniqueCountEl) {
                console.log('Updating unique-count to:', stats.unique_function_names);
                uniqueCountEl.textContent = stats.unique_function_names;
            }
            if (stats.is_watching !== undefined && watchStatusEl) {
                console.log('Updating watch-status to:', stats.is_watching);
                watchStatusEl.textContent = stats.is_watching ? '🟢 Active' : '🔴 Inactive';
                watchStatusEl.style.color = stats.is_watching ? '#10b981' : '#ef4444';
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
    if let Some((indexed_files, total_functions, unique_names, is_watching)) = *CURRENT_STATS.read().await {
        let stats_message = json!({
            "type": "stats",
            "indexed_files_count": indexed_files,
            "total_functions": total_functions,
            "unique_function_names": unique_names,
            "is_watching": is_watching
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

    // クライアントからのメッセージを受信（ping/pong等）
    let recv_task = tokio::spawn(async move {
        while let Some(msg) = ws_receiver.next().await {
            match msg {
                Ok(Message::Text(_)) => {
                    // クライアントからのメッセージは現在は無視
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

    pub fn send_stats(&self, indexed_files: usize, total_functions: usize, unique_names: usize, is_watching: bool) {
        // グローバル統計を更新
        tokio::spawn(async move {
            let mut stats = CURRENT_STATS.write().await;
            *stats = Some((indexed_files, total_functions, unique_names, is_watching));
        });
        
        let stats_message = json!({
            "type": "stats",
            "indexed_files_count": indexed_files,
            "total_functions": total_functions,
            "unique_function_names": unique_names,
            "is_watching": is_watching
        });
        let _ = self.sender.send(stats_message.to_string());
    }
}

use axum::extract::ws::CloseFrame;
use futures_util::{SinkExt, StreamExt};