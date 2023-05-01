use std::sync::Mutex;

use wasm_bindgen::prelude::*;
use web_sys;
use xmtp::persistence::{InMemoryPersistence, Persistence};
use xmtp::{Client, ClientBuilder};

static CLIENT_LIST: Mutex<Vec<Client<LocalStoragePersistence>>> = Mutex::new(Vec::new());

pub struct LocalStoragePersistence {}

impl LocalStoragePersistence {
    pub fn new() -> Self {
        LocalStoragePersistence {}
    }

    fn storage(&self) -> web_sys::Storage {
        web_sys::window()
            .expect("Global Window not found - are you running in a browser?")
            .local_storage()
            .expect("Local Storage not found - are you running in a browser?")
            .expect("Window.localStorage not found - are you running in a browser?")
    }
}

impl Default for LocalStoragePersistence {
    fn default() -> Self {
        Self::new()
    }
}

impl Persistence for LocalStoragePersistence {
    fn write(&mut self, key: String, value: &[u8]) -> Result<(), String> {
        let value = String::from_utf8(value.to_vec()).unwrap();
        let key = format!("xmtp_{}", key);
        self.storage()
            .set_item(&key, &value)
            .expect("Failed to write to local storage");
        Ok(())
    }

    fn read(&self, key: String) -> Result<Option<Vec<u8>>, String> {
        let key = format!("xmtp_{}", key);
        let value = self
            .storage()
            .get_item(&key)
            .expect("Failed to read from local storage");
        if value.is_none() {
            return Ok(None);
        }
        let value = value.unwrap();
        Ok(Some(value.as_bytes().to_vec()))
    }
}

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

// #[wasm_bindgen]
// pub fn e2e_test(word: &str) -> String {
//     let key = "test";
//     let value = word.to_string();
//     let bytes = value.as_bytes();
//     write_wrapper(key, bytes);
//     let result = read_wrapper(key);
//     let result = String::from_utf8(result).unwrap();
//     result
// }
