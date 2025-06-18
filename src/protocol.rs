use serde::{Deserialize, Serialize};

/// サーバー・クライアント間の通信プロトコル
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerRequest {
    pub id: u64,
    pub method: String,
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerResponse {
    pub id: u64,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

/// サーバーへのリクエストメソッド
pub mod methods {
    pub const FIND_DEFINITION: &str = "find_definition";
    pub const FIND_USAGES: &str = "find_usages";
    pub const LIST_SYMBOLS: &str = "list_symbols";
    pub const GET_STATS: &str = "get_stats";
    pub const HEALTH_CHECK: &str = "health_check";
}

/// find_definition のパラメータ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindDefinitionParams {
    pub function_name: String,
}

/// find_definition のレスポンス
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindDefinitionResponse {
    pub definitions: Vec<FunctionDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    pub name: String,
    pub file_path: String,
    pub line: usize,
    pub column: usize,
    pub signature: String,
    pub visibility: String,
}

/// get_stats のレスポンス
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsResponse {
    pub total_functions: usize,
    pub unique_function_names: usize,
    pub indexed_files_count: usize,
}

impl From<crate::parser::FunctionInfo> for FunctionDefinition {
    fn from(func_info: crate::parser::FunctionInfo) -> Self {
        Self {
            name: func_info.name,
            file_path: func_info.file_path,
            line: func_info.line,
            column: func_info.column,
            signature: func_info.signature,
            visibility: func_info.visibility,
        }
    }
}

impl From<crate::indexer::IndexStats> for StatsResponse {
    fn from(stats: crate::indexer::IndexStats) -> Self {
        Self {
            total_functions: stats.total_functions,
            unique_function_names: stats.unique_function_names,
            indexed_files_count: stats.indexed_files_count,
        }
    }
}