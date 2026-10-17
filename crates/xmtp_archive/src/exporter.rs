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
    sink: impl io::Write,
) -> Result<BackupMetadataSave, ArchiveError> {
    let mut writer = ElementWriter::new(key, sink)?;
    let metadata = snapshot::read(&db, &options, |element| writer.write(element))?;
    writer.finish()?;
    Ok(metadata)
}

/// Streams standard archive elements to a synchronous sink.
///
/// The caller writes metadata first and each group before its messages. On any
/// error, discard the sink. Only `finish` completes the compressed stream.
#[allow(deprecated)]
pub struct ElementWriter<W: io::Write> {
    cipher: aes_gcm::Aes256Gcm,
    nonce: GenericArray<u8, aes_gcm::aead::consts::U12>,
    zstd: ZstdEncoder<AllowStdIo<W>>,
}

impl<W: io::Write> ElementWriter<W> {
    /// Checks the key before writing the header.
    pub fn new(key: &[u8], mut sink: W) -> Result<Self, ArchiveError> {
        let cipher = crate::cipher(key)?;
        let nonce = xmtp_common::rand_array::<NONCE_SIZE>();
        sink.write_all(&BACKUP_VERSION.to_le_bytes())?;
        sink.write_all(&nonce)?;
        #[allow(deprecated)]
        Ok(Self {
            cipher,
            nonce: GenericArray::clone_from_slice(&nonce),
            zstd: ZstdEncoder::new(AllowStdIo::new(sink)),
        })
    }

    /// Writes one encrypted frame and advances its nonce.
    // implements: ARCH-001
    pub fn write(&mut self, element: Element) -> Result<(), ArchiveError> {
        let plaintext = BackupElement {
            element: Some(element),
        }
        .encode_to_vec();
        let ciphertext = self.cipher.encrypt(&self.nonce, &*plaintext)?;
        self.nonce.increment();
        let length = u32::try_from(ciphertext.len())
            .map_err(|_| io::Error::other("archive frame exceeds u32 length"))?;
        ready(self.zstd.write_all(&length.to_le_bytes()))?;
        Ok(ready(self.zstd.write_all(&ciphertext))?)
    }

    /// Completes the compressed stream. A successful write alone is incomplete.
    pub fn finish(mut self) -> Result<(), ArchiveError> {
        Ok(ready(self.zstd.close())?)
    }
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
    /// failed export leaves `path` as it was. The archive is a new file:
    /// owner-only on unix and the directory's default ACL on Windows, never
    /// the permissions of whatever `path` held. Dropping the future cancels the
    /// export at its next write, which counts as a failure. Must be called within a tokio runtime.
    pub async fn export_to_file(
        options: ArchiveOptions,
        db: impl ConnectionExt + 'static,
        path: impl AsRef<std::path::Path>,
        key: &[u8],
    ) -> Result<BackupMetadataSave, ArchiveError> {
        let path = path.as_ref().to_owned();
        offload(key, move |key, cancel| {
            write_file(options, db, &path, key, cancel)
        })
        .await
    }

    /// Exports into a buffer, as [`export`], on tokio's blocking pool so the
    /// snapshot never stalls an async worker. Dropping the future cancels the
    /// export at its next write. Must be called within a tokio runtime.
    pub async fn export_to_bytes(
        options: ArchiveOptions,
        db: impl ConnectionExt + 'static,
        key: &[u8],
    ) -> Result<Vec<u8>, ArchiveError> {
        offload(key, move |key, cancel| {
            let mut bytes = Vec::new();
            export(options, db, key, Cancellable(&mut bytes, cancel))?;
            Ok(bytes)
        })
        .await
    }
}

/// Checks `key`, then runs `f` on tokio's blocking pool with a token that is
/// cancelled when the returned future is dropped.
#[cfg(not(target_arch = "wasm32"))]
async fn offload<T: Send + 'static>(
    key: &[u8],
    f: impl FnOnce(&[u8], &tokio_util::sync::CancellationToken) -> Result<T, ArchiveError>
    + Send
    + 'static,
) -> Result<T, ArchiveError> {
    crate::check_key(key)?;
    let key = key.to_vec();
    let cancel = tokio_util::sync::CancellationToken::new();
    let _cancel_on_drop = cancel.clone().drop_guard();
    tokio::task::spawn_blocking(move || f(&key, &cancel))
        .await
        .map_err(io::Error::other)?
}

/// Exports to a sibling temporary file until `cancel` fires, then renames it
/// over `path`. A failed or cancelled export removes the temporary file and
/// leaves any archive already at `path` untouched.
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
    let mut create = std::fs::OpenOptions::new();
    create.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut create, 0o600);
    // Created before the cleanup below, which may then only remove a file
    // this export owns.
    let file = create.open(&partial)?;
    let exported = (|| -> Result<_, ArchiveError> {
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
