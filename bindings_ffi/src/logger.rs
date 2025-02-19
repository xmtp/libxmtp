use log::Subscriber;
use std::sync::Once;
use tracing_subscriber::{
    layer::SubscriberExt, registry::LookupSpan, util::SubscriberInitExt, Layer,
};

#[cfg(target_os = "android")]
pub use android::*;
#[cfg(target_os = "android")]
mod android {
    use super::*;
    pub fn native_layer<S>() -> impl Layer<S>
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
    {
        let api_calls_filter = EnvFilter::builder().parse_lossy("xmtp_api=debug");
        vec![
            paranoid_android::layer(env!("CARGO_PKG_NAME"))
                .with_thread_names(true)
                .with_filter(tracing_subscriber::filter::LevelFilter::DEBUG),
            tracing_android_trace::AndroidTraceAsyncLayer::new().with_filter(api_calls_filter),
        ]
    }
}

#[cfg(target_os = "ios")]
pub use ios::*;
#[cfg(target_os = "ios")]
mod ios {
    use super::*;
    // use tracing_subscriber::Layer;
    pub fn native_layer<S>() -> impl Layer<S>
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
    {
        use tracing_oslog::OsLogger;
        let subsystem = format!("org.xmtp.{}", env!("CARGO_PKG_NAME"));
        OsLogger::new(subsystem, "default")
            .with_filter(tracing_subscriber::filter::LevelFilter::DEBUG)
    }
}

// production logger for anything not ios/android mobile
#[cfg(not(any(target_os = "ios", target_os = "android", test)))]
pub use other::*;
#[cfg(not(any(target_os = "ios", target_os = "android", test)))]
mod other {
    use super::*;

    pub fn native_layer<S>() -> impl Layer<S>
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
    {
        use tracing_subscriber::{
            fmt::{self, format},
            EnvFilter, Layer,
        };
        let filter = EnvFilter::builder()
            .with_default_directive(tracing::metadata::LevelFilter::INFO.into())
            .from_env_lossy();
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
            .with_filter(filter)
    }
}

static LOGGER_INIT: Once = Once::new();
pub fn init_logger() {
    LOGGER_INIT.call_once(|| {
        let native_layer = native_layer();
        let _ = tracing_subscriber::registry().with(native_layer).try_init();
    });
}

#[cfg(test)]
pub use test_logger::*;

#[cfg(test)]
mod test_logger {
    use super::*;

    pub fn native_layer<S>() -> impl Layer<S>
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
    {
        xmtp_common::logger_layer()
    }
}
