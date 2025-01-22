use super::BackupExporter;
use crate::groups::device_sync::DeviceSyncError;
use futures::AsyncReadExt;
use std::path::Path;
use tokio::{fs::File, io::AsyncWriteExt};

impl BackupExporter {
    pub async fn write_to_file(&mut self, path: impl AsRef<Path>) -> Result<(), DeviceSyncError> {
        let mut file = File::create(path.as_ref()).await?;
        let mut buffer = [0u8; 1024];

        let mut amount = self.read(&mut buffer).await?;
        while amount != 0 {
            let _ = file.write(&buffer[..amount]).await?;
            amount = self.read(&mut buffer).await?;
        }

        file.flush().await?;

        Ok(())
    }
}
