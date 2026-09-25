use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    FileSystemDirectoryHandle, FileSystemFileHandle, FileSystemGetDirectoryOptions,
    FileSystemGetFileOptions, FileSystemRemoveOptions, FileSystemSyncAccessHandle,
    WorkerGlobalScope,
};

use super::{LocalStore, StagedFile, StoreWriter, validate_relative, validate_temp};
use crate::{AttachmentError, AttachmentFailureCause as Cause};

fn storage_error(_: impl Sized) -> AttachmentError {
    AttachmentError::new(Cause::LocalStorage)
}

fn lookup_absent(error: &JsValue) -> Result<bool, AttachmentError> {
    match error
        .dyn_ref::<web_sys::DomException>()
        .map(|e| e.name())
        .as_deref()
    {
        Some("NotFoundError") => Ok(true),
        Some("TypeMismatchError") => Ok(false),
        _ => Err(storage_error(())),
    }
}

/// Files below one OPFS attachments directory. Call from a dedicated worker.
#[derive(Clone)]
pub struct OpfsStore {
    root: FileSystemDirectoryHandle,
}

impl OpfsStore {
    pub async fn new(path: &str) -> Result<Self, AttachmentError> {
        validate_relative(path)?;
        let global: WorkerGlobalScope = js_sys::global().unchecked_into();
        let root = JsFuture::from(global.navigator().storage().get_directory())
            .await
            .map_err(storage_error)?
            .dyn_into::<FileSystemDirectoryHandle>()
            .map_err(storage_error)?;
        let root = Self::directories(root, path, true).await?;
        Ok(Self { root })
    }

    async fn directories(
        mut directory: FileSystemDirectoryHandle,
        path: &str,
        create: bool,
    ) -> Result<FileSystemDirectoryHandle, AttachmentError> {
        for part in path.split('/') {
            let options = FileSystemGetDirectoryOptions::new();
            options.set_create(create);
            directory = JsFuture::from(directory.get_directory_handle_with_options(part, &options))
                .await
                .map_err(storage_error)?
                .dyn_into()
                .map_err(storage_error)?;
        }
        Ok(directory)
    }

    async fn parent(
        &self,
        path: &str,
        create: bool,
    ) -> Result<(FileSystemDirectoryHandle, String), AttachmentError> {
        validate_relative(path)?;
        let (parent, name) = path.rsplit_once('/').unwrap_or(("", path));
        let directory = if parent.is_empty() {
            self.root.clone()
        } else {
            Self::directories(self.root.clone(), parent, create).await?
        };
        Ok((directory, name.to_owned()))
    }

    async fn file_handle(
        &self,
        path: &str,
        create: bool,
    ) -> Result<FileSystemFileHandle, AttachmentError> {
        let (parent, name) = self.parent(path, create).await?;
        let options = FileSystemGetFileOptions::new();
        options.set_create(create);
        JsFuture::from(parent.get_file_handle_with_options(&name, &options))
            .await
            .map_err(storage_error)?
            .dyn_into()
            .map_err(storage_error)
    }

    async fn create_file(&self, path: &str) -> Result<StoreWriter, AttachmentError> {
        if self.exists(path).await? {
            return Err(AttachmentError::new(Cause::LocalStorage));
        }
        let file = self.file_handle(path, true).await?;
        let handle = JsFuture::from(file.create_sync_access_handle())
            .await
            .map_err(storage_error)?
            .dyn_into::<FileSystemSyncAccessHandle>()
            .map_err(storage_error)?;
        Ok(StoreWriter { handle, offset: 0 })
    }
}

#[async_trait::async_trait(?Send)]
impl LocalStore for OpfsStore {
    async fn open_read(&self, path: &str) -> Result<StagedFile, AttachmentError> {
        let file = JsFuture::from(self.file_handle(path, false).await?.get_file())
            .await
            .map_err(storage_error)?
            .dyn_into()
            .map_err(storage_error)?;
        Ok(StagedFile { file })
    }

    async fn create_temp(&self, path: &str) -> Result<StoreWriter, AttachmentError> {
        validate_temp(path)?;
        self.create_file(path).await
    }

    async fn rename(&self, from: &str, to: &str) -> Result<(), AttachmentError> {
        if self.exists(to).await? {
            return Err(AttachmentError::new(Cause::LocalStorage));
        }
        let source = self.file_handle(from, false).await?;
        let (target_parent, target_name) = self.parent(to, true).await?;
        let move_method = js_sys::Reflect::get(source.as_ref(), &JsValue::from_str("move"))
            .map_err(storage_error)?;
        let move_method = move_method
            .dyn_ref::<js_sys::Function>()
            .ok_or(AttachmentError::new(Cause::LocalStorage))?;
        let promise = move_method
            .call2(
                source.as_ref(),
                target_parent.as_ref(),
                &JsValue::from_str(&target_name),
            )
            .map_err(storage_error)?
            .dyn_into::<js_sys::Promise>()
            .map_err(storage_error)?;
        JsFuture::from(promise).await.map_err(storage_error)?;
        Ok(())
    }

