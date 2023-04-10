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

// Returns a handle to a voodoo instance
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
    let instances = INSTANCE_MAP.lock().map_err(|e| {
        JsValue::from_str(&format!("Error getting instance map lock: {}", e))
    })?;
    let mut sending_instance = instances
        .get(sending_handle)
        .ok_or("sending_handle not found")?
        .borrow_mut();

    let receiving_instance = instances
        .get(receiving_handle)
        .ok_or("receiving_handle not found")?
        .borrow();

    // Get other party's public situation
    let contact_bundle = receiving_instance.next_contact_bundle();

    // Create the session
    let result = sending_instance.create_outbound_session_serialized(&contact_bundle, message);

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
    let instances = INSTANCE_MAP.lock().map_err(|e| {
        JsValue::from_str(&format!("Error getting instance map lock: {}", e))
    })?;
    let sending_instance = instances
        .get(sending_handle)
        .ok_or("sending_handle not found")?
        .borrow();
    let mut receiving_instance = instances
        .get(receiving_handle)
        .ok_or("receiving_handle not found")?
        .borrow_mut();

    // Create the session
    let result = receiving_instance.create_inbound_session_serialized(sending_instance.identity_key(), message);

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
    let instances = INSTANCE_MAP.lock().map_err(|e| {
        JsValue::from_str(&format!("Error getting instance map lock: {}", e))
    })?;

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
    let instances = INSTANCE_MAP.lock().map_err(|e| {
        JsValue::from_str(&format!("Error getting instance map lock: {}", e))
    })?;
    
    let mut instance = instances
        .get(handle)
        .ok_or("handle not found")?
        .borrow_mut();

    let result = instance.decrypt_message_serialized(session_id, ciphertext);

    result
        .map(|plaintext| JsValue::from_str(&plaintext))
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

// This function is the most barebones (incorrect) implementation of obtaining
// a "contact bundle" for a given VoodooIdentity by handle. The output string
// is a JSON object which can subsequently be imported into another VoodooInstance
#[wasm_bindgen]
pub fn get_public_account_json(handle: &str) -> Result<String, JsValue> {
    let instances = INSTANCE_MAP.lock().map_err(|e| {
        JsValue::from_str(&format!("Error getting instance map lock: {}", e))
    })?;
    let instance = instances
        .get(handle)
        .ok_or("handle not found")?
        .borrow_mut();
    instance.public_account_json().map_err(|e| {
        JsValue::from_str(&format!("Error getting public account json: {}", e))
    })
}

// This function takes a public account json (as returned by get_public_account_json)
// and creates a new Voodoo instance referenced by the returned handle. TODO: there is
// no distinction between "public" and "private" accounts in this implementation yet.
#[wasm_bindgen]
pub fn add_or_get_public_account_from_json(public_account_json: &str) -> Result<String, JsValue> {
    let mut instances = INSTANCE_MAP.lock().map_err(|e| {
        JsValue::from_str(&format!("Error getting instance map lock: {}", e))
    })?;

    let public_instance = VoodooInstance::from_public_account_json(public_account_json).map_err(|e| {
        JsValue::from_str(&format!("Error creating VoodooInstance from public account json: {}", e))
    })?;
    // First, check if we have this account already
    // TODO: this is a linear search, which is not ideal, but we're reworking the entire contact
    // system anyways, so this is fine for now.
    for (handle, instance) in instances.iter() {
        // NOTE: this uses PartialEq trait which relies on identity_key() comparison (Eq is a
        // derived trait for identity_keys)
        if instance.borrow_mut().account.identity_keys() == public_instance.account.identity_keys() {
            return Ok(handle.clone());
        }
    }

    let handle = (instances.len() as u64).to_string();
    instances.insert(handle.clone(), RefCell::new(public_instance));
    Ok(handle)
}

#[wasm_bindgen]
pub fn e2e_selftest() -> Result<bool, JsValue> {
    xmtpv3::manager::e2e_selftest()
        .map(|x| x == "Self test successful")
        .map_err(|e| JsValue::from_str(&e.to_string()))
}
