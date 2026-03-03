use log::{LevelFilter, Log, Metadata, Record, kv};
use std::io::{self, StdoutLock, Write};
use std::time;

/// Start logging.
pub fn start(level: LevelFilter) {
    let logger = Box::new(Logger {});
    log::set_boxed_logger(logger).expect("Could not start logging");
    log::set_max_level(level);
}

#[derive(Debug)]
pub(crate) struct Logger {}

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record<'_>) {
        if self.enabled(record.metadata()) {
            let stdout = io::stdout();
            let mut handle = stdout.lock();
            let level = get_level(record.level());
            let time = time::UNIX_EPOCH.elapsed().unwrap().as_millis();
            write!(
                &mut handle,
                "{{\"level\":{},\"time\":{},\"msg\":",
                level, time
            )
            .unwrap();
            serde_json::to_writer(&mut handle, record.args()).unwrap();
            format_kv_pairs(&mut handle, record);
            writeln!(&mut handle, "}}").unwrap();
        }
    }
    fn flush(&self) {}
}

fn get_level(level: log::Level) -> u8 {
    use log::Level::*;
    match level {
        Trace => 10,
        Debug => 20,
        Info => 30,
        Warn => 40,
        Error => 50,
    }
}

fn format_kv_pairs(out: &mut StdoutLock<'_>, record: &Record) {
    struct Visitor<'a, 'b> {
        string: &'a mut StdoutLock<'b>,
    }

    impl<'kvs, 'a, 'b> kv::Visitor<'kvs> for Visitor<'a, 'b> {
        fn visit_pair(
            &mut self,
            key: kv::Key<'kvs>,
            val: kv::Value<'kvs>,
        ) -> Result<(), kv::Error> {
            if let Ok(value_str) = serde_json::to_string(&val) {
                write!(self.string, ",\"{}\":{}", key, value_str)?;
            } else {
                write!(self.string, ",\"{}\":\"{}\"", key, val)?;
            }

            Ok(())
        }
    }

    let mut visitor = Visitor { string: out };
    record.key_values().visit(&mut visitor).unwrap();
}

pub fn make_value<ValueType>(value: &ValueType) -> log::kv::Value<'_>
where
    ValueType: serde::Serialize,
{
    log::kv::Value::from_serde(value)
}
