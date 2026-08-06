use tracing_subscriber::Layer;
use tracing_subscriber::registry::LookupSpan;
use tracing_web::{MakeWebConsoleWriter, performance_layer};

// These return concrete `impl Layer<S>` rather than boxed trait objects. The
// browser layers are not `Send + Sync` (wasm is single-threaded and they hold
// `JsValue`s), and `tracing-subscriber` only implements `Layer` for
// `Box<dyn Layer + Send + Sync>`. Keeping them unboxed lets the wasm subscriber
// chain them with `.with(..)` without ever needing the `Send + Sync` bound.

/// Browser console log layer (no ANSI, no timestamps — the console adds its own).
pub(crate) fn console_layer<S>() -> impl Layer<S>
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .without_time()
        .with_writer(MakeWebConsoleWriter::new())
}

/// Browser performance-timeline layer (User Timing API marks/measures).
pub(crate) fn perf_layer<S>() -> impl Layer<S>
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    use tracing_subscriber::fmt::format::Pretty;
    performance_layer().with_details_from_fields(Pretty::default())
}
