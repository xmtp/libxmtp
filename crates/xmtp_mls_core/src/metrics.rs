use crate::worker::metrics::WorkerMetrics;

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
pub enum SyncMetric {
    Init,
    SyncGroupCreated,
    HmacSent,
    HmacReceived,
    ConsentSent,
    ConsentReceived,
}

impl WorkerMetrics<SyncMetric> {
    pub async fn wait_for_init(&self) -> Result<(), xmtp_common::time::Expired> {
        self.register_interest(SyncMetric::Init, 1).wait().await
    }
}
