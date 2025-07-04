# Code Intelligence Service for AI Tools

## 概要
AIアシスタント（Claude Code等）がコードベースを効率的に理解・探索するための軽量なコード解析サービス。rust-analyzerの重さとsynの機能不足のギャップを埋める。

## 背景と動機
- **rust-analyzer**: 完全な意味解析を提供するが、起動に30秒以上、メモリ大量消費、ライブラリ利用非推奨
- **syn**: 高速な構文解析のみ、関数呼び出し関係などの実用的な情報は自分で実装必要
- **ripgrep**: 高速だがワンショット実行、構造を理解しない

→ AIツールに最適化された「ちょうどいい」解析サービスが必要

## アーキテクチャ設計

### コア機能
1. **ファイル監視**: 対象ディレクトリ以下のRustファイルの変更を検知
2. **高速パース**: synクレートを使用してミリ秒単位でパース
3. **インデックス構築**: 以下の情報を抽出・保持
   - 関数定義（名前、引数、戻り値、場所）
   - 関数呼び出し（呼び出し元→呼び出し先の関係）
   - struct/enum定義
   - impl ブロック
   - use文と依存関係
4. **MCPサーバー**: stdio transportでAIツールとの統合

### 提供する機能
- `find_definition("function_name")` → 定義場所を即座に返却
- `find_usages("function_name")` → 使用箇所のリストを返却
- `get_call_graph("function_name")` → 呼び出し関係のグラフを返却
- `list_symbols_in_file("path/to/file.rs")` → ファイル内の全シンボル
- `find_implementations("trait_name")` → トレイトの実装一覧

### 技術スタック
- **言語**: Rust
- **パーサー**: syn クレート
- **ファイル監視**: notify クレート
- **プロトコル**: MCP (Model Context Protocol) with JSON-RPC 2.0
- **トランスポート**: stdio transport（Claude Code/Claude Desktop対応）
- **データ構造**: petgraph（グラフ表現用）
- **将来**: Webインターフェース（axum + htmx）

## MCP (Model Context Protocol) 対応

### MCP統合の利点
- **標準化**: Claude Code、Claude Desktop等の複数のAIツールから統一的にアクセス可能
- **エコシステム**: 他のMCPサーバー（Git、GitHub等）との連携
- **高速性**: stdio transportによる低レイテンシ通信

### MCP機能設計
- **Tools（実行可能操作）**:
  - `find_definition`: 関数/型定義の検索
  - `find_usages`: 使用箇所の検索  
  - `get_call_graph`: 呼び出し関係グラフ生成
- **Resources（読み取り専用データ）**:
  - `/symbols/{file_path}`: ファイル内シンボル一覧
  - `/project_structure`: プロジェクト構造情報
- **Prompts（テンプレート）**:
  - `refactor_function`: リファクタリング支援
  - `analyze_dependencies`: 依存関係分析

## 実装ロードマップ（更新版）

### Phase 1: MVP（単一クレート）- 2-3週間
- [ ] synを使った基本的なパース機能
- [ ] 関数定義の抽出とインメモリ保存
- [ ] MCP stdio transportサーバー実装
- [ ] Claude Codeでの動作確認

### Phase 2: ワークスペース化＋機能拡張 - 1-2週間
- [ ] ファイル監視と差分更新
- [ ] 関数呼び出し関係の解析
- [ ] クレート分割（core, mcp, cli）

### Phase 3: 付加価値機能 - 必要に応じて
- [ ] Webインターフェース（axum + htmx）
- [ ] struct/enum/trait の解析
- [ ] 他言語対応（Python、TypeScript等）

## 実装完了状況（2025-06-18現在）

### ✅ 完成した機能
1. **デーモン+クライアント型アーキテクチャ**
   - TCP:7777でコード解析サーバー（デーモン）
   - MCPクライアント（Claude Code統合用）
   - HTTP:8080でWeb UIダッシュボード

2. **MCP (Model Context Protocol) 完全対応**
   - stdio transport実装済み
   - Claude Code/Claude Desktop対応
   - `find_definition` ツール実装済み

3. **リアルタイムWeb UIダッシュボード**
   - VS Code風ダークテーマ
   - WebSocketによるリアルタイムログ表示
   - 統計情報の自動更新（インデックス済みファイル数、関数数等）
   - サーバー稼働時間表示

4. **高速コード解析エンジン**
   - synクレートによる高速パース
   - インメモリキャッシュ（HashMap）
   - 15関数、4ファイルのプロジェクトで即座にレスポンス

