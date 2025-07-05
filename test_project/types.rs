// Test file for struct, enum, and trait parsing

pub struct User {
    pub id: u64,
    pub name: String,
    pub email: String,
}

pub struct GenericContainer<T> {
    items: Vec<T>,
}

pub enum Status {
    Active,
    Inactive,
    Pending(String),
}

pub enum Result<T, E> {
    Ok(T),
    Err(E),
}

pub trait Drawable {
    fn draw(&self);
    fn get_bounds(&self) -> (f64, f64);
}

pub trait Container<T> {
    fn add(&mut self, item: T);
    fn get(&self, index: usize) -> Option<&T>;
}

// Private types should also be parsed
struct InternalState {
    counter: u32,
}

enum PrivateEnum {
    OptionA,
    OptionB,
}

trait InternalTrait {
    fn internal_method(&self);
}