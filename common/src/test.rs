//! Common Test Utilites
use crate::time::Expired;
use rand::distributions::DistString;
use rand::{Rng, distributions::Alphanumeric, seq::IteratorRandom};
use std::collections::HashMap;
use std::sync::LazyLock;
use std::{future::Future, sync::OnceLock};
use tokio::sync;

mod macros;

mod openmls;
pub use openmls::*;

crate::if_native! {
    use parking_lot::Mutex;
    pub mod traced_test;
    pub use traced_test::TestWriter;
    mod logger;

    use once_cell::sync::Lazy;
    static REPLACE_IDS: Lazy<Mutex<HashMap<String, String>>> = Lazy::new(|| Mutex::new(HashMap::new()));
}

static INIT: OnceLock<()> = OnceLock::new();

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

/// Replace inbox id in Contextual output with a name (i.e Alix, Bo, etc.)
#[derive(Default)]
pub struct TestLogReplace {
    #[allow(unused)]
    ids: HashMap<String, String>,
}

impl TestLogReplace {
    pub fn add(&mut self, id: &str, name: &str) {
        crate::wasm_or_native! {
            wasm => { let _ = (id, name); },
            native => {
                self.ids.insert(id.to_string(), name.to_string());
                let mut ids = REPLACE_IDS.lock();
                ids.insert(id.to_string(), name.to_string());
            },
        }
    }
}

// remove ids for replacement from map on drop
impl Drop for TestLogReplace {
    fn drop(&mut self) {
        crate::wasm_or_native! {
            wasm => {},
            native => {
                let mut ids = REPLACE_IDS.lock();
                for id in self.ids.keys() {
                    let _ = ids.remove(id.as_str());
                }
            },
        }
    }
}

#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
pub fn logger_layer<S>() -> impl tracing_subscriber::Layer<S>
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    use tracing_subscriber::{
        EnvFilter, Layer,
        fmt::{self, format},
    };
    let structured = std::env::var("STRUCTURED");
    let contextual = std::env::var("CONTEXTUAL");
    let show_spans = std::env::var("SHOW_SPAN_FIELDS");

    let is_structured = matches!(structured, Ok(s) if s == "true" || s == "1");
    let is_contextual = matches!(contextual, Ok(c) if c == "true" || c == "1");
    let show_spans = matches!(show_spans, Ok(c) if c == "true" || c == "1");
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
                    .with_ansi(true)
                    .without_time()
                    .with_test_writer()
                    .fmt_fields({
                        format::debug_fn(move |writer, field, value| {
                            if show_spans && (field.name() != "message") {
                                write!(writer, ", {}={:?}", field.name(), value)?;
                            } else if field.name() == "message" {
                                let mut message = format!("{value:?}");
                                let ids = REPLACE_IDS.lock();
                                for (id, name) in ids.iter() {
                                    message = message.replace(id, name);
                                    message = message.replace(&crate::fmt::truncate_hex(id), name);
                                }
                                write!(writer, "{message}")?;
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
pub fn logger() {
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

    crate::wasm_or_native! {
        wasm => {
            INIT.get_or_init(|| {
                let filter = tracing_subscriber::EnvFilter::builder().parse("debug").unwrap();

                tracing_subscriber::registry()
                    .with(tracing_wasm::WASMLayer::default())
                    .with(filter)
                    .init();

                console_error_panic_hook::set_once();
                wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);
            });
        },
        native => {
            INIT.get_or_init(|| {
                let _ = tracing_subscriber::registry()
                    .with(logger_layer())
                    .try_init();
            });
        },
    }
}

// Execute once before any tests are run
#[cfg(all(test, not(target_arch = "wasm32"), feature = "test-utils"))]
#[ctor::ctor]
fn ctor_logging_setup() {
    crate::logger();
    let _ = fdlimit::raise_fd_limit();
}

// must be in an arc so we only ever have one subscriber
crate::if_native! {
    static SCOPED_SUBSCRIBER: LazyLock<std::sync::Arc<Box<dyn tracing::Subscriber + Send + Sync>>> =
        LazyLock::new(|| {
            use tracing_subscriber::layer::SubscriberExt;

            std::sync::Arc::new(Box::new(
                tracing_subscriber::registry().with(logger_layer()),
            ))
        });

    pub fn subscriber() -> impl tracing::Subscriber {
        (*SCOPED_SUBSCRIBER).clone()
    }
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
    crate::rng().r#gen()
}

pub fn rand_i64() -> i64 {
    crate::rng().r#gen()
}

pub fn tmp_path() -> String {
    let db_name = crate::rand_string::<24>();
    crate::wasm_or_native_expr! {
        native => format!("{}/{db_name}.db3", std::env::temp_dir().to_str().unwrap()),
        wasm => format!("test_db/{db_name}.db3"),
    }
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
