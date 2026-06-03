//! Common Test Utilities
use crate::time::Expired;
use rand::distr::SampleString;
use rand::{RngExt, distr::Alphanumeric, seq::IteratorRandom};
use std::future::Future;
use std::sync::LazyLock;
use tokio::sync;

mod macros;

mod openmls;
pub use openmls::*;

crate::if_native! {
    pub mod traced_test;
    pub use traced_test::TestWriter;
}

use toxiproxy_rust::TOXIPROXY;

static TOXIPROXY_TEST_LOCK: LazyLock<sync::Mutex<()>> = LazyLock::new(|| sync::Mutex::new(()));

// TODO: can add this to the macro
pub async fn toxiproxy_test<T, F: AsyncFn() -> T>(f: F) -> T {
    let _g = TOXIPROXY_TEST_LOCK.lock().await;
    TOXIPROXY.reset().await.unwrap();
    f().await
}

pub trait Generate {
    /// generate a struct containing random data
    fn generate() -> Self;
}

/// A simple test logger that defaults to the INFO level.
///
/// Delegates to [`xmtp_logging::logger`]; the test subscriber itself now lives
/// in the `xmtp_logging` crate (under its `test-utils` feature).
pub fn logger() {
    xmtp_logging::logger()
}

// Execute once before any tests are run
#[cfg(all(test, not(target_arch = "wasm32"), feature = "test-utils"))]
#[ctor::ctor]
fn ctor_logging_setup() {
    crate::logger();
    let _ = fdlimit::raise_fd_limit();
}

pub fn rand_hexstring() -> String {
    let mut rng = crate::rng();
    let hex_chars = "0123456789abcdef";
    let v: String = (0..40)
        .map(|_| hex_chars.chars().choose(&mut rng).unwrap())
        .collect();

    format!("0x{v}")
}

pub fn rand_account_address() -> String {
    Alphanumeric.sample_string(&mut crate::rng(), 42)
}

pub fn rand_u64() -> u64 {
    crate::rng().random()
}

pub fn rand_i64() -> i64 {
    crate::rng().random()
}

pub fn tmp_path() -> String {
    let db_name = crate::rand_string::<24>();
    crate::wasm_or_native_expr! {
        native => format!("{}/{db_name}.db3", std::env::temp_dir().to_str().unwrap()),
        wasm => format!("test_db/{db_name}.db3"),
    }
}

pub fn rand_time() -> i64 {
    let mut rng = rand::rng();
    rng.random_range(0..1_000_000_000)
}

pub async fn wait_for_some<F, Fut, T>(f: F) -> Option<T>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Option<T>>,
{
    crate::time::timeout(crate::time::Duration::from_secs(20), async {
        loop {
            if let Some(r) = f().await {
                return r;
            } else {
                crate::task::yield_now().await;
            }
        }
    })
    .await
    .ok()
}

pub async fn wait_for_ok<F, Fut, T, E>(f: F) -> Result<T, Expired>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    crate::time::timeout(crate::time::Duration::from_secs(20), async {
        loop {
            if let Ok(r) = f().await {
                return r;
            } else {
                crate::task::yield_now().await;
            }
        }
    })
    .await
}

pub async fn wait_for_eq<F, Fut, T>(f: F, expected: T) -> Result<(), Expired>
where
    F: Fn() -> Fut,
    Fut: Future<Output = T>,
    T: std::fmt::Debug + PartialEq,
{
    let result = crate::time::timeout(crate::time::Duration::from_secs(20), async {
        loop {
            let result = f().await;
            if expected == result {
                return result;
            } else {
                crate::task::yield_now().await;
            }
        }
    })
    .await?;

    assert_eq!(expected, result);
    Ok(())
}

pub async fn wait_for_ge<F, Fut, T>(f: F, expected: T) -> Result<(), Expired>
where
    F: Fn() -> Fut,
    Fut: Future<Output = T>,
    T: std::fmt::Debug + PartialEq + PartialOrd,
{
    crate::time::timeout(crate::time::Duration::from_secs(20), async {
        loop {
            let result = f().await;
            if result >= expected {
                return result;
            } else {
                crate::task::yield_now().await;
            }
        }
    })
    .await?;

    Ok(())
}

/// Extension trait for formatting collections of Debug items in tests
pub trait DebugDisplay {
    /// Format items as debug output, one per line
    fn format_list(&self) -> String;

    /// Format items with enumeration (index -- item)
    fn format_enumerated(&self) -> String;
}

impl<T: std::fmt::Debug> DebugDisplay for [T] {
    fn format_list(&self) -> String {
        self.iter()
            .map(|item| format!("{:?}", item))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn format_enumerated(&self) -> String {
        self.iter()
            .enumerate()
            .map(|(i, item)| format!("{} -- {:?}", i, item))
            .collect::<Vec<_>>()
            .join("\n")
    }
}
