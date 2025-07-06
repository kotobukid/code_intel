# Code Intelligence Service - Claude AI 向け開発情報

## プロジェクト概要
AIアシスタント向けの軽量コード解析サービス。rust-analyzerより軽く、synより実用的な「ちょうどいい」解析ツール。

## 技術スタック
- **言語**: Rust (Edition 2024)
- **非同期**: tokio
- **パーサー**: syn v2.0
- **Web**: axum v0.7 + WebSocket
- **プロトコル**: MCP (Model Context Protocol)
- **CLI**: clap v4.0

## アーキテクチャ

### デーモン+クライアント型
```
code_intel serve (デーモン)      code_intel mcp-client (瞬時起動)
├── TCP:7777 (MCP通信)    ←─────── MCPクライアント
├── HTTP:8080 (Web UI)           └── Claude Code統合用
├── インメモリキャッシュ
└── ファイル監視（将来）
```

## モジュール構成
- `main.rs` - CLI エントリーポイント、モード分岐
- `parser.rs` - syn使用の高速パーサー
- `indexer.rs` - ファイル監視・インデックス管理
- `protocol.rs` - サーバー・クライアント間通信プロトコル
- `server.rs` - TCPサーバー（デーモン側）
- `client.rs` - TCPクライアント
- `mcp_client.rs` - MCPプロトコル実装（Claude Code統合）
- `web_ui.rs` - WebUIダッシュボード・WebSocket

## 実装済み機能

### MCP対応
- **stdio transport**: Claude Code/Claude Desktop対応
- **find_definition ツール**: 関数・型定義検索
- **find_usages ツール**: シンボル使用箇所検索（NEW!）
- **JSON-RPC 2.0**: 完全対応

### Web UIダッシュボード
- **リアルタイムログ**: WebSocketでログ配信
- **統計情報**: インデックス状況の可視化
- **VS Code風UI**: ダークテーマ
- **自動再接続**: WebSocket切断時の復旧
- **プロジェクト変更**: WebUIから対象ディレクトリを動的変更
- **ディレクトリ選択**: File System Access API対応のGUI選択
- **--openオプション**: ブラウザ自動起動

### コア機能
- **高速パース**: syn使用、ミリ秒単位レスポンス
- **シンボル定義検索**: 関数・struct・enum・traitの定義場所を即座に検索
- **使用箇所検索**: シンボルの使用箇所を種類別（関数呼び出し、型使用、インポート等）で検索
- **コールグラフ生成**: 関数呼び出し関係の可視化（人間用CLIツール）（NEW!）
- **並行処理**: tokioによる非同期処理
- **エラー処理**: パース失敗時の継続処理

### ファイル監視機能
- **Rustファイル専用監視**: `*.rs`ファイルのみを対象に限定
- **スロットル機能**: 2秒間隔のバッチ処理で連続変更を効率化
- **差分更新**: 変更ファイルのみ再インデックス
- **ログノイズ削減**: 関連ファイルのみログ出力

## 使用方法

### 開発時
```bash
# サーバー起動（Web UI付き）
cargo run -- serve ./test_project --web-ui

# サーバー起動（Web UI + ブラウザ自動起動）
cargo run -- serve ./test_project --web-ui --open

# 別ターミナルでテスト
echo '{"jsonrpc":"2.0","method":"tools/call","params":{"name":"find_definition","arguments":{"symbol_name":"main"}},"id":1}' | cargo run -- mcp-client

# 使用箇所検索のテスト
echo '{"jsonrpc":"2.0","method":"tools/call","params":{"name":"find_usages","arguments":{"symbol_name":"add"}},"id":2}' | cargo run -- mcp-client

# コールグラフ生成（人間用）
cargo run -- graph ./test_project
cargo run -- graph --function main ./test_project
cargo run -- graph --format mermaid ./test_project

# 状態確認
cargo run -- status

# Web UI: http://localhost:8080
```

### Claude Code統合
```bash
# ビルド
cargo build --release

# MCP登録
claude mcp add code-intel -- /path/to/target/release/code_intel mcp-client

# 例：プロジェクトディレクトリが /home/user/code_intel の場合
claude mcp add code-intel -- /home/user/code_intel/target/release/code_intel mcp-client

# または、プロジェクトディレクトリから実行する場合
claude mcp add code-intel -- $(pwd)/target/release/code_intel mcp-client

# 注意事項:
# - 必ず `cargo build --release` でリリースビルドを作成してから登録
# - 絶対パスを使用すること（相対パスは動作しない場合がある）
# - バイナリ名は code_intel（アンダースコア付き）
```

## ユーザビリティ向上機能

