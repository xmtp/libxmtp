//! Archive writing. [`export`] writes the version and the starting nonce in the
//! clear, then one zstd stream of length-prefixed AES-GCM frames: the metadata,
//! then every element of one database snapshot. Frames are written as rows are
//! read, so memory does not grow with the history.

use super::BACKUP_VERSION;
use crate::archive_options::ArchiveOptions;
use crate::{ArchiveError, NONCE_SIZE, snapshot, util::GenericArrayExt};
use aes_gcm::aead::Aead;
use async_compression::futures::write::ZstdEncoder;
use futures::{AsyncRead, FutureExt, io::AllowStdIo};
use futures_util::AsyncWriteExt;
use prost::Message;
#[allow(deprecated)]
use sha2::digest::generic_array::GenericArray;
use std::{
    collections::VecDeque,
    io,
    pin::Pin,
    task::{Context, Poll},
};
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

/// An archive as an [`AsyncRead`] byte stream, for callers that consume one
/// (the wasm binding). [`ArchiveExporter::new`] runs [`export`] into chunks
/// that reads release, so memory peaks near one copy of the archive. A caller
/// that only wants the bytes should pass its buffer to [`export`] instead. An
/// export failure is returned by every read as an [`io::Error`] with its
/// message, and no archive byte is served.
pub struct ArchiveExporter {
    archive: Result<VecDeque<Vec<u8>>, String>,
}

impl ArchiveExporter {
    pub fn new(options: ArchiveOptions, db: impl ConnectionExt, key: &[u8]) -> Self {
        let mut chunks = Chunks::default();
        let archive = export(options, db, key, &mut chunks)
            .map(|_| chunks.0)
            .map_err(|e| e.to_string());
        Self { archive }
    }

    /// Exports to a new file at `path`, as [`export`], on tokio's blocking
    /// pool so the snapshot never stalls an async worker, and removes the file
    /// if the export fails. Dropping the future cancels the export at its next
    /// write, which removes the file too. Must be called within a tokio runtime.
    #[cfg(not(target_arch = "wasm32"))]
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

/// Exports to a new file at `path` until `cancel` fires, and removes the file
/// if the export fails or is cancelled.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn write_file(
    options: ArchiveOptions,
    db: impl ConnectionExt,
    path: &std::path::Path,
    key: &[u8],
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<BackupMetadataSave, ArchiveError> {
    let mut file = io::BufWriter::new(std::fs::File::create(path)?);
    let exported = export(options, db, key, Cancellable(&mut file, cancel)).and_then(|metadata| {
        io::Write::flush(&mut file)?;
        Ok(metadata)
    });
    if exported.is_err() {
        drop(file);
        if let Err(e) = std::fs::remove_file(path) {
            tracing::warn!(path = %path.display(), error = %e, "failed export left a partial archive");
        }
    }
    exported
}

/// A sink that fails every write once its token is cancelled.
#[cfg(not(target_arch = "wasm32"))]
struct Cancellable<'a, W>(W, &'a tokio_util::sync::CancellationToken);

#[cfg(not(target_arch = "wasm32"))]
impl<W: io::Write> io::Write for Cancellable<'_, W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.1.is_cancelled() {
            return Err(io::Error::other("archive export cancelled"));
        }
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl AsyncRead for ArchiveExporter {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let chunks = match &mut self.get_mut().archive {
            Ok(chunks) => chunks,
            Err(error) => return Poll::Ready(Err(io::Error::other(error.clone()))),
        };
        let Some(chunk) = chunks.front_mut() else {
            return Poll::Ready(Ok(0));
        };
        let amount = chunk.len().min(buf.len());
        buf[..amount].copy_from_slice(&chunk[..amount]);
        chunk.drain(..amount);
        if chunk.is_empty() {
            chunks.pop_front();
        }
        Poll::Ready(Ok(amount))
    }
}

/// A sink that keeps each write as a chunk, so a reader can release them.
#[derive(Default)]
struct Chunks(VecDeque<Vec<u8>>);

impl io::Write for Chunks {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.push_back(buf.to_vec());
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
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
