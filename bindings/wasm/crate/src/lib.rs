use std::cell::Cell;
use std::collections::HashMap;
use std::sync::Mutex;

use serde_json::json;
use wasm_bindgen::prelude::*;

use xmtpv3::VoodooInstance;

#[macro_use]
extern crate lazy_static;

// This whole strategy is to keep a singleton in WASM memory world
lazy_static! {
    static ref INSTANCE_MAP: Mutex<HashMap<String, Cell<VoodooInstance>>> =
        Mutex::new(HashMap::new());
}

// Returns a handle to a keystore instance
#[wasm_bindgen]
pub fn new_voodoo_instance() -> String {
    let mut instances = INSTANCE_MAP.lock().unwrap();
    let handle = (instances.len() as u64).to_string();
    instances.insert(handle.clone(), Cell::new(VoodooInstance::new()));
    handle
}

#[wasm_bindgen]
pub fn create_outbound_session(
    sending_handle: &str,
    receiving_handle: &str,
    message: &str,
) -> Result<String, JsValue> {
    // Look up both handles in INSTANCE_MAP
    let instances = INSTANCE_MAP.lock().unwrap();
    let mut sending_instance = instances
        .get(sending_handle)
        .ok_or("sending_handle not found")?
        .take();
    let mut receiving_instance = instances
        .get(receiving_handle)
        .ok_or("receiving_handle not found")?
        .take();

    // Create the session
    let result = sending_instance
        .create_outbound_session_serialized(&mut receiving_instance.account, message);

    // Put the sending_instance and receiving_instance back, we know the cells exist
    instances.get(sending_handle).unwrap().set(sending_instance);
    instances
        .get(receiving_handle)
        .unwrap()
        .set(receiving_instance);
    match result {
        Ok((session_id, ciphertext_json)) => Ok(json!({
            "session_id": session_id,
            "ciphertext": ciphertext_json
        })
        .to_string()),
        Err(e) => Err(JsValue::from_str(&e.to_string())),
    }
}

#[wasm_bindgen]
pub fn create_inbound_session(
    sending_handle: &str,
    receiving_handle: &str,
    message: &str,
) -> Result<String, JsValue> {
    // Look up both handles in INSTANCE_MAP
    let instances = INSTANCE_MAP.lock().unwrap();
    let mut sending_instance = instances
        .get(sending_handle)
        .ok_or("sending_handle not found")?
        .take();
    let mut receiving_instance = instances
        .get(receiving_handle)
        .ok_or("receiving_handle not found")?
        .take();

    // Create the session
    let result = receiving_instance
        .create_inbound_session_serialized(&mut sending_instance.account, message);

    // Put the instances back in their cells
    instances.get(sending_handle).unwrap().set(sending_instance);
    instances
        .get(receiving_handle)
        .unwrap()
        .set(receiving_instance);

    match result {
        Ok((session_id, plaintext)) => Ok(json!({
            "session_id": session_id,
            "plaintext": plaintext
        })
        .to_string()),
        Err(e) => Err(JsValue::from_str(&e.to_string())),
    }
}

#[wasm_bindgen]
pub fn e2e_selftest() -> Result<bool, JsValue> {
    xmtpv3::e2e_selftest()
        .map(|x| x == "Self test successful")
        .map_err(|e| JsValue::from_str(&e.to_string()))
}
