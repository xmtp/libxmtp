use std::sync::Mutex;

use libxmtp_core::client::Client;
use libxmtp_core::persistence::{InMemoryPersistence, Persistence};
use wasm_bindgen::prelude::*;
use web_sys;

static CLIENT_LIST: Mutex<Vec<Client<LocalStoragePersistence>>> = Mutex::new(Vec::new());

#[wasm_bindgen(module = "/js/foo.js")]
extern "C" {
    #[wasm_bindgen(js_name = writeWrapper)]
    fn write_wrapper(key: &str, bytes: &[u8]) -> bool;
    #[wasm_bindgen(js_name = readWrapper)]
    fn read_wrapper(key: &str) -> Vec<u8>;
}

struct LocalStoragePersistence {}

impl LocalStoragePersistence {
    pub fn new() -> Self {
        LocalStoragePersistence {}
    }
}

impl Persistence for LocalStoragePersistence {
    fn write(&mut self, key: String, value: &[u8]) -> Result<(), String> {
        let value = String::from_utf8(value.to_vec()).unwrap();
        let key = format!("xmtp_{}", key);
        println!("Writing to local storage: {} = {}", key, value);
        web_sys::window()
            .unwrap()
            .local_storage()
            .unwrap()
            .unwrap()
            .set_item(&key, &value)
            .unwrap();
        Ok(())
    }

    fn read(&self, key: String) -> Result<Option<Vec<u8>>, String> {
        let key = format!("xmtp_{}", key);
        println!("Reading from local storage: {}", key);
        let value = web_sys::window()
            .unwrap()
            .local_storage()
            .unwrap()
            .unwrap()
            .get_item(&key)
            .unwrap();
        if value.is_none() {
            return Ok(None);
        }
        let value = value.unwrap();
        Ok(Some(value.as_bytes().to_vec()))
    }
}

#[wasm_bindgen]
pub fn client_create() -> usize {
    let mut clients = CLIENT_LIST.lock().unwrap();
    clients.push(Client::new(LocalStoragePersistence::new()));
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

#[wasm_bindgen]
pub fn e2e_test(word: &str) -> String {
    let key = "test";
    let value = word.to_string();
    let bytes = value.as_bytes();
    write_wrapper(key, bytes);
    let result = read_wrapper(key);
    let result = String::from_utf8(result).unwrap();
    result
}
