mod parser;
mod indexer;
mod protocol;
mod server;
mod client;
mod mcp_client;
mod web_ui;

use clap::{Parser, Subcommand};
use server::{CodeIntelServer, DEFAULT_PORT};
use mcp_client::McpClient;
use web_ui::WebUIServer;
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
    // ログの初期化
    fmt()
        .with_env_filter(EnvFilter::new("code_intel=debug,info"))
        .with_writer(std::io::stderr) // stderrにログを出力
        .init();

    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Serve { project_path, port, web_ui, web_port } => {
            info!("Starting code_intel server for project: {}", project_path.display());
            
            if web_ui {
                // Web UIを有効にして起動
                let (web_server, log_sender) = WebUIServer::new();
                let server = CodeIntelServer::new(project_path).with_web_ui(log_sender);
                
                // Web UIサーバーを別タスクで起動
                let web_task = tokio::spawn(async move {
                    if let Err(e) = web_server.start(web_port).await {
                        error!("Web UI server error: {}", e);
                    }
                });
                
                // メインサーバーを起動
                let server_task = tokio::spawn(async move {
                    server.start(port).await
                });
                
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
                let server = CodeIntelServer::new(project_path);
                server.start(port).await
            }
        }
        Commands::McpClient { port } => {
            let mcp_client = McpClient::new(port);
            mcp_client.run_stdio().await
        }
        Commands::Status { port } => {
            check_server_status(port).await
        }
    };

    result
}

async fn check_server_status(port: u16) -> Result<(), anyhow::Error> {
    use client::CodeIntelClient;
    
    let client = CodeIntelClient::new(port);
    
    if client.is_server_running().await {
        let stats = client.get_stats().await?;
        println!("✅ Server is running on port {}", port);
        println!("📊 Stats: {}", serde_json::to_string_pretty(&stats)?);
    } else {
        println!("❌ Server is not running on port {}", port);
        std::process::exit(1);
    }
    
    Ok(())
}
