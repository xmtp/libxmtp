use bindings_wasm::{client_create, client_read_from_persistence, client_write_to_persistence};
use wasm_bindgen::JsError;
use wasm_bindgen_test::*;

// JSError does not implement Debug or Display, so we can't use unwrap() or render an
// error message
fn unwrap_js_error<T>(result: Result<T, JsError>) -> T {
    match result {
        Ok(value) => value,
        Err(_e) => panic!(),
    }
}

wasm_bindgen_test_configure!(run_in_browser);
#[wasm_bindgen_test]
fn can_pass_persistence_methods() {
    let client_id = unwrap_js_error(client_create());
    assert_eq!(
        unwrap_js_error(client_read_from_persistence(client_id, "foo")),
        None
    );
    unwrap_js_error(client_write_to_persistence(client_id, "foo", b"bar"));
    assert_eq!(
        unwrap_js_error(client_read_from_persistence(client_id, "foo")),
        Some(b"bar".to_vec())
    );
}
