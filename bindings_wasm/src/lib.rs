mod local_storage_persistence;

use local_storage_persistence::LocalStoragePersistence;
use std::sync::Mutex;
use wasm_bindgen::prelude::*;
use xmtp::{networking::MockXmtpApiClient, Client, ClientBuilder};

static CLIENT_LIST: Mutex<Vec<Client<LocalStoragePersistence, MockXmtpApiClient>>> =
    Mutex::new(Vec::new());

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
    key: &str,
    value: &[u8],
) -> Result<(), String> {
    let mut clients = CLIENT_LIST.lock().unwrap();
    let client = clients.get_mut(client_id).expect("Client not found");
    client.write_to_persistence(key, value)
}

#[wasm_bindgen]
pub fn client_read_from_persistence(
    client_id: usize,
    key: &str,
) -> Result<Option<Vec<u8>>, String> {
    let mut clients = CLIENT_LIST.lock().unwrap();
    let client = clients.get_mut(client_id).expect("Client not found");
    client.read_from_persistence(key)
}
