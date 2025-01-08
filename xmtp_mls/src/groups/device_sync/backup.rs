use serde::{Deserialize, Serialize};

mod consent_save;
mod group_save;

#[derive(Serialize, Deserialize)]
pub enum BackupElement {
    Messages,
    Consent,
}

#[derive(Serialize, Deserialize)]
pub struct BackupMetadata {
    exported_at_ns: u64,
    exported_fields: Vec<BackupElement>,
    /// Range of timestamp messages from_ns..to_ns
    from_ns: u64,
    to_ns: u64,
}

pub struct BackupOptions {
    from_ns: u64,
    to_ns: u64,
    elements: Vec<BackupElement>,
}
