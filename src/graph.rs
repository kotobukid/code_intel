use crate::indexer::CodeIndexer;
use anyhow::Result;
use std::collections::HashSet;
use std::path::Path;

pub struct CallGraphGenerator {
    indexer: CodeIndexer,
}

#[derive(Debug, Clone)]
pub struct GraphNode {
    pub name: String,
    pub file_path: String,
    pub line: usize,
    pub children: Vec<String>,
    pub parents: Vec<String>,
}

impl CallGraphGenerator {
    pub fn new() -> Self {
        Self {
            indexer: CodeIndexer::new(),
        }
    }

    pub fn analyze_project<P: AsRef<Path>>(&mut self, project_path: P) -> Result<()> {
        self.indexer.index_directory(project_path)?;
        Ok(())
    }

    pub fn generate_tree_format(&self, function_name: Option<&str>, max_depth: usize, reverse: bool) -> String {
        if reverse {
            self.generate_callers_tree(function_name, max_depth)
        } else {
            self.generate_callees_tree(function_name, max_depth)
        }
    }

    pub fn generate_mermaid_format(&self, function_name: Option<&str>) -> String {
        let mut result = String::from("```mermaid\ngraph TD\n");
        
        let calls = self.indexer.get_parser().get_call_graph();
        let mut nodes = HashSet::new();
        let mut edges = HashSet::new();

        // 特定関数に絞り込む場合
        if let Some(func_name) = function_name {
            let related_calls: Vec<_> = calls.iter()
                .filter(|call| call.caller == func_name || call.callee == func_name)
                .collect();
            
            for call in related_calls {
                nodes.insert(&call.caller);
                nodes.insert(&call.callee);
                edges.insert((&call.caller, &call.callee));
            }
        } else {
            // 全体のグラフ
            for call in calls {
                nodes.insert(&call.caller);
                nodes.insert(&call.callee);
                edges.insert((&call.caller, &call.callee));
            }
        }

        // ノードの定義
        for node in &nodes {
            result.push_str(&format!("    {}[{}]\n", self.node_id(node), node));
        }

        // エッジの定義
        for (caller, callee) in edges {
            result.push_str(&format!("    {} --> {}\n", 
                self.node_id(caller), self.node_id(callee)));
        }

        result.push_str("```\n");
        result
    }

    fn generate_callees_tree(&self, function_name: Option<&str>, max_depth: usize) -> String {
        let mut result = String::new();
        
        if let Some(func_name) = function_name {
            result.push_str(&format!("📞 Call Graph for: {}\n\n", func_name));
            self.print_callees_recursive(func_name, 0, max_depth, &mut result, &mut HashSet::new());
        } else {
            result.push_str("📞 Full Call Graph\n\n");
            let all_functions = self.get_all_functions();
            let entry_points = self.find_entry_points(&all_functions);
            
            for entry in entry_points {
                self.print_callees_recursive(&entry, 0, max_depth, &mut result, &mut HashSet::new());
                result.push('\n');
            }
        }

        result
    }

    fn generate_callers_tree(&self, function_name: Option<&str>, max_depth: usize) -> String {
        let mut result = String::new();
        
        if let Some(func_name) = function_name {
            result.push_str(&format!("📞 Callers of: {}\n\n", func_name));
            self.print_callers_recursive(func_name, 0, max_depth, &mut result, &mut HashSet::new());
        } else {
            result.push_str("📞 Reverse Call Graph\n\n");
            let all_functions = self.get_all_functions();
            let leaf_functions = self.find_leaf_functions(&all_functions);
            
            for leaf in leaf_functions {
                self.print_callers_recursive(&leaf, 0, max_depth, &mut result, &mut HashSet::new());
                result.push('\n');
            }
        }

        result
    }