### サーバー起動時の改善
- **--openオプション**: Web UI起動時にブラウザを自動で開く
- **起動時ヘルスチェック**: サービス状態を視覚的に表示
- **URL自動表示**: Web UIのURLを標準出力に表示

### WebUI機能拡張
- **プロジェクト変更**: サーバーを再起動せずにディレクトリを変更
- **ディレクトリ選択**: File System Access APIによるGUI選択
- **フォールバック**: 古いブラウザでは従来の入力方式

### ファイル監視の最適化
- **フィルタリング**: `*.rs`ファイルのみ監視でノイズ削減
- **スロットル**: 2秒間隔のバッチ処理でパフォーマンス向上
- **git pull対応**: 大量ファイル変更時の効率的処理

## 開発上の注意点

### ファイル構造
- `test_project/` - テスト用サンプルプロジェクト
- `src/` - メインソースコード
- `target/` - ビルド成果物

### ポート使用
- **TCP:7777** - MCP通信（デーモン）
- **HTTP:8080** - Web UI

### ログ出力
- **stderr** - 開発者向けログ（tracing）
- **stdout** - MCP通信専用（JSON-RPC）
- **WebSocket** - Web UI向けログ配信

## 設計思想

### パフォーマンス重視
- synによる高速パース
- インメモリキャッシュ
- 非同期処理（tokio）
- 軽量プロトコル（JSON-RPC over TCP）

### 開発者体験
- リアルタイム監視（Web UI）
- 詳細なログ出力
- 簡単なCLI操作
- ホットリロード対応（将来）

### AI統合特化
- MCP完全対応
- 即座にレスポンス
- 構造化データ出力
- 拡張性のあるプロトコル

## 将来計画

### Phase 2 (ファイル監視) ✅ 完了
- notifyによる変更検知（Rustファイル専用）
- 差分更新
- ホットリロード
- スロットル機能による効率化

### Phase 3 (機能拡張) 
- find_usages実装 ✅ 完了
- コールグラフ生成 ✅ 完了（人間用CLIツール）
- struct/enum/trait解析
- 複数言語対応

### Phase 4 (企業級)
- 認証・認可
- メトリクス・分析
- クラスター対応
- API制限

## MCP実装の重要な注意点

### 「数秒でおかしくなる」問題の原因と解決

#### 問題の症状
- Claude CodeでMCPサーバーが「接続中...」から「失敗」になる
- 30秒後にタイムアウトエラーが発生
- MCPプロセスは起動しているが通信できない

#### 根本原因
1. **JSON-RPCレスポンスの形式問題**
   - `error: null`を含むとClaude CLIが正しく解析できない
   - 解決: `#[serde(skip_serializing_if = "Option::is_none")]`を使用

2. **ログ出力の干渉**
   - stdoutへのログ出力がJSON-RPC通信を妨害
   - 解決: MCPクライアントモードでは完全にログを無効化

#### 実装時の必須事項
```rust
// 1. main.rsでMCPモード時のログ無効化
if !matches!(cli.command, Commands::McpClient { .. }) {
    // 通常モードのみログ初期化
}

// 2. JSON-RPCレスポンスでnullフィールドを除外
#[derive(Serialize)]
struct JsonRpcResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
    // ...
}

// 3. MCPモードではstderrへの出力も禁止
// eprintln!() は使用しない
```

## トラブルシューティング

### よくある問題
1. **ポート競合**: 7777/8080が使用中の場合は`--port`オプション使用
2. **権限エラー**: プロジェクトディレクトリの読み取り権限確認
3. **ビルドエラー**: Rust 2024 edition対応のコンパイラ使用
4. **MCP接続エラー**: 上記「MCP実装の重要な注意点」参照

### デバッグ方法
1. Web UI (http://localhost:8080) でリアルタイムログ確認
2. `cargo run -- status` でサーバー状態確認
3. `RUST_LOG=debug` で詳細ログ出力（MCPモード以外）
4. MCPログ確認: `~/.cache/claude-cli-nodejs/*/mcp-logs-code-intel/`

## 性能実績
- **テストケース**: 30シンボル（17関数、5構造体、3列挙型、5トレイト）、6ファイル
- **応答時間**: 数ミリ秒
- **find_usages実績**: IconRed関数で5箇所の使用箇所を瞬時に検出
- **コールグラフ実績**: 21関数、8呼び出し関係を階層表示・Mermaid記法で可視化
- **メモリ使用量**: 軽量（詳細測定予定）
- **安定性**: 長時間稼働確認済み

## 貢献・拡張時の指針
- rust-analyzerより軽量を維持
- synより実用的な機能追加
- AIツールとの統合を最優先
- 開発者体験の向上を重視