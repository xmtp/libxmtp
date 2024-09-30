use log::{LevelFilter, Metadata, Record};
use std::sync::Once;

pub trait FfiLogger: Send + Sync {
    fn log(&self, level: u32, level_label: String, message: String);
}

struct RustLogger {
    logger: parking_lot::Mutex<Box<dyn FfiLogger>>,
}

impl log::Log for RustLogger {
    f
    n enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            self.logger.lock().log(
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
    LOGGER_INIT.call_once(|| {
        let logger = RustLogger {
            logger: parking_lot::Mutex::new(logger),
        };
        log::set_boxed_logger(Box::new(logger))
            .map(|()| log::set_max_level(LevelFilter::Info))
            .expect("Failed to initialize logger");
        log::info!("Logger initialized");
    });
}
