use std::collections::HashMap;
use std::path::Path;
use syn::{File, Item, ItemFn, Signature, Visibility};
use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct FunctionInfo {
    pub name: String,
    pub file_path: String,
    pub line: usize,
    pub column: usize,
    pub signature: String,
    pub visibility: String,
}

pub struct RustParser {
    functions: HashMap<String, Vec<FunctionInfo>>,
}

impl RustParser {
    pub fn new() -> Self {
        Self {
            functions: HashMap::new(),
        }
    }

    pub fn parse_file<P: AsRef<Path>>(&mut self, file_path: P) -> Result<()> {
        let file_path = file_path.as_ref();
        let content = std::fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read file: {}", file_path.display()))?;

        let syntax_tree = syn::parse_file(&content)
            .with_context(|| format!("Failed to parse file: {}", file_path.display()))?;

        self.extract_functions(&syntax_tree, file_path.to_string_lossy().to_string())?;
        Ok(())
    }

    fn extract_functions(&mut self, syntax_tree: &File, file_path: String) -> Result<()> {
        for item in &syntax_tree.items {
            if let Item::Fn(item_fn) = item {
                let func_info = self.extract_function_info(item_fn, &file_path)?;
                
                // 関数名でグループ化（オーバーロードは考慮しない）
                self.functions
                    .entry(func_info.name.clone())
                    .or_insert_with(Vec::new)
                    .push(func_info);
            }
        }
        Ok(())
    }

    fn extract_function_info(&self, item_fn: &ItemFn, file_path: &str) -> Result<FunctionInfo> {
        let name = item_fn.sig.ident.to_string();
        let signature = self.format_signature(&item_fn.sig);
        let visibility = self.format_visibility(&item_fn.vis);
        
        // syn の span から行番号を取得（1ベース）
        // 注意: proc_macro2::Span は行番号情報を直接提供しないため、
        // とりあえず1を設定（将来的には別の方法で取得）
        let line = 1;
        let column = 0;

        Ok(FunctionInfo {
            name,
            file_path: file_path.to_string(),
            line,
            column,
            signature,
            visibility,
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

    pub fn find_function(&self, name: &str) -> Option<&Vec<FunctionInfo>> {
        self.functions.get(name)
    }

    pub fn get_all_functions(&self) -> &HashMap<String, Vec<FunctionInfo>> {
        &self.functions
    }

    /// 指定ファイルの関数をすべて削除（ファイル監視用）
    pub fn remove_file_functions(&mut self, file_path: &str) {
        // 各関数名について、該当ファイルの関数を削除
        let mut function_names_to_remove = Vec::new();
        
        for (func_name, func_infos) in self.functions.iter_mut() {
            // このファイルに属する関数を除外
            func_infos.retain(|info| info.file_path != file_path);
            
            // 関数リストが空になったら、関数名も削除
            if func_infos.is_empty() {
                function_names_to_remove.push(func_name.clone());
            }
        }
        
        // 空になった関数名を削除
        for func_name in function_names_to_remove {
            self.functions.remove(&func_name);
        }
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

        let hello_fn = parser.find_function("hello_world").unwrap();
        assert_eq!(hello_fn.len(), 1);
        assert_eq!(hello_fn[0].name, "hello_world");
        assert_eq!(hello_fn[0].visibility, "private");

        let add_fn = parser.find_function("add").unwrap();
        assert_eq!(add_fn.len(), 1);
        assert_eq!(add_fn[0].name, "add");
        assert_eq!(add_fn[0].visibility, "pub");
    }
}