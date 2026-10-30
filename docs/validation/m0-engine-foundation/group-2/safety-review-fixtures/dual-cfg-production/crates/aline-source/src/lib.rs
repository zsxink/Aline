#[cfg(test)] mod shared;
#[cfg(not(test))] mod shared;
pub fn run() { shared::check(); }
