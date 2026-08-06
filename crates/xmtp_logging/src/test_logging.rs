//! Simplified test subscriber for libxmtp.
//!
//! This is the test/dev logger installed by the `#[xmtp_common::test]` macro
//! (which now delegates to [`logger`]). It intentionally has no dependency on
//! `xmtp_common` so that `xmtp_common[test-utils] -> xmtp_logging[test-utils]`
//! is a one-way dev edge with no build cycle.
//!
//! Behavior is controlled by environment variables:
//! - `STRUCTURED=true|1` — emit JSON logs.
//! - `SHOW_SPAN_FIELDS=true|1` — include span/event fields (other than the
//!   message) in the default compact human output.
//! - otherwise — a compact, human-readable, ANSI-colored layer.
//!
//! The env-filter defaults to `INFO` and is read from `RUST_LOG` via
//! [`tracing_subscriber::EnvFilter::from_env`].

use std::sync::OnceLock;

static INIT: OnceLock<()> = OnceLock::new();

/// Build the test logging layer(s).
///
/// Honors `STRUCTURED` (json) and `SHOW_SPAN_FIELDS`; otherwise emits a compact
/// human-readable layer. The env-filter defaults to `INFO`.
#[cfg(not(target_arch = "wasm32"))]
pub fn logger_layer<S>() -> impl tracing_subscriber::Layer<S>
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    use tracing_subscriber::{
        EnvFilter, Layer,
        fmt::{self, format},
    };

    let structured = std::env::var("STRUCTURED");
    let show_spans = std::env::var("SHOW_SPAN_FIELDS");

    let is_structured = matches!(structured, Ok(s) if s == "true" || s == "1");
    let show_spans = matches!(show_spans, Ok(c) if c == "true" || c == "1");

    let filter = || {
        EnvFilter::builder()
            .with_default_directive(tracing::metadata::LevelFilter::INFO.into())
            .from_env()
            .expect("invalid environment log filter")
    };

    vec![
        is_structured
            .then(|| {
                tracing_subscriber::fmt::layer()
                    .json()
                    .with_filter(filter())
            })
            .boxed(),
        // default logger
        (!is_structured)
            .then(|| {
                fmt::layer()
                    .compact()
                    .with_ansi(true)
                    .with_file(true)
                    .with_line_number(true)
                    .with_target(false)
                    .with_test_writer()
                    .fmt_fields({
                        format::debug_fn(move |writer, field, value| {
                            if show_spans && (field.name() != "message") {
                                write!(writer, ", {}={:?}", field.name(), value)?;
                            } else if field.name() == "message" {
                                write!(writer, "{value:?}")?;
                            }
                            Ok(())
                        })
                    })
                    .with_filter(filter())
            })
            .boxed(),
    ]
}

/// A simple test logger that defaults to the INFO level.
///
/// Installs the test subscriber exactly once for the lifetime of the process;
/// subsequent calls are no-ops.
pub fn logger() {
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

    #[cfg(not(target_arch = "wasm32"))]
    {
        INIT.get_or_init(|| {
            let _ = tracing_subscriber::registry()
                .with(logger_layer())
                .try_init();
        });
    }

    #[cfg(target_arch = "wasm32")]
    {
        INIT.get_or_init(|| {
            let filter = tracing_subscriber::EnvFilter::builder()
                .with_default_directive(tracing::metadata::LevelFilter::DEBUG.into())
                .from_env()
                .expect("invalid environment log filter");

            let _ = tracing_subscriber::registry()
                .with(crate::layers::web::console_layer())
                .with(filter)
                .try_init();

            console_error_panic_hook::set_once();
        });
    }
}
