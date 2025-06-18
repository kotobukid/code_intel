pub fn format_number(num: i32) -> String {
    format!("Number: {}", num)
}

pub fn is_even(num: i32) -> bool {
    num % 2 == 0
}

pub(crate) fn internal_utility() -> i32 {
    42
}

fn private_helper() -> &'static str {
    "helper"
}

pub struct Helper {
    value: i32,
}

impl Helper {
    pub fn new(value: i32) -> Self {
        Self { value }
    }
    
    pub fn get_value(&self) -> i32 {
        self.value
    }
}