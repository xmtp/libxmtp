use tracing_subscriber::Layer;
use tracing_subscriber::registry::LookupSpan;
use tracing_web::{MakeWebConsoleWriter, performance_layer};

/// Browser console log layer (no ANSI, no timestamps — the console adds its own).
pub(crate) fn console_layer<S>() -> Box<dyn Layer<S>>
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .without_time()
        .with_writer(MakeWebConsoleWriter::new())
        .boxed()
}

/// Browser performance-timeline layer (User Timing API marks/measures).
pub(crate) fn perf_layer<S>() -> Box<dyn Layer<S>>
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    use tracing_subscriber::fmt::format::Pretty;
    performance_layer()
        .with_details_from_fields(Pretty::default())
        .boxed()
}
