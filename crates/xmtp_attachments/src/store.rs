//! Local attachment files, rooted below one attachments directory.

use std::time::Duration;

use crate::{
    AttachmentDecoder, AttachmentError, AttachmentFailureCause as Cause, ContentChunk, DecodedMeta,
    sanitize::is_reserved_device_name,
};

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
mod opfs;

#[cfg(not(target_arch = "wasm32"))]
pub use native::{NativeStore, create_private_directory};
#[cfg(target_arch = "wasm32")]
pub use opfs::OpfsStore;

/// The maximum amount of data buffered for one file operation.
pub const CHUNK_SIZE: usize = 64 * 1024;

#[cfg(target_arch = "wasm32")]
const MAX_EXACT_JS_OFFSET: u64 = (1_u64 << 53) - 1;

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

pub(crate) fn is_reconcile_dir(name: &str) -> bool {
    name == ".tmp"
        || name == ".staged"
        || (name.len() == 64
            && name
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
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
        || path.split('/').any(|part| {
            part.is_empty()
                || part == "."
                || part == ".."
                || part.contains(':')
                || is_reserved_device_name(part)
        })
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
    /// A file opened by the native store without following a path link.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) opened: Option<std::sync::Arc<std::fs::File>>,
    #[cfg(target_arch = "wasm32")]
    pub(crate) file: web_sys::File,
}

#[cfg(all(test, not(target_arch = "wasm32")))]
impl StagedFile {
    pub(crate) fn from_path_for_test(path: std::path::PathBuf) -> Self {
        Self { path, opened: None }
    }
}

/// A writable file used as the sink for a download.
pub struct StoreWriter {
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) file: tokio::fs::File,
    #[cfg(target_arch = "wasm32")]
    pub(crate) handle: web_sys::FileSystemSyncAccessHandle,
    #[cfg(target_arch = "wasm32")]
    pub(crate) offset: u64,
}

impl StoreWriter {
    /// Apply a decoder content event to this temporary file.
    pub async fn write_content(&mut self, chunk: ContentChunk<'_>) -> Result<(), AttachmentError> {
        match chunk {
            ContentChunk::Reset => {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    use tokio::io::AsyncSeekExt;
                    self.file
                        .set_len(0)
                        .await
                        .map_err(|_| AttachmentError::new(Cause::LocalStorage))?;
                    self.file
                        .rewind()
                        .await
                        .map(|_| ())
                        .map_err(|_| AttachmentError::new(Cause::LocalStorage))
                }
                #[cfg(target_arch = "wasm32")]
                {
                    self.handle
                        .truncate_with_u32(0)
                        .map_err(|_| AttachmentError::new(Cause::LocalStorage))?;
                    self.offset = 0;
                    Ok(())
                }
            }
            ContentChunk::Bytes(bytes) => self.write(bytes).await,
        }
    }
}

/// A file found during attachment storage reconciliation.
pub struct StoreFile {
    pub path: String,
    pub modified_at_ns: i64,
}

