//! Common Test Utilites
use rand::{
    distributions::{Alphanumeric, DistString},
    seq::IteratorRandom,
    Rng,
};
use std::{future::Future, sync::OnceLock};
use xmtp_cryptography::utils as crypto_utils;

#[cfg(not(target_arch = "wasm32"))]
pub mod traced_test;
#[cfg(not(target_arch = "wasm32"))]
pub use traced_test::TestWriter;

use crate::time::Expired;

mod logger;
mod macros;

static INIT: OnceLock<()> = OnceLock::new();

#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
use tracing_subscriber::{
    fmt::{self, format},
    registry::LookupSpan,
    EnvFilter, Layer,
};

#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
pub fn logger_layer<S>() -> impl Layer<S>
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    let structured = std::env::var("STRUCTURED");
    let contextual = std::env::var("CONTEXTUAL");

    let is_structured = matches!(structured, Ok(s) if s == "true" || s == "1");
    let is_contextual = matches!(contextual, Ok(c) if c == "true" || c == "1");
    let filter = || {
        EnvFilter::builder()
            .with_default_directive(tracing::metadata::LevelFilter::INFO.into())
            .from_env_lossy()
    };

    vec![
        is_structured
            .then(|| {
                tracing_subscriber::fmt::layer()
                    .json()
                    .flatten_event(true)
                    .with_level(true)
                    .with_filter(filter())
            })
            .boxed(),
        is_contextual
            .then(|| {
                let processor =
                    tracing_forest::printer::Printer::new().formatter(logger::Contextual);
                tracing_forest::ForestLayer::new(processor, tracing_forest::tag::NoTag)
                    .with_filter(filter())
            })
            .boxed(),
        // default logger
        (!is_structured && !is_contextual)
            .then(|| {
                fmt::layer()
                    .compact()
                    .fmt_fields({
                        format::debug_fn(move |writer, field, value| {
                            if field.name() == "message" {
                                write!(writer, "{:?}", value)?;
                            }
                            Ok(())
                        })
                    })
                    .with_filter(filter())
            })
            .boxed(),
    ]
}

/// A simple test logger that defaults to the INFO level
#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
pub fn logger() {
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

    INIT.get_or_init(|| {
        let _ = tracing_subscriber::registry()
            .with(logger_layer())
            .try_init();
    });
}

/// A simple test logger that defaults to the INFO level
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
pub fn logger() {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::EnvFilter;

    INIT.get_or_init(|| {
        let filter = EnvFilter::builder().parse_lossy("xmtp_mls::subscriptions=debug");
        // .parse_lossy("xmtp_mls::subscriptions=TRACE,xmtp_api_http=TRACE,xmtp_common=TRACE,wasm_streams=TRACE,reqwest=TRACE");
        // .with_default_directive(tracing::metadata::LevelFilter::TRACE.into());

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

pub fn rand_i64() -> i64 {
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
                crate::yield_().await;
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
                crate::yield_().await;
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
                crate::yield_().await;
            }
        }
    })
    .await?;

    assert_eq!(expected, result);
    Ok(())
}
