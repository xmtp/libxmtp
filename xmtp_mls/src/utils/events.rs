use crate::{
    client::ClientError,
    context::XmtpMlsLocalContext,
    groups::device_sync::DeviceSyncError,
    worker::{NeedsDbReconnect, Worker, WorkerKind},
};
use parking_lot::Mutex;
use serde::Serialize;
use std::sync::{Arc, LazyLock};
use thiserror::Error;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use xmtp_api::XmtpApi;
use xmtp_archive::exporter::ArchiveExporter;
use xmtp_common::time::now_ns;
use xmtp_db::{
    events::{Event, Events},
    StorageError, Store, XmtpDb, XmtpOpenMlsProvider,
};
use xmtp_proto::xmtp::device_sync::{BackupElementSelection, BackupOptions};

#[derive(Debug, Error)]
pub enum EventError {
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("client error: {0}")]
    Client(#[from] ClientError),
}
impl NeedsDbReconnect for EventError {
    fn needs_db_reconnect(&self) -> bool {
        match self {
            Self::Storage(s) => s.db_needs_connection(),
            Self::Client(s) => s.db_needs_connection(),
        }
    }
}

static EVENT_TX: LazyLock<Mutex<Option<UnboundedSender<Events>>>> =
    LazyLock::new(|| Mutex::default());

struct EventBuilder<E: AsRef<Event>, D: Serialize> {
    event: E,
    details: D,
    group_id: Option<Vec<u8>>,
}

impl<E, D> EventBuilder<E, D>
where
    E: AsRef<Event>,
    D: Serialize,
{
    fn build(self) -> Result<Events, serde_json::Error> {
        Ok(Events {
            created_at_ns: now_ns(),
            details: serde_json::to_value(&self.details)?,
            event: serde_json::to_string(self.event.as_ref())?,
            group_id: self.group_id,
        })
    }

    fn track(self) {
        let event = match self.build() {
            Ok(event) => event,
            Err(err) => {
                tracing::warn!("Unable to track event: {err:?}");
                return;
            }
        };

        if let Some(tx) = &*EVENT_TX.lock() {
            if let Err(err) = tx.send(event) {
                tracing::warn!("Unable to send event to writing worker: {err:?}");
            }
        }
    }
}

#[macro_export]
macro_rules! track {
    ($event:ident, ) => {};
}

pub struct EventWorker<ApiClient, Db> {
    rx: UnboundedReceiver<Events>,
    context: Arc<XmtpMlsLocalContext<ApiClient, Db>>,
}

impl<ApiClient, Db> EventWorker<ApiClient, Db> {
    pub(crate) fn new(context: &Arc<XmtpMlsLocalContext<ApiClient, Db>>) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        *EVENT_TX.lock() = Some(tx);
        Self {
            rx,
            context: context.clone(),
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl<ApiClient, Db> Worker for EventWorker<ApiClient, Db>
where
    ApiClient: XmtpApi + 'static + Send + Sync,
    Db: XmtpDb + 'static + Send + Sync,
{
    type Error = EventError;

    fn kind(&self) -> WorkerKind {
        WorkerKind::Event
    }

    async fn run_tasks(&mut self) -> Result<(), Self::Error> {
        while let Some(event) = self.rx.recv().await {
            event.store(&self.context.db())?;
        }
        Ok(())
    }
}

pub async fn upload_debug_archive(
    provider: &Arc<XmtpOpenMlsProvider>,
    device_sync_server_url: impl AsRef<str>,
) -> Result<String, DeviceSyncError> {
    let provider = provider.clone();
    let device_sync_server_url = device_sync_server_url.as_ref();

    let options = BackupOptions {
        elements: vec![BackupElementSelection::Event as i32],
        ..Default::default()
    };

    // Generate a random encryption key
    let key = xmtp_common::rand_vec::<32>();

    // Build the exporter
    let exporter = ArchiveExporter::new(options, provider.clone(), &key);

    let url = format!("{device_sync_server_url}/upload");
    let response = exporter.post_to_url(&url).await?;

    Ok(format!("{response}:{}", hex::encode(key)))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::{configuration::DeviceSyncUrls, tester, utils::events::upload_debug_archive};

    #[xmtp_common::test(unwrap_try = "true")]
    async fn test_debug_pkg() {
        tester!(alix, stream);
        tester!(bo);
        tester!(caro);

        let (bo_dm, _msg) = bo.test_talk_in_dm_with(&alix).await?;

        let alix_dm = alix.group(&bo_dm.group_id)?;
        alix_dm.send_message(b"Hello there").await?;
        tokio::time::sleep(Duration::from_millis(1000)).await;
        alix_dm.send_message(b"Hello there").await?;

        caro.test_talk_in_dm_with(&alix).await?;
        alix.sync_welcomes().await?;

        let g = alix
            .create_group_with_inbox_ids(&[bo.inbox_id().to_string()], None, None)
            .await?;
        g.update_group_name("Group with the buds".to_string())
            .await?;
        g.send_message(b"Hello there").await?;
        g.sync().await?;

        bo.sync_welcomes().await?;
        let bo_g = bo.group(&g.group_id)?;
        bo_g.send_message(b"Gonna add Caro").await?;
        bo_g.add_members_by_inbox_id(&[caro.inbox_id()]).await?;

        caro.sync_welcomes().await?;
        let caro_g = caro.group(&g.group_id)?;
        caro_g.send_message(b"hi guise!").await?;

        g.sync().await?;

        let k = upload_debug_archive(&alix.provider, DeviceSyncUrls::LOCAL_ADDRESS).await?;
        tracing::info!("{k}");

        // Exported and uploaded no problem
    }
}
