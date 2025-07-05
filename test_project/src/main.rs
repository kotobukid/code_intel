fn main() {
    println!("Hello from test project!");
    let result = calculate_sum(10, 20);
    println!("Sum: {}", result);
    
    greet_user("Alice");
    let doubled = double_value(42);
    println!("Doubled: {}", doubled);
}

pub fn calculate_sum(a: i32, b: i32) -> i32 {
    a + b
}

fn greet_user(name: &str) {
    println!("Hello, {}!", name);
}

pub async fn async_function() -> Result<String, Box<dyn std::error::Error>> {
    Ok("Async result".to_string())
}

fn double_value(x: i32) -> i32 {
    x * 2
}

trait Hoge {
    fn call(&self);
}