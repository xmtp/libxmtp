use super::ArchiveExporter;
use crate::ArchiveError;
use futures_util::AsyncReadExt;
use std::path::Path;
use tokio::{fs::File, io::AsyncWriteExt};
use xmtp_api::XmtpApi;

impl<C: XmtpApi> ArchiveExporter<C> {
    pub async fn write_to_file(&mut self, path: impl AsRef<Path>) -> Result<(), ArchiveError> {
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
