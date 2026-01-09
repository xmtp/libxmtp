use crate::GenericError;
use std::sync::Arc;

#[derive(uniffi::Object)]
pub struct FfiSyncWorker {
    pub handle: Option<Arc<WorkerMetrics<SyncMetric>>>,
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiSyncWorker {
    pub async fn wait(&self, metric: FfiSyncMetric, count: u64) -> Result<(), GenericError> {
        let Some(handle) = self.handle.clone() else {
            tracing::warn!("Tried to wait on a worker without a handle.");
            return Ok(());
        };

        handle
            .register_interest(metric.into(), count as usize)
            .wait()
            .await?;

        Ok(())
    }
}

#[derive(uniffi::Enum)]
pub enum FfiSyncMetric {
    Init,
    SyncGroupCreated,
    SyncGroupWelcomesProcessed,
    RequestReceived,
    PayloadSent,
    PayloadProcessed,
    HmacSent,
    HmacReceived,
    ConsentSent,
    ConsentReceived,
}

impl From<FfiSyncMetric> for SyncMetric {
    fn from(value: FfiSyncMetric) -> Self {
        match value {
            FfiSyncMetric::Init => Self::Init,
            FfiSyncMetric::SyncGroupCreated => Self::SyncGroupCreated,
            FfiSyncMetric::SyncGroupWelcomesProcessed => Self::SyncGroupWelcomesProcessed,
            FfiSyncMetric::RequestReceived => Self::RequestReceived,
            FfiSyncMetric::PayloadSent => Self::PayloadSent,
            FfiSyncMetric::PayloadProcessed => Self::PayloadProcessed,
            FfiSyncMetric::HmacSent => Self::HmacSent,
            FfiSyncMetric::HmacReceived => Self::HmacReceived,
            FfiSyncMetric::ConsentSent => Self::ConsentSent,
            FfiSyncMetric::ConsentReceived => Self::ConsentReceived,
        }
    }
}
use xmtp_mls::{
    builder::SyncWorkerMode, groups::device_sync::worker::SyncMetric,
    worker::metrics::WorkerMetrics,
};

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
