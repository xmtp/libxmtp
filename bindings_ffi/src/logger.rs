use crate::GenericError;
use log::level_filters::LevelFilter;
use log::Subscriber;
use parking_lot::Mutex;
use std::io::Write;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, LazyLock, OnceLock,
};
use tracing_appender::non_blocking::NonBlockingBuilder;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::RollingFileAppender;
use tracing_subscriber::fmt::format::DefaultFields;
use tracing_subscriber::fmt::format::Format;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::{
    filter::Filtered, fmt, layer::Layered, layer::SubscriberExt, registry::LookupSpan,
    registry::Registry, reload, util::SubscriberInitExt, EnvFilter, Layer,
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
        use tracing_subscriber::EnvFilter;
        let api_calls_filter = EnvFilter::builder().parse_lossy("xmtp_api=debug");
        let libxmtp_filter = xmtp_common::filter_directive("debug");

        vec![
            paranoid_android::layer(env!("CARGO_PKG_NAME"))
                .with_thread_names(true)
                .with_filter(libxmtp_filter)
                .boxed(),
            tracing_android_trace::AndroidTraceAsyncLayer::new()
                .with_filter(api_calls_filter)
                .boxed(),
        ]
    }
}

#[cfg(target_os = "ios")]
pub use ios::*;
#[cfg(target_os = "ios")]
mod ios {
    use super::*;
    use tracing_oslog::OsLogger;
    use tracing_subscriber::EnvFilter;

    pub fn native_layer<S>() -> impl Layer<S>
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
    {
        let libxmtp_filter = xmtp_common::filter_directive("debug");
        let subsystem = format!("org.xmtp.{}", env!("CARGO_PKG_NAME"));
        OsLogger::new(subsystem, "default").with_filter(libxmtp_filter)
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

enum EmptyOrFileWriter {
    Empty,
    File(tracing_appender::non_blocking::NonBlocking),
}

impl Default for EmptyOrFileWriter {
    fn default() -> Self {
        Self::Empty
    }
}

impl Write for EmptyOrFileWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Empty => Ok(buf.len()),
            Self::File(ref mut f) => f.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Empty => Ok(()),
            Self::File(ref mut f) => f.flush(),
        }
    }
}

impl MakeWriter<'_> for EmptyOrFileWriter {
    type Writer = Self;

    fn make_writer(&self) -> Self::Writer {
        match self {
            Self::Empty => Self::Empty,
            Self::File(f) => Self::File(f.make_writer()),
        }
    }
}

// this is a crazy type b/c tracing uses recursive "Layer" types to allow for an arbitrary number
// of layers
// however, this allows us to dynamically reload the debug file at runtime
#[allow(clippy::type_complexity)]
static LOGGER: LazyLock<
    Arc<
        Mutex<
            reload::Handle<
                Filtered<
                    fmt::Layer<
                        Layered<Box<dyn Layer<Registry> + Send + Sync>, Registry>,
                        DefaultFields,
                        Format,
                        EmptyOrFileWriter,
                    >,
                    EnvFilter,
                    Layered<Box<dyn Layer<Registry> + Send + Sync>, Registry>,
                >,
                Layered<Box<dyn Layer<Registry> + Send + Sync>, Registry>,
            >,
        >,
    >,
> = LazyLock::new(|| {
    let native_layer = native_layer();
    // just turn the layer off for now
    let fmt = fmt::Layer::default()
        .with_writer(EmptyOrFileWriter::default())
        .with_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::OFF.into())
                .parse_lossy("off"),
        );
    let (filter, reload_handle) = reload::Layer::new(fmt);
    let _ = tracing_subscriber::registry()
        .with(native_layer.boxed())
        .with(filter)
        .try_init();
    Arc::new(Mutex::new(reload_handle))
});

// needs to be alive for the duration of execution
static WORKER: OnceLock<Arc<Mutex<Option<WorkerGuard>>>> = OnceLock::new();

static FILE_INITIALIZED: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(false)));

/// Enum representing log file rotation options
#[derive(uniffi::Enum, PartialEq, Debug, Clone)]
pub enum FfiLogRotation {
    /// Rotate log files every minute
    Minutely = 0,
    /// Rotate log files every hour
    Hourly = 1,
    /// Rotate log files every day
    Daily = 2,
    /// Never rotate log files
    Never = 3,
}

impl From<FfiLogRotation> for tracing_appender::rolling::Rotation {
    fn from(rotation: FfiLogRotation) -> Self {
        match rotation {
            FfiLogRotation::Minutely => tracing_appender::rolling::Rotation::MINUTELY,
            FfiLogRotation::Hourly => tracing_appender::rolling::Rotation::HOURLY,
            FfiLogRotation::Daily => tracing_appender::rolling::Rotation::DAILY,
            FfiLogRotation::Never => tracing_appender::rolling::Rotation::NEVER,
        }
    }
}

/// Enum representing log levels
#[derive(uniffi::Enum, PartialEq, Debug, Clone)]
pub enum FfiLogLevel {
    /// Error level logs only
    Error = 0,
    /// Warning level and above
    Warn = 1,
    /// Info level and above
    Info = 2,
    /// Debug level and above
    Debug = 3,
    /// Trace level and all logs
    Trace = 4,
}

