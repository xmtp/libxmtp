use xmtp_mls::builder::SyncWorkerMode;

#[derive(uniffi::Enum)]
pub enum FfiSyncWorkerMode {
    Enabled,
    Disabled,
}

impl From<FfiSyncWorkerMode> for SyncWorkerMode {
    fn from(value: FfiSyncWorkerMode) -> Self {
        match value {
            FfiSyncWorkerMode::Enabled => Self::Enabled,
            FfiSyncWorkerMode::Disabled => Self::Disabled,
        }
    }
}

impl From<SyncWorkerMode> for FfiSyncWorkerMode {
    fn from(value: SyncWorkerMode) -> Self {
        match value {
            SyncWorkerMode::Enabled => Self::Enabled,
            SyncWorkerMode::Disabled => Self::Disabled,
        }
    }
}
