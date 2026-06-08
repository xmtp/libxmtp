use crate::config::Level;
use tracing_subscriber::reload;
use tracing_subscriber::{EnvFilter, Layer, Registry};

/// The native primary layer plus the reloadable filter handles for each native
/// layer. The Vec holds one handle per reloadable native layer (driven by
/// [`crate::LoggingHandle::set_native_level`]): one element on the non-mobile
/// server build (its stdout fmt-layer `EnvFilter`, set to `native_level` via
/// explicit per-crate directives), one element on iOS, and two on android
/// (logcat + AndroidTrace, each with its own cell).
pub(crate) type NativeLayer = (
    Box<dyn Layer<Registry> + Send + Sync>,
    Vec<reload::Handle<EnvFilter, Registry>>,
);

/// Server / non-mobile native layer: a compact stdout fmt layer whose only field
/// formatting concern is rendering the `message` field, with a per-crate
/// `EnvFilter` at `native_level`. Reloadable, so the handle Vec carries one
/// element driving `set_native_level`.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub(crate) fn native_layer(native_level: Level) -> NativeLayer {
    use tracing_subscriber::fmt::{self, format};
    // Per-crate directives (like the global filter + `set_native_level`) so
    // `native_level` actually narrows below `level`; a bare default would not.
    let filter = crate::filter::filter_directive(native_level.as_str());
    let (reloadable, handle) = reload::Layer::new(filter);
    let layer = fmt::layer()
        .compact()
        .fmt_fields(format::debug_fn(move |writer, field, value| {
            if field.name() == "message" {
                write!(writer, "{:?}", value)?;
            }
            Ok(())
        }))
        .with_filter(reloadable)
        .boxed();
    (layer, vec![handle])
}

/// Android native layer: `paranoid_android` logcat plus an `xmtp_api` system-trace
/// layer. Only logcat's filter is reloadable; the `AndroidTraceAsyncLayer` keeps a
/// fixed `xmtp_api=debug` filter because it `expect()`s a self-consistent span set,
/// which a wider filter would violate.
#[cfg(target_os = "android")]
pub(crate) fn native_layer(native_level: Level) -> NativeLayer {
    use tracing_subscriber::EnvFilter;

    let (logcat_filter, logcat_handle) =
        reload::Layer::new(crate::filter_directive(native_level.as_str()));
    let api_calls_filter = EnvFilter::builder().parse_lossy("xmtp_api=debug");
    let layers: Vec<Box<dyn Layer<Registry> + Send + Sync>> = vec![
        paranoid_android::layer(env!("CARGO_PKG_NAME"))
            .with_thread_names(true)
            .with_filter(logcat_filter)
            .boxed(),
        tracing_android_trace::AndroidTraceAsyncLayer::new()
            .with_filter(api_calls_filter)
            .boxed(),
    ];
    (layers.boxed(), vec![logcat_handle])
}

/// iOS native layer: os_log output via `tracing_oslog`, with activity spans
/// surfaced as os_signpost. The filter is reloadable via `set_native_level`.
#[cfg(target_os = "ios")]
pub(crate) fn native_layer(native_level: Level) -> NativeLayer {
    use tracing_oslog::OsLogger;
    let (libxmtp_filter, handle) =
        reload::Layer::new(crate::filter_directive(native_level.as_str()));
    let subsystem = format!("org.xmtp.{}", env!("CARGO_PKG_NAME"));
    let layer = OsLogger::new(subsystem, "default")
        .with_filter(libxmtp_filter)
        .boxed();
    (layer, vec![handle])
}