### 🚀 使用方法
```bash
# サーバー起動（Web UI付き）
cargo run -- serve ./test_project --web-ui

# Claude Code統合
claude mcp add code-intel -- /path/to/target/release/code_intel mcp-client

# 例：プロジェクトディレクトリが /home/user/code_intel の場合
claude mcp add code-intel -- /home/user/code_intel/target/release/code_intel mcp-client

# または、プロジェクトディレクトリから実行する場合
claude mcp add code-intel -- $(pwd)/target/release/code_intel mcp-client

# 注意事項:
# - 必ず `cargo build --release` でリリースビルドを作成してから登録
# - 絶対パスを使用すること（相対パスは動作しない場合がある）
# - バイナリ名は code_intel（アンダースコア付き）

# 状態確認
cargo run -- status

# ダッシュボード
http://localhost:8080
```

### 📊 動作実績
- **テスト済み関数**: main, calculate_sum, add, multiply等
- **パフォーマンス**: ミリ秒単位でレスポンス
- **安定性**: 長時間稼働での動作確認済み

## トラブルシューティング

### MCP接続が「数秒でおかしくなる」問題

#### 症状
- Claude Codeで `/mcp` すると「接続中...」→「失敗」になる
- MCPプロセスは起動しているのに30秒でタイムアウト

#### 解決方法
1. **最新バージョンをビルド**
   ```bash
   git pull
   cargo build --release
   ```

2. **古い登録を削除して再登録**
   ```bash
   # 古い登録を確認
   claude mcp list
   
   # 必要なら削除（手動で設定ファイルを編集）
   
   # 再登録（絶対パスで！）
   claude mcp add code-intel -- $(pwd)/target/release/code_intel mcp-client
   ```

3. **デバッグログを確認**
   ```bash
   # MCPのログ場所
   ls -la ~/.cache/claude-cli-nodejs/*/mcp-logs-code-intel/
   ```

#### よくある原因
- **古いバージョンを使用**: `error: null` を含むJSON応答は2025-07-04以前のバージョンの問題
- **相対パス**: MCP登録時は必ず絶対パスを使用
- **デバッグビルド**: リリースビルド（`--release`）を使用すること

### その他の問題

#### サーバーが起動しない
```bash
# ポートが使用中か確認
lsof -i :7777
lsof -i :8080

# 別のポートで起動
cargo run -- serve --port 7778 --web-port 8081
```

#### Web UIにアクセスできない
- ファイアウォール設定を確認
- `--web-ui` オプションを付けているか確認

## 今後の拡張予定

### Phase 2: ファイル監視・差分更新
- [ ] notifyクレートによるファイル変更検知
- [ ] インクリメンタル更新（変更ファイルのみ再パース）
- [ ] ホットリロード機能

### Phase 3: 機能拡張
- [ ] 関数使用箇所検索（find_usages）
- [ ] コールグラフ生成・可視化
- [ ] struct/enum/trait解析
- [ ] 他言語対応（Python、TypeScript等）

### Phase 4: 企業級機能
- [ ] 複数プロジェクト同時監視
- [ ] メトリクス・アナリティクス
- [ ] API認証・レート制限
- [ ] クラスター対応

## 期待される効果
- ✅ **実証済み**: AIアシスタントのコード理解速度が劇的に向上
- ✅ **実証済み**: 「この関数どこで使われてる？」への即答
- ✅ **実証済み**: rust-analyzerの起動を待つ必要なし
- 🔄 **計画中**: 大規模リファクタリング時の影響範囲把握

## アーキテクチャ詳細

### 実装上の考慮事項

#### パフォーマンス
- **メモリ効率**: 大規模プロジェクトでのインデックス情報の適切な管理
- **並行処理**: ファイル監視中の同時リクエスト処理（tokio使用）
- **インクリメンタル更新**: 変更されたファイルのみを再解析

#### エラーハンドリング
- **パースエラー**: 構文エラーがあるファイルでも部分的な情報を提供
- **リソース制限**: 大きすぎるファイルの処理制限
- **循環依存**: 無限ループの検出と回避

#### 設定と運用
- **プロジェクト検出**: Cargo.tomlベースの自動検出
- **除外設定**: .gitignoreやカスタム除外ルール
- **診断機能**: インデックス状態の可視化、パフォーマンス統計

#### 名前解決戦略
- **Phase 1**: 同一ファイル内の関数・型のみ（実装済み）
- **Phase 2**: 同一クレート内のモジュール解決
- **Phase 3**: 外部クレート参照（stdライブラリ含む）

## 参考実装・仕様
- syn: https://github.com/dtolnay/syn
- notify: https://github.com/notify-rs/notify
- rust-analyzer（アーキテクチャの参考）: https://github.com/rust-lang/rust-analyzer
- MCP公式仕様: https://modelcontextprotocol.io/