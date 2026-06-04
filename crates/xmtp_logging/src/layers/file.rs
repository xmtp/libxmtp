use crate::config::{FileConfig, Rotation};
use std::io::Write;
use tracing_appender::non_blocking::{NonBlocking, NonBlockingBuilder, WorkerGuard};
use tracing_appender::rolling::RollingFileAppender;
use tracing_subscriber::fmt::MakeWriter;

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

/// The swappable writer behind the always-present file fmt layer. `Empty` discards
/// everything (file logging off); `File` wraps the non-blocking appender (on).
#[derive(Default)]
pub(crate) enum EmptyOrFileWriter {
    #[default]
    Empty,
    File(NonBlocking),
}

impl Write for EmptyOrFileWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Empty => Ok(buf.len()),
            Self::File(f) => f.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Empty => Ok(()),
            Self::File(f) => f.flush(),
        }
    }
}

impl<'a> MakeWriter<'a> for EmptyOrFileWriter {
    type Writer = Self;

    fn make_writer(&'a self) -> Self::Writer {
        match self {
            Self::Empty => Self::Empty,
            Self::File(f) => Self::File(f.make_writer()),
        }
    }
}

/// Build the non-blocking file writer + its worker guard from a `FileConfig`. The
/// fallible part of enabling file logging (opening the file / spawning the writer).
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
