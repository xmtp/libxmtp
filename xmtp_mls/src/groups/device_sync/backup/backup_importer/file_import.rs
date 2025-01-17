use super::BackupImporter;
use crate::groups::device_sync::{backup::BackupError, DeviceSyncError};
use async_compression::futures::bufread::ZstdDecoder;
use futures::io::BufReader;
use std::pin::Pin;
use tokio::io::AsyncRead;
use tokio_util::compat::{FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};
use xmtp_proto::xmtp::device_sync::{backup_element::Element, BackupElement, BackupMetadata};

impl BackupImporter {
    pub async fn open(reader: impl AsyncRead + Send + 'static) -> Result<Self, DeviceSyncError> {
        let reader = reader.compat();
        let reader = BufReader::new(reader);
        let reader = Box::pin(reader) as Pin<Box<_>>;
        let decoder = ZstdDecoder::new(reader);

        let mut importer = Self {
            decoder,
            decoded: vec![],
            metadata: BackupMetadata::default(),
        };

        let Some(BackupElement {
            element: Some(Element::Metadata(metadata)),
        }) = importer.next_element().await?
        else {
            return Err(BackupError::MissingMetadata)?;
        };

        importer.metadata = metadata;
        Ok(importer)
    }
}
