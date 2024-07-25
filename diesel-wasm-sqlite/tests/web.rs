#![cfg(target_arch = "wasm32")]

use diesel::connection::Connection;
use diesel_wasm_sqlite::rust_establish;
use wasm_bindgen_test::*;
use web_sys::console;
wasm_bindgen_test_configure!(run_in_dedicated_worker);

#[wasm_bindgen_test]
async fn test_establish() {
    let rng: u16 = rand::random();
    /* let url = format!(
        "{}/wasmtest-{}.db3",
        std::env::temp_dir().to_str().unwrap(),
        rng
    );
    */
    let mut conn = rust_establish("test").await.unwrap();
    console::log_1(&"CONNECTED".into());
    // assert 1 == 2 is here b/c can't get --nocapture to work yet
    assert_eq!(1, 2);
}
