//! Common Test Utilites
use rand::{
    distributions::{Alphanumeric, DistString},
    seq::IteratorRandom,
    Rng,
};
use std::sync::OnceLock;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use xmtp_cryptography::utils as crypto_utils;

#[cfg(not(target_arch = "wasm32"))]
pub mod traced_test;
#[cfg(not(target_arch = "wasm32"))]
pub use traced_test::TestWriter;

mod macros;

static INIT: OnceLock<()> = OnceLock::new();

/// A simple test logger that defaults to the INFO level
#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
pub fn logger() {
    use tracing_subscriber::fmt;
    INIT.get_or_init(|| {
        let filter = EnvFilter::builder()
            .with_default_directive(tracing::metadata::LevelFilter::INFO.into())
            .from_env_lossy();

        tracing_subscriber::registry()
            .with(fmt::layer())
            .with(filter)
            .init();
    });
}

/// A simple test logger that defaults to the INFO level
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
pub fn logger() {
    INIT.get_or_init(|| {
        let filter = EnvFilter::builder()
            .with_default_directive(tracing::metadata::LevelFilter::DEBUG.into())
            .from_env_lossy();

        tracing_subscriber::registry()
            .with(tracing_wasm::WASMLayer::default())
            .with(filter)
            .init();

        console_error_panic_hook::set_once();
    });
}

pub fn rand_hexstring() -> String {
    let mut rng = crypto_utils::rng();
    let hex_chars = "0123456789abcdef";
    let v: String = (0..40)
        .map(|_| hex_chars.chars().choose(&mut rng).unwrap())
        .collect();

    format!("0x{}", v)
}

pub fn rand_account_address() -> String {
    Alphanumeric.sample_string(&mut crypto_utils::rng(), 42)
}

pub fn rand_vec<const N: usize>() -> Vec<u8> {
    crate::rand_array::<N>().to_vec()
}

pub fn rand_u64() -> u64 {
    crypto_utils::rng().gen()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn tmp_path() -> String {
    let db_name = crate::rand_string::<24>();
    format!("{}/{}.db3", std::env::temp_dir().to_str().unwrap(), db_name)
}

#[cfg(target_arch = "wasm32")]
pub fn tmp_path() -> String {
    let db_name = crate::rand_string::<24>();
    format!("{}/{}.db3", "test_db", db_name)
}

pub fn rand_time() -> i64 {
    let mut rng = rand::thread_rng();
    rng.gen_range(0..1_000_000_000)
}
