use tracing_subscriber::Layer;
use tracing_subscriber::fmt;
use tracing_subscriber::registry::LookupSpan;

/// A stdout fmt layer: JSON (flattened) when `json`, else compact.
pub(crate) fn stdout_layer<S>(json: bool) -> Box<dyn Layer<S> + Send + Sync>
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    if json {
        fmt::layer()
            .json()
            .flatten_event(true)
            .with_level(true)
            .with_target(true)
            .boxed()
    } else {
        fmt::layer().boxed()
    }
}
