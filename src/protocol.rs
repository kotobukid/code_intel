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
    pub const CHANGE_PROJECT: &str = "change_project";
}

/// シンボルの種類
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SymbolType {
    Function,
    Struct,
    Enum,
    Trait,
}

/// find_definition のパラメータ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindDefinitionParams {
    pub symbol_name: String,
    pub symbol_type: Option<SymbolType>,  // None の場合は全種類を検索
}

/// find_definition のレスポンス
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindDefinitionResponse {
    pub definitions: Vec<SymbolDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolDefinition {
    pub name: String,
    pub symbol_type: SymbolType,
    pub file_path: String,
    pub line: usize,
    pub column: usize,
    pub signature: String,
    pub visibility: String,
    pub generics: Option<String>,  // ジェネリクスパラメータ
}

/// get_stats のレスポンス
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsResponse {
    pub total_symbols: usize,
    pub total_functions: usize,
    pub total_structs: usize,
    pub total_enums: usize,
    pub total_traits: usize,
    pub unique_symbol_names: usize,
    pub indexed_files_count: usize,
}

/// change_project のパラメータ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeProjectParams {
    pub project_path: String,
}

/// change_project のレスポンス
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeProjectResponse {
    pub success: bool,
    pub message: String,
    pub stats: Option<StatsResponse>,
}

/// find_usages のパラメータ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindUsagesParams {
    pub symbol_name: String,
    pub symbol_type: Option<SymbolType>,  // None の場合は全種類を検索
}

/// find_usages のレスポンス
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindUsagesResponse {
    pub usages: Vec<SymbolUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolUsage {
    pub symbol_name: String,
    pub file_path: String,
    pub line: usize,
    pub column: usize,
    pub usage_type: UsageType,
    pub context: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum UsageType {
    FunctionCall,
    TypeUsage,
    TraitUsage,
    Import,
    Reference,
}

impl From<crate::parser::SymbolInfo> for SymbolDefinition {
    fn from(symbol_info: crate::parser::SymbolInfo) -> Self {
        Self {
            name: symbol_info.name,
            symbol_type: symbol_info.symbol_type,
            file_path: symbol_info.file_path,
            line: symbol_info.line,
            column: symbol_info.column,
            signature: symbol_info.signature,
            visibility: symbol_info.visibility,
            generics: symbol_info.generics,
        }
    }
}

impl From<crate::parser::UsageInfo> for SymbolUsage {
    fn from(usage_info: crate::parser::UsageInfo) -> Self {
        Self {
            symbol_name: usage_info.symbol_name,
            file_path: usage_info.file_path,
            line: usage_info.line,
            column: usage_info.column,
            usage_type: match usage_info.usage_type {
                crate::parser::UsageType::FunctionCall => UsageType::FunctionCall,
                crate::parser::UsageType::TypeUsage => UsageType::TypeUsage,
                crate::parser::UsageType::TraitUsage => UsageType::TraitUsage,
                crate::parser::UsageType::Import => UsageType::Import,
                crate::parser::UsageType::Reference => UsageType::Reference,
            },
            context: usage_info.context,
        }
    }
}

impl From<crate::indexer::IndexStats> for StatsResponse {
    fn from(stats: crate::indexer::IndexStats) -> Self {
        Self {
            total_symbols: stats.total_symbols,
            total_functions: stats.total_functions,
            total_structs: stats.total_structs,
            total_enums: stats.total_enums,
            total_traits: stats.total_traits,
            unique_symbol_names: stats.unique_symbol_names,
            indexed_files_count: stats.indexed_files_count,
        }
    }
}