mod local_storage_persistence;

use base64::{engine::general_purpose, Engine as _};
use local_storage_persistence::LocalStoragePersistence;
use std::sync::Mutex;
use wasm_bindgen::prelude::*;
use xmtp::{
    account::{Account, AccountCreator},
    Client, ClientBuilder, Signable,
};

static CLIENT_LIST: Mutex<Vec<Client<LocalStoragePersistence>>> = Mutex::new(Vec::new());
static ACCOUNTS: Mutex<Vec<Account>> = Mutex::new(Vec::new());

// TODO: Custom JS Error subclasses (https://github.com/xmtp/libxmtp/issues/104)

// Passing Vecs across wasm boundary using Uint8Array views results in unsafe code
// use base64 instead
pub fn to_base64(bytes: Vec<u8>) -> String {
    general_purpose::STANDARD_NO_PAD.encode(bytes)
}
pub fn from_base64(s: &str) -> Result<Vec<u8>, base64::DecodeError> {
    let bytes = general_purpose::STANDARD_NO_PAD.decode(s)?;
    Ok(bytes)
}

#[wasm_bindgen]
pub fn client_create(account_id: usize) -> Result<usize, JsError> {
    console_error_panic_hook::set_once();
    let mut clients = CLIENT_LIST.lock().unwrap();
    clients.push(
        ClientBuilder::new()
            .persistence(LocalStoragePersistence::new())
            .wallet_address("unknown".to_string())
            .build()?,
    );
    Ok(clients.len() - 1)
}

#[wasm_bindgen]
pub fn client_write_to_persistence(
    client_id: usize,
    key: &str,
    value: &[u8],
) -> Result<(), JsError> {
    let mut clients = CLIENT_LIST.lock().unwrap();
    let client = clients.get_mut(client_id).expect("Client not found");
    client.write_to_persistence(key, value)?;
    Ok(())
}

#[wasm_bindgen]
pub fn client_read_from_persistence(
    client_id: usize,
    key: &str,
) -> Result<Option<Vec<u8>>, JsError> {
    let mut clients = CLIENT_LIST.lock().unwrap();
    let client = clients.get_mut(client_id).expect("Client not found");
    let value = client.read_from_persistence(key)?;
    Ok(value)
}

#[wasm_bindgen]
pub fn register(f: js_sys::Function) -> Result<usize, JsError> {
    console_error_panic_hook::set_once();

    let account_creator = AccountCreator::new();
    let key_bytes = to_base64(account_creator.bytes_to_sign());
    let sig = f
        .call1(&JsValue::NULL, &JsValue::from_str(&key_bytes))
        .unwrap()
        .as_string()
        .unwrap();

    let account = account_creator.finalize_key(from_base64(&sig)?);
    let mut accounts = ACCOUNTS.lock().unwrap();
    accounts.push(account);
    Ok(accounts.len() - 1)
}
