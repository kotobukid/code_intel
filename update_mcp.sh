#!/bin/bash

# MCPツール再登録スクリプト
# Usage: ./update_mcp.sh

set -e

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BINARY_PATH="$PROJECT_DIR/target/release/code_intel"

echo "🔄 Code Intelligence MCP Tool Update Script"
echo "📁 Project directory: $PROJECT_DIR"

# 既存のMCPツールを削除
echo "🗑️  Removing existing MCP tool..."
if claude mcp remove code-intel 2>/dev/null; then
    echo "✅ Removed existing code-intel tool"
else
    echo "ℹ️  No existing code-intel tool found (this is OK)"
fi

# リリースビルドを作成
echo "🔨 Building release binary..."
cd "$PROJECT_DIR"
cargo build --release

# バイナリの存在確認
if [ ! -f "$BINARY_PATH" ]; then
    echo "❌ Error: Binary not found at $BINARY_PATH"
    echo "   Make sure cargo build --release completed successfully"
    exit 1
fi

echo "✅ Binary found at: $BINARY_PATH"

# MCPツールを登録
echo "📝 Registering MCP tool..."
claude mcp add code-intel -- "$BINARY_PATH" mcp-client

echo "✅ MCP tool registered successfully!"
echo ""
echo "🧪 Testing tool registration..."
if claude mcp list | grep -q "code-intel"; then
    echo "✅ Tool is properly registered"
else
    echo "❌ Tool registration failed"
    exit 1
fi

echo ""
echo "🎉 Update completed successfully!"
echo ""
echo "📋 Usage:"
echo "   1. Make sure the server is running: cargo run -- serve ./test_project --web-ui"
echo "   2. The MCP tool is now available in Claude Code"
echo "   3. Use find_definition to search for functions"
echo ""
echo "🌐 Web UI: http://localhost:8080"
echo "🔧 Server port: 7777"