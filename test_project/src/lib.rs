pub mod utils;
pub mod calculator;

pub fn library_function(input: &str) -> String {
    format!("Processed: {}", input)
}

pub struct DataProcessor {
    name: String,
}

impl DataProcessor {
    pub fn new(name: String) -> Self {
        Self { name }
    }
    
    pub fn process_data(&self, data: Vec<i32>) -> Vec<i32> {
        data.iter().map(|x| x * 2).collect()
    }
    
    fn internal_helper(&self) -> &str {
        &self.name
    }
}

pub trait Processable {
    fn process(&self) -> String;
}

impl Processable for DataProcessor {
    fn process(&self) -> String {
        format!("Processing with {}", self.name)
    }
}