/// The result of a move that must not replace an existing file.
#[derive(Debug, thiserror::Error)]
pub enum StoreMoveError {
    #[error("attachment destination exists")]
    DestinationExists,
    #[error(transparent)]
    Other(#[from] AttachmentError),
}

impl StoreMoveError {
    pub fn cause(&self) -> Cause {
        match self {
            Self::DestinationExists => Cause::LocalStorage,
            Self::Other(error) => error.cause,
        }
    }
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
    /// Create a temporary file. Callers must use unique names because OPFS
    /// cannot create a file exclusively.
    async fn create_temp(&self, path: &str) -> Result<StoreWriter, AttachmentError>;
    /// Return true when this call created the directory.
    async fn create_dir_if_absent(&self, path: &str) -> Result<bool, AttachmentError>;
    /// Move a file only when the destination does not exist.
    async fn rename(&self, from: &str, to: &str) -> Result<(), StoreMoveError>;
    /// Atomically replace the destination with a completed temporary file.
    async fn replace(&self, from: &str, to: &str) -> Result<(), AttachmentError>;
    async fn remove_dir_all(&self, path: &str) -> Result<(), AttachmentError>;
    /// Remove only an empty directory.
    async fn remove_empty_dir(&self, path: &str) -> Result<(), AttachmentError>;
    async fn remove_file(&self, path: &str) -> Result<(), AttachmentError>;
    async fn exists(&self, path: &str) -> Result<bool, AttachmentError>;
    async fn sync(&self, writer: &mut StoreWriter) -> Result<(), AttachmentError>;
    async fn finish_decode(
        &self,
        decoder: AttachmentDecoder,
        source: &str,
        output: &str,
    ) -> Result<DecodedMeta, AttachmentError>;
    /// List files one level below the managed directories. Do not enter app directories.
    async fn list_files(&self) -> Result<Vec<StoreFile>, AttachmentError>;
}

impl StagedFile {
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn read_chunk(
        &self,
        offset: u64,
        max_bytes: usize,
    ) -> Result<Vec<u8>, AttachmentError> {
        use tokio::io::{AsyncReadExt as _, AsyncSeekExt as _};
        let mut file = tokio::fs::File::open(&self.path)
            .await
            .map_err(|_| AttachmentError::new(Cause::LocalStorage))?;
        file.seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(|_| AttachmentError::new(Cause::LocalStorage))?;
        let mut bytes = vec![0; max_bytes];
        let count = file
            .read(&mut bytes)
            .await
            .map_err(|_| AttachmentError::new(Cause::LocalStorage))?;
        bytes.truncate(count);
        Ok(bytes)
    }

    #[cfg(target_arch = "wasm32")]
    pub fn len(&self) -> u64 {
        self.file.size() as u64
    }

    #[cfg(target_arch = "wasm32")]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Read at most one chunk from an OPFS source file.
    #[cfg(target_arch = "wasm32")]
    pub async fn read_chunk(
        &self,
        offset: u64,
        max_bytes: usize,
    ) -> Result<Vec<u8>, AttachmentError> {
        use wasm_bindgen_futures::JsFuture;
        let end = offset.saturating_add(max_bytes as u64).min(self.len());
        if offset >= end {
            return Ok(Vec::new());
        }
        let slice = self
            .file
            .slice_with_f64_and_f64(offset as f64, end as f64)
            .map_err(|_| AttachmentError::new(Cause::LocalStorage))?;
        let buffer = JsFuture::from(slice.array_buffer())
            .await
            .map_err(|_| AttachmentError::new(Cause::LocalStorage))?;
        Ok(js_sys::Uint8Array::new(&buffer).to_vec())
    }

