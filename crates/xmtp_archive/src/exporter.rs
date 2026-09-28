//! Archive writing. [`export`] writes the version and the starting nonce in the
//! clear, then one zstd stream of length-prefixed AES-GCM frames: the metadata,
//! then every element of one database snapshot. Frames are written as rows are
//! read, so memory does not grow with the history.

use super::BACKUP_VERSION;
use crate::archive_options::ArchiveOptions;
use crate::{ArchiveError, NONCE_SIZE, snapshot, util::GenericArrayExt};
use aes_gcm::{Aes256Gcm, KeyInit, aead::Aead};
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
use xmtp_common::time::now_ns;
use xmtp_db::ConnectionExt;
use xmtp_proto::xmtp::device_sync::{
    BackupElement, BackupElementSelection as BackupElementSelectionProto, BackupMetadataSave,
    backup_element::Element,
};

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
    let exported_at_ns = now_ns();
    let metadata = BackupMetadataSave {
        elements: options
            .elements
            .iter()
            .map(|&e| BackupElementSelectionProto::from(e) as i32)
            .collect(),
        exported_at_ns,
        start_ns: options.start_ns,
        end_ns: options.end_ns,
    };
    let nonce = xmtp_common::rand_array::<NONCE_SIZE>();
    sink.write_all(&BACKUP_VERSION.to_le_bytes())?;
    sink.write_all(&nonce)?;

    #[allow(deprecated)]
    let cipher = Aes256Gcm::new(GenericArray::from_slice(key));
    #[allow(deprecated)]
    let mut nonce = GenericArray::clone_from_slice(&nonce);
    let mut zstd = ZstdEncoder::new(AllowStdIo::new(sink));
    let mut write = |element: Element| -> Result<(), ArchiveError> {
        let plaintext = BackupElement {
            element: Some(element),
        }
        .encode_to_vec();
        let ciphertext = cipher.encrypt(&nonce, &*plaintext)?;
        nonce.increment();
        ready(zstd.write_all(&(ciphertext.len() as u32).to_le_bytes()))?;
        Ok(ready(zstd.write_all(&ciphertext))?)
    };
    write(Element::Metadata(metadata.clone()))?;
    snapshot::read(&db, &options, exported_at_ns, write)?;
    ready(zstd.close())?;
    Ok(metadata)
}

/// An archive as an [`AsyncRead`] byte stream, for callers that consume one.
/// [`ArchiveExporter::new`] runs [`export`] into chunks that reads release, so
/// memory peaks near one copy of the archive. An export failure is returned by
/// every read as an [`io::Error`] with its message, and no archive byte is served.
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

    /// Exports to a new file at `path`, as [`export`], and removes the file
    /// if the export fails.
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn export_to_file(
        options: ArchiveOptions,
        db: impl ConnectionExt,
        path: impl AsRef<std::path::Path>,
        key: &[u8],
    ) -> Result<BackupMetadataSave, ArchiveError> {
        let path = path.as_ref();
        let mut file = io::BufWriter::new(std::fs::File::create(path)?);
        let exported = export(options, db, key, &mut file).and_then(|metadata| {
            io::Write::flush(&mut file)?;
            Ok(metadata)
        });
        if exported.is_err() {
            drop(file);
            let _ = std::fs::remove_file(path);
        }
        exported
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