impl FfiLogLevel {
    fn to_str(&self) -> &str {
        match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }
}

/// turns on logging to a file on-disk in the directory specified.
/// files will be prefixed with 'libxmtp.log' and suffixed with the timestamp,
/// i.e "libxmtp.log.2025-04-02"
/// A maximum of 'max_files' log files are kept.
#[uniffi::export]
pub fn enter_debug_writer(
    directory: String,
    log_level: FfiLogLevel,
    rotation: FfiLogRotation,
    max_files: u32,
) -> Result<(), GenericError> {
    enter_debug_writer_with_level(directory, rotation, max_files, log_level)
}

/// turns on logging to a file on-disk with a specified log level.
/// files will be prefixed with 'libxmtp.log' and suffixed with the timestamp,
/// i.e "libxmtp.log.2025-04-02"
/// A maximum of 'max_files' log files are kept.
#[uniffi::export]
pub fn enter_debug_writer_with_level(
    directory: String,
    rotation: FfiLogRotation,
    max_files: u32,
    log_level: FfiLogLevel,
) -> Result<(), GenericError> {
    if !FILE_INITIALIZED.load(Ordering::Relaxed) {
        enable_debug_file_inner(directory, rotation, max_files, log_level)?;
        FILE_INITIALIZED.store(true, Ordering::Relaxed);
    }
    Ok(())
}

fn enable_debug_file_inner(
    directory: String,
    rotation: FfiLogRotation,
    max_files: u32,
    log_level: FfiLogLevel,
) -> Result<(), GenericError> {
    // First, ensure any previous logger is properly shut down
    let _ = exit_debug_writer();

    let version = env!("CARGO_PKG_VERSION");
    let commit_sha = option_env!("VERGEN_GIT_SHA").unwrap_or("unknown");
    let file_appender = RollingFileAppender::builder()
        .filename_prefix(format!("libxmtp-v{}.{}.log", version, commit_sha))
        .rotation(rotation.into())
        .max_log_files(max_files as usize)
        .build(&directory)?;

    let (non_blocking, worker) = NonBlockingBuilder::default()
        .thread_name("libxmtp-log-writer")
        .finish(file_appender);

    // Initialize the worker container if needed
    if WORKER.get().is_none() {
        let _ = WORKER.set(Arc::new(Mutex::new(None)));
    }

    // Now we can safely update the worker
    if let Some(worker_container) = WORKER.get() {
        *worker_container.lock() = Some(worker);
    }

    let handle = LOGGER.lock();
    handle.modify(|l| {
        *l.inner_mut().writer_mut() = EmptyOrFileWriter::File(non_blocking);
        let filter = xmtp_common::filter_directive(log_level.to_str());
        *l.filter_mut() = filter;
    })?;
    Ok(())
}

/// Flush loglines from libxmtp log writer to the file, ensuring logs are written.
/// This should be called before the program exits, to ensure all the logs in memory have been
/// written. this ends the writer thread.
#[uniffi::export]
pub fn exit_debug_writer() -> Result<(), GenericError> {
    let handle = LOGGER.lock();
    if let Some(w) = WORKER.get() {
        if let Some(w) = w.lock().take() {
            drop(w)
        }
    }
    handle.modify(|l| {
        *l.inner_mut().writer_mut() = EmptyOrFileWriter::Empty;
        *l.filter_mut() = EnvFilter::builder()
            .with_default_directive(LevelFilter::OFF.into())
            .parse_lossy("off");
    })?;
    FILE_INITIALIZED.store(false, Ordering::Relaxed);
    Ok(())
}

pub fn init_logger() {
    let _ = *LOGGER;
}

#[cfg(test)]
pub use test_logger::*;

#[cfg(test)]
mod test_logger {
    use super::*;
    use std::io::Read;

    pub fn native_layer<S>() -> impl Layer<S>
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
    {
        xmtp_common::logger_layer()
    }

    #[test]
    fn test_file_appender() {
        init_logger();
        let s = xmtp_common::rand_hexstring();
        let path = std::env::temp_dir().join(format!("{}-log-test", s));
        enter_debug_writer(
            path.display().to_string(),
            FfiLogLevel::Trace,
            FfiLogRotation::Minutely,
            10,
        )
        .unwrap();
        let rand_nums = hex::encode(xmtp_common::rand_vec::<100>());
        tracing::info!("test log");
        tracing::trace!(rand_nums);
        tracing::info!("test log");
        exit_debug_writer().unwrap();

        let entries = std::fs::read_dir(path).unwrap().collect::<Vec<_>>();
        assert_eq!(entries.len(), 1);
        for entry in entries.iter() {
            println!("entry  {:?}", entry);
            let mut file = std::fs::File::open(entry.as_ref().unwrap().path()).unwrap();
            let mut contents = String::new();
            file.read_to_string(&mut contents).unwrap();
            assert!(contents.contains(&rand_nums));
            std::fs::remove_file(entry.as_ref().unwrap().path()).unwrap();
        }
    }
}
