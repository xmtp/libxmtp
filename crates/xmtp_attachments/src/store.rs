//! Local attachment files, rooted below one attachments directory.

use std::time::Duration;

use crate::{AttachmentError, AttachmentFailureCause as Cause};

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
mod opfs;

#[cfg(not(target_arch = "wasm32"))]
pub use native::NativeStore;
#[cfg(target_arch = "wasm32")]
pub use opfs::OpfsStore;

/// The maximum amount of data buffered for one file operation.
pub const CHUNK_SIZE: usize = 64 * 1024;

/// A path for staged ciphertext. It cannot collide with an attachment key.
pub fn staged_path(content_digest: &str) -> Result<String, AttachmentError> {
    if content_digest.len() != 64
        || !content_digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(AttachmentError::new(Cause::Malformed));
    }
    Ok(format!(".staged/{content_digest}"))
}

/// A path for a temporary file. Temporary files never sit under key directories.
pub fn temporary_path(name: &str) -> Result<String, AttachmentError> {
    validate_relative(name)?;
    if name.contains('/') {
        return Err(AttachmentError::new(Cause::Malformed));
    }
    Ok(format!(".tmp/{name}"))
}

pub(crate) fn validate_relative(path: &str) -> Result<(), AttachmentError> {
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\\')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(AttachmentError::new(Cause::Malformed));
    }
    Ok(())
}

pub(crate) fn validate_temp(path: &str) -> Result<(), AttachmentError> {
    validate_relative(path)?;
    if !path.starts_with(".tmp/") {
        return Err(AttachmentError::new(Cause::Malformed));
    }
    Ok(())
}

/// A staged file that can be uploaded without loading it into memory.
#[derive(Clone)]
pub struct StagedFile {
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) path: std::path::PathBuf,
    #[cfg(target_arch = "wasm32")]
    pub(crate) file: web_sys::File,
}

/// A writable file used as the sink for a download.
pub struct StoreWriter {
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) file: tokio::fs::File,
    #[cfg(target_arch = "wasm32")]
    pub(crate) handle: web_sys::FileSystemSyncAccessHandle,
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait DownloadSink: xmtp_common::wasm::MaybeSend {
    async fn write(&mut self, bytes: &[u8]) -> Result<(), AttachmentError>;
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait LocalStore: xmtp_common::wasm::MaybeSend + xmtp_common::wasm::MaybeSync {
    async fn open_read(&self, path: &str) -> Result<StagedFile, AttachmentError>;
    async fn create_temp(&self, path: &str) -> Result<StoreWriter, AttachmentError>;
    async fn rename(&self, from: &str, to: &str) -> Result<(), AttachmentError>;
    async fn remove_dir_all(&self, path: &str) -> Result<(), AttachmentError>;
    async fn exists(&self, path: &str) -> Result<bool, AttachmentError>;
    async fn sync(&self, writer: &mut StoreWriter) -> Result<(), AttachmentError>;
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl DownloadSink for StoreWriter {
    async fn write(&mut self, bytes: &[u8]) -> Result<(), AttachmentError> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            use tokio::io::AsyncWriteExt;
            self.file
                .write_all(bytes)
                .await
                .map_err(|_| AttachmentError::new(Cause::LocalStorage))
        }
        #[cfg(target_arch = "wasm32")]
        {
            let mut remaining = bytes;
            while !remaining.is_empty() {
                let count = self
                    .handle
                    .write_with_u8_array(remaining)
                    .map_err(|_| AttachmentError::new(Cause::LocalStorage))?
                    as usize;
                if count == 0 {
                    return Err(AttachmentError::new(Cause::LocalStorage));
                }
                remaining = &remaining[count..];
            }
            Ok(())
        }
    }
}

/// Transfer limits. The caller supplies the snapshot default in `get`.
#[derive(Clone, Debug, Default)]
pub struct AttachmentOptions {
    pub max_download_bytes: Option<u64>,
    pub allow_private_network: bool,
    pub max_pending_age: Option<Duration>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // verifies: ATCH-049
    #[xmtp_common::test(unwrap_try = true)]
    fn staged_outside_key_dirs() {
        let digest = "a".repeat(64);
        assert_eq!(staged_path(&digest)?, format!(".staged/{digest}"));
        assert_eq!(temporary_path("nonce")?, ".tmp/nonce");
        assert!(staged_path("../bad").is_err());
        assert!(temporary_path("../bad").is_err());
        assert!(staged_path(&"A".repeat(64)).is_err());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_common::test(unwrap_try = true)]
    async fn native_store_paths() {
        let directory = tempfile::tempdir()?;
        let store = NativeStore::new(directory.path()).await?;
        assert!(store.create_temp("key/file").await.is_err());
        let mut writer = store.create_temp(".tmp/a").await?;
        writer.write(b"saved").await?;
        store.sync(&mut writer).await?;
        drop(writer);
        assert!(store.exists(".tmp/a").await?);
        store.rename(".tmp/a", "key/file").await?;
        assert!(!store.exists(".tmp/a").await?);
        assert!(store.exists("key/file").await?);
        store.open_read("key/file").await?;
        store.remove_dir_all("key").await?;
        assert!(!store.exists("key/file").await?);
    }
}
