use std::io::{Read, Seek, SeekFrom, Write};

use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    FileSystemDirectoryHandle, FileSystemFileHandle, FileSystemGetDirectoryOptions,
    FileSystemGetFileOptions, FileSystemReadWriteOptions, FileSystemRemoveOptions,
    FileSystemSyncAccessHandle, WorkerGlobalScope,
};

use super::{
    LocalStore, StagedFile, StoreFile, StoreMoveError, StoreWriter, is_reconcile_dir,
    validate_relative, validate_temp,
};
use crate::{AttachmentDecoder, AttachmentError, AttachmentFailureCause as Cause, DecodedMeta};

#[wasm_bindgen::prelude::wasm_bindgen(inline_js = r#"
export function acquireOpfsDestinationLock(name) {
    let release;
    const held = new Promise(resolve => { release = resolve; });
    let readyResolve;
    let readyReject;
    const ready = new Promise((resolve, reject) => {
        readyResolve = resolve;
        readyReject = reject;
    });
    navigator.locks.request(name, { mode: "exclusive" }, () => {
        readyResolve();
        return held;
    }).catch(readyReject);
    return [ready, release];
}
"#)]
extern "C" {
    #[wasm_bindgen::prelude::wasm_bindgen(catch, js_name = acquireOpfsDestinationLock)]
    fn acquire_opfs_destination_lock(name: &str) -> Result<JsValue, JsValue>;
}

struct DestinationLock(js_sys::Function);

impl Drop for DestinationLock {
    fn drop(&mut self) {
        let _ = self.0.call0(&JsValue::NULL);
    }
}

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

fn io_error(error: JsValue) -> std::io::Error {
    std::io::Error::other(format!("OPFS error: {error:?}"))
}

struct OpfsReader {
    handle: FileSystemSyncAccessHandle,
    position: u64,
}

impl Read for OpfsReader {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        let options = FileSystemReadWriteOptions::new();
        options.set_at(self.position as f64);
        let count = self
            .handle
            .read_with_u8_array_and_options(bytes, &options)
            .map_err(io_error)? as usize;
        self.position += count as u64;
        Ok(count)
    }
}

impl Seek for OpfsReader {
    fn seek(&mut self, from: SeekFrom) -> std::io::Result<u64> {
        let base = match from {
            SeekFrom::Start(position) => {
                self.position = position;
                return Ok(position);
            }
            SeekFrom::Current(_) => self.position,
            SeekFrom::End(_) => self.handle.get_size().map_err(io_error)? as u64,
        };
        let offset = match from {
            SeekFrom::Current(offset) | SeekFrom::End(offset) => offset,
            SeekFrom::Start(_) => unreachable!(),
        };
        self.position = base
            .checked_add_signed(offset)
            .ok_or_else(|| std::io::Error::other("invalid OPFS seek"))?;
        Ok(self.position)
    }
}

impl Drop for OpfsReader {
    fn drop(&mut self) {
        self.handle.close();
    }
}

impl Write for StoreWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.handle
            .write_with_u8_array(bytes)
            .map(|count| count as usize)
            .map_err(io_error)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.handle.flush().map_err(io_error)
    }
}

/// Files below one OPFS attachments directory. Call from a dedicated worker.
#[derive(Clone)]
pub struct OpfsStore {
    root: FileSystemDirectoryHandle,
    root_path: String,
    #[cfg(test)]
    rename_pause: Option<(
        std::sync::Arc<tokio::sync::Notify>,
        std::sync::Arc<tokio::sync::Notify>,
    )>,
    #[cfg(test)]
    move_promise_pause: Option<(
        std::sync::Arc<tokio::sync::Notify>,
        std::sync::Arc<tokio::sync::Notify>,
        std::sync::Arc<tokio::sync::Notify>,
    )>,
}

impl OpfsStore {
    /// Open the origin's OPFS root for a source path supplied by the app.
    pub async fn new_root() -> Result<Self, AttachmentError> {
        let global: WorkerGlobalScope = js_sys::global().unchecked_into();
        let root = JsFuture::from(global.navigator().storage().get_directory())
            .await
            .map_err(storage_error)?
            .dyn_into::<FileSystemDirectoryHandle>()
            .map_err(storage_error)?;
        Ok(Self {
            root,
            root_path: String::new(),
            #[cfg(test)]
            rename_pause: None,
            #[cfg(test)]
            move_promise_pause: None,
        })
    }

