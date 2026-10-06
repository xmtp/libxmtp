//! Native source access. SQLite only opens files owned by the working copy.
use crate::{InputError, MigrationError, MigrationReport, PrepareMigrationArchiveArgs};
use diesel::{connection::SimpleConnection, prelude::*};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};
use tokio_util::sync::CancellationToken;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows::{check_locks, same_file};

const SIDECARS: &[&str] = &["", "-wal", "-shm", "-journal", ".sqlcipher_salt"];
const KEY_BYTES: usize = 32;
const SALT_BYTES: usize = 16;

fn input(error: impl Into<InputError>) -> MigrationError {
    MigrationError::InvalidInput(error.into())
}
fn output(error: io::Error) -> MigrationError {
    MigrationError::Output(xmtp_archive::ArchiveError::from(error).into())
}
fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    name.into()
}

pub(crate) async fn prepare(
    args: PrepareMigrationArchiveArgs,
) -> Result<MigrationReport, MigrationError> {
    if args.archive_key.len() != KEY_BYTES
        || args
            .database_key
            .as_ref()
            .is_some_and(|k| k.len() != KEY_BYTES)
    {
        return Err(MigrationError::invalid("keys must contain 32 bytes"));
    }
    offload(move |cancel| run(args, cancel)).await
}

/// The guard cancels the worker when its caller drops the returned future.
async fn offload<T: Send + 'static>(
    work: impl FnOnce(&CancellationToken) -> Result<T, MigrationError> + Send + 'static,
) -> Result<T, MigrationError> {
    let cancel = CancellationToken::new();
    let _guard = cancel.clone().drop_guard();
    tokio::task::spawn_blocking(move || work(&cancel))
        .await
        .map_err(|e| output(io::Error::other(e)))?
}

/// Copy under the caller-closed precondition. Do not open source files through
/// SQLite: its final close can checkpoint WAL and change source bytes.
// implements: MIG-002
fn working_copy(source: &Path, destination: &Path) -> Result<tempfile::TempDir, MigrationError> {
    let source = source.canonicalize().map_err(input)?;
    if !source.is_file() {
        return Err(MigrationError::invalid("source is not a database file"));
    }
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let target = match destination.canonicalize() {
        Ok(path) => path,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            parent.canonicalize().map_err(input)?.join(
                destination
                    .file_name()
                    .ok_or_else(|| MigrationError::invalid("output has no file name"))?,
            )
        }
        Err(e) => return Err(input(e)),
    };
    let mut files = Vec::new();
    for suffix in SIDECARS {
        let path = sidecar(&source, suffix);
        if target == path {
            return Err(MigrationError::invalid("output aliases source storage"));
        }
        match fs::File::open(&path) {
            Ok(file) => {
                if target == path.canonicalize().map_err(input)? || same_file(&file, destination)? {
                    return Err(MigrationError::invalid("output aliases source storage"));
                }
                if suffix.is_empty() {
                    check_locks(&file, 0x4000_0000, 512)?;
                }
                if *suffix == "-shm" {
                    check_locks(&file, 120, 8)?;
                }
                files.push((suffix, file));
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound && !suffix.is_empty() => (),
            Err(e) => return Err(input(e)),
        }
    }
    let directory = tempfile::tempdir().map_err(input)?;
    for (suffix, mut file) in files {
        // Shared memory is coordination state, not committed database content.
        if *suffix == "-shm" {
            continue;
        }
        let mut target = fs::File::create(sidecar(&directory.path().join("source.db3"), suffix))
            .map_err(input)?;
        io::copy(&mut file, &mut target).map_err(input)?;
    }
    Ok(directory)
}

