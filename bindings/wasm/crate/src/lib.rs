use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Mutex;

use wasm_bindgen::prelude::*;

use xmtpv3::manager::VoodooInstance;

#[macro_use]
extern crate lazy_static;

// This whole strategy is to keep a singleton in WASM memory world
lazy_static! {
    static ref INSTANCE_MAP: Mutex<HashMap<String, RefCell<VoodooInstance>>> =
        Mutex::new(HashMap::new());
}

// Returns a handle to a keystore instance
#[wasm_bindgen]
pub fn new_voodoo_instance() -> String {
    let mut instances = INSTANCE_MAP.lock().unwrap();
    let handle = (instances.len() as u64).to_string();
    instances.insert(handle.clone(), RefCell::new(VoodooInstance::new()));
    handle
}

#[wasm_bindgen]
pub fn create_outbound_session(
    sending_handle: &str,
    receiving_handle: &str,
    message: &str,
) -> Result<Box<[JsValue]>, JsValue> {
    // Look up both handles in INSTANCE_MAP
    let instances = INSTANCE_MAP.lock().unwrap();
    let mut sending_instance = instances
        .get(sending_handle)
        .ok_or("sending_handle not found")?
        .borrow_mut();

    let receiving_instance = instances
        .get(receiving_handle)
        .ok_or("receiving_handle not found")?
        .borrow();

    // Get other party's public situation
    let receiving_public = receiving_instance.public_account();

    // Create the session
    let result = sending_instance.create_outbound_session_serialized(&receiving_public, message);

    match result {
        Ok((session_id, ciphertext_json)) => Ok(vec![
            JsValue::from_str(&session_id),
            JsValue::from_str(&ciphertext_json),
        ]
        .into_boxed_slice()),
        Err(e) => Err(JsValue::from_str(&e.to_string())),
    }
}

#[wasm_bindgen]
pub fn create_inbound_session(
    sending_handle: &str,
    receiving_handle: &str,
    message: &str,
) -> Result<Box<[JsValue]>, JsValue> {
    // Look up both handles in INSTANCE_MAP
    let instances = INSTANCE_MAP.lock().unwrap();
    let sending_instance = instances
        .get(sending_handle)
        .ok_or("sending_handle not found")?
        .borrow();
    let mut receiving_instance = instances
        .get(receiving_handle)
        .ok_or("receiving_handle not found")?
        .borrow_mut();

    // Get sender party public
    let sending_public = sending_instance.public_account();

    // Create the session
    let result = receiving_instance.create_inbound_session_serialized(&sending_public, message);

    match result {
        Ok((session_id, ciphertext_json)) => Ok(vec![
            JsValue::from_str(&session_id),
            JsValue::from_str(&ciphertext_json),
        ]
        .into_boxed_slice()),
        Err(e) => Err(JsValue::from_str(&e.to_string())),
    }
}

#[wasm_bindgen]
pub fn encrypt_message(
    sending_handle: &str,
    session_id: &str,
    message: &str,
) -> Result<JsValue, JsValue> {
    let instances = INSTANCE_MAP.lock().unwrap();
    let mut instance = instances
        .get(sending_handle)
        .ok_or("sending_handle not found")?
        .borrow_mut();

    let result = instance.encrypt_message_serialized(session_id, message);

    result
        .map(|ciphertext| JsValue::from_str(&ciphertext))
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

#[wasm_bindgen]
pub fn decrypt_message(
    handle: &str,
    session_id: &str,
    ciphertext: &str,
) -> Result<JsValue, JsValue> {
    let instances = INSTANCE_MAP.lock().unwrap();
    let mut instance = instances
        .get(handle)
        .ok_or("handle not found")?
        .borrow_mut();

    let result = instance.decrypt_message_serialized(session_id, ciphertext);

    result
        .map(|plaintext| JsValue::from_str(&plaintext))
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

#[wasm_bindgen]
pub fn e2e_selftest() -> Result<bool, JsValue> {
    xmtpv3::manager::e2e_selftest()
        .map(|x| x == "Self test successful")
        .map_err(|e| JsValue::from_str(&e.to_string()))
}
