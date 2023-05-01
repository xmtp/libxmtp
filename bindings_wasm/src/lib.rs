mod local_storage_persistence;

use local_storage_persistence::LocalStoragePersistence;
use std::sync::Mutex;
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;
use xmtp::{Client, ClientBuilder};

static CLIENT_LIST: Mutex<Vec<Client<LocalStoragePersistence>>> = Mutex::new(Vec::new());

#[wasm_bindgen]
pub fn client_create() -> usize {
    console_error_panic_hook::set_once();
    let mut clients = CLIENT_LIST.lock().unwrap();
    clients.push(
        ClientBuilder::new()
            .persistence(LocalStoragePersistence::new())
            .wallet_address("unknown".to_string())
            .build()
            .expect("Failed to create client"),
    );
    clients.len() - 1
}

#[wasm_bindgen]
pub fn client_write_to_persistence(
    client_id: usize,
    key: String,
    value: &[u8],
) -> Result<(), String> {
    let mut clients = CLIENT_LIST.lock().unwrap();
    let client = clients.get_mut(client_id).expect("Client not found");
    client.write_to_persistence(key, value)
}

#[wasm_bindgen]
pub fn client_read_from_persistence(
    client_id: usize,
    key: String,
) -> Result<Option<Vec<u8>>, String> {
    let mut clients = CLIENT_LIST.lock().unwrap();
    let client = clients.get_mut(client_id).expect("Client not found");
    client.read_from_persistence(key)
}

wasm_bindgen_test_configure!(run_in_browser);
#[wasm_bindgen_test]
fn can_pass_persistence_methods() {
    let client_id = client_create();
    assert_eq!(
        client_read_from_persistence(client_id, "foo".to_string()).unwrap(),
        None
    );
    assert_eq!(
        client_write_to_persistence(client_id, "foo".to_string(), b"bar").unwrap(),
        ()
    );
    assert_eq!(
        client_read_from_persistence(client_id, "foo".to_string()).unwrap(),
        Some(b"bar".to_vec())
    );
}
