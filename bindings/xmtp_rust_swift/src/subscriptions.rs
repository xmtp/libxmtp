use lazy_static::lazy_static;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use xmtp_networking::grpc_api_helper::Subscription;
use xmtp_proto::xmtp::message_api::v1::Envelope;

// Define the global HashMap for storing Subscriptions
lazy_static! {
    static ref SUBSCRIPTIONS: Mutex<HashMap<String, Arc<Subscription>>> =
        Mutex::new(HashMap::new());
}

// Function to add a subscription to the global HashMap
pub fn add_subscription(id: String, sub: Subscription) {
    let mut subscriptions = SUBSCRIPTIONS.lock().unwrap();
    subscriptions.insert(id, Arc::new(sub));
}

// Function to get a subscription by ID
pub fn get_messages(id: String) -> Option<Vec<Envelope>> {
    let subscriptions = SUBSCRIPTIONS.lock().unwrap();
    let sub = subscriptions.get(&id);
    let new_messages = sub?.get_and_reset_pending();

    if !new_messages.is_empty() {
        return Some(new_messages);
    }

    None
}