    /// Hash the stored ciphertext in fixed-size chunks before upload.
    pub async fn sha256(&self) -> Result<([u8; 32], u64), AttachmentError> {
        use sha2::{Digest as _, Sha256};
        let mut hash = Sha256::new();
        let mut length = 0u64;
        #[cfg(not(target_arch = "wasm32"))]
        {
            use tokio::io::AsyncReadExt as _;
            let mut file = tokio::fs::File::open(&self.path)
                .await
                .map_err(|_| AttachmentError::new(Cause::LocalStorage))?;
            let mut chunk = [0u8; CHUNK_SIZE];
            loop {
                let count = file
                    .read(&mut chunk)
                    .await
                    .map_err(|_| AttachmentError::new(Cause::LocalStorage))?;
                if count == 0 {
                    break;
                }
                hash.update(&chunk[..count]);
                length += count as u64;
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let size = self.len();
            while length < size {
                let bytes = self.read_chunk(length, CHUNK_SIZE).await?;
                if bytes.is_empty() {
                    return Err(AttachmentError::new(Cause::LocalStorage));
                }
                hash.update(&bytes);
                length += bytes.len() as u64;
            }
        }
        Ok((hash.finalize().into(), length))
    }
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
                if self.offset > MAX_EXACT_JS_OFFSET {
                    return Err(AttachmentError::new(Cause::LocalStorage));
                }
                let options = web_sys::FileSystemReadWriteOptions::new();
                options.set_at(self.offset as f64);
                let count = self
                    .handle
                    .write_with_u8_array_and_options(remaining, &options)
                    .map_err(|_| AttachmentError::new(Cause::LocalStorage))?
                    as usize;
                if count == 0 || count > remaining.len() {
                    return Err(AttachmentError::new(Cause::LocalStorage));
                }
                self.offset = self
                    .offset
                    .checked_add(count as u64)
                    .ok_or(AttachmentError::new(Cause::LocalStorage))?;
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

    #[xmtp_common::test(unwrap_try = true)]
    fn validate_relative_rejects_drive_and_stream_forms() {
        for path in ["C:/outside", "C:relative", "key/a:b"] {
            assert_eq!(
                validate_relative(path).unwrap_err().cause,
                Cause::Malformed,
                "{path}"
            );
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn validate_relative_rejects_device_names() {
        for path in ["CON", "key/nul.txt", "COM1", "lpt³.bin"] {
            assert_eq!(
                validate_relative(path).unwrap_err().cause,
                Cause::Malformed,
                "{path}"
            );
        }
        for path in ["CONOUT$X", "COM0"] {
            validate_relative(path)?;
        }
    }

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

    // verifies: ATCH-077
    #[cfg(unix)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn native_create_temp_rejects_symlinked_parent() {
        let root = tempfile::tempdir()?;
        let outside = tempfile::tempdir()?;
        let store = NativeStore::new(root.path()).await?;
        std::os::unix::fs::symlink(outside.path(), root.path().join(".tmp"))?;

        assert_eq!(
            store.create_temp(".tmp/file").await.err().unwrap().cause,
            Cause::LocalStorage
        );
        assert!(!outside.path().join("file").exists());
    }

    #[cfg(unix)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn native_open_read_rejects_symlinked_parent() {
        let root = tempfile::tempdir()?;
        let outside = tempfile::tempdir()?;
        std::fs::write(outside.path().join("file"), b"outside")?;
        let store = NativeStore::new(root.path()).await?;
        std::os::unix::fs::symlink(outside.path(), root.path().join("key"))?;

        assert_eq!(
            store.open_read("key/file").await.err().unwrap().cause,
            Cause::LocalStorage
        );
        assert_eq!(std::fs::read(outside.path().join("file"))?, b"outside");
    }

    #[cfg(unix)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn native_open_read_rejects_symlinked_file() {
        let root = tempfile::tempdir()?;
        let outside = tempfile::tempdir()?;
        std::fs::create_dir(root.path().join("key"))?;
        std::fs::write(outside.path().join("file"), b"outside")?;
        let store = NativeStore::new(root.path()).await?;
        std::os::unix::fs::symlink(outside.path().join("file"), root.path().join("key/file"))?;

        assert_eq!(
            store.open_read("key/file").await.err().unwrap().cause,
            Cause::LocalStorage
        );
        assert_eq!(std::fs::read(outside.path().join("file"))?, b"outside");
    }

    #[cfg(unix)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn native_rename_rejects_symlinked_destination() {
        let root = tempfile::tempdir()?;
        let outside = tempfile::tempdir()?;
        let store = NativeStore::new(root.path()).await?;
        let mut writer = store.create_temp(".tmp/source").await?;
        writer.write(b"source").await?;
        store.sync(&mut writer).await?;
        drop(writer);
        std::os::unix::fs::symlink(outside.path(), root.path().join("key"))?;

        assert_eq!(
            store
                .rename(".tmp/source", "key/file")
                .await
                .unwrap_err()
                .cause,
            Cause::LocalStorage
        );
        assert!(!outside.path().join("file").exists());
        assert_eq!(std::fs::read(root.path().join(".tmp/source"))?, b"source");
    }

