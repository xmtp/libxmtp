//! Archive writing. [`export`] writes the version and the starting nonce in the
//! clear, then one zstd stream of length-prefixed AES-GCM frames: the metadata,
//! then every element of one database snapshot. Frames are written as rows are
//! read, so memory does not grow with the history.

use super::BACKUP_VERSION;
use crate::archive_options::ArchiveOptions;
use crate::{ArchiveError, NONCE_SIZE, snapshot, util::GenericArrayExt};
use aes_gcm::aead::Aead;
use async_compression::futures::write::ZstdEncoder;
use futures::{FutureExt, io::AllowStdIo};
use futures_util::AsyncWriteExt;
use prost::Message;
#[allow(deprecated)]
use sha2::digest::generic_array::GenericArray;
use std::io;
use xmtp_db::ConnectionExt;
use xmtp_proto::xmtp::device_sync::{BackupElement, BackupMetadataSave, backup_element::Element};

/// Writes an archive of everything `options` selects, read in one snapshot
/// measured at one export time, to `sink`, and returns its metadata. Fails,
/// rather than omitting it, when a selected record cannot be read; on failure
/// `sink` holds an incomplete archive that the caller must discard. `sink` is
/// written while the snapshot's read transaction is open, so it must not use
/// the database.
pub fn export(
    options: ArchiveOptions,
    db: impl ConnectionExt,
    key: &[u8],
    mut sink: impl io::Write,
) -> Result<BackupMetadataSave, ArchiveError> {
    let cipher = crate::cipher(key)?;
    let nonce = xmtp_common::rand_array::<NONCE_SIZE>();
    sink.write_all(&BACKUP_VERSION.to_le_bytes())?;
    sink.write_all(&nonce)?;

    #[allow(deprecated)]
    let mut nonce = GenericArray::clone_from_slice(&nonce);
    let mut zstd = ZstdEncoder::new(AllowStdIo::new(sink));
    let write = |element: Element| -> Result<(), ArchiveError> {
        let plaintext = BackupElement {
            element: Some(element),
        }
        .encode_to_vec();
        let ciphertext = cipher.encrypt(&nonce, &*plaintext)?;
        nonce.increment();
        ready(zstd.write_all(&(ciphertext.len() as u32).to_le_bytes()))?;
        Ok(ready(zstd.write_all(&ciphertext))?)
    };
    let metadata = snapshot::read(&db, &options, write)?;
    ready(zstd.close())?;
    Ok(metadata)
}

/// Namespace for [`ArchiveExporter::export_to_file`], which the native
/// bindings call.
#[cfg(not(target_arch = "wasm32"))]
pub struct ArchiveExporter;

#[cfg(not(target_arch = "wasm32"))]
impl ArchiveExporter {
    /// Exports to a file at `path`, as [`export`], on tokio's blocking pool so
    /// the snapshot never stalls an async worker. The archive is written to a
    /// sibling temporary file and renamed over `path` only once complete, so a
    /// failed export leaves `path` as it was. Dropping the future cancels the
    /// export at its next write, which counts as a failure. Must be called within a tokio runtime.
    pub async fn export_to_file(
        options: ArchiveOptions,
        db: impl ConnectionExt + 'static,
        path: impl AsRef<std::path::Path>,
        key: &[u8],
    ) -> Result<BackupMetadataSave, ArchiveError> {
        crate::check_key(key)?;
        let (path, key) = (path.as_ref().to_owned(), key.to_vec());
        let cancel = tokio_util::sync::CancellationToken::new();
        let _cancel_on_drop = cancel.clone().drop_guard();
        tokio::task::spawn_blocking(move || write_file(options, db, &path, &key, &cancel))
            .await
            .map_err(io::Error::other)?
    }
}

/// Exports to a sibling temporary file until `cancel` fires, then renames it
/// over `path`. A failed or cancelled export removes the temporary file and
/// leaves any archive already at `path` untouched. The file takes the
/// permissions of the archive it replaces; a new one is owner-only on unix.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn write_file(
    options: ArchiveOptions,
    db: impl ConnectionExt,
    path: &std::path::Path,
    key: &[u8],
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<BackupMetadataSave, ArchiveError> {
    let mut partial = path.as_os_str().to_owned();
    partial.push(format!(
        ".{:016x}.partial",
        u64::from_le_bytes(xmtp_common::rand_array())
    ));
    let partial = std::path::PathBuf::from(partial);
    let exported = (|| -> Result<_, ArchiveError> {
        let mut create = std::fs::OpenOptions::new();
        create.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut create, 0o600);
        let file = create.open(&partial)?;
        if let Ok(existing) = std::fs::metadata(path) {
            file.set_permissions(existing.permissions())?;
        }
        let mut file = io::BufWriter::new(file);
        let metadata = export(options, db, key, Cancellable(&mut file, cancel))?;
        file.into_inner()
            .map_err(io::IntoInnerError::into_error)?
            .sync_all()?;
        live(cancel)?;
        std::fs::rename(&partial, path)?;
        Ok(metadata)
    })();
    if exported.is_err()
        && let Err(e) = std::fs::remove_file(&partial)
        && e.kind() != io::ErrorKind::NotFound
    {
        tracing::warn!(path = %partial.display(), error = %e, "failed export left a partial archive");
    }
    exported
}

/// Fails once `cancel` fires, so a cancelled export takes the cleanup path.
#[cfg(not(target_arch = "wasm32"))]
fn live(cancel: &tokio_util::sync::CancellationToken) -> io::Result<()> {
    if cancel.is_cancelled() {
        return Err(io::Error::other("archive export cancelled"));
    }
    Ok(())
}

/// A sink that fails every write once its token is cancelled.
#[cfg(not(target_arch = "wasm32"))]
struct Cancellable<'a, W>(W, &'a tokio_util::sync::CancellationToken);

#[cfg(not(target_arch = "wasm32"))]
impl<W: io::Write> io::Write for Cancellable<'_, W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        live(self.1)?;
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

/// Resolves an encoder operation over a synchronous sink, which never pends.
fn ready(op: impl Future<Output = io::Result<()>>) -> io::Result<()> {
    op.now_or_never().unwrap_or_else(|| {
        Err(io::Error::other(
            "synchronous archive sink returned pending",
        ))
    })
}
