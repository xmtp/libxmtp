#![cfg(target_arch = "wasm32")]

use diesel_wasm_sqlite::WasmSqliteConnection;
use wasm_bindgen_test::*;
use web_sys::console;
wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn test_console_log() {
    diesel_wasm_sqlite::utils::set_panic_hook();
    console::log_1(&"TEST THIS WORKS????".into());
    assert_eq!(1, 2);
}

/*
#[wasm_bindgen_test]
fn test_establish() {
    let rng: u16 = rand::random();
    let url = format!(
        "{}/wasmtest-{}.db3",
        std::env::temp_dir().to_str().unwrap(),
        rng
    );
    let mut conn = WasmSqliteConnection::establish(&url).unwrap();
    println!("{:?}", conn);
}
*/