    pub async fn new(path: &str) -> Result<Self, AttachmentError> {
        validate_relative(path)?;
        let root = Self::new_root().await?.root;
        let root = Self::directories(root, path, true).await?;
        Ok(Self {
            root,
            root_path: path.to_owned(),
            #[cfg(test)]
            rename_pause: None,
            #[cfg(test)]
            move_promise_pause: None,
        })
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

    async fn destination_lock(&self, path: &str) -> Result<DestinationLock, AttachmentError> {
        validate_relative(path)?;
        let full_path = if self.root_path.is_empty() {
            path.to_owned()
        } else {
            format!("{}/{path}", self.root_path)
        };
        let parts = acquire_opfs_destination_lock(&format!("xmtp-opfs-file:{full_path}"))
            .map_err(storage_error)?;
        let parts: js_sys::Array = parts.dyn_into().map_err(storage_error)?;
        let ready: js_sys::Promise = parts.get(0).dyn_into().map_err(storage_error)?;
        let release: js_sys::Function = parts.get(1).dyn_into().map_err(storage_error)?;
        let guard = DestinationLock(release);
        JsFuture::from(ready).await.map_err(storage_error)?;
        Ok(guard)
    }

    /// Atomically replace one OPFS file with a completed temporary file.
    pub async fn replace(&self, from: &str, to: &str) -> Result<(), AttachmentError> {
        let guard = self.destination_lock(to).await?;
        let store = self.clone();
        let source = from.to_owned();
        let destination = to.to_owned();
        let task = xmtp_common::task::spawn(async move {
            let _guard = guard;
            store.replace_unlocked(&source, &destination).await
        });
        task.await.map_err(storage_error)?
    }

    async fn replace_unlocked(&self, from: &str, to: &str) -> Result<(), AttachmentError> {
        let source = self.file_handle(from, false).await?;
        let (target_parent, target_name) = self.parent(to, true).await?;
        #[cfg(test)]
        if let Some((entered, resume, settled)) = &self.move_promise_pause {
            let (entered, resume, settled) = (entered.clone(), resume.clone(), settled.clone());
            let promise = wasm_bindgen_futures::future_to_promise(async move {
                entered.notify_one();
                resume.notified().await;
                let move_method =
                    js_sys::Reflect::get(source.as_ref(), &JsValue::from_str("move"))?;
                let move_method: js_sys::Function = move_method.dyn_into()?;
                let result = async {
                    let promise: js_sys::Promise = move_method
                        .call2(
                            source.as_ref(),
                            target_parent.as_ref(),
                            &JsValue::from_str(&target_name),
                        )?
                        .dyn_into()?;
                    JsFuture::from(promise).await
                }
                .await;
                settled.notify_one();
                result
            });
            JsFuture::from(promise).await.map_err(storage_error)?;
            return Ok(());
        }
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

    async fn create_dir_if_absent(&self, path: &str) -> Result<bool, AttachmentError> {
        let existed = self.exists(path).await?;
        Self::directories(self.root.clone(), path, !existed).await?;
        Ok(!existed)
    }

    async fn rename(&self, from: &str, to: &str) -> Result<(), StoreMoveError> {
        let guard = self.destination_lock(to).await?;
        if self.exists(to).await? {
            return Err(StoreMoveError::DestinationExists);
        }
        #[cfg(test)]
        if let Some((entered, resume)) = &self.rename_pause {
            entered.notify_one();
            resume.notified().await;
        }
        let store = self.clone();
        let source = from.to_owned();
        let destination = to.to_owned();
        let task = xmtp_common::task::spawn(async move {
            let _guard = guard;
            match store.replace_unlocked(&source, &destination).await {
                Ok(()) => Ok(()),
                Err(_error) if store.exists(&destination).await.unwrap_or(false) => {
                    Err(StoreMoveError::DestinationExists)
                }
                Err(error) => Err(error.into()),
            }
        });
        task.await
            .map_err(|error| StoreMoveError::Other(storage_error(error)))?
    }

    async fn replace(&self, from: &str, to: &str) -> Result<(), AttachmentError> {
        OpfsStore::replace(self, from, to).await
    }

    async fn remove_dir_all(&self, path: &str) -> Result<(), AttachmentError> {
        let (parent, name) = self.parent(path, false).await?;
        // A directory handle lookup rejects a file at this name.
        JsFuture::from(parent.get_directory_handle(&name))
            .await
            .map_err(storage_error)?;
        let options = FileSystemRemoveOptions::new();
        options.set_recursive(true);
        JsFuture::from(parent.remove_entry_with_options(&name, &options))
            .await
            .map_err(storage_error)?;
        Ok(())
    }

    async fn prepare_remove_dir(&self, path: &str) -> Result<bool, AttachmentError> {
        if !self.exists(path).await? {
            return Ok(false);
        }
        let (parent, name) = self.parent(path, false).await?;
        // A directory handle lookup rejects a file at this name.
        JsFuture::from(parent.get_directory_handle(&name))
            .await
            .map_err(storage_error)?;
        Ok(true)
    }

    async fn remove_empty_dir(&self, path: &str) -> Result<(), AttachmentError> {
        let (parent, name) = self.parent(path, false).await?;
        JsFuture::from(parent.get_directory_handle(&name))
            .await
            .map_err(storage_error)?;
        JsFuture::from(parent.remove_entry(&name))
            .await
            .map_err(storage_error)?;
        Ok(())
    }

    async fn remove_file(&self, path: &str) -> Result<(), AttachmentError> {
        let (parent, name) = self.parent(path, false).await?;
        // A file handle lookup rejects a directory at this name.
        JsFuture::from(parent.get_file_handle(&name))
            .await
            .map_err(storage_error)?;
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

    async fn finish_decode(
        &self,
        decoder: AttachmentDecoder,
        source: &str,
        output: &str,
    ) -> Result<DecodedMeta, AttachmentError> {
        validate_temp(source)?;
        validate_temp(output)?;
        let file = self.file_handle(source, false).await?;
        let mut input = OpfsReader {
            handle: JsFuture::from(file.create_sync_access_handle())
                .await
                .map_err(storage_error)?
                .dyn_into()
                .map_err(storage_error)?,
            position: 0,
        };
        let mut decoded = self.create_temp(output).await?;
        let meta = decoder.finish(&mut input, &mut decoded)?;
        self.sync(&mut decoded).await?;
        Ok(meta)
    }

    async fn list_files(&self) -> Result<Vec<StoreFile>, AttachmentError> {
        let mut files = Vec::new();
        let mut dirs = vec![(self.root.clone(), String::new())];
        while let Some((dir, prefix)) = dirs.pop() {
            let entries: js_sys::Function = js_sys::Reflect::get(dir.as_ref(), &"entries".into())
                .map_err(storage_error)?
                .dyn_into()
                .map_err(storage_error)?;
            let iterator = entries.call0(dir.as_ref()).map_err(storage_error)?;
            let next: js_sys::Function = js_sys::Reflect::get(&iterator, &"next".into())
                .map_err(storage_error)?
                .dyn_into()
                .map_err(storage_error)?;
            loop {
                let promise: js_sys::Promise = next
                    .call0(&iterator)
                    .map_err(storage_error)?
                    .dyn_into()
                    .map_err(storage_error)?;
                let step = JsFuture::from(promise).await.map_err(storage_error)?;
                if js_sys::Reflect::get(&step, &"done".into())
                    .map_err(storage_error)?
                    .as_bool()
                    == Some(true)
                {
                    break;
                }
                let pair: js_sys::Array = js_sys::Reflect::get(&step, &"value".into())
                    .map_err(storage_error)?
                    .dyn_into()
                    .map_err(storage_error)?;
                let name = pair.get(0).as_string().ok_or_else(|| storage_error(()))?;
                let descend = prefix.is_empty() && is_reconcile_dir(&name);
                let path = if prefix.is_empty() {
                    name
                } else {
                    format!("{prefix}/{name}")
                };
                let handle = pair.get(1);
                if let Some(child) = handle.dyn_ref::<FileSystemDirectoryHandle>() {
                    if descend {
                        dirs.push((child.clone(), path));
                    }
                } else if !prefix.is_empty() {
                    let file: FileSystemFileHandle = handle.dyn_into().map_err(storage_error)?;
                    let file: web_sys::File = JsFuture::from(file.get_file())
                        .await
                        .map_err(storage_error)?
                        .dyn_into()
                        .map_err(storage_error)?;
                    files.push(StoreFile {
                        path,
                        modified_at_ns: (file.last_modified() as i64).saturating_mul(1_000_000),
                    });
                }
            }
        }
        Ok(files)
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
    use std::io::SeekFrom;

    fn test_path() -> String {
        format!(
            "attachment-tests/{}",
            hex::encode(xmtp_common::rand_array::<16>())
        )
    }

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
        DownloadSink::write(&mut writer, b"OPFS").await?;
        store.sync(&mut writer).await?;
        drop(writer);
        assert!(store.exists(path).await?);
        assert_eq!(store.open_read(path).await?.file.size(), 4.0);
        store.rename(path, "key/file").await?;
        assert!(!store.exists(path).await?);
        assert!(store.exists("key/file").await?);
        let source = OpfsStore::new_root()
            .await?
            .open_read("attachment-tests/lane-c/key/file")
            .await?;
        assert_eq!(source.len(), 4);
        assert_eq!(source.read_chunk(0, 64).await?, b"OPFS");
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

    // verifies: ATCH-046, ATCH-048
    // Covers plan P24.
    #[xmtp_common::test(unwrap_try = true)]
    async fn opfs_list_files_and_mtime() {
        let store = OpfsStore::new(&test_path()).await?;
        let key = "a".repeat(64);
        for path in [".tmp/one", ".tmp/two"] {
            let mut writer = store.create_temp(path).await?;
            DownloadSink::write(&mut writer, b"file").await?;
            store.sync(&mut writer).await?;
        }
        let plain = format!("{key}/two");
        store.rename(".tmp/two", &plain).await?;
        for (source, destination) in [
            (".tmp/deep", format!("{key}/nested/deep")),
            (".tmp/app", "app/file".to_owned()),
        ] {
            let mut writer = store.create_temp(source).await?;
            DownloadSink::write(&mut writer, b"file").await?;
            store.sync(&mut writer).await?;
            drop(writer);
            store.rename(source, &destination).await?;
        }
        let expected = store.file_handle(&plain, false).await?;
        let expected: web_sys::File = JsFuture::from(expected.get_file()).await?.dyn_into()?;
        let files = store.list_files().await?;
        assert_eq!(files.len(), 2);
        let listed = files.iter().find(|file| file.path == plain).unwrap();
        assert_eq!(
            listed.modified_at_ns,
            (expected.last_modified() as i64) * 1_000_000
        );
        assert!(files.iter().any(|file| file.path == ".tmp/one"));
    }

    // verifies: ATCH-046
    #[xmtp_common::test(unwrap_try = true)]
    async fn opfs_rename_reports_existing_destination() {
        let store = OpfsStore::new(&test_path()).await?;
        let mut original = store.create_temp(".tmp/original").await?;
        DownloadSink::write(&mut original, b"original").await?;
        store.sync(&mut original).await?;
        drop(original);
        store.rename(".tmp/original", "key/file").await?;
        let mut second = store.create_temp(".tmp/second").await?;
        DownloadSink::write(&mut second, b"second").await?;
        store.sync(&mut second).await?;
        drop(second);
        let error = match store.rename(".tmp/second", "key/file").await {
            Ok(()) => panic!("existing destination was replaced"),
            Err(error) => error,
        };
        assert!(matches!(error, StoreMoveError::DestinationExists));
        let file = store.file_handle("key/file", false).await?.get_file();
        let file: web_sys::File = JsFuture::from(file).await?.dyn_into()?;
        let bytes = JsFuture::from(file.array_buffer()).await?;
        assert_eq!(js_sys::Uint8Array::new(&bytes).to_vec(), b"original");
    }

    // verifies: ATCH-046
    #[xmtp_common::test(unwrap_try = true)]
    async fn independent_stores_cannot_replace_a_racing_plaintext_file() {
        use std::sync::Arc;
        use xmtp_common::time::{Duration, timeout};

        let path = test_path();
        let mut first = OpfsStore::new(&path).await?;
        let second = OpfsStore::new(&path).await?;
        for (store, source, bytes) in [
            (&first, ".tmp/first", b"first".as_slice()),
            (&second, ".tmp/second", b"second".as_slice()),
        ] {
            let mut writer = store.create_temp(source).await?;
            DownloadSink::write(&mut writer, bytes).await?;
            store.sync(&mut writer).await?;
            drop(writer);
        }
        let entered = Arc::new(tokio::sync::Notify::new());
        let resume = Arc::new(tokio::sync::Notify::new());
        first.rename_pause = Some((entered.clone(), resume.clone()));
        let first_move =
            xmtp_common::task::spawn(async move { first.rename(".tmp/first", "key/file").await });
        timeout(Duration::from_secs(3), entered.notified()).await?;
        let (sent, mut received) = tokio::sync::oneshot::channel();
        drop(xmtp_common::task::spawn(async move {
            let _ = sent.send(second.rename(".tmp/second", "key/file").await);
        }));
        xmtp_common::time::sleep(Duration::from_millis(100)).await;
        let early = received.try_recv().ok();
        resume.notify_one();
        timeout(Duration::from_secs(3), first_move).await???;
        let second_result = match early {
            Some(result) => result,
            None => timeout(Duration::from_secs(3), received).await??,
        };
        assert!(matches!(
            second_result,
            Err(StoreMoveError::DestinationExists)
        ));
        let store = OpfsStore::new(&path).await?;
        let file = store.file_handle("key/file", false).await?.get_file();
        let file: web_sys::File = JsFuture::from(file).await?.dyn_into()?;
        let bytes = JsFuture::from(file.array_buffer()).await?;
        assert_eq!(js_sys::Uint8Array::new(&bytes).to_vec(), b"first");
    }

    // verifies: ATCH-046
    #[xmtp_common::test(unwrap_try = true)]
    async fn cancelled_move_keeps_destination_locked_until_promise_settles() {
        use std::sync::Arc;
        use xmtp_common::time::{Duration, timeout};

        let path = test_path();
        let mut first = OpfsStore::new(&path).await?;
        let second = OpfsStore::new(&path).await?;
        for (store, source, bytes) in [
            (&first, ".tmp/first", b"first".as_slice()),
            (&second, ".tmp/second", b"second".as_slice()),
        ] {
            let mut writer = store.create_temp(source).await?;
            DownloadSink::write(&mut writer, bytes).await?;
            store.sync(&mut writer).await?;
            drop(writer);
        }
        let entered = Arc::new(tokio::sync::Notify::new());
        let resume = Arc::new(tokio::sync::Notify::new());
        let settled = Arc::new(tokio::sync::Notify::new());
        first.move_promise_pause = Some((entered.clone(), resume.clone(), settled.clone()));
        let (move_future, abort) =
            futures_util::future::abortable(
                async move { first.rename(".tmp/first", "key/file").await },
            );
        let first_move = xmtp_common::task::spawn(move_future);
        timeout(Duration::from_secs(3), entered.notified()).await?;
        abort.abort();
        assert!(timeout(Duration::from_secs(3), first_move).await??.is_err());

        let (sent, mut received) = tokio::sync::oneshot::channel();
        drop(xmtp_common::task::spawn(async move {
            let _ = sent.send(second.rename(".tmp/second", "key/file").await);
        }));
        xmtp_common::time::sleep(Duration::from_millis(100)).await;
        let early = received.try_recv().ok();
        let finished_early = early.is_some();
        resume.notify_one();
        timeout(Duration::from_secs(3), settled.notified()).await?;
        let second_result = match early {
            Some(result) => result,
            None => timeout(Duration::from_secs(3), received).await??,
        };
        assert!(!finished_early, "second publish passed an unsettled move");
        assert!(matches!(
            second_result,
            Err(StoreMoveError::DestinationExists)
        ));
        let store = OpfsStore::new(&path).await?;
        let file = store.file_handle("key/file", false).await?.get_file();
        let file: web_sys::File = JsFuture::from(file).await?.dyn_into()?;
        let bytes = JsFuture::from(file.array_buffer()).await?;
        assert_eq!(js_sys::Uint8Array::new(&bytes).to_vec(), b"first");
    }

    // verifies: ATCH-046
    #[xmtp_common::test(unwrap_try = true)]
    async fn opfs_remove_file_rejects_directory() {
        let store = OpfsStore::new(&test_path()).await?;
        store.create_dir_if_absent("key/filename").await?;
        let error = match store.remove_file("key/filename").await {
            Ok(()) => panic!("directory was removed as a file"),
            Err(error) => error,
        };
        assert_eq!(error.cause, Cause::LocalStorage);
        assert!(store.exists("key/filename").await?);
    }

    // verifies: ATCH-046
    #[xmtp_common::test(unwrap_try = true)]
    async fn opfs_directory_removal_rejects_file() {
        let store = OpfsStore::new(&test_path()).await?;
        let mut writer = store.create_temp(".tmp/stray").await?;
        DownloadSink::write(&mut writer, b"stray").await?;
        store.sync(&mut writer).await?;
        drop(writer);
        store.rename(".tmp/stray", "key").await?;
        assert!(!store.prepare_remove_dir("absent").await?);
        assert_eq!(
            store.prepare_remove_dir("key").await.unwrap_err().cause,
            Cause::LocalStorage
        );
        assert_eq!(
            store.remove_dir_all("key").await.unwrap_err().cause,
            Cause::LocalStorage
        );
        assert_eq!(
            store.remove_empty_dir("key").await.unwrap_err().cause,
            Cause::LocalStorage
        );
        assert_eq!(
            store.open_read("key").await?.read_chunk(0, 64).await?,
            b"stray"
        );
    }

    // verifies: ATCH-043, ATCH-051
    #[xmtp_common::test(unwrap_try = true)]
    async fn opfs_finish_decode_compressed_second_pass() {
        use flate2::{Compression, write::GzEncoder};
        use prost::Message as _;
        use xmtp_proto::xmtp::mls::message_contents::{
            Compression as WireCompression, EncodedContent,
        };

        let store = OpfsStore::new(&test_path()).await?;
        let mut gzip = GzEncoder::new(Vec::new(), Compression::default());
        std::io::Write::write_all(&mut gzip, b"decoded content")?;
        let mut envelope = EncodedContent::decode(
            crate::encoded_prefix(Some("file.txt"), "text/plain", 0).as_slice(),
        )?;
        envelope.compression = Some(WireCompression::Gzip as i32);
        envelope.content = gzip.finish()?;
        let mut decoder = AttachmentDecoder::new();
        let mut content = store.create_temp(".tmp/content").await?;
        for chunk in decoder.push(&envelope.encode_to_vec())? {
            content.write_content(chunk).await?;
        }
        store.sync(&mut content).await?;
        drop(content);
        let meta = store
            .finish_decode(decoder, ".tmp/content", ".tmp/decoded")
            .await?;
        assert!(meta.compressed);
        assert_eq!(meta.mime_type, "text/plain");
        assert_eq!(meta.filename.as_deref(), Some("file.txt"));
        assert_eq!(
            store
                .open_read(".tmp/decoded")
                .await?
                .read_chunk(0, 64)
                .await?,
            b"decoded content"
        );
    }

    // verifies: ATCH-048
    #[xmtp_common::test(unwrap_try = true)]
    async fn opfs_reader_seek() {
        let store = OpfsStore::new(&test_path()).await?;
        let mut writer = store.create_temp(".tmp/reader").await?;
        DownloadSink::write(&mut writer, b"abcdef").await?;
        store.sync(&mut writer).await?;
        drop(writer);
        let file = store.file_handle(".tmp/reader", false).await?;
        let mut reader = OpfsReader {
            handle: JsFuture::from(file.create_sync_access_handle())
                .await?
                .dyn_into()?,
            position: 0,
        };
        let mut bytes = [0u8; 2];
        assert_eq!(std::io::Read::read(&mut reader, &mut bytes)?, 2);
        assert_eq!(&bytes, b"ab");
        assert_eq!(std::io::Seek::seek(&mut reader, SeekFrom::Current(2))?, 4);
        assert_eq!(std::io::Read::read(&mut reader, &mut bytes)?, 2);
        assert_eq!(&bytes, b"ef");
        assert_eq!(std::io::Seek::seek(&mut reader, SeekFrom::Start(1))?, 1);
        assert_eq!(std::io::Seek::seek(&mut reader, SeekFrom::End(-3))?, 3);
        assert_eq!(std::io::Read::read(&mut reader, &mut bytes)?, 2);
        assert_eq!(&bytes, b"de");
    }
}
