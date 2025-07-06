mod parser;
mod indexer;
mod protocol;
mod server;
mod client;
mod mcp_client;
mod web_ui;
mod graph;

use clap::{Parser, Subcommand};
use server::{CodeIntelServer, DEFAULT_PORT};
use mcp_client::McpClient;
use web_ui::WebUIServer;
use graph::CallGraphGenerator;
use tracing::{info, error};
use tracing_subscriber::{EnvFilter, fmt};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "code_intel")]
#[command(about = "Code Intelligence Service for AI Tools")]
#[command(version = "0.1.0")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the code intelligence server
    Serve {
        /// Project path to index
        #[arg(default_value = ".")]
        project_path: PathBuf,
        
        /// Port to listen on
        #[arg(short, long, default_value_t = DEFAULT_PORT)]
        port: u16,
        
        /// Enable web UI dashboard
        #[arg(long)]
        web_ui: bool,
        
        /// Web UI port
        #[arg(long, default_value_t = 8080)]
        web_port: u16,
        
        /// Open browser automatically when web UI is enabled
        #[arg(long)]
        open: bool,
    },
    /// Generate call graph visualization
    Graph {
        /// Project path to analyze
        #[arg(default_value = ".")]
        project_path: PathBuf,
        
        /// Focus on specific function
        #[arg(short, long)]
        function: Option<String>,
        
        /// Output format
        #[arg(short = 'o', long, default_value = "tree")]
        format: String,
        
        /// Maximum depth for call graph
        #[arg(short, long, default_value_t = 10)]
        depth: usize,
        
        /// Show only callers (reverse call graph)
        #[arg(long)]
        callers: bool,
    },
    /// Run as MCP client (for Claude Code integration)
    McpClient {
        /// Port to connect to
        #[arg(short, long, default_value_t = DEFAULT_PORT)]
        port: u16,
    },
    /// Check server status
    Status {
        /// Port to check
        #[arg(short, long, default_value_t = DEFAULT_PORT)]
        port: u16,
    },
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let cli = Cli::parse();
    
    // MCPクライアントモードではログを初期化しない
    if !matches!(cli.command, Commands::McpClient { .. }) {
        // ログの初期化
        fmt()
            .with_env_filter(EnvFilter::new("code_intel=debug,info"))
            .with_writer(std::io::stderr) // stderrにログを出力
            .init();
    }

    

    match cli.command {
        Commands::Serve { project_path, port, web_ui, web_port, open } => {
            info!("Starting code_intel server for project: {}", project_path.display());
            
            if web_ui {
                // Web UIを有効にして起動
                let (web_server, log_sender) = WebUIServer::new(port);
                let server = CodeIntelServer::new(project_path.clone()).with_web_ui(log_sender);
                
                // Web UIサーバーを別タスクで起動
                let web_port_clone = web_port;
                let web_task = tokio::spawn(async move {
                    if let Err(e) = web_server.start(web_port_clone).await {
                        error!("Web UI server error: {}", e);
                    }
                });
                
                // メインサーバーを起動
                let port_clone = port;
                let server_task = tokio::spawn(async move {
                    server.start(port_clone).await
                });
                
                // サービスが起動するまで少し待機
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                
                // ヘルスチェック表示
                println!("\n🚀 Code Intel Service Started\n");
                println!("  ✅ TCP Server:    http://localhost:{port}");
                println!("  ✅ Web UI:        http://localhost:{web_port}");
                println!("  ✅ Project Path:  {}", project_path.display());
                println!("  ✅ MCP Ready:     Yes\n");
                
                // Web UIのURLを構築
                let web_url = format!("http://localhost:{web_port}");
                
                // ブラウザを開く（--openオプションが指定された場合）
                if open {
                    println!("Opening browser...");
                    if let Err(e) = open::that(&web_url) {
                        error!("Failed to open browser: {}", e);
                        eprintln!("Please open {web_url} manually");
                    }
                }
                
                println!("Press Ctrl+C to stop the server\n");
                
                // どちらかが終了するまで待機
                tokio::select! {
                    result = server_task => {
                        match result {
                            Ok(r) => r,
                            Err(e) => return Err(anyhow::anyhow!("Server task error: {}", e))
                        }
                    }
                    result = web_task => {
                        match result {
                            Ok(_) => Ok(()),
                            Err(e) => Err(anyhow::anyhow!("Web UI task error: {}", e))
                        }
                    }
                }
            } else {
                // 通常モード
                let server = CodeIntelServer::new(project_path.clone());
                
                // サーバーを別タスクで起動
                let port_clone = port;
                let server_task = tokio::spawn(async move {
                    server.start(port_clone).await
                });
                
                // サービスが起動するまで少し待機
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                
                // ヘルスチェック表示（通常モード）
                println!("\n🚀 Code Intel Service Started (CLI Mode)\n");
                println!("  ✅ TCP Server:    http://localhost:{port}");
                println!("  ✅ Project Path:  {}", project_path.display());
                println!("  ✅ MCP Ready:     Yes");
                println!("  ℹ️  Web UI:        Disabled (use --web-ui to enable)\n");
                println!("Press Ctrl+C to stop the server\n");
                
                // サーバータスクの終了を待機
                match server_task.await {
                    Ok(r) => r,
                    Err(e) => Err(anyhow::anyhow!("Server task error: {}", e))
                }
            }
        }
        Commands::McpClient { port } => {
            let mcp_client = McpClient::new(port);
            mcp_client.run_stdio().await
        }
        Commands::Graph { project_path, function, format, depth, callers } => {
            generate_call_graph(project_path, function, format, depth, callers).await
        }
        Commands::Status { port } => {
            check_server_status(port).await
        }
    }
}

async fn check_server_status(port: u16) -> Result<(), anyhow::Error> {
    use client::CodeIntelClient;
    
    let client = CodeIntelClient::new(port);
    
    if client.is_server_running().await {
        let stats = client.get_stats().await?;
        println!("✅ Server is running on port {port}");
        println!("📊 Stats: {}", serde_json::to_string_pretty(&stats)?);
    } else {
        println!("❌ Server is not running on port {port}");
        std::process::exit(1);
    }
    
    Ok(())
}

async fn generate_call_graph(
    project_path: PathBuf, 
    function: Option<String>, 
    format: String, 
    depth: usize, 
    callers: bool
) -> Result<(), anyhow::Error> {
    // ログは初期化しない（CLIツールとして使用）
    
    let mut generator = CallGraphGenerator::new();
    generator.analyze_project(&project_path)?;
    
    match format.as_str() {
        "tree" => {
            let result = generator.generate_tree_format(function.as_deref(), depth, callers);
            println!("{}", result);
        }
        "mermaid" => {
            let result = generator.generate_mermaid_format(function.as_deref());
            println!("{}", result);
        }
        "stats" => {
            let result = generator.get_stats();
            println!("{}", result);
        }
        _ => {
            eprintln!("Unknown format: {}. Available formats: tree, mermaid, stats", format);
            std::process::exit(1);
        }
    }
    
    Ok(())
}