    fn print_callees_recursive(&self, function_name: &str, depth: usize, max_depth: usize, 
                              result: &mut String, visited: &mut HashSet<String>) {
        if depth > max_depth || visited.contains(function_name) {
            if visited.contains(function_name) {
                result.push_str(&format!("{}├── {} [🔄 recursive]\n", 
                    "│   ".repeat(depth), function_name));
            }
            return;
        }

        visited.insert(function_name.to_string());

        let indent = if depth == 0 { 
            String::new() 
        } else { 
            "│   ".repeat(depth - 1) + "├── " 
        };

        // 関数の情報を取得
        let func_info = self.get_function_info(function_name);
        result.push_str(&format!("{}{}{}\n", 
            indent, function_name, func_info));

        // この関数が呼び出している関数を表示
        let callees = self.indexer.get_parser().get_calls_from_function(function_name);
        for call in callees {
            self.print_callees_recursive(&call.callee, depth + 1, max_depth, result, visited);
        }

        visited.remove(function_name);
    }

    fn print_callers_recursive(&self, function_name: &str, depth: usize, max_depth: usize, 
                              result: &mut String, visited: &mut HashSet<String>) {
        if depth > max_depth || visited.contains(function_name) {
            if visited.contains(function_name) {
                result.push_str(&format!("{}├── {} [🔄 recursive]\n", 
                    "│   ".repeat(depth), function_name));
            }
            return;
        }

        visited.insert(function_name.to_string());

        let indent = if depth == 0 { 
            String::new() 
        } else { 
            "│   ".repeat(depth - 1) + "├── " 
        };

        let func_info = self.get_function_info(function_name);
        result.push_str(&format!("{}{}{}\n", 
            indent, function_name, func_info));

        // この関数を呼び出している関数を表示
        let callers = self.indexer.get_parser().get_calls_to_function(function_name);
        for call in callers {
            self.print_callers_recursive(&call.caller, depth + 1, max_depth, result, visited);
        }

        visited.remove(function_name);
    }

    fn get_function_info(&self, function_name: &str) -> String {
        if let Some(symbols) = self.indexer.find_definition(function_name, None) {
            if let Some(symbol) = symbols.first() {
                return format!(" @ {}:{}", 
                    symbol.file_path.split('/').last().unwrap_or(&symbol.file_path),
                    symbol.line);
            }
        }
        String::new()
    }

    fn get_all_functions(&self) -> HashSet<String> {
        let mut functions = HashSet::new();
        
        for call in self.indexer.get_parser().get_call_graph() {
            functions.insert(call.caller.clone());
            functions.insert(call.callee.clone());
        }

        // 定義された関数も追加
        for (name, symbols) in self.indexer.get_parser().get_all_symbols() {
            for symbol in symbols {
                if symbol.symbol_type == crate::protocol::SymbolType::Function {
                    functions.insert(name.clone());
                }
            }
        }

        functions
    }

    fn find_entry_points(&self, all_functions: &HashSet<String>) -> Vec<String> {
        let mut entry_points = Vec::new();
        
        for func in all_functions {
            let callers = self.indexer.get_parser().get_calls_to_function(func);
            if callers.is_empty() || func == "main" {
                entry_points.push(func.clone());
            }
        }

        // mainがあれば優先
        if entry_points.contains(&"main".to_string()) {
            vec!["main".to_string()]
        } else {
            entry_points
        }
    }

    fn find_leaf_functions(&self, all_functions: &HashSet<String>) -> Vec<String> {
        let mut leaf_functions = Vec::new();
        
        for func in all_functions {
            let callees = self.indexer.get_parser().get_calls_from_function(func);
            if callees.is_empty() {
                leaf_functions.push(func.clone());
            }
        }

        leaf_functions
    }

    fn node_id(&self, name: &str) -> String {
        // Mermaid用のID生成（英数字のみ）
        name.chars()
            .map(|c| if c.is_alphanumeric() { c } else { '_' })
            .collect()
    }

    pub fn get_stats(&self) -> String {
        let calls = self.indexer.get_parser().get_call_graph();
        let all_functions = self.get_all_functions();
        
        format!("📊 Call Graph Statistics:\n\
                 ├── Total Functions: {}\n\
                 ├── Total Calls: {}\n\
                 ├── Entry Points: {}\n\
                 └── Leaf Functions: {}\n",
                all_functions.len(),
                calls.len(),
                self.find_entry_points(&all_functions).len(),
                self.find_leaf_functions(&all_functions).len())
    }
}