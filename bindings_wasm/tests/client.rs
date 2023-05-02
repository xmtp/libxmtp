use bindings_wasm::{client_create, client_read_from_persistence, client_write_to_persistence};
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);
#[wasm_bindgen_test]
fn can_pass_persistence_methods() {
    let client_id = client_create();
    assert_eq!(
        client_read_from_persistence(client_id, "foo".to_string()).unwrap(),
        None
    );
    client_write_to_persistence(client_id, "foo".to_string(), b"bar").unwrap();
    assert_eq!(
        client_read_from_persistence(client_id, "foo".to_string()).unwrap(),
        Some(b"bar".to_vec())
    );
}
