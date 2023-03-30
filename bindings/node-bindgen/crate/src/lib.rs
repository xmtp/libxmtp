use node_bindgen::derive::node_bindgen;
use xmtpv3;

#[node_bindgen]
fn sum(first: i32, second: i32) -> i32 {
    first + second
}

#[node_bindgen]
fn test() -> String {
    xmtpv3::e2e_selftest().map_err(|x| x.to_string()).unwrap()
}
