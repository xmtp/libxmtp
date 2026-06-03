use tracing_subscriber::Layer;
use tracing_subscriber::registry::LookupSpan;

/// Server / non-mobile native layer: a compact stdout fmt layer whose only field
/// formatting concern is rendering the `message` field, with an `EnvFilter`
/// defaulting to INFO.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub(crate) fn native_layer<S>() -> Box<dyn Layer<S> + Send + Sync>
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    use tracing_subscriber::{
        EnvFilter,
        fmt::{self, format},
    };
    let filter = EnvFilter::builder()
        .with_default_directive(tracing::metadata::LevelFilter::INFO.into())
        .from_env_lossy();
    fmt::layer()
        .compact()
        .fmt_fields(format::debug_fn(move |writer, field, value| {
            if field.name() == "message" {
                write!(writer, "{:?}", value)?;
            }
            Ok(())
        }))
        .with_filter(filter)
        .boxed()
}

/// Android native layer: logcat output via `paranoid_android` plus
/// `xmtp_api` activity traces routed to the Android system trace buffer.
#[cfg(target_os = "android")]
pub(crate) fn native_layer<S>() -> Box<dyn Layer<S> + Send + Sync>
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    use tracing_subscriber::EnvFilter;

    let api_calls_filter = EnvFilter::builder().parse_lossy("xmtp_api=debug");
    let libxmtp_filter = crate::filter_directive("info");

    let layers: Vec<Box<dyn Layer<S> + Send + Sync>> = vec![
        paranoid_android::layer(env!("CARGO_PKG_NAME"))
            .with_thread_names(true)
            .with_filter(libxmtp_filter)
            .boxed(),
        tracing_android_trace::AndroidTraceAsyncLayer::new()
            .with_filter(api_calls_filter)
            .boxed(),
    ];
    layers.boxed()
}

/// iOS native layer: os_log output via `tracing_oslog`, with activity spans
/// surfaced as os_signpost.
#[cfg(target_os = "ios")]
pub(crate) fn native_layer<S>() -> Box<dyn Layer<S> + Send + Sync>
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    use tracing_oslog::OsLogger;

    let libxmtp_filter = crate::filter_directive("info");
    let subsystem = format!("org.xmtp.{}", env!("CARGO_PKG_NAME"));
    OsLogger::new(subsystem, "default")
        .with_filter(libxmtp_filter)
        .boxed()
}
