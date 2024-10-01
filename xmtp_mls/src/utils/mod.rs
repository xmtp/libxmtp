#[cfg(feature = "bench")]
pub mod bench;
#[cfg(any(test, feature = "test-utils"))]
pub mod test;

pub mod hash {
    pub use xmtp_cryptography::hash::sha256_bytes as sha256;
}

pub mod time {
    use wasm_timer::{SystemTime, UNIX_EPOCH};

    pub const NS_IN_SEC: i64 = 1_000_000_000;

    pub fn now_ns() -> i64 {
        let now = SystemTime::now();

        now.duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_nanos() as i64
    }
}

pub mod id {
    /// Relies on a client-created idempotency_key (which could be a timestamp)
    pub fn calculate_message_id(
        group_id: &[u8],
        decrypted_message_bytes: &[u8],
        idempotency_key: &str,
    ) -> Vec<u8> {
        let separator = b"\t";
        let mut id_vec = Vec::new();
        id_vec.extend_from_slice(group_id);
        id_vec.extend_from_slice(separator);
        id_vec.extend_from_slice(idempotency_key.as_bytes());
        id_vec.extend_from_slice(separator);
        id_vec.extend_from_slice(decrypted_message_bytes);
        super::hash::sha256(&id_vec)
    }

    pub fn serialize_group_id(group_id: &[u8]) -> String {
        hex::encode(group_id)
    }
}

#[cfg(all(target_arch = "wasm32", test))]
pub mod wasm {
    use tokio::sync::OnceCell;
    static INIT: OnceCell<()> = OnceCell::const_new();

    /// can be used to debug wasm tests
    /// normal tracing logs are output to the browser console
    pub async fn init() {
        use web_sys::console;

        INIT.get_or_init(|| async {
            console::log_1(&"INIT".into());
            tracing_wasm::set_as_global_default();
            console_error_panic_hook::set_once();
            diesel_wasm_sqlite::init_sqlite().await;
        })
        .await;
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub mod wasm {
    pub async fn init() {}
}
