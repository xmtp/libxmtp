use base64::{engine::general_purpose, Engine as _};
use std::time::{SystemTime, UNIX_EPOCH};
use vodozemac::Curve25519PublicKey;
use xmtp_cryptography::hash::keccak256;

use xmtp_proto::xmtp::message_api::v1::Envelope;

pub fn get_current_time_ns() -> u64 {
    let now = SystemTime::now();
    // Allowing this to panic, since things have gone very wrong if this expect is hit
    let since_epoch = now.duration_since(UNIX_EPOCH).expect("Time went backwards");

    since_epoch.as_nanos() as u64
}

pub fn build_user_contact_topic(wallet_address: String) -> String {
    format!("/xmtp/3/contact-{}/proto", wallet_address)
}

pub fn build_user_invite_topic(public_key: String) -> String {
    format!("/xmtp/3/invite-{}/proto", public_key)
}

pub fn build_installation_message_topic(installation_id: &str) -> String {
    format!("/xmtp/3/message-{}/proto", installation_id)
}

pub fn build_envelope(content_topic: String, message: Vec<u8>) -> Envelope {
    Envelope {
        content_topic,
        message,
        timestamp_ns: get_current_time_ns(),
    }
}

pub fn base64_encode(bytes: &[u8]) -> String {
    general_purpose::STANDARD_NO_PAD.encode(bytes)
}

pub fn key_fingerprint(key: &Curve25519PublicKey) -> String {
    base64_encode(keccak256(key.to_string().as_str()).as_slice())
}

pub fn is_wallet_address(address: &str) -> bool {
    if !address.starts_with("0x") {
        return false;
    }

    if address.len() != 42 {
        return false;
    }

    if !address[2..].chars().all(|c| char::is_ascii_hexdigit(&c)) {
        return false;
    }
    true
}
