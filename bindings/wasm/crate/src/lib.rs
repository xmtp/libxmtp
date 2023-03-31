use std::collections::HashMap;
use std::sync::Mutex;

use js_sys::{Array, Uint8Array};
use serde_json::json;
use wasm_bindgen::prelude::*;

use xmtpv3::{VoodooInstance};

#[macro_use]
extern crate lazy_static;

// This whole strategy is to keep a singleton in WASM memory world
lazy_static! {
    static ref INSTANCE_MAP: Mutex<HashMap<String, VoodooInstance>> = Mutex::new(HashMap::new());
}

// Returns a handle to a keystore instance
#[wasm_bindgen]
pub fn new_voodoo_instance() -> String {
    let mut instances = INSTANCE_MAP.lock().unwrap();
    let handle = (instances.len() as u64).to_string();
    instances.insert(handle.clone(), VoodooInstance::new());
    return handle;
}

#[wasm_bindgen]
pub fn create_outbound_session(sending_handle: &str, receiving_handle: &str, message: &str) -> Result<String, JsValue> {
    // Look up both handles in INSTANCE_MAP
    let instances = INSTANCE_MAP.lock().unwrap();
    let sending_instance = instances.get_mut(sending_handle).ok_or("sending_handle not found")?;
    let receiving_instance = instances.get_mut(receiving_handle).ok_or("receiving_handle not found")?;

    // Create the session
    let session = sending_instance.create_outbound_session(receiving_instance, message).map_err(|e| JsValue::from_str(&e.to_string()))?;

    // Serialize the session
    let session_json = json!({
        "sending_handle": sending_handle,
        "receiving_handle": receiving_handle,
        "session": session,
    });
    let session_json_string = session_json.to_string();

    // Return the serialized session
    Ok(session_json_string)
}

#[wasm_bindgen]
pub fn create_inbound_session(sending_handle: &str, receiving_handle: &str, message: &str) -> Result<String, JsValue> {
    // Look up both handles in INSTANCE_MAP
    let instances = INSTANCE_MAP.lock().unwrap();
    let sending_instance = instances.get(sending_handle).ok_or("sending_handle not found")?;
    let receiving_instance = instances.get(receiving_handle).ok_or("receiving_handle not found")?;

    // Create the session
    let session = receiving_instance.create_inbound_session(sending_instance, message).map_err(|e| JsValue::from_str(&e.to_string()))?;

    // Serialize the session
    let session_json = json!({
        "sending_handle": sending_handle,
        "receiving_handle": receiving_handle,
        "session": session,
    });
    let session_json_string = session_json.to_string();

    // Return the serialized session
    Ok(session_json_string)
}

#[wasm_bindgen]
pub fn e2e_selftest() -> Result<bool, JsValue> {
    xmtpv3::e2e_selftest().map(|x| x == "Self test successful").map_err(|e| JsValue::from_str(&e.to_string()))
}
