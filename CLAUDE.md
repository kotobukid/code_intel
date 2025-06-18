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
- **find_definition ツール**: 関数定義検索
- **JSON-RPC 2.0**: 完全対応

### Web UIダッシュボード
- **リアルタイムログ**: WebSocketでログ配信
- **統計情報**: インデックス状況の可視化
- **VS Code風UI**: ダークテーマ
- **自動再接続**: WebSocket切断時の復旧

### コア機能
- **高速パース**: syn使用、ミリ秒単位レスポンス
- **関数検索**: 名前・シグネチャ・可視性・位置情報
- **並行処理**: tokioによる非同期処理
- **エラー処理**: パース失敗時の継続処理

## 使用方法

### 開発時
```bash
# サーバー起動（Web UI付き）
cargo run -- serve ./test_project --web-ui

# 別ターミナルでテスト
echo '{"jsonrpc":"2.0","method":"tools/call","params":{"name":"find_definition","arguments":{"function_name":"main"}},"id":1}' | cargo run -- mcp-client

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
```

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

### Phase 2 (ファイル監視)
- notifyによる変更検知
- 差分更新
- ホットリロード

### Phase 3 (機能拡張)
- find_usages実装
- コールグラフ生成
- struct/enum/trait解析
- 複数言語対応

### Phase 4 (企業級)
- 認証・認可
- メトリクス・分析
- クラスター対応
- API制限

## トラブルシューティング

### よくある問題
1. **ポート競合**: 7777/8080が使用中の場合は`--port`オプション使用
2. **権限エラー**: プロジェクトディレクトリの読み取り権限確認
3. **ビルドエラー**: Rust 2024 edition対応のコンパイラ使用

### デバッグ方法
1. Web UI (http://localhost:8080) でリアルタイムログ確認
2. `cargo run -- status` でサーバー状態確認
3. `RUST_LOG=debug` で詳細ログ出力

## 性能実績
- **テストケース**: 15関数、4ファイル
- **応答時間**: 数ミリ秒
- **メモリ使用量**: 軽量（詳細測定予定）
- **安定性**: 長時間稼働確認済み

## 貢献・拡張時の指針
- rust-analyzerより軽量を維持
- synより実用的な機能追加
- AIツールとの統合を最優先
- 開発者体験の向上を重視