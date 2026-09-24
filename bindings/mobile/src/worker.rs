use crate::FfiError;
use std::sync::Arc;

#[derive(uniffi::Object)]
pub struct FfiSyncWorker {
    pub handle: Option<Arc<WorkerMetrics<SyncMetric>>>,
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiSyncWorker {
    #[xmtp_common::err_span]
    pub async fn wait(&self, metric: FfiSyncMetric, count: u64) -> Result<(), FfiError> {
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
    builder::DeviceSyncMode, worker::device_sync::worker::SyncMetric,
    worker::metrics::WorkerMetrics,
};

#[derive(uniffi::Enum)]
pub enum FfiDeviceSyncMode {
    Enabled,
    Disabled,
}

impl From<DeviceSyncMode> for FfiDeviceSyncMode {
    fn from(value: DeviceSyncMode) -> Self {
        match value {
            DeviceSyncMode::Enabled => Self::Enabled,
            DeviceSyncMode::Disabled => Self::Disabled,
        }
    }
}

impl From<FfiDeviceSyncMode> for DeviceSyncMode {
    fn from(value: FfiDeviceSyncMode) -> Self {
        match value {
            FfiDeviceSyncMode::Enabled => Self::Enabled,
            FfiDeviceSyncMode::Disabled => Self::Disabled,
        }
    }
}

use xmtp_mls::worker::WorkerKind;

#[derive(uniffi::Enum, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FfiWorkerKind {
    DeviceSync,
    DisappearingMessages,
    KeyPackageCleaner,
    CommitLog,
    TaskRunner,
}

impl From<FfiWorkerKind> for WorkerKind {
    fn from(k: FfiWorkerKind) -> Self {
        match k {
            FfiWorkerKind::DeviceSync => Self::DeviceSync,
            FfiWorkerKind::DisappearingMessages => Self::DisappearingMessages,
            FfiWorkerKind::KeyPackageCleaner => Self::KeyPackageCleaner,
            FfiWorkerKind::CommitLog => Self::CommitLog,
            FfiWorkerKind::TaskRunner => Self::TaskRunner,
        }
    }
}

impl From<WorkerKind> for FfiWorkerKind {
    fn from(k: WorkerKind) -> Self {
        match k {
            WorkerKind::DeviceSync => Self::DeviceSync,
            WorkerKind::DisappearingMessages => Self::DisappearingMessages,
            WorkerKind::KeyPackageCleaner => Self::KeyPackageCleaner,
            WorkerKind::CommitLog => Self::CommitLog,
            WorkerKind::TaskRunner => Self::TaskRunner,
        }
    }
}
