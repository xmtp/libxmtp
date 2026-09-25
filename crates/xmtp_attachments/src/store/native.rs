use std::path::{Path, PathBuf};

use super::{LocalStore, StagedFile, StoreFile, StoreWriter, validate_relative, validate_temp};
use crate::{AttachmentDecoder, AttachmentError, AttachmentFailureCause as Cause, DecodedMeta};

/// Files below one native attachments directory.
#[derive(Clone, Debug)]
pub struct NativeStore {
    root: PathBuf,
    #[cfg(test)]
    forced_hard_link_error: Option<std::io::ErrorKind>,
    #[cfg(test)]
    forced_source_unlink_error: Option<std::io::ErrorKind>,
}

impl NativeStore {
    pub async fn new(root: impl AsRef<Path>) -> Result<Self, AttachmentError> {
        let root = std::path::absolute(root.as_ref())
            .map_err(|_| AttachmentError::new(Cause::LocalStorage))?;
        tokio::fs::create_dir_all(&root)
            .await
            .map_err(|_| AttachmentError::new(Cause::LocalStorage))?;
        Ok(Self {
            root,
            #[cfg(test)]
            forced_hard_link_error: None,
            #[cfg(test)]
            forced_source_unlink_error: None,
        })
    }

    #[cfg(test)]
    pub(crate) fn with_forced_hard_link_error(mut self, kind: std::io::ErrorKind) -> Self {
        self.forced_hard_link_error = Some(kind);
        self
    }

    #[cfg(test)]
    pub(crate) fn with_forced_source_unlink_error(mut self, kind: std::io::ErrorKind) -> Self {
        self.forced_source_unlink_error = Some(kind);
        self
    }

    async fn remove_source(&self, path: &Path) -> std::io::Result<()> {
        #[cfg(test)]
        if let Some(kind) = self.forced_source_unlink_error {
            return Err(std::io::Error::from(kind));
        }
        tokio::fs::remove_file(path).await
    }

    async fn hard_link(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        #[cfg(test)]
        if let Some(kind) = self.forced_hard_link_error {
            return Err(std::io::Error::from(kind));
        }
        tokio::fs::hard_link(from, to).await
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
        match self.hard_link(&from, &to).await {
            Ok(()) => match self.remove_source(&from).await {
                Ok(()) => Ok(()),
                Err(_) => {
                    let _ = tokio::fs::remove_file(&to).await;
                    Err(AttachmentError::new(Cause::LocalStorage))
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                Err(AttachmentError::new(Cause::LocalStorage))
            }
            Err(_) => {
                // Some file systems cannot make hard links. This check and
                // rename are not atomic. Callers keep each path single-flight
                // (one pending attachment per digest, one fetch per path), as
                // with the OPFS check-then-move path.
                if tokio::fs::try_exists(&to)
                    .await
                    .map_err(|_| AttachmentError::new(Cause::LocalStorage))?
                {
                    return Err(AttachmentError::new(Cause::LocalStorage));
                }
                tokio::fs::rename(from, to)
                    .await
                    .map_err(|_| AttachmentError::new(Cause::LocalStorage))
            }
        }
    }

    async fn remove_dir_all(&self, path: &str) -> Result<(), AttachmentError> {
        tokio::fs::remove_dir_all(self.path(path)?)
            .await
            .map_err(|_| AttachmentError::new(Cause::LocalStorage))
    }

    async fn remove_file(&self, path: &str) -> Result<(), AttachmentError> {
        tokio::fs::remove_file(self.path(path)?)
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

    async fn finish_decode(
        &self,
        decoder: AttachmentDecoder,
        source: &str,
        output: &str,
    ) -> Result<DecodedMeta, AttachmentError> {
        validate_temp(source)?;
        validate_temp(output)?;
        let source = self.path(source)?;
        let output = self.path(output)?;
        xmtp_common::task::spawn_blocking(move || {
            let mut input = std::fs::File::open(source)
                .map_err(|_| AttachmentError::new(Cause::LocalStorage))?;
            let mut decoded = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(output)
                .map_err(|_| AttachmentError::new(Cause::LocalStorage))?;
            let meta = decoder.finish(&mut input, &mut decoded)?;
            decoded
                .sync_all()
                .map_err(|_| AttachmentError::new(Cause::LocalStorage))?;
            Ok(meta)
        })
        .await
        .map_err(|_| AttachmentError::new(Cause::LocalStorage))?
    }

    async fn list_files(&self) -> Result<Vec<StoreFile>, AttachmentError> {
        use std::time::UNIX_EPOCH;
        let mut files = Vec::new();
        let mut dirs = vec![(self.root.clone(), String::new())];
        while let Some((dir, prefix)) = dirs.pop() {
            let mut entries = tokio::fs::read_dir(dir)
                .await
                .map_err(|_| AttachmentError::new(Cause::LocalStorage))?;
            while let Some(entry) = entries
                .next_entry()
                .await
                .map_err(|_| AttachmentError::new(Cause::LocalStorage))?
            {
                let name = entry.file_name().to_string_lossy().into_owned();
                let path = if prefix.is_empty() {
                    name
                } else {
                    format!("{prefix}/{name}")
                };
                let kind = entry
                    .file_type()
                    .await
                    .map_err(|_| AttachmentError::new(Cause::LocalStorage))?;
                if kind.is_dir() {
                    dirs.push((entry.path(), path));
                } else if kind.is_file() {
                    let meta = entry
                        .metadata()
                        .await
                        .map_err(|_| AttachmentError::new(Cause::LocalStorage))?;
                    let modified_at_ns = meta
                        .modified()
                        .ok()
                        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                        .map_or(0, |age| age.as_nanos().min(i64::MAX as u128) as i64);
                    files.push(StoreFile {
                        path,
                        modified_at_ns,
                    });
                }
            }
        }
        Ok(files)
    }
}
