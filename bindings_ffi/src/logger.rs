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
        paranoid_android::layer(env!("CARGO_PKG_NAME"))
            .with_thread_names(true)
            .with_filter(tracing_subscriber::filter::LevelFilter::DEBUG)
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

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use other::*;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod other {
    use super::*;

    pub fn native_layer<S>() -> impl Layer<S>
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
    {
        xmtp_common::logger_layer()
    }
}

static LOGGER_INIT: Once = Once::new();
pub fn init_logger() {
    LOGGER_INIT.call_once(|| {
        let native_layer = native_layer();
        let _ = tracing_subscriber::registry().with(native_layer).try_init();
    });
}
