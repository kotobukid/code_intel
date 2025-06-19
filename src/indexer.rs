use crate::parser::{RustParser, FunctionInfo};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use tracing::{info, warn, debug, error};
use notify::{RecommendedWatcher, Watcher, RecursiveMode, Event, EventKind};
use tokio::sync::mpsc;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct CodeIndexer {
    parser: RustParser,
    indexed_files: HashMap<PathBuf, u64>, // ファイルパス -> 最終更新時刻のハッシュ
    watcher: Option<RecommendedWatcher>,
    watch_tx: Option<mpsc::UnboundedSender<notify::Result<Event>>>,
}

pub type FileWatchReceiver = mpsc::UnboundedReceiver<notify::Result<Event>>;

impl CodeIndexer {
    pub fn new() -> Self {
        Self {
            parser: RustParser::new(),
            indexed_files: HashMap::new(),
            watcher: None,
            watch_tx: None,
        }
    }

    /// ディレクトリを再帰的にインデックス
    pub fn index_directory<P: AsRef<Path>>(&mut self, dir_path: P) -> Result<()> {
        let dir_path = dir_path.as_ref();
        info!("Indexing directory: {}", dir_path.display());

        self.walk_directory(dir_path)?;
        
        let total_functions: usize = self.parser.get_all_functions()
            .values()
            .map(|funcs| funcs.len())
            .sum();
        
        info!("Indexing completed. Found {} functions in {} files", 
              total_functions, self.indexed_files.len());
        
        Ok(())
    }

    /// 単一ファイルをインデックス
    pub fn index_file<P: AsRef<Path>>(&mut self, file_path: P) -> Result<()> {
        let file_path = file_path.as_ref();
        
        if !self.is_rust_file(file_path) {
            return Ok(());
        }

        debug!("Indexing file: {}", file_path.display());
        
        match self.parser.parse_file(file_path) {
            Ok(()) => {
                // ファイルのメタデータを記録
                if let Ok(metadata) = std::fs::metadata(file_path) {
                    if let Ok(modified) = metadata.modified() {
                        let hash = self.compute_time_hash(modified);
                        self.indexed_files.insert(file_path.to_path_buf(), hash);
                    }
                }
                debug!("Successfully indexed: {}", file_path.display());
            }
            Err(e) => {
                warn!("Failed to parse file {}: {}", file_path.display(), e);
                // パースエラーがあっても続行
            }
        }
        
        Ok(())
    }

    /// 関数定義を検索
    pub fn find_definition(&self, function_name: &str) -> Option<&Vec<FunctionInfo>> {
        self.parser.find_function(function_name)
    }

    /// すべての関数情報を取得
    pub fn get_all_functions(&self) -> &HashMap<String, Vec<FunctionInfo>> {
        self.parser.get_all_functions()
    }

    /// インデックス統計を取得
    pub fn get_stats(&self) -> IndexStats {
        let total_functions: usize = self.parser.get_all_functions()
            .values()
            .map(|funcs| funcs.len())
            .sum();
        
        let unique_function_names = self.parser.get_all_functions().len();
        let indexed_files_count = self.indexed_files.len();

        IndexStats {
            total_functions,
            unique_function_names,
            indexed_files_count,
            is_watching: self.watcher.is_some(),
        }
    }

    /// ファイル監視を開始
    pub fn start_watching<P: AsRef<Path>>(&mut self, watch_path: P) -> Result<FileWatchReceiver> {
        let (tx, rx) = mpsc::unbounded_channel();
        let watch_tx = tx.clone();

        let mut watcher = RecommendedWatcher::new(
            move |res: notify::Result<Event>| {
                if let Err(_) = tx.send(res) {
                    error!("Failed to send file watch event");
                }
            },
            notify::Config::default(),
        )?;

        watcher.watch(watch_path.as_ref(), RecursiveMode::Recursive)?;
        
        info!("Started watching directory: {}", watch_path.as_ref().display());
        
        self.watcher = Some(watcher);
        self.watch_tx = Some(watch_tx);
        
        Ok(rx)
    }

    /// ファイル監視を停止
    pub fn stop_watching(&mut self) {
        if let Some(mut watcher) = self.watcher.take() {
            info!("Stopping file watcher");
            // Watcherがdropされると自動的に監視停止
        }
        self.watch_tx = None;
    }

