use tracing_subscriber::Layer;
use tracing_subscriber::fmt;
use tracing_subscriber::registry::LookupSpan;

use crate::config::Level;
use crate::filter::filter_directive;

/// A stdout fmt layer: JSON (flattened) when `json`, else compact.
///
/// Filtered via `filter_directive` (explicit per-crate directives at
/// `stdout_level`) so it overrides the global per-crate filter and narrows
/// stdout below `level` — a bare default directive would not (INFO leaks).
pub(crate) fn stdout_layer<S>(json: bool, stdout_level: Level) -> Box<dyn Layer<S> + Send + Sync>
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    let filter = filter_directive(stdout_level.as_str());
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
