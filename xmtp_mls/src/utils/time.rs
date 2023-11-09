use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_ns() -> u64 {
    let now = SystemTime::now();

    now.duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_nanos() as u64
}
