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

// TODO: can't run a test like this with wasm targets
// #[cfg(test)]
// mod tests {
//     use crate::*;
//
//     #[test]
//     pub fn test_anything() {
//         // This test isn't perfect because we're utilizing a static instance but suffices for now
//         // Try creating two voodoo instances and doing a simple conversation
//         let alice_handle = new_voodoo_instance();
//         let bob_handle = new_voodoo_instance();
//
//         // Create a session from Alice to Bob
//         let alice_to_bob = create_outbound_session(&alice_handle, &bob_handle, "{}").unwrap();
//         let alice_to_bob: serde_json::Value = serde_json::from_str(&alice_to_bob).unwrap();
//         let alice_to_bob_session_id = alice_to_bob["session_id"].as_str().unwrap();
//         let alice_to_bob_ciphertext = alice_to_bob["ciphertext"].as_str().unwrap();
//
//         // Create a session from Bob to Alice
//         let bob_to_alice = create_inbound_session(&bob_handle, &alice_handle, alice_to_bob_ciphertext)
//             .unwrap();
//         let bob_to_alice: serde_json::Value = serde_json::from_str(&bob_to_alice).unwrap();
//         let bob_to_alice_session_id = bob_to_alice["session_id"].as_str().unwrap();
//         let bob_to_alice_plaintext = bob_to_alice["plaintext"].as_str().unwrap();
//
//         // Make sure the session ids match
//         assert_eq!(alice_to_bob_session_id, bob_to_alice_session_id);
//
//         // Make sure the plaintext matches
//         assert_eq!(bob_to_alice_plaintext, "{}");
//     }
// }
