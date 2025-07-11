use std::collections::HashMap;
use std::path::Path;
use syn::{File, Item, ItemFn, ItemStruct, ItemEnum, ItemTrait, Signature, Visibility};
use anyhow::{Context, Result};
use crate::protocol::SymbolType;

#[derive(Debug, Clone)]
pub struct SymbolInfo {
    pub name: String,
    pub symbol_type: SymbolType,
    pub file_path: String,
    pub line: usize,
    pub column: usize,
    pub signature: String,
    pub visibility: String,
    pub generics: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UsageInfo {
    pub symbol_name: String,
    pub file_path: String,
    pub line: usize,
    pub column: usize,
    pub usage_type: UsageType,
    pub context: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UsageType {
    FunctionCall,
    TypeUsage,
    TraitUsage,
    Import,
    Reference,
}

#[derive(Debug, Clone)]
pub struct CallInfo {
    pub caller: String,
    pub caller_file: String,
    pub caller_line: usize,
    pub callee: String,
    pub call_line: usize,
    pub call_column: usize,
    pub call_context: String,
}

pub struct RustParser {
    symbols: HashMap<String, Vec<SymbolInfo>>,
    call_graph: Vec<CallInfo>,
}

impl RustParser {
    pub fn new() -> Self {
        Self {
            symbols: HashMap::new(),
            call_graph: Vec::new(),
        }
    }

    pub fn parse_file<P: AsRef<Path>>(&mut self, file_path: P) -> Result<()> {
        let file_path = file_path.as_ref();
        let content = std::fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read file: {}", file_path.display()))?;

        let syntax_tree = syn::parse_file(&content)
            .with_context(|| format!("Failed to parse file: {}", file_path.display()))?;

        self.extract_symbols(&syntax_tree, file_path.to_string_lossy().to_string(), &content)?;
        self.extract_function_calls(&syntax_tree, file_path.to_string_lossy().to_string(), &content)?;
        Ok(())
    }

    fn extract_symbols(&mut self, syntax_tree: &File, file_path: String, content: &str) -> Result<()> {
        for item in &syntax_tree.items {
            let symbol_info = match item {
                Item::Fn(item_fn) => Some(self.extract_function_info(item_fn, &file_path, content)?),
                Item::Struct(item_struct) => Some(self.extract_struct_info(item_struct, &file_path, content)?),
                Item::Enum(item_enum) => Some(self.extract_enum_info(item_enum, &file_path, content)?),
                Item::Trait(item_trait) => Some(self.extract_trait_info(item_trait, &file_path, content)?),
                _ => None,
            };
            
            if let Some(info) = symbol_info {
                // シンボル名でグループ化
                self.symbols
                    .entry(info.name.clone())
                    .or_default()
                    .push(info);
            }
        }
        Ok(())
    }

    fn extract_function_info(&self, item_fn: &ItemFn, file_path: &str, content: &str) -> Result<SymbolInfo> {
        let name = item_fn.sig.ident.to_string();
        let signature = self.format_signature(&item_fn.sig);
        let visibility = self.format_visibility(&item_fn.vis);
        let generics = self.format_generics(&item_fn.sig.generics);
        
        // 関数定義の行番号を見つける
        let (line, column) = self.find_symbol_location(&name, content, "fn");

        Ok(SymbolInfo {
            name,
            symbol_type: SymbolType::Function,
            file_path: file_path.to_string(),
            line,
            column,
            signature,
            visibility,
            generics,
        })
    }

    fn extract_struct_info(&self, item_struct: &ItemStruct, file_path: &str, content: &str) -> Result<SymbolInfo> {
        let name = item_struct.ident.to_string();
        let visibility = self.format_visibility(&item_struct.vis);
        let generics = self.format_generics(&item_struct.generics);
        
        // struct定義のシグネチャ
        let signature = format!("struct {}{}", name, generics.as_deref().unwrap_or(""));
        
        let (line, column) = self.find_symbol_location(&name, content, "struct");

        Ok(SymbolInfo {
            name,
            symbol_type: SymbolType::Struct,
            file_path: file_path.to_string(),
            line,
            column,
            signature,
            visibility,
            generics,
        })
    }

    fn extract_enum_info(&self, item_enum: &ItemEnum, file_path: &str, content: &str) -> Result<SymbolInfo> {
        let name = item_enum.ident.to_string();
        let visibility = self.format_visibility(&item_enum.vis);
        let generics = self.format_generics(&item_enum.generics);
        
        // enum定義のシグネチャ
        let signature = format!("enum {}{}", name, generics.as_deref().unwrap_or(""));
        
        let (line, column) = self.find_symbol_location(&name, content, "enum");

        Ok(SymbolInfo {
            name,
            symbol_type: SymbolType::Enum,
            file_path: file_path.to_string(),
            line,
            column,
            signature,
            visibility,
            generics,
        })
    }

    fn extract_trait_info(&self, item_trait: &ItemTrait, file_path: &str, content: &str) -> Result<SymbolInfo> {
        let name = item_trait.ident.to_string();
        let visibility = self.format_visibility(&item_trait.vis);
        let generics = self.format_generics(&item_trait.generics);
        
        // trait定義のシグネチャ
        let signature = format!("trait {}{}", name, generics.as_deref().unwrap_or(""));
        
        let (line, column) = self.find_symbol_location(&name, content, "trait");

        Ok(SymbolInfo {
            name,
            symbol_type: SymbolType::Trait,
            file_path: file_path.to_string(),
            line,
            column,
            signature,
            visibility,
            generics,
        })
    }

    fn format_signature(&self, sig: &Signature) -> String {
        // 簡易的なシグネチャ文字列生成
        let mut result = String::new();
        
        if sig.asyncness.is_some() {
            result.push_str("async ");
        }
        
        result.push_str("fn ");
        result.push_str(&sig.ident.to_string());
        
        // パラメータ
        result.push('(');
        for (i, input) in sig.inputs.iter().enumerate() {
            if i > 0 {
                result.push_str(", ");
            }
            result.push_str(&format!("{}", quote::quote!(#input)));
        }
        result.push(')');
        
        // 戻り値
        if let syn::ReturnType::Type(_, ty) = &sig.output {
            result.push_str(" -> ");
            result.push_str(&format!("{}", quote::quote!(#ty)));
        }
        
        result
    }

    fn format_visibility(&self, vis: &Visibility) -> String {
        match vis {
            Visibility::Public(_) => "pub".to_string(),
            Visibility::Restricted(restricted) if restricted.path.is_ident("crate") => "pub(crate)".to_string(),
            Visibility::Restricted(restricted) => {
                format!("pub({})", quote::quote!(#restricted.path))
            }
            Visibility::Inherited => "private".to_string(),
        }
    }

    pub fn find_symbol(&self, name: &str, symbol_type: Option<SymbolType>) -> Option<Vec<&SymbolInfo>> {
        self.symbols.get(name).map(|symbols| {
            symbols.iter()
                .filter(|s| match symbol_type {
                    None => true,
                    Some(ref t) => s.symbol_type == *t,
                })
                .collect()
        })
    }

    pub fn get_all_symbols(&self) -> &HashMap<String, Vec<SymbolInfo>> {
        &self.symbols
    }

    /// シンボルの位置を見つける
    fn find_symbol_location(&self, symbol_name: &str, content: &str, keyword: &str) -> (usize, usize) {
        let lines: Vec<&str> = content.lines().collect();
        
        // 各行を検索して関数定義を見つける
        for (line_idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            
            // シンボル定義のパターンをチェック
            // キーワードの後にシンボル名が来るパターンを探す
            let patterns = vec![
                format!("{} {}", keyword, symbol_name),
                format!("pub {} {}", keyword, symbol_name),
                format!("pub(crate) {} {}", keyword, symbol_name),
                format!("pub(super) {} {}", keyword, symbol_name),
                format!("async {} {}", keyword, symbol_name), // async fnの場合
                format!("pub async {} {}", keyword, symbol_name), // pub async fnの場合
            ];
            
            for pattern in patterns {
                if trimmed.contains(&pattern) {
                    // 行番号は1ベース、列番号はシンボル名の開始位置
                    let col = line.find(symbol_name).unwrap_or(0);
                    return (line_idx + 1, col);
                }
            }
        }
        
        // 見つからない場合のフォールバック
        (1, 0)
    }

    /// 関数呼び出し関係を抽出
    fn extract_function_calls(&mut self, syntax_tree: &File, file_path: String, content: &str) -> Result<()> {
        for item in &syntax_tree.items {
            if let Item::Fn(item_fn) = item {
                let caller_name = item_fn.sig.ident.to_string();
                let caller_line = self.find_symbol_location(&caller_name, content, "fn").0;
                
                // 関数本体の中の関数呼び出しを解析
                self.extract_calls_from_block(&item_fn.block, &caller_name, &file_path, caller_line, content);
            }
        }
        Ok(())
    }
    
    /// ブロック内の関数呼び出しを抽出
    fn extract_calls_from_block(&mut self, block: &syn::Block, caller: &str, caller_file: &str, caller_line: usize, content: &str) {
        for stmt in &block.stmts {
            self.extract_calls_from_stmt(stmt, caller, caller_file, caller_line, content);
        }
    }
    
    /// ステートメントから関数呼び出しを抽出
    fn extract_calls_from_stmt(&mut self, stmt: &syn::Stmt, caller: &str, caller_file: &str, caller_line: usize, content: &str) {
        match stmt {
            syn::Stmt::Local(local) => {
                if let Some(init) = &local.init {
                    self.extract_calls_from_expr(&init.expr, caller, caller_file, caller_line, content);
                }
            }
            syn::Stmt::Item(_) => {
                // アイテム内の処理は既に extract_function_calls で処理済み
            }
            syn::Stmt::Expr(expr, _) => {
                self.extract_calls_from_expr(expr, caller, caller_file, caller_line, content);
            }
            syn::Stmt::Macro(_) => {
                // マクロ呼び出しは現在スキップ
            }
        }
    }
    
    /// 式から関数呼び出しを抽出
    fn extract_calls_from_expr(&mut self, expr: &syn::Expr, caller: &str, caller_file: &str, caller_line: usize, content: &str) {
        match expr {
            syn::Expr::Call(call_expr) => {
                // 関数呼び出しを発見
                if let syn::Expr::Path(path_expr) = &*call_expr.func {
                    if let Some(ident) = path_expr.path.get_ident() {
                        let callee = ident.to_string();
                        
                        // 関数呼び出しの位置を特定
                        let (call_line, call_column) = self.find_call_location(&callee, content, caller_line);
                        let call_context = self.get_line_context(content, call_line);
                        
                        self.call_graph.push(CallInfo {
                            caller: caller.to_string(),
                            caller_file: caller_file.to_string(),
                            caller_line,
                            callee,
                            call_line,
                            call_column,
                            call_context,
                        });
                    }
                }
                
                // 引数内の関数呼び出しも再帰的に解析
                for arg in &call_expr.args {
                    self.extract_calls_from_expr(arg, caller, caller_file, caller_line, content);
                }
            }
            syn::Expr::MethodCall(method_call) => {
                // メソッド呼び出し
                let method_name = method_call.method.to_string();
                let (call_line, call_column) = self.find_call_location(&method_name, content, caller_line);
                let call_context = self.get_line_context(content, call_line);
                
                self.call_graph.push(CallInfo {
                    caller: caller.to_string(),
                    caller_file: caller_file.to_string(),
                    caller_line,
                    callee: method_name,
                    call_line,
                    call_column,
                    call_context,
                });
                
                // レシーバーと引数も再帰的に解析
                self.extract_calls_from_expr(&method_call.receiver, caller, caller_file, caller_line, content);
                for arg in &method_call.args {
                    self.extract_calls_from_expr(arg, caller, caller_file, caller_line, content);
                }
            }
            syn::Expr::Block(block_expr) => {
                self.extract_calls_from_block(&block_expr.block, caller, caller_file, caller_line, content);
            }
            syn::Expr::If(if_expr) => {
                self.extract_calls_from_expr(&if_expr.cond, caller, caller_file, caller_line, content);
                self.extract_calls_from_block(&if_expr.then_branch, caller, caller_file, caller_line, content);
                if let Some((_, else_branch)) = &if_expr.else_branch {
                    self.extract_calls_from_expr(else_branch, caller, caller_file, caller_line, content);
                }
            }
            syn::Expr::Match(match_expr) => {
                self.extract_calls_from_expr(&match_expr.expr, caller, caller_file, caller_line, content);
                for arm in &match_expr.arms {
                    self.extract_calls_from_expr(&arm.body, caller, caller_file, caller_line, content);
                }
            }
            syn::Expr::Binary(binary) => {
                self.extract_calls_from_expr(&binary.left, caller, caller_file, caller_line, content);
                self.extract_calls_from_expr(&binary.right, caller, caller_file, caller_line, content);
            }
            // 他の式タイプも必要に応じて追加
            _ => {}
        }
    }
    
    /// 関数呼び出しの位置を特定
    fn find_call_location(&self, callee: &str, content: &str, start_line: usize) -> (usize, usize) {
        let lines: Vec<&str> = content.lines().collect();
        
        // caller関数内から検索開始
        for (line_idx, line) in lines.iter().enumerate().skip(start_line.saturating_sub(1)) {
            if let Some(col) = line.find(&format!("{}(", callee)) {
                return (line_idx + 1, col);
            }
        }
        
        (start_line, 0)
    }
    
    /// 指定行のコンテキストを取得
    fn get_line_context(&self, content: &str, line: usize) -> String {
        let lines: Vec<&str> = content.lines().collect();
        if line > 0 && line <= lines.len() {
            lines[line - 1].trim().to_string()
        } else {
            String::new()
        }
    }
    
    /// コールグラフを取得
    pub fn get_call_graph(&self) -> &Vec<CallInfo> {
        &self.call_graph
    }
    
    /// 特定関数のコールグラフを取得
    pub fn get_calls_from_function(&self, function_name: &str) -> Vec<&CallInfo> {
        self.call_graph.iter()
            .filter(|call| call.caller == function_name)
            .collect()
    }
    
    /// 特定関数への呼び出しを取得
    pub fn get_calls_to_function(&self, function_name: &str) -> Vec<&CallInfo> {
        self.call_graph.iter()
            .filter(|call| call.callee == function_name)
            .collect()
    }

    /// 指定ファイルのシンボルをすべて削除（ファイル監視用）
    pub fn remove_file_symbols(&mut self, file_path: &str) {
        // 各シンボル名について、該当ファイルのシンボルを削除
        let mut symbol_names_to_remove = Vec::new();
        
        for (symbol_name, symbol_infos) in self.symbols.iter_mut() {
            // このファイルに属するシンボルを除外
            symbol_infos.retain(|info| info.file_path != file_path);
            
            // シンボルリストが空になったら、シンボル名も削除
            if symbol_infos.is_empty() {
                symbol_names_to_remove.push(symbol_name.clone());
            }
        }
        
        // 空になったシンボル名を削除
        for symbol_name in symbol_names_to_remove {
            self.symbols.remove(&symbol_name);
        }
        
        // コールグラフからも該当ファイルの情報を削除
        self.call_graph.retain(|call| call.caller_file != file_path);
    }

    /// 指定シンボルの使用箇所を検索
    pub fn find_usages(&self, symbol_name: &str, symbol_type: Option<SymbolType>) -> Vec<UsageInfo> {
        let mut usages = Vec::new();
        
        // 全ファイルから使用箇所を検索
        for (_, symbol_infos) in &self.symbols {
            for symbol_info in symbol_infos {
                // ファイル内容を読み込んで使用箇所を検索
                if let Ok(content) = std::fs::read_to_string(&symbol_info.file_path) {
                    let file_usages = self.find_usages_in_content(symbol_name, symbol_type.as_ref(), &content, &symbol_info.file_path);
                    usages.extend(file_usages);
                }
            }
        }
        
        // 重複を削除
        usages.sort_by(|a, b| {
            a.file_path.cmp(&b.file_path)
                .then(a.line.cmp(&b.line))
                .then(a.column.cmp(&b.column))
        });
        usages.dedup_by(|a, b| {
            a.file_path == b.file_path && a.line == b.line && a.column == b.column
        });
        
        usages
    }
    
    /// ファイル内容から使用箇所を検索
    fn find_usages_in_content(&self, symbol_name: &str, symbol_type: Option<&SymbolType>, content: &str, file_path: &str) -> Vec<UsageInfo> {
        let mut usages = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        
        for (line_idx, line) in lines.iter().enumerate() {
            let mut char_offset = 0;
            
            // 行内でシンボル名の出現を検索
            while let Some(pos) = line[char_offset..].find(symbol_name) {
                let absolute_pos = char_offset + pos;
                
                // 前後の文字をチェックして、単語境界であることを確認
                let is_word_boundary = {
                    let before_char = if absolute_pos > 0 {
                        line.chars().nth(absolute_pos - 1)
                    } else {
                        None
                    };
                    let after_char = line.chars().nth(absolute_pos + symbol_name.len());
                    
                    let before_ok = before_char.map_or(true, |c| !c.is_alphanumeric() && c != '_');
                    let after_ok = after_char.map_or(true, |c| !c.is_alphanumeric() && c != '_');
                    
                    before_ok && after_ok
                };
                
                if is_word_boundary {
                    // 使用箇所の種類を判定
                    let usage_type = self.determine_usage_type(line, absolute_pos, symbol_name, symbol_type);
                    
                    // 定義行でない場合のみ使用箇所として記録
                    if !self.is_definition_line(line, symbol_name, symbol_type) {
                        usages.push(UsageInfo {
                            symbol_name: symbol_name.to_string(),
                            file_path: file_path.to_string(),
                            line: line_idx + 1, // 1ベースの行番号
                            column: absolute_pos,
                            usage_type,
                            context: line.trim().to_string(),
                        });
                    }
                }
                
                char_offset = absolute_pos + 1;
            }
        }
        
        usages
    }
    
    /// 使用箇所の種類を判定
    fn determine_usage_type(&self, line: &str, pos: usize, symbol_name: &str, symbol_type: Option<&SymbolType>) -> UsageType {
        let trimmed = line.trim();
        
        // 関数呼び出しパターン
        if let Some(after_symbol) = line.get(pos + symbol_name.len()..) {
            if after_symbol.trim_start().starts_with('(') {
                return UsageType::FunctionCall;
            }
        }
        
        // 型注釈やstruct初期化
        if symbol_type == Some(&SymbolType::Struct) || symbol_type == Some(&SymbolType::Enum) {
            if trimmed.contains("::") || trimmed.contains('{') {
                return UsageType::TypeUsage;
            }
        }
        
        // トレイト使用
        if symbol_type == Some(&SymbolType::Trait) {
            if trimmed.contains("impl") || trimmed.contains("for") {
                return UsageType::TraitUsage;
            }
        }
        
        // インポート
        if trimmed.starts_with("use ") {
            return UsageType::Import;
        }
        
        UsageType::Reference
    }
    
    /// 定義行かどうかを判定
    fn is_definition_line(&self, line: &str, symbol_name: &str, symbol_type: Option<&SymbolType>) -> bool {
        let trimmed = line.trim();
        
        // 各シンボル種別の定義パターンをチェック
        let patterns = match symbol_type {
            Some(SymbolType::Function) => vec![
                format!("fn {}", symbol_name),
                format!("async fn {}", symbol_name),
            ],
            Some(SymbolType::Struct) => vec![
                format!("struct {}", symbol_name),
            ],
            Some(SymbolType::Enum) => vec![
                format!("enum {}", symbol_name),
            ],
            Some(SymbolType::Trait) => vec![
                format!("trait {}", symbol_name),
            ],
            None => vec![
                format!("fn {}", symbol_name),
                format!("async fn {}", symbol_name),
                format!("struct {}", symbol_name),
                format!("enum {}", symbol_name),
                format!("trait {}", symbol_name),
            ],
        };
        
        // 可視性修飾子も考慮
        for pattern in patterns {
            if trimmed.contains(&pattern) ||
               trimmed.contains(&format!("pub {}", pattern)) ||
               trimmed.contains(&format!("pub(crate) {}", pattern)) ||
               trimmed.contains(&format!("pub(super) {}", pattern)) {
                return true;
            }
        }
        
        false
    }

    /// ジェネリクスパラメータをフォーマット
    fn format_generics(&self, generics: &syn::Generics) -> Option<String> {
        if generics.params.is_empty() {
            return None;
        }
        
        let params = generics.params.iter()
            .map(|p| quote::quote!(#p).to_string())
            .collect::<Vec<_>>()
            .join(", ");
        
        Some(format!("<{params}>"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_parse_simple_function() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.rs");
        
        fs::write(&file_path, r#"
fn hello_world() {
    println!("Hello, world!");
}

pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
"#).unwrap();

        let mut parser = RustParser::new();
        parser.parse_file(&file_path).unwrap();

        let hello_fn = parser.find_symbol("hello_world", Some(SymbolType::Function)).unwrap();
        assert_eq!(hello_fn.len(), 1);
        assert_eq!(hello_fn[0].name, "hello_world");
        assert_eq!(hello_fn[0].visibility, "private");

        let add_fn = parser.find_symbol("add", Some(SymbolType::Function)).unwrap();
        assert_eq!(add_fn.len(), 1);
        assert_eq!(add_fn[0].name, "add");
        assert_eq!(add_fn[0].visibility, "pub");
    }
    
    #[test]
    fn test_parse_types() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("types.rs");
        
        fs::write(&file_path, r#"
pub struct User {
    pub id: u64,
    pub name: String,
}

pub enum Status {
    Active,
    Inactive,
}

pub trait Drawable {
    fn draw(&self);
}

struct InternalState {
    counter: u32,
}
"#).unwrap();

        let mut parser = RustParser::new();
        parser.parse_file(&file_path).unwrap();

        // Test struct parsing
        let user_struct = parser.find_symbol("User", Some(SymbolType::Struct)).unwrap();
        assert_eq!(user_struct.len(), 1);
        assert_eq!(user_struct[0].name, "User");
        assert_eq!(user_struct[0].visibility, "pub");
        assert_eq!(user_struct[0].symbol_type, SymbolType::Struct);

        // Test enum parsing
        let status_enum = parser.find_symbol("Status", Some(SymbolType::Enum)).unwrap();
        assert_eq!(status_enum.len(), 1);
        assert_eq!(status_enum[0].name, "Status");
        assert_eq!(status_enum[0].visibility, "pub");
        assert_eq!(status_enum[0].symbol_type, SymbolType::Enum);

        // Test trait parsing
        let drawable_trait = parser.find_symbol("Drawable", Some(SymbolType::Trait)).unwrap();
        assert_eq!(drawable_trait.len(), 1);
        assert_eq!(drawable_trait[0].name, "Drawable");
        assert_eq!(drawable_trait[0].visibility, "pub");
        assert_eq!(drawable_trait[0].symbol_type, SymbolType::Trait);

        // Test private struct
        let internal_struct = parser.find_symbol("InternalState", Some(SymbolType::Struct)).unwrap();
        assert_eq!(internal_struct.len(), 1);
        assert_eq!(internal_struct[0].visibility, "private");
    }
    
    #[test]
    fn test_parse_generics() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("generics.rs");
        
        fs::write(&file_path, r#"
pub struct Container<T> {
    items: Vec<T>,
}

pub enum Result<T, E> {
    Ok(T),
    Err(E),
}

pub trait Iterator<Item> {
    fn next(&mut self) -> Option<Item>;
}
"#).unwrap();

        let mut parser = RustParser::new();
        parser.parse_file(&file_path).unwrap();

        // Test generic struct
        let container = parser.find_symbol("Container", Some(SymbolType::Struct)).unwrap();
        assert_eq!(container.len(), 1);
        assert_eq!(container[0].generics, Some("<T>".to_string()));
        
        // Test generic enum
        let result = parser.find_symbol("Result", Some(SymbolType::Enum)).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].generics, Some("<T, E>".to_string()));
        
        // Test generic trait
        let iterator = parser.find_symbol("Iterator", Some(SymbolType::Trait)).unwrap();
        assert_eq!(iterator.len(), 1);
        assert_eq!(iterator[0].generics, Some("<Item>".to_string()));
    }
}