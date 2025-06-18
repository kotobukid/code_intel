use crate::parser::{RustParser, FunctionInfo};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use tracing::{info, warn, debug};

pub struct CodeIndexer {
    parser: RustParser,
    indexed_files: HashMap<PathBuf, u64>, // ファイルパス -> 最終更新時刻のハッシュ
}

impl CodeIndexer {
    pub fn new() -> Self {
        Self {
            parser: RustParser::new(),
            indexed_files: HashMap::new(),
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
        }
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
}

impl std::fmt::Display for IndexStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "IndexStats {{ total_functions: {}, unique_names: {}, files: {} }}", 
               self.total_functions, self.unique_function_names, self.indexed_files_count)
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

        // main関数を検索
        let main_funcs = indexer.find_definition("main").unwrap();
        assert_eq!(main_funcs.len(), 1);
        assert_eq!(main_funcs[0].name, "main");
    }
}