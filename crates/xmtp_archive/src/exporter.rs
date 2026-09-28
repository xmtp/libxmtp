//! Archive writing. [`export`] writes the version and the starting nonce in the
//! clear, then one zstd stream of length-prefixed AES-GCM frames: the metadata,
//! then every element of one database snapshot. Frames are written as rows are
//! read, so memory does not grow with the history.

use super::BACKUP_VERSION;
use crate::archive_options::ArchiveOptions;
use crate::{ArchiveError, NONCE_SIZE, snapshot, util::GenericArrayExt};
use aes_gcm::{Aes256Gcm, KeyInit, aead::Aead};
use async_compression::futures::write::ZstdEncoder;
use futures::{FutureExt, io::AllowStdIo};
use futures_util::AsyncWriteExt;
use prost::Message;
#[allow(deprecated)]
use sha2::digest::generic_array::GenericArray;
use std::io;
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

/// Exports to a new file at `path`, as [`export`], and removes the file if the
/// export fails.
#[cfg(not(target_arch = "wasm32"))]
pub fn export_to_file(
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

/// Resolves an encoder operation over a synchronous sink, which never pends.
fn ready(op: impl Future<Output = io::Result<()>>) -> io::Result<()> {
    op.now_or_never().unwrap_or_else(|| {
        Err(io::Error::other(
            "synchronous archive sink returned pending",
        ))
    })
}
