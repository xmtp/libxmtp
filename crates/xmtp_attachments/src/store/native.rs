use std::path::{Path, PathBuf};

use super::{LocalStore, StagedFile, StoreWriter, validate_relative, validate_temp};
use crate::{AttachmentError, AttachmentFailureCause as Cause};

/// Files below one native attachments directory.
#[derive(Clone, Debug)]
pub struct NativeStore {
    root: PathBuf,
}

impl NativeStore {
    pub async fn new(root: impl AsRef<Path>) -> Result<Self, AttachmentError> {
        tokio::fs::create_dir_all(&root)
            .await
            .map_err(|_| AttachmentError::new(Cause::LocalStorage))?;
        Ok(Self {
            root: root.as_ref().to_path_buf(),
        })
    }

    fn path(&self, relative: &str) -> Result<PathBuf, AttachmentError> {
        validate_relative(relative)?;
        Ok(self.root.join(relative))
    }
}

#[async_trait::async_trait]
impl LocalStore for NativeStore {
    async fn open_read(&self, path: &str) -> Result<StagedFile, AttachmentError> {
        let path = self.path(path)?;
        tokio::fs::File::open(&path)
            .await
            .map_err(|_| AttachmentError::new(Cause::LocalStorage))?;
        Ok(StagedFile { path })
    }

    async fn create_temp(&self, path: &str) -> Result<StoreWriter, AttachmentError> {
        validate_temp(path)?;
        let path = self.path(path)?;
        let parent = path
            .parent()
            .ok_or(AttachmentError::new(Cause::Malformed))?;
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|_| AttachmentError::new(Cause::LocalStorage))?;
        let file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .await
            .map_err(|_| AttachmentError::new(Cause::LocalStorage))?;
        Ok(StoreWriter { file })
    }

    async fn rename(&self, from: &str, to: &str) -> Result<(), AttachmentError> {
        let from = self.path(from)?;
        let to = self.path(to)?;
        let parent = to.parent().ok_or(AttachmentError::new(Cause::Malformed))?;
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|_| AttachmentError::new(Cause::LocalStorage))?;
        tokio::fs::hard_link(&from, &to)
            .await
            .map_err(|_| AttachmentError::new(Cause::LocalStorage))?;
        tokio::fs::remove_file(from)
            .await
            .map_err(|_| AttachmentError::new(Cause::LocalStorage))
    }

    async fn remove_dir_all(&self, path: &str) -> Result<(), AttachmentError> {
        tokio::fs::remove_dir_all(self.path(path)?)
            .await
            .map_err(|_| AttachmentError::new(Cause::LocalStorage))
    }

    async fn exists(&self, path: &str) -> Result<bool, AttachmentError> {
        Ok(tokio::fs::try_exists(self.path(path)?)
            .await
            .map_err(|_| AttachmentError::new(Cause::LocalStorage))?)
    }

    async fn sync(&self, writer: &mut StoreWriter) -> Result<(), AttachmentError> {
        writer
            .file
            .sync_all()
            .await
            .map_err(|_| AttachmentError::new(Cause::LocalStorage))
    }
}
