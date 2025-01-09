use backup_element::BackupElement;
use futures::Stream;
use serde::{Deserialize, Serialize};

mod backup_element;

#[derive(Serialize, Deserialize)]
pub struct BackupMetadata {
    exported_at_ns: u64,
    exported_elements: Vec<BackupSelection>,
    /// Range of timestamp messages from_ns..to_ns
    from_ns: u64,
    to_ns: u64,
}

pub struct BackupOptions {
    from_ns: u64,
    to_ns: u64,
    elements: Vec<BackupSelection>,
}

#[derive(Serialize, Deserialize)]
pub enum BackupSelection {
    Messages,
    Consent,
}

impl BackupOptions {
    pub fn write(self) -> BackupWriter {
        BackupWriter { options: self }
    }
}

struct BackupWriter {
    options: BackupOptions,
    input_sreams: Vec<Box<dyn Stream<Item = Vec<u8>>>>,
}

impl Stream for BackupWriter {
    type Item = Vec<BackupElement>;
    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
    }
}
