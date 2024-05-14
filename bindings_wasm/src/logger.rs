// use log::{LevelFilter, Metadata, Record};
// use std::sync::Once;

// pub trait WasmLoggerT: Send + Sync {
//     fn log(&self, level: u32, level_label: String, message: String);
// }

// pub struct WasmLogger;

// impl WasmLogger {
//     pub fn log(level: u32, level_label: &str, message: &str) {
//         // JavaScript console logging could be used directly or mapped here.
//         log2(&format!("{}: {} - {}", level, level_label, message));
//     }
// }

// struct RustLogger {
//     logger: std::sync::Mutex<Box<dyn WasmLogger>>,
// }

// impl log::Log for RustLogger {
//     fn enabled(&self, _metadata: &Metadata) -> bool {
//         true
//     }

//     fn log(&self, record: &Record) {
//         if self.enabled(record.metadata()) {
//             // TODO handle errors
//             self.logger.lock().expect("Logger mutex is poisoned!").log(
//                 record.level() as u32,
//                 record.level().to_string(),
//                 format!("[libxmtp] {}", record.args()),
//             );
//         }
//     }

//     fn flush(&self) {}
// }

// static LOGGER_INIT: Once = Once::new();
// pub fn init_logger(logger: Box<dyn WasmLogger>) {
//     // TODO handle errors
//     LOGGER_INIT.call_once(|| {
//         let logger = RustLogger {
//             logger: std::sync::Mutex::new(logger),
//         };
//         log::set_boxed_logger(Box::new(logger))
//             .map(|()| log::set_max_level(LevelFilter::Info))
//             .expect("Failed to initialize logger");
//         log::info!("Logger initialized");
//     });
// }
