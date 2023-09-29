use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_ns() -> u64 {
    let now = SystemTime::now();
    // Allowing this to panic, since things have gone very wrong if this expect is hit
    let since_epoch = now.duration_since(UNIX_EPOCH).expect("Time went backwards");

    since_epoch.as_nanos() as u64
}
