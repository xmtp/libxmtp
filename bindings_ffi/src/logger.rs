use log::{LevelFilter, Metadata, Record};
use std::sync::Once;

pub trait FfiLogger: Send + Sync {
    fn log(&self, level: u32, level_label: String, message: String);
}

struct RustLogger {
    logger: std::sync::Mutex<Box<dyn FfiLogger>>,
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
                format!("[libxmtp][t:{}] {}", thread_id::get(), record.args()),
            );
        }
    }

    fn flush(&self) {}
}

static LOGGER_INIT: Once = Once::new();
pub fn init_logger(logger: Box<dyn FfiLogger>) {
    // TODO handle errors
    LOGGER_INIT.call_once(|| {
        let logger = RustLogger {
            logger: std::sync::Mutex::new(logger),
        };
        log::set_boxed_logger(Box::new(logger))
            .map(|()| log::set_max_level(LevelFilter::Info))
            .expect("Failed to initialize logger");
        log::info!("Logger initialized");
    });
}
