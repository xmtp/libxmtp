use wasm_timer::{SystemTime, UNIX_EPOCH};

pub const NS_IN_SEC: i64 = 1_000_000_000;

pub fn now_ns() -> i64 {
    let now = SystemTime::now();

    now.duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_nanos() as i64
}