    /// 監視イベントを処理して差分更新
    pub fn handle_watch_event(&mut self, event: Event) -> Result<Vec<String>> {
        let mut updated_functions = Vec::new();
        
        debug!("Processing watch event: {:?}", event);

        match event.kind {
            EventKind::Create(_) | EventKind::Modify(_) => {
                for path in event.paths {
                    if self.is_rust_file(&path) {
                        info!("File changed, re-indexing: {}", path.display());
                        
                        // 変更前の関数を記録
                        let old_functions: Vec<String> = self.parser.get_all_functions()
                            .iter()
                            .filter(|(_, funcs)| {
                                funcs.iter().any(|f| f.file_path == path.to_string_lossy())
                            })
                            .map(|(name, _)| name.clone())
                            .collect();

                        // ファイルを再インデックス
                        self.reindex_file(&path)?;
                        
                        // 変更後の関数を記録
                        let new_functions: Vec<String> = self.parser.get_all_functions()
                            .iter()
                            .filter(|(_, funcs)| {
                                funcs.iter().any(|f| f.file_path == path.to_string_lossy())
                            })
                            .map(|(name, _)| name.clone())
                            .collect();

                        // 変更された関数名を記録
                        for func_name in old_functions.iter().chain(new_functions.iter()) {
                            if !updated_functions.contains(func_name) {
                                updated_functions.push(func_name.clone());
                            }
                        }
                    }
                }
            }
            EventKind::Remove(_) => {
                for path in event.paths {
                    if self.is_rust_file(&path) {
                        info!("File removed, cleaning index: {}", path.display());
                        
                        // 削除されたファイルの関数を記録
                        let removed_functions: Vec<String> = self.parser.get_all_functions()
                            .iter()
                            .filter(|(_, funcs)| {
                                funcs.iter().any(|f| f.file_path == path.to_string_lossy())
                            })
                            .map(|(name, _)| name.clone())
                            .collect();

                        // インデックスから削除
                        self.remove_file_from_index(&path);
                        
                        updated_functions.extend(removed_functions);
                    }
                }
            }
            _ => {
                // その他のイベントは無視
            }
        }

        Ok(updated_functions)
    }

    /// 単一ファイルを再インデックス（差分更新用）
    fn reindex_file<P: AsRef<Path>>(&mut self, file_path: P) -> Result<()> {
        let file_path = file_path.as_ref();
        
        // まず古いデータを削除
        self.remove_file_from_index(file_path);
        
        // 新しくインデックス
        self.index_file(file_path)?;
        
        Ok(())
    }

    /// ファイルをインデックスから削除
    fn remove_file_from_index<P: AsRef<Path>>(&mut self, file_path: P) {
        let file_path = file_path.as_ref();
        let file_path_str = file_path.to_string_lossy();
        
        // パーサーから該当ファイルの関数を削除
        self.parser.remove_file_functions(&file_path_str);
        
        // インデックスファイル記録からも削除
        self.indexed_files.remove(file_path);
        
        debug!("Removed file from index: {}", file_path.display());
    }

    fn walk_directory(&mut self, dir_path: &Path) -> Result<()> {
        let entries = std::fs::read_dir(dir_path)
            .with_context(|| format!("Failed to read directory: {}", dir_path.display()))?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                // サブディレクトリを再帰的に処理（.git などは除外）
                if let Some(dir_name) = path.file_name() {
                    if !self.should_skip_directory(dir_name.to_string_lossy().as_ref()) {
                        self.walk_directory(&path)?;
                    }
                }
            } else if self.is_rust_file(&path) {
                self.index_file(&path)?;
            }
        }

        Ok(())
    }

    fn is_rust_file(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext == "rs")
            .unwrap_or(false)
    }

    fn should_skip_directory(&self, dir_name: &str) -> bool {
        matches!(dir_name, ".git" | "target" | "node_modules" | ".idea" | ".vscode")
    }

    fn compute_time_hash(&self, time: std::time::SystemTime) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        if let Ok(duration) = time.duration_since(std::time::UNIX_EPOCH) {
            duration.as_secs().hash(&mut hasher);
        }
        hasher.finish()
    }
}

#[derive(Debug)]
pub struct IndexStats {
    pub total_functions: usize,
    pub unique_function_names: usize,
    pub indexed_files_count: usize,
    pub is_watching: bool,
}

impl std::fmt::Display for IndexStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "IndexStats {{ total_functions: {}, unique_names: {}, files: {}, watching: {} }}", 
               self.total_functions, self.unique_function_names, self.indexed_files_count, self.is_watching)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_index_directory() {
        let dir = tempdir().unwrap();
        
        // テスト用ファイルを作成
        fs::write(dir.path().join("main.rs"), r#"
fn main() {
    println!("Hello, world!");
}

pub fn helper() -> i32 {
    42
}
"#).unwrap();

        fs::write(dir.path().join("lib.rs"), r#"
pub fn library_function(x: i32) -> i32 {
    x * 2
}
"#).unwrap();

        let mut indexer = CodeIndexer::new();
        indexer.index_directory(dir.path()).unwrap();

        let stats = indexer.get_stats();
        assert_eq!(stats.total_functions, 3);
        assert_eq!(stats.indexed_files_count, 2);
        assert_eq!(stats.is_watching, false);

        // main関数を検索
        let main_funcs = indexer.find_definition("main").unwrap();
        assert_eq!(main_funcs.len(), 1);
        assert_eq!(main_funcs[0].name, "main");
    }
}