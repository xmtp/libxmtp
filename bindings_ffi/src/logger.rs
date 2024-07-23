use log::{LevelFilter, Metadata, Record};
use std::sync::Once;

pub trait FfiLogger: Send + Sync {
    fn log(&self, level: u32, level_label: String, message: String);
}

struct RustLogger {
    logger: std::sync::Mutex<Box<dyn FfiLogger>>,
    #[cfg(feature = "sentry")]
    sentry: sentry::ClientInitGuard,
}

impl log::Log for RustLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            // TODO handle errors
            self.logger.lock().expect("Logger mutex is poisoned!").log(
                record.level() as u32,
                record.level().to_string(),
                format!("[libxmtp] {}", record.args()),
            );
        }
    }

    fn flush(&self) {}
}

static LOGGER_INIT: Once = Once::new();
#[cfg(not(feature = "sentry"))]
pub fn init_logger(logger: Box<dyn FfiLogger>) {
    // TODO handle errors
    LOGGER_INIT.call_once(|| {
        let logger = RustLogger {
            logger: std::sync::Mutex::new(logger),
        };
        log::set_boxed_logger(Box::new(logger)).expect("Failed to initialize logger");
        log::set_max_level(LevelFilter::Info);
        log::info!("Logger initialized");
    });
}

#[cfg(feature = "sentry")]
pub fn init_logger(logger: Box<dyn FfiLogger>) {
    LOGGER_INIT.call_once(|| {
        let _guard = sentry::init((
            "KEY_ENV_HERE",
            sentry::ClientOptions {
                release: sentry::release_name!(),
                ..Default::default()
            },
        ));

        let logger = sentry_log::SentryLogger::with_dest(RustLogger {
            logger: std::sync::Mutex::new(logger),
            sentry: _guard,
        });

        log::set_boxed_logger(Box::new(logger)).expect("Failed to initialize logger");
        log::set_max_level(LevelFilter::Info);
        log::info!("Sentry Logger initialized");
    });
}
