use tracing_subscriber::Layer;
use tracing_subscriber::fmt;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{EnvFilter, filter::LevelFilter};

use crate::config::Level;

/// A stdout fmt layer: JSON (flattened) when `json`, else compact.
///
/// The layer carries its own per-layer `EnvFilter` defaulting to `stdout_level`
/// (RUST_LOG still overrides), so stdout can be quieted independently of the
/// global `level` — e.g. `level = Info` exports INFO+ to OTLP while
/// `stdout_level = Warn` keeps the console/log-shipper at WARN+ (no duplicate of
/// the OTLP stream). The per-layer filter narrows under the global filter.
pub(crate) fn stdout_layer<S>(json: bool, stdout_level: Level) -> Box<dyn Layer<S> + Send + Sync>
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    let filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::from(stdout_level).into())
        .from_env_lossy();
    if json {
        fmt::layer()
            .json()
            .flatten_event(true)
            .with_level(true)
            .with_target(true)
            .with_filter(filter)
            .boxed()
    } else {
        fmt::layer().with_filter(filter).boxed()
    }
}
