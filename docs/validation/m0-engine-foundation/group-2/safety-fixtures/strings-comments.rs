// panic!() unwrap()
/* outer /* unsafe */ assert!() */ const X: &str = r###"OnceLock panic!() assert!() unwrap()"###; fn f<'a>(x: &'a str) { let c = 'x'; let q = '\''; let s = "panic!()\"unwrap()"; }
