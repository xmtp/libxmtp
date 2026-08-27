use crate::FfiError;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use xmtp_logging::{FileConfig, Level, LoggingHandle, ProcessType, Rotation, XmtpLogging};

// Process-global logging handle, built once on first use. `None` when the host
// process already installed a subscriber (install -> AlreadyInitialized); the
// runtime log controls then become no-ops rather than fighting it.
static HANDLE: OnceLock<Option<LoggingHandle>> = OnceLock::new();

// Guards the one-shot debug-file enable: only the first `enter_debug_writer` wins
// until `exit_debug_writer` resets it.
static FILE_INITIALIZED: AtomicBool = AtomicBool::new(false);

fn handle() -> Option<&'static LoggingHandle> {
    HANDLE
        .get_or_init(|| {
            match XmtpLogging::builder()
                .level(Level::Trace)
                .with_native(true)
                .install()
            {
                Ok(h) => Some(h),
                // Already installed by the host: don't panic across the FFI
                // boundary — leave logging to whoever owns the subscriber.
                Err(e) => {
                    tracing::debug!("xmtp_logging install skipped: {e}");
                    None
                }
            }
        })
        .as_ref()
}

/// Force-initialize the global logging handle (installs the subscriber).
pub fn init_logger() {
    let _ = handle();
}

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

impl From<FfiLogRotation> for Rotation {
    fn from(r: FfiLogRotation) -> Self {
        match r {
            FfiLogRotation::Minutely => Rotation::Minutely,
            FfiLogRotation::Hourly => Rotation::Hourly,
            FfiLogRotation::Daily => Rotation::Daily,
            FfiLogRotation::Never => Rotation::Never,
        }
    }
}

/// Enum representing process types for logging
#[derive(uniffi::Enum, PartialEq, Debug, Clone)]
pub enum FfiProcessType {
    /// Main application process
    Main = 0,
    /// Notification extension/service process
    NotificationExtension = 1,
}

