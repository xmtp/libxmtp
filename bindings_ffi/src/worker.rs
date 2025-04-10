use std::sync::Arc;

use xmtp_mls::groups::device_sync::handle::{SyncMetric, WorkerHandle};

use crate::GenericError;

#[derive(uniffi::Object)]
pub struct FfiSyncWorker {
    pub handle: Option<Arc<WorkerHandle<SyncMetric>>>,
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiSyncWorker {
    pub async fn wait(&self, metric: FfiSyncMetric, count: u64) -> Result<(), GenericError> {
        let Some(handle) = self.handle.clone() else {
            tracing::warn!("Tried to wait on a worker without a handle.");
            return Ok(());
        };

        handle.wait(metric.into(), count as usize).await?;

        Ok(())
    }
}

#[derive(uniffi::Enum)]
pub enum FfiSyncMetric {
    Init,
    SyncGroupWelcomesProcessed,
    RequestReceived,
    PayloadSent,
    PayloadProcessed,
    HmacSent,
    HmacReceived,
    ConsentSent,
    ConsentReceived,

    V1ConsentSent,
    V1HmacSent,
    V1PayloadSent,
    V1PayloadProcessed,
}

impl From<FfiSyncMetric> for SyncMetric {
    fn from(value: FfiSyncMetric) -> Self {
        match value {
            FfiSyncMetric::Init => Self::Init,
            FfiSyncMetric::SyncGroupWelcomesProcessed => Self::SyncGroupWelcomesProcessed,
            FfiSyncMetric::RequestReceived => Self::RequestReceived,
            FfiSyncMetric::PayloadSent => Self::PayloadSent,
            FfiSyncMetric::PayloadProcessed => Self::PayloadProcessed,
            FfiSyncMetric::HmacSent => Self::HmacSent,
            FfiSyncMetric::HmacReceived => Self::HmacReceived,
            FfiSyncMetric::ConsentSent => Self::ConsentSent,
            FfiSyncMetric::ConsentReceived => Self::ConsentReceived,
            FfiSyncMetric::V1ConsentSent => Self::V1ConsentSent,
            FfiSyncMetric::V1HmacSent => Self::V1HmacSent,
            FfiSyncMetric::V1PayloadSent => Self::V1PayloadSent,
            FfiSyncMetric::V1PayloadProcessed => Self::V1PayloadProcessed,
        }
    }
}
