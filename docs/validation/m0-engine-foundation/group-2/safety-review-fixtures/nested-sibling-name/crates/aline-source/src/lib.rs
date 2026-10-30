mod tests;
#[cfg(test)] mod outer { #[cfg(test)] mod tests; }
pub fn run() { tests::check(); }