impl From<FfiProcessType> for ProcessType {
    fn from(p: FfiProcessType) -> Self {
        match p {
            FfiProcessType::Main => ProcessType::Main,
            FfiProcessType::NotificationExtension => ProcessType::NotificationExtension,
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

impl From<FfiLogLevel> for Level {
    fn from(l: FfiLogLevel) -> Self {
        match l {
            FfiLogLevel::Error => Level::Error,
            FfiLogLevel::Warn => Level::Warn,
            FfiLogLevel::Info => Level::Info,
            FfiLogLevel::Debug => Level::Debug,
            FfiLogLevel::Trace => Level::Trace,
        }
    }
}

// Map to `Log` (not `Generic`) so mobile keeps the stable `[Log]` error code.
// Into `GenericError` because the blanket `From<Into<GenericError>>` for
// `FfiError` would conflict with a direct `FfiError` impl.
impl From<xmtp_logging::Error> for crate::GenericError {
    fn from(e: xmtp_logging::Error) -> Self {
        crate::GenericError::Log(e.to_string())
    }
}

/// turns on logging to a file on-disk in the directory specified.
/// files will be prefixed with 'libxmtp-v{version}.{commit}.{process_type}.{pid}.log' and suffixed with the timestamp,
/// i.e "libxmtp-v1.6.0.abc123.main.12345.log.2025-04-02"
/// A maximum of 'max_files' log files are kept.
#[uniffi::export]
#[xmtp_common::err_span]
pub fn enter_debug_writer(
    directory: String,
    log_level: FfiLogLevel,
    rotation: FfiLogRotation,
    max_files: u32,
    process_type: FfiProcessType,
) -> Result<(), FfiError> {
    enter_debug_writer_with_level(directory, rotation, max_files, log_level, process_type)
}

/// turns on logging to a file on-disk with a specified log level.
/// files will be prefixed with 'libxmtp-v{version}.{commit}.{process_type}.{pid}.log' and suffixed with the timestamp,
/// i.e "libxmtp-v1.6.0.abc123.notif.67890.log.2025-04-02"
/// A maximum of 'max_files' log files are kept.
#[uniffi::export]
#[xmtp_common::err_span]
pub fn enter_debug_writer_with_level(
    directory: String,
    rotation: FfiLogRotation,
    max_files: u32,
    log_level: FfiLogLevel,
    process_type: FfiProcessType,
) -> Result<(), FfiError> {
    let Some(h) = handle() else { return Ok(()) };
    if FILE_INITIALIZED
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        let cfg = FileConfig {
            dir: directory,
            rotation: rotation.into(),
            max_files,
            process_type: process_type.into(),
            level: log_level.into(),
        };
        if let Err(e) = h.enable_file(cfg) {
            FILE_INITIALIZED.store(false, Ordering::Release);
            return Err(e.into());
        }
    }
    Ok(())
}

/// Flush loglines from libxmtp log writer to the file, ensuring logs are written.
/// This should be called before the program exits, to ensure all the logs in memory have been
/// written. this ends the writer thread.
#[uniffi::export]
#[xmtp_common::err_span]
pub fn exit_debug_writer() -> Result<(), FfiError> {
    if let Some(h) = handle() {
        h.disable_file()?;
    }
    FILE_INITIALIZED.store(false, Ordering::Release);
    Ok(())
}

/// Updates the log level of the native log layer (oslog on iOS, logcat on Android).
/// Activity spans are emitted as os_signpost on iOS — set to `Trace` to see span
/// activity in Console.app / Instruments. No-op on non-mobile builds.
#[uniffi::export]
#[xmtp_common::err_span]
pub fn set_native_log_level(log_level: FfiLogLevel) -> Result<(), FfiError> {
    if let Some(h) = handle() {
        h.set_native_level(log_level.into())?;
    }
    Ok(())
}

/// Sentry telemetry configuration. `user_stable_id` is the app-computed HKDF
/// stable id (MetricsStableIdEncoder derivation), never a raw inbox id.
#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiSentryConfig {
    /// The app's Sentry DSN.
    pub dsn: String,
    /// Sentry environment name (e.g. "production", "staging").
    pub environment: Option<String>,
    /// Release identifier reported to Sentry. Defaults to the libxmtp version
    /// when `None`.
    pub release: Option<String>,
    /// Fraction of transactions sampled for tracing. Valid range is
    /// `[0.0, 1.0]`; `0.0` reports error events only, with no transactions.
    pub traces_sample_rate: f32,
    /// Number of breadcrumbs kept in the rolling buffer before the oldest are
    /// evicted; pass 100 to match the underlying crate default.
    pub max_breadcrumbs: u32,
    /// The app-computed HKDF stable id, never a raw inbox id.
    pub user_stable_id: Option<String>,
    /// Tags attached to every event. `component=libxmtp` is always added.
    pub tags: Vec<FfiSentryTag>,
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiSentryTag {
    /// Tag name.
    pub key: String,
    /// Tag value.
    pub value: String,
}

impl From<FfiSentryConfig> for xmtp_logging::SentryConfig {
    fn from(c: FfiSentryConfig) -> Self {
        Self {
            dsn: c.dsn,
            environment: c.environment,
            release: c.release,
            traces_sample_rate: c.traces_sample_rate,
            max_breadcrumbs: c.max_breadcrumbs as usize,
            user_stable_id: c.user_stable_id,
            tags: c.tags.into_iter().map(|t| (t.key, t.value)).collect(),
        }
    }
}

// Sentry setup errors carry the actionable reason (bad DSN, sample rate, OTLP
// already active), so keep the message instead of the `Log` variant's fixed
// "error initializing debug log file" text.
fn sentry_err(e: xmtp_logging::Error) -> FfiError {
    FfiError::generic(e.to_string())
}

const NO_HANDLE: &str = "logging subscriber owned by host process";

/// Enable Sentry error/trace export. Errors if logging is owned by the host
/// process, the DSN is invalid, `traces_sample_rate` is outside `[0.0, 1.0]`,
/// or OTLP telemetry is already active.
#[uniffi::export]
#[xmtp_common::err_span]
pub fn enable_sentry_telemetry(config: FfiSentryConfig) -> Result<(), FfiError> {
    let h = handle().ok_or_else(|| FfiError::generic(NO_HANDLE))?;
    h.enable_sentry(config.into()).map_err(sentry_err)
}

/// Disable Sentry export: remove the layer, then, if this handle owns the
/// installed client, flush and drop it. Clears the stamped user id.
#[uniffi::export]
#[xmtp_common::err_span]
pub fn disable_sentry_telemetry() -> Result<(), FfiError> {
    // Clear the staged user id even when logging is host-owned and there is no
    // handle to disable: the slot is ours regardless, and a stale id would
    // attribute a later session's events.
    xmtp_logging::sentry::set_user_stable_id(None);
    let h = handle().ok_or_else(|| FfiError::generic(NO_HANDLE))?;
    h.disable_sentry().map_err(sentry_err)
}

/// Late identify: stamp (or clear) the pseudonymous user id on future events.
/// Call at inbox-ready with the HKDF stable id.
#[uniffi::export]
pub fn set_sentry_user(stable_id: Option<String>) {
    xmtp_logging::sentry::set_user_stable_id(stable_id);
}

/// Flush pending telemetry (file, OTLP, and Sentry). Call on app background.
#[uniffi::export]
pub fn flush_telemetry() {
    // Flush only an already-installed pipeline: `handle()` would lazily install
    // our global subscriber, permanently claiming it from a host that flushes
    // before configuring logging.
    if let Some(h) = HANDLE.get().and_then(|h| h.as_ref()) {
        h.flush();
    }
}

#[cfg(test)]
mod sentry_tests {
    use super::*;

    #[test]
    fn ffi_config_maps_and_bad_dsn_errors() {
        // Ordered first inside the one test fn: flush before any init must not
        // lazily install the subscriber (the other calls below do install it).
        flush_telemetry();
        assert!(
            HANDLE.get().is_none(),
            "flush_telemetry must not install the logging pipeline"
        );
        // A user id staged with no handle/enable must not survive disable.
        set_sentry_user(Some("staged".into()));
        let _ = disable_sentry_telemetry();
        assert!(!xmtp_logging::sentry::user_stable_id_is_set());
        let cfg = FfiSentryConfig {
            dsn: "not a dsn".into(),
            environment: Some("dev".into()),
            release: None,
            traces_sample_rate: 0.0,
            max_breadcrumbs: 50,
            user_stable_id: None,
            tags: vec![],
        };
        // Invalid DSN must surface as an error, not a silent no-op (unlike the
        // log-level controls, which no-op without a handle). Which of the two
        // error paths fires depends on whether this test binary won the
        // subscriber install race, so accept either.
        let err = enable_sentry_telemetry(cfg).unwrap_err().to_string();
        assert!(
            err.contains("invalid sentry dsn") || err.contains("owned by host process"),
            "unexpected error: {err}"
        );
        set_sentry_user(None);
        flush_telemetry();
    }
}

#[cfg(test)]
mod test_logger {
    use super::*;
    use std::io::Read;

    // _NOTE:_ this test **fails** if there are rogue loggers
    // started with `ctor::ctor` in other crates.
    // crates should ensure their test `ctor`'s are
    // using `cfg(test)` and cannot be activated
    // with `test-utils`.
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
            FfiProcessType::Main,
        )
        .unwrap();
        let rand_nums = hex::encode(xmtp_common::rand_vec::<100>());
        tracing::info!("test log");
        tracing::debug!(rand_nums);
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
