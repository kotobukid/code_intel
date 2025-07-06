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

pub struct RustParser {
    symbols: HashMap<String, Vec<SymbolInfo>>,
}

impl RustParser {
    pub fn new() -> Self {
        Self {
            symbols: HashMap::new(),
        }
    }

    pub fn parse_file<P: AsRef<Path>>(&mut self, file_path: P) -> Result<()> {
        let file_path = file_path.as_ref();
        let content = std::fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read file: {}", file_path.display()))?;

        let syntax_tree = syn::parse_file(&content)
            .with_context(|| format!("Failed to parse file: {}", file_path.display()))?;

        self.extract_symbols(&syntax_tree, file_path.to_string_lossy().to_string(), &content)?;
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