#[cfg(unix)]
fn same_file(source: &fs::File, target: &Path) -> Result<bool, MigrationError> {
    use std::os::unix::fs::MetadataExt;
    match fs::metadata(target) {
        Ok(other) => {
            let source = source.metadata().map_err(input)?;
            Ok(source.dev() == other.dev() && source.ino() == other.ino())
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(input(e)),
    }
}

/// F_GETLK observes the byte locks used by SQLite without changing source data.
/// POSIX locks owned by this process are not visible. Closing the legacy SDK is
/// still required, including clients in this process and idle connections.
#[cfg(unix)]
fn check_locks(file: &fs::File, start: i64, len: i64) -> Result<(), MigrationError> {
    use std::os::fd::AsRawFd;
    // SAFETY: zero is valid for all flock integer fields; set the query fields.
    let mut lock: libc::flock = unsafe { std::mem::zeroed() };
    lock.l_type = libc::F_WRLCK as _;
    lock.l_whence = libc::SEEK_SET as _;
    lock.l_start = start;
    lock.l_len = len;
    // SAFETY: the file descriptor is live and lock points to a valid flock.
    if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETLK, &mut lock) } == -1 {
        return Err(input(io::Error::last_os_error()));
    }
    if lock.l_type != libc::F_UNLCK as libc::c_short {
        return Err(MigrationError::SourceBusy);
    }
    Ok(())
}

fn open_copy(path: &Path, key: Option<&[u8]>) -> Result<SqliteConnection, MigrationError> {
    let path_text = path
        .to_str()
        .ok_or_else(|| MigrationError::invalid("database path is not UTF-8"))?;
    let mut conn = SqliteConnection::establish(path_text).map_err(input)?;
    if let Some(key) = key {
        conn.batch_execute(&format!("PRAGMA key=\"x'{}'\";", hex::encode(key)))
            .map_err(input)?;
        let salt = sidecar(path, ".sqlcipher_salt");
        if salt.exists() {
            let salt = hex::decode(fs::read(salt).map_err(input)?).map_err(input)?;
            if salt.len() != SALT_BYTES {
                return Err(MigrationError::invalid(
                    "SQLCipher salt must contain 16 bytes",
                ));
            }
            conn.batch_execute(&format!(
                "PRAGMA cipher_plaintext_header_size=32; PRAGMA cipher_salt=\"x'{}'\";",
                hex::encode(salt)
            ))
            .map_err(input)?;
        }
    }
    conn.batch_execute("PRAGMA busy_timeout=0; SELECT count(*) FROM sqlite_master;")
        .map_err(input)?;
    Ok(conn)
}

/// All output remains private until the encoder and filesystem flush complete.
// implements: MIG-004
fn run(
    args: PrepareMigrationArchiveArgs,
    cancel: &CancellationToken,
) -> Result<MigrationReport, MigrationError> {
    let output_path = Path::new(&args.output_path);
    let directory = working_copy(Path::new(&args.database_path), output_path)?;
    live(cancel).map_err(output)?;
    let mut conn = open_copy(
        &directory.path().join("source.db3"),
        args.database_key.as_deref(),
    )?;
    crate::migrations::apply(&mut conn)?;
    write_output(output_path, cancel, |sink| {
        let mut writer = xmtp_archive::exporter::ElementWriter::new(&args.archive_key, sink)?;
        let report = crate::records::export(&mut conn, args.output_path.clone(), |element| {
            live(cancel).map_err(output)?;
            writer.write(element).map_err(MigrationError::from)
        })?;
        writer.finish()?;
        Ok(report)
    })
}

/// Publication is the commit point. Cancellation afterward does not undo it.
fn write_output<T>(
    path: &Path,
    cancel: &CancellationToken,
    write: impl FnOnce(&mut dyn Write) -> Result<T, MigrationError>,
) -> Result<T, MigrationError> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut partial = tempfile::NamedTempFile::new_in(parent).map_err(output)?;
    let result = {
        let mut buffer = io::BufWriter::new(partial.as_file_mut());
        let result = write(&mut Cancellable(&mut buffer, cancel))?;
        buffer.flush().map_err(output)?;
        result
    };
    partial.as_file().sync_all().map_err(output)?;
    live(cancel).map_err(output)?;
    partial.persist(path).map_err(|e| output(e.error))?;
    Ok(result)
}

fn live(cancel: &CancellationToken) -> io::Result<()> {
    if cancel.is_cancelled() {
        Err(io::Error::other("migration cancelled"))
    } else {
        Ok(())
    }
}
struct Cancellable<'a, W>(W, &'a CancellationToken);
impl<W: Write> Write for Cancellable<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        live(self.1)?;
        self.0.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        live(self.1)?;
        self.0.flush()
    }
}

#[cfg(test)]
mod tests;
