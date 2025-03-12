#[cfg(not(target_arch = "wasm32"))]
fn main() {
    // make sure we are statically linking sqlcipher at build time
    pkg_config::Config::new()
        .statik(true)
        .probe("sqlcipher")
        .unwrap();
}
