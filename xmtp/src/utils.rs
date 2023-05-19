use std::time::{SystemTime, UNIX_EPOCH};

use xmtp_proto::xmtp::message_api::v1::Envelope;

pub fn get_current_time_ns() -> u64 {
    let now = SystemTime::now();
    // Allowing this to panic, since things have gone very wrong if this expect is hit
    let since_epoch = now.duration_since(UNIX_EPOCH).expect("Time went backwards");
    let nanos = since_epoch.as_nanos() as u64;

    nanos
}

pub fn build_user_contact_topic(wallet_address: String) -> String {
    format!("xmtp/1/contact-{}", wallet_address)
}

pub fn build_envelope(content_topic: String, message: Vec<u8>) -> Envelope {
    Envelope {
        content_topic,
        message,
        timestamp_ns: get_current_time_ns(),
    }
}