    #[cfg(unix)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn native_rename_rejects_symlinked_source() {
        let root = tempfile::tempdir()?;
        let outside = tempfile::tempdir()?;
        std::fs::write(outside.path().join("file"), b"outside")?;
        let store = NativeStore::new(root.path()).await?;
        std::os::unix::fs::symlink(outside.path(), root.path().join("key"))?;

        assert_eq!(
            store
                .rename("key/file", "safe/file")
                .await
                .unwrap_err()
                .cause,
            Cause::LocalStorage
        );
        assert_eq!(std::fs::read(outside.path().join("file"))?, b"outside");
        assert!(!root.path().join("safe/file").exists());
    }

    #[cfg(unix)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn native_exists_rejects_symlinked_parent() {
        let root = tempfile::tempdir()?;
        let outside = tempfile::tempdir()?;
        std::fs::write(outside.path().join("file"), b"outside")?;
        let store = NativeStore::new(root.path()).await?;
        std::os::unix::fs::symlink(outside.path(), root.path().join("key"))?;

        assert_eq!(
            store.exists("key/file").await.unwrap_err().cause,
            Cause::LocalStorage
        );
        assert_eq!(std::fs::read(outside.path().join("file"))?, b"outside");
    }

    #[cfg(unix)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn native_exists_rejects_symlinked_file() {
        let root = tempfile::tempdir()?;
        let outside = tempfile::tempdir()?;
        std::fs::create_dir(root.path().join("key"))?;
        std::fs::write(outside.path().join("file"), b"outside")?;
        let store = NativeStore::new(root.path()).await?;
        std::os::unix::fs::symlink(outside.path().join("file"), root.path().join("key/file"))?;

        assert_eq!(
            store.exists("key/file").await.unwrap_err().cause,
            Cause::LocalStorage
        );
        assert_eq!(std::fs::read(outside.path().join("file"))?, b"outside");
    }

    #[cfg(unix)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn native_remove_dir_rejects_symlinked_parent() {
        let root = tempfile::tempdir()?;
        let outside = tempfile::tempdir()?;
        std::fs::create_dir(outside.path().join("sub"))?;
        std::fs::write(outside.path().join("sub/file"), b"outside")?;
        let store = NativeStore::new(root.path()).await?;
        std::os::unix::fs::symlink(outside.path(), root.path().join("key"))?;

        assert_eq!(
            store.remove_dir_all("key/sub").await.unwrap_err().cause,
            Cause::LocalStorage
        );
        assert_eq!(std::fs::read(outside.path().join("sub/file"))?, b"outside");
    }

    #[cfg(unix)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn native_plaintext_files_are_owner_only() {
        use std::os::unix::fs::PermissionsExt as _;
        let directory = tempfile::tempdir()?;
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o755))?;
        let store = NativeStore::new(directory.path()).await?;
        assert_eq!(
            std::fs::metadata(directory.path())?.permissions().mode() & 0o777,
            0o755
        );
        let mut writer = store.create_temp(".tmp/plaintext").await?;
        writer.write(b"private").await?;
        drop(writer);
        for path in [".tmp", ".tmp/plaintext"] {
            let mode = std::fs::metadata(directory.path().join(path))?
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, if path == ".tmp" { 0o700 } else { 0o600 });
        }
        store.rename(".tmp/plaintext", "key/plaintext").await?;
        assert_eq!(
            std::fs::metadata(directory.path().join("key"))?
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(directory.path().join("key/plaintext"))?
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    // verifies: ATCH-077
    #[cfg(unix)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn chmod_failure_does_not_stop_native_store() {
        use std::os::unix::fs::PermissionsExt as _;
        let directory = tempfile::tempdir()?;
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o755))?;
        let store = NativeStore::new(directory.path())
            .await?
            .with_forced_chmod_error();
        assert_eq!(
            std::fs::metadata(directory.path())?.permissions().mode() & 0o777,
            0o755
        );
        let mut writer = store.create_temp(".tmp/plaintext").await?;
        writer.write(b"private").await?;
        drop(writer);
        store.rename(".tmp/plaintext", "key/plaintext").await?;
        assert!(store.exists("key/plaintext").await?);
    }

    // verifies: ATCH-077
    #[cfg(unix)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn missing_native_root_is_private_with_zero_umask() {
        use std::os::unix::fs::PermissionsExt as _;

        struct RestoreUmask(libc::mode_t);
        impl Drop for RestoreUmask {
            fn drop(&mut self) {
                unsafe { libc::umask(self.0) };
            }
        }

        let directory = tempfile::tempdir()?;
        let root = directory.path().join("parent/root");
        let previous = unsafe { libc::umask(0) };
        let _restore = RestoreUmask(previous);
        let _store = NativeStore::new(&root).await?;
        for path in [&root, &directory.path().join("parent")] {
            assert_eq!(std::fs::metadata(path)?.permissions().mode() & 0o777, 0o700);
        }
    }

    // verifies: ATCH-077
    #[cfg(unix)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn existing_foreign_owned_directory_is_rejected() {
        let directory = tempfile::tempdir()?;
        tokio::fs::create_dir(directory.path().join(".tmp")).await?;
        let store = NativeStore::new(directory.path())
            .await?
            .with_forced_foreign_owner();
        let error = store
            .create_temp(".tmp/file")
            .await
            .err()
            .expect("foreign-owned directory must fail");
        assert_eq!(error.cause, Cause::LocalStorage);
        assert!(!directory.path().join(".tmp/file").exists());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_common::test(unwrap_try = true)]
    async fn native_reconcile_scan_stops_at_key_level() {
        let directory = tempfile::tempdir()?;
        let store = NativeStore::new(directory.path()).await?;
        let key = "a".repeat(64);
        for path in [
            format!("{key}/plain.txt"),
            format!("{key}/nested/deep.txt"),
            "app/file.txt".to_owned(),
            ".DS_Store".to_owned(),
        ] {
            let path = directory.path().join(path);
            tokio::fs::create_dir_all(path.parent().unwrap()).await?;
            tokio::fs::write(path, b"file").await?;
        }
        let files = store.list_files().await?;
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, format!("{key}/plain.txt"));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_common::test(unwrap_try = true)]
    async fn native_writer_reset_rewinds() {
        let directory = tempfile::tempdir()?;
        let store = NativeStore::new(directory.path()).await?;
        let mut writer = store.create_temp(".tmp/repeated").await?;
        writer.write_content(ContentChunk::Bytes(b"first")).await?;
        writer.write_content(ContentChunk::Reset).await?;
        writer.write_content(ContentChunk::Bytes(b"second")).await?;
        store.sync(&mut writer).await?;
        drop(writer);
        assert_eq!(
            tokio::fs::read(directory.path().join(".tmp/repeated")).await?,
            b"second"
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_common::test(unwrap_try = true)]
    async fn native_rename_refuses_existing_destination() {
        let directory = tempfile::tempdir()?;
        let store = NativeStore::new(directory.path()).await?;
        let mut source = store.create_temp(".tmp/source").await?;
        source.write(b"source").await?;
        drop(source);
        let mut destination = store.create_temp(".tmp/destination").await?;
        destination.write(b"destination").await?;
        drop(destination);
        let error = store
            .rename(".tmp/source", ".tmp/destination")
            .await
            .unwrap_err();
        assert!(matches!(error, StoreMoveError::DestinationExists));
        assert_eq!(
            tokio::fs::read(directory.path().join(".tmp/source")).await?,
            b"source"
        );
        assert_eq!(
            tokio::fs::read(directory.path().join(".tmp/destination")).await?,
            b"destination"
        );
    }

    #[cfg(any(target_vendor = "apple", target_os = "linux", target_os = "android"))]
    #[xmtp_common::test(unwrap_try = true)]
    async fn native_rename_falls_back_without_hard_links() {
        let directory = tempfile::tempdir()?;
        let store = NativeStore::new(directory.path())
            .await?
            .with_forced_hard_link_error(std::io::ErrorKind::PermissionDenied);
        let mut source = store.create_temp(".tmp/source").await?;
        source.write(b"source").await?;
        drop(source);
        store.rename(".tmp/source", "key/file").await?;
        assert!(!store.exists(".tmp/source").await?);
        assert_eq!(
            tokio::fs::read(directory.path().join("key/file")).await?,
            b"source"
        );

        let mut source = store.create_temp(".tmp/second").await?;
        source.write(b"second").await?;
        drop(source);
        let error = store.rename(".tmp/second", "key/file").await.unwrap_err();
        assert!(matches!(error, StoreMoveError::DestinationExists));
        assert_eq!(
            tokio::fs::read(directory.path().join("key/file")).await?,
            b"source"
        );
        assert_eq!(
            tokio::fs::read(directory.path().join(".tmp/second")).await?,
            b"second"
        );
    }

    #[cfg(windows)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn windows_rename_fallback_moves_without_replacing() {
        let directory = tempfile::tempdir()?;
        let store = NativeStore::new(directory.path())
            .await?
            .with_forced_hard_link_error(std::io::ErrorKind::Unsupported);
        let mut source = store.create_temp(".tmp/source").await?;
        source.write(b"source").await?;
        drop(source);
        store.rename(".tmp/source", "key/file").await?;
        assert!(!store.exists(".tmp/source").await?);
        assert_eq!(std::fs::read(directory.path().join("key/file"))?, b"source");

        let racing = store.with_fallback_destination_race(b"destination".to_vec());
        let mut source = racing.create_temp(".tmp/second").await?;
        source.write(b"second").await?;
        drop(source);
        assert_eq!(
            racing
                .rename(".tmp/second", "key/raced")
                .await
                .unwrap_err()
                .cause,
            Cause::LocalStorage
        );
        assert_eq!(
            std::fs::read(directory.path().join("key/raced"))?,
            b"destination"
        );
        assert_eq!(
            std::fs::read(directory.path().join(".tmp/second"))?,
            b"second"
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_common::test(unwrap_try = true)]
    async fn native_rename_fallback_never_replaces_racing_destination() {
        let directory = tempfile::tempdir()?;
        let store = NativeStore::new(directory.path())
            .await?
            .with_forced_hard_link_error(std::io::ErrorKind::PermissionDenied)
            .with_fallback_destination_race(b"destination".to_vec());
        let mut source = store.create_temp(".tmp/source").await?;
        source.write(b"source").await?;
        store.sync(&mut source).await?;
        drop(source);

        assert_eq!(
            store
                .rename(".tmp/source", "key/file")
                .await
                .unwrap_err()
                .cause,
            Cause::LocalStorage
        );
        assert_eq!(
            std::fs::read(directory.path().join("key/file"))?,
            b"destination"
        );
        assert_eq!(
            std::fs::read(directory.path().join(".tmp/source"))?,
            b"source"
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_common::test(unwrap_try = true)]
    async fn native_store_root_is_absolute() {
        use std::path::PathBuf;

        struct RestoreCwd(PathBuf);
        impl Drop for RestoreCwd {
            fn drop(&mut self) {
                let _ = std::env::set_current_dir(&self.0);
            }
        }

        let original = std::env::current_dir()?;
        let directory = tempfile::tempdir_in(&original)?;
        let relative = directory.path().strip_prefix(&original)?;
        let store = NativeStore::new(relative).await?;
        let elsewhere = tempfile::tempdir()?;
        let _restore = RestoreCwd(original);
        std::env::set_current_dir(elsewhere.path())?;
        let mut writer = store.create_temp(".tmp/file").await?;
        writer.write(b"original root").await?;
        drop(writer);
        assert_eq!(
            tokio::fs::read(directory.path().join(".tmp/file")).await?,
            b"original root"
        );
        assert!(!elsewhere.path().join(relative).join(".tmp/file").exists());
    }

    #[cfg(unix)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn native_created_directory_modes_ignore_umask() {
        use std::os::unix::fs::PermissionsExt;

        let base = tempfile::tempdir()?;
        let app = base.path().join("existing-app-dir");
        std::fs::create_dir(&app)?;
        std::fs::set_permissions(&app, std::fs::Permissions::from_mode(0o755))?;
        let executable = std::env::current_exe()?;
        for (mask, suffix) in [("0177", "restrictive"), ("0000", "open")] {
            let root = app.join(suffix).join("nested").join("attachments");
            let output = std::process::Command::new("sh")
                .arg("-c")
                .arg(format!(
                    "umask {mask}; exec \"$1\" --exact store::tests::native_directory_mode_child"
                ))
                .arg("sh")
                .arg(&executable)
                .env("XMTP_TEST_ATTACHMENTS_ROOT", &root)
                .output()?;
            assert!(
                output.status.success(),
                "umask {mask}: {}",
                String::from_utf8_lossy(&output.stdout)
            );
            for directory in [
                app.join(suffix),
                app.join(suffix).join("nested"),
                root.clone(),
                root.join(".tmp"),
            ] {
                assert_eq!(
                    std::fs::metadata(&directory)?.permissions().mode() & 0o777,
                    0o700,
                    "{} under umask {mask}",
                    directory.display()
                );
            }
            assert_eq!(
                std::fs::metadata(root.join(".tmp/file"))?
                    .permissions()
                    .mode()
                    & 0o777,
                0o600,
                "file under umask {mask}"
            );
            assert_eq!(std::fs::metadata(&app)?.permissions().mode() & 0o777, 0o755);
        }
    }

    #[cfg(unix)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn native_directory_mode_child() {
        let Some(root) = std::env::var_os("XMTP_TEST_ATTACHMENTS_ROOT") else {
            return;
        };
        let verify_initial = std::env::var_os("XMTP_TEST_INITIAL_MODES").is_some();
        if verify_initial {
            super::native::take_initial_modes();
        }
        let store = NativeStore::new(std::path::PathBuf::from(root)).await?;
        let mut writer = store.create_temp(".tmp/file").await?;
        writer.write(b"private").await?;
        store.sync(&mut writer).await?;
        if verify_initial {
            let modes = super::native::take_initial_modes();
            assert_eq!(
                modes
                    .iter()
                    .filter(|(_, expected)| *expected == 0o700)
                    .count(),
                4
            );
            assert_eq!(
                modes
                    .iter()
                    .filter(|(_, expected)| *expected == 0o600)
                    .count(),
                1
            );
            for (actual, expected) in modes {
                assert_eq!(
                    actual & !expected,
                    0,
                    "initial mode {actual:o} exceeds {expected:o}"
                );
            }
        }
    }

    #[cfg(unix)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn native_initial_modes_never_exceed_private() {
        let base = tempfile::tempdir()?;
        let root = base.path().join("new").join("nested").join("attachments");
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg("umask 0000; exec \"$1\" --exact store::tests::native_directory_mode_child")
            .arg("sh")
            .arg(std::env::current_exe()?)
            .env("XMTP_TEST_ATTACHMENTS_ROOT", &root)
            .env("XMTP_TEST_INITIAL_MODES", "1")
            .output()?;
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_common::test(unwrap_try = true)]
    async fn rename_rolls_back_when_source_unlink_fails() {
        let directory = tempfile::tempdir()?;
        let store = NativeStore::new(directory.path())
            .await?
            .with_forced_source_unlink_error(std::io::ErrorKind::PermissionDenied);
        let mut writer = store.create_temp(".tmp/source").await?;
        writer.write(b"saved").await?;
        drop(writer);
        assert_eq!(
            store
                .rename(".tmp/source", "key/final")
                .await
                .unwrap_err()
                .cause(),
            Cause::LocalStorage
        );
        assert!(store.exists(".tmp/source").await?);
        assert!(!store.exists("key/final").await?);
    }
}