    async fn remove_dir_all(&self, path: &str) -> Result<(), AttachmentError> {
        let (parent, name) = self.parent(path, false).await?;
        let options = FileSystemRemoveOptions::new();
        options.set_recursive(true);
        JsFuture::from(parent.remove_entry_with_options(&name, &options))
            .await
            .map_err(storage_error)?;
        Ok(())
    }

    async fn remove_file(&self, path: &str) -> Result<(), AttachmentError> {
        let (parent, name) = self.parent(path, false).await?;
        JsFuture::from(parent.remove_entry(&name))
            .await
            .map_err(storage_error)?;
        Ok(())
    }

    async fn exists(&self, path: &str) -> Result<bool, AttachmentError> {
        validate_relative(path)?;
        let (parent_path, name) = path.rsplit_once('/').unwrap_or(("", path));
        let mut parent = self.root.clone();
        if !parent_path.is_empty() {
            for part in parent_path.split('/') {
                parent = match JsFuture::from(parent.get_directory_handle(part)).await {
                    Ok(handle) => handle.dyn_into().map_err(storage_error)?,
                    Err(error) => {
                        lookup_absent(&error)?;
                        return Ok(false);
                    }
                };
            }
        }

        let file_is_other_kind = match JsFuture::from(parent.get_file_handle(name)).await {
            Ok(_) => return Ok(true),
            Err(error) => !lookup_absent(&error)?,
        };
        let directory_is_other_kind = match JsFuture::from(parent.get_directory_handle(name)).await
        {
            Ok(_) => return Ok(true),
            Err(error) => !lookup_absent(&error)?,
        };
        Ok(file_is_other_kind || directory_is_other_kind)
    }

    async fn sync(&self, writer: &mut StoreWriter) -> Result<(), AttachmentError> {
        writer.handle.flush().map_err(storage_error)
    }
}

impl Drop for StoreWriter {
    fn drop(&mut self) {
        self.handle.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::DownloadSink;

    #[xmtp_common::test(unwrap_try = true)]
    fn opfs_lookup_errors_are_not_absence() {
        let error = |name| {
            web_sys::DomException::new_with_message_and_name("lookup failed", name)
                .map_err(storage_error)
        };
        assert!(lookup_absent(error("NotFoundError")?.as_ref())?);
        assert!(!lookup_absent(error("TypeMismatchError")?.as_ref())?);
        for name in ["UnknownError", "InvalidStateError"] {
            assert_eq!(
                lookup_absent(error(name)?.as_ref()).unwrap_err().cause,
                Cause::LocalStorage,
                "{name}"
            );
        }
    }

    // verifies: ATCH-048
    #[xmtp_common::test(unwrap_try = true)]
    async fn opfs_paths() {
        let store = OpfsStore::new("attachment-tests/lane-c").await?;
        assert!(store.create_temp("key/file").await.is_err());
        let path = ".tmp/opfs-path-test";
        if store.exists(path).await? {
            store.remove_dir_all(".tmp").await?;
        }
        let mut writer = store.create_temp(path).await?;
        writer.write(b"OPFS").await?;
        store.sync(&mut writer).await?;
        drop(writer);
        assert!(store.exists(path).await?);
        assert_eq!(store.open_read(path).await?.file.size(), 4.0);
        store.rename(path, "key/file").await?;
        assert!(!store.exists(path).await?);
        assert!(store.exists("key/file").await?);
        store.remove_dir_all("key").await?;
        assert!(!store.exists("key/file").await?);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn opfs_writer_reset_rewinds() {
        let store = OpfsStore::new("attachment-tests/lane-c-reset").await?;
        if store.exists(".tmp/repeated").await? {
            store.remove_dir_all(".tmp").await?;
        }
        let mut writer = store.create_temp(".tmp/repeated").await?;
        writer
            .write_content(crate::ContentChunk::Bytes(b"first"))
            .await?;
        writer.write_content(crate::ContentChunk::Reset).await?;
        writer
            .write_content(crate::ContentChunk::Bytes(b"second"))
            .await?;
        store.sync(&mut writer).await?;
        drop(writer);
        let file = store.open_read(".tmp/repeated").await?.file;
        let bytes = JsFuture::from(file.array_buffer()).await?;
        assert_eq!(js_sys::Uint8Array::new(&bytes).to_vec(), b"second");
    }
}
