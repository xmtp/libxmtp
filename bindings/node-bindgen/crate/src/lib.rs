use node_bindgen::derive::node_bindgen;


#[node_bindgen]
fn test() -> String {
    xmtpv3::manager::e2e_selftest()
        .map_err(|x| x.to_string())
        .unwrap()
}
