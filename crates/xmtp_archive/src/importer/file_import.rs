use super::ArchiveImporter;
use crate::ArchiveError;
use futures_util::io::BufReader;
use std::{path::Path, pin::Pin};
use tokio_util::compat::TokioAsyncReadCompatExt;

impl ArchiveImporter {
    pub async fn from_file(path: impl AsRef<Path>, key: &[u8]) -> Result<Self, ArchiveError> {
        let reader = tokio::fs::File::open(path.as_ref()).await?;
        let reader = BufReader::new(reader.compat());
        let reader = Box::pin(reader) as Pin<Box<_>>;

        Self::load(reader, key).await
    }
}
