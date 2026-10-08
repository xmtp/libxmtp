//! Disk-backed legacy copies under the OPFS lifecycle guard.
use super::{
    POOL_TRANSITION,
    restore::{PoolChange, closed_target},
    resume_sqlite,
};
use crate::StorageError;
use sqlite_wasm_rs as ffi;
use sqlite_wasm_vfs::sahpool::{OpfsSAHPoolCfg, OpfsSAHPoolUtil};
use std::{ffi::CString, ptr::NonNull};

const WORK_VFS: &str = "xmtp-migration-working";
const WORK_DIRECTORY: &str = ".xmtp-migration-working";
const WORK_DATABASE: &str = "working.db3";
const COPY_CHUNK: usize = 64 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum OpfsWorkingCopyError {
    #[error("source OPFS access failed: {0}")]
    Source(#[source] StorageError),
    #[error("working-copy OPFS access failed: {0}")]
    Output(#[source] StorageError),
    #[error("source VFS operation failed with SQLite code {0}")]
    SourceIo(i32),
    #[error("working-copy VFS operation failed with SQLite code {0}")]
    OutputIo(i32),
    #[error("source is not a SQLite database")]
    InvalidDatabase,
}
fn source(error: super::PlatformStorageError) -> OpfsWorkingCopyError {
    OpfsWorkingCopyError::Source(error.into())
}
fn output(error: sqlite_wasm_vfs::sahpool::OpfsSAHError) -> OpfsWorkingCopyError {
    OpfsWorkingCopyError::Output(super::PlatformStorageError::from(error).into())
}

/// Keeps all persistent opens excluded until the private SQLite copy closes.
pub struct OpfsWorkingCopy {
    pool: OpfsSAHPoolUtil,
    finished: bool,
    _admission: PoolChange,
}
impl OpfsWorkingCopy {
    /// Copy raw closed storage without opening the source through SQLite.
    pub async fn new(path: &str) -> Result<Self, OpfsWorkingCopyError> {
        super::validate_persistent_path(path).map_err(source)?;
        let admission = PoolChange::acquire().map_err(source)?;
        let _transition = POOL_TRANSITION.lock().await;
        let source_pool = resume_sqlite(true).await.map_err(source)?;
        if !source_pool
            .exists(path)
            .map_err(|error| source(error.into()))?
        {
            return Err(OpfsWorkingCopyError::SourceIo(ffi::SQLITE_CANTOPEN));
        }
        closed_target(None).map_err(source)?;
        let mut original = RawFile::open(
            xmtp_configuration::WASM_VFS_NAME,
            path,
            ffi::SQLITE_OPEN_READONLY | ffi::SQLITE_OPEN_MAIN_DB,
        )
        .map_err(OpfsWorkingCopyError::SourceIo)?;
        let size = original.size().map_err(OpfsWorkingCopyError::SourceIo)?;
        if size < 100 {
            return Err(OpfsWorkingCopyError::InvalidDatabase);
        }
        let mut buffer = vec![0; COPY_CHUNK];
        let first = size.min(COPY_CHUNK as i64) as usize;
        original
            .read(&mut buffer[..first], 0)
            .map_err(OpfsWorkingCopyError::SourceIo)?;
        if !buffer.starts_with(b"SQLite format 3\0") {
            return Err(OpfsWorkingCopyError::InvalidDatabase);
        }
        let config = OpfsSAHPoolCfg {
            vfs_name: WORK_VFS.into(),
            directory: WORK_DIRECTORY.into(),
            clear_on_init: true,
            initial_capacity: 6,
        };
        let pool = sqlite_wasm_vfs::sahpool::install::<ffi::WasmOsCallback>(&config, false)
            .await
            .map_err(output)?;
        pool.unpause_vfs().await.map_err(output)?;
        pool.clear_all().await.map_err(output)?;
        let working = Self {
            pool,
            finished: false,
            _admission: admission,
        };
        {
            let mut target = RawFile::open(
                WORK_VFS,
                WORK_DATABASE,
                ffi::SQLITE_OPEN_READWRITE | ffi::SQLITE_OPEN_CREATE | ffi::SQLITE_OPEN_MAIN_DB,
            )
            .map_err(OpfsWorkingCopyError::OutputIo)?;
            // Only the copy changes journal mode. The closed source has no WAL.
            buffer[18] = 1;
            buffer[19] = 1;
            target
                .write(&buffer[..first], 0)
                .map_err(OpfsWorkingCopyError::OutputIo)?;
            let mut offset = first as i64;
            while offset < size {
                let count = (size - offset).min(COPY_CHUNK as i64) as usize;
                original
                    .read(&mut buffer[..count], offset)
                    .map_err(OpfsWorkingCopyError::SourceIo)?;
                target
                    .write(&buffer[..count], offset)
                    .map_err(OpfsWorkingCopyError::OutputIo)?;
                offset += count as i64;
            }
            target.sync().map_err(OpfsWorkingCopyError::OutputIo)?;
        }
        Ok(working)
    }
    /// Select the private VFS without changing SQLite's default VFS.
    pub fn database_uri(&self) -> String {
        format!("file:{WORK_DATABASE}?vfs={WORK_VFS}")
    }
    /// Call after closing SQLite and before publishing an archive.
    pub fn finish(mut self) -> Result<(), OpfsWorkingCopyError> {
        self.cleanup()?;
        self.finished = true;
        Ok(())
    }
    fn cleanup(&self) -> Result<(), OpfsWorkingCopyError> {
        for file in self.pool.list() {
            self.pool.delete_db(&file).map_err(output)?;
        }
        self.pool.pause_vfs().map_err(output)
    }
}
impl Drop for OpfsWorkingCopy {
    fn drop(&mut self) {
        if !self.finished
            && let Err(error) = self.cleanup()
        {
            tracing::warn!(%error, "could not remove the private OPFS working copy");
        }
    }
}

/// Owns the small VFS file object. It never opens a SQLite connection.
struct RawFile {
    file: NonNull<ffi::sqlite3_file>,
}
impl RawFile {
    fn open(vfs: &str, path: &str, flags: i32) -> Result<Self, i32> {
        let vfs = CString::new(vfs).map_err(|_| ffi::SQLITE_CANTOPEN)?;
        let path = CString::new(path).map_err(|_| ffi::SQLITE_CANTOPEN)?;
        // SQLite owns the registered VFS. Its file size is independent of DB size.
        unsafe {
            let vfs = ffi::sqlite3_vfs_find(vfs.as_ptr());
            if vfs.is_null() {
                return Err(ffi::SQLITE_CANTOPEN);
            }
            let memory = ffi::sqlite3_malloc((*vfs).szOsFile);
            let file = NonNull::new(memory.cast::<ffi::sqlite3_file>()).ok_or(ffi::SQLITE_NOMEM)?;
            std::ptr::write_bytes(memory.cast::<u8>(), 0, (*vfs).szOsFile as usize);
            let opened = Self { file };
            let mut actual_flags = 0;
            let result = ((*vfs).xOpen.ok_or(ffi::SQLITE_CANTOPEN)?)(
                vfs,
                path.as_ptr(),
                file.as_ptr(),
                flags,
                &mut actual_flags,
            );
            if result != ffi::SQLITE_OK {
                return Err(result);
            }
            Ok(opened)
        }
    }
    fn size(&mut self) -> Result<i64, i32> {
        let mut size = 0;
        // Successful xOpen initializes pMethods. SQLite retains it until xClose.
        let code = unsafe {
            ((*(*self.file.as_ptr()).pMethods).xFileSize.unwrap())(self.file.as_ptr(), &mut size)
        };
        check(code)?;
        if size < 0 {
            return Err(ffi::SQLITE_IOERR);
        }
        Ok(size)
    }
    fn read(&mut self, bytes: &mut [u8], offset: i64) -> Result<(), i32> {
        // The slice remains valid for this synchronous VFS call.
        let code = unsafe {
            ((*(*self.file.as_ptr()).pMethods).xRead.unwrap())(
                self.file.as_ptr(),
                bytes.as_mut_ptr().cast(),
                bytes.len() as i32,
                offset,
            )
        };
        check(code)
    }
    fn write(&mut self, bytes: &[u8], offset: i64) -> Result<(), i32> {
        // The slice remains valid for this synchronous VFS call.
        let code = unsafe {
            ((*(*self.file.as_ptr()).pMethods).xWrite.unwrap())(
                self.file.as_ptr(),
                bytes.as_ptr().cast(),
                bytes.len() as i32,
                offset,
            )
        };
        check(code)
    }
    fn sync(&mut self) -> Result<(), i32> {
        // The file is still open and owned by this wrapper.
        check(unsafe {
            ((*(*self.file.as_ptr()).pMethods).xSync.unwrap())(
                self.file.as_ptr(),
                ffi::SQLITE_SYNC_FULL,
            )
        })
    }
}
impl Drop for RawFile {
    fn drop(&mut self) {
        // xClose releases VFS state; sqlite3_free releases only this file object.
        unsafe {
            let methods = (*self.file.as_ptr()).pMethods;
            if !methods.is_null()
                && let Some(close) = (*methods).xClose
            {
                close(self.file.as_ptr());
            }
            ffi::sqlite3_free(self.file.as_ptr().cast());
        }
    }
}
fn check(code: i32) -> Result<(), i32> {
    if code == ffi::SQLITE_OK {
        Ok(())
    } else {
        Err(code)
    }
}
