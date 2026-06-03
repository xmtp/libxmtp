use crate::config::{FileConfig, Rotation};
use tracing_appender::non_blocking::{NonBlocking, NonBlockingBuilder, WorkerGuard};
use tracing_appender::rolling::RollingFileAppender;

impl From<Rotation> for tracing_appender::rolling::Rotation {
    fn from(r: Rotation) -> Self {
        match r {
            Rotation::Minutely => Self::MINUTELY,
            Rotation::Hourly => Self::HOURLY,
            Rotation::Daily => Self::DAILY,
            Rotation::Never => Self::NEVER,
        }
    }
}

/// Build the non-blocking file writer + its worker guard from a `FileConfig`.
///
/// The reloadable file slot in [`crate::LoggingHandle`] holds an
/// `Option<Box<dyn Layer>>`, so a sentinel "empty writer" is not needed — the
/// slot is simply `None` when file logging is off.
pub(crate) fn file_writer(
    cfg: &FileConfig,
) -> Result<(NonBlocking, WorkerGuard), Box<dyn std::error::Error + Send + Sync>> {
    let version = env!("CARGO_PKG_VERSION");
    let commit_sha = option_env!("VERGEN_GIT_SHA").unwrap_or("unknown");
    let pid = std::process::id();
    let appender = RollingFileAppender::builder()
        .filename_prefix(format!(
            "libxmtp-v{}.{}.{}.{}.log",
            version,
            commit_sha,
            cfg.process_type.as_str(),
            pid
        ))
        .rotation(cfg.rotation.into())
        .max_log_files(cfg.max_files as usize)
        .build(&cfg.dir)?;
    let (non_blocking, guard) = NonBlockingBuilder::default()
        .thread_name("libxmtp-log-writer")
        .finish(appender);
    Ok((non_blocking, guard))
}
