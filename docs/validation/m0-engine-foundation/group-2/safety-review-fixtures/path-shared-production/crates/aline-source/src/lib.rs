#[cfg(test)] mod tests;
#[path="tests.rs"] mod production;
pub fn run() { production::check(); }
