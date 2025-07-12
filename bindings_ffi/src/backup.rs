use xmtp_mls::error::GenericError;
use xmtp_mls::backup::{BackupMetadata, BackupOptions, BackupElementSelection};

pub struct FfiBackupMetadata {
    backup_version: u16,
    elements: Vec<FfiBackupElementSelection>,
    exported_at_ns: i64,
    start_ns: Option<i64>,
    end_ns: Option<i64>,
}

pub struct FfiArchiveOptions {
    start_ns: Option<i64>,
    end_ns: Option<i64>,
    elements: Vec<FfiBackupElementSelection>,
}

pub enum FfiBackupElementSelection {
    Messages,
    Consent,
}

impl From<BackupMetadata> for FfiBackupMetadata {
    fn from(value: BackupMetadata) -> Self {
        // ... existing code ...
    }
}

impl From<FfiArchiveOptions> for BackupOptions {
    fn from(value: FfiArchiveOptions) -> Self {
        // ... existing code ...
    }
}

impl From<FfiBackupElementSelection> for BackupElementSelection {
    fn from(value: FfiBackupElementSelection) -> Self {
        // ... existing code ...
    }
}

impl TryFrom<BackupElementSelection> for FfiBackupElementSelection {
    type Error = DeserializationError;

    fn try_from(value: BackupElementSelection) -> Result<Self, Self::Error> {
        // ... existing code ...
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::*;
    use xmtp_cryptography::utils::LocalWallet;

    #[tokio::test]
    async fn test_backup() {
        let client = new_test_client().await;
        let backup = client.backup().await.unwrap();

        assert!(!backup.is_empty());
    }

    #[tokio::test]
    async fn test_restore() {
        let client_a = new_test_client().await;
        let backup = client_a.backup().await.unwrap();

        let client_b = new_test_client().await;
        client_b.restore(backup).await.unwrap();

        assert_eq!(client_a.inbox_id(), client_b.inbox_id());
    }

    #[tokio::test]
    async fn test_restore_with_invalid_backup() {
        let client = new_test_client().await;
        let result = client.restore(vec![0, 1, 2, 3]).await;

        assert!(result.is_err());
    }
} 