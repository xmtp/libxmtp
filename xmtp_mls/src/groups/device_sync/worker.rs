use super::handle::{SyncMetric, WorkerHandle};
use super::preference_sync::UserPreferenceUpdate;
use super::{OldDeviceSyncContent, DeviceSyncError};
use crate::{configuration::WORKER_RESTART_DELAY, subscriptions::SyncEvent};
use crate::{
    subscriptions::{LocalEvents, StreamMessages, SubscribeError},
    Client,
};
use futures::{Stream, StreamExt};
use std::{pin::Pin, sync::Arc};
use tokio::sync::OnceCell;
use tracing::{info_span, instrument, Instrument};
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_proto::api_client::trait_impls::XmtpApi;

pub struct SyncWorker<ApiClient, V> {
    client: Client<ApiClient, V>,
    /// The sync events stream
    #[allow(clippy::type_complexity)]
    stream: Pin<Box<dyn Stream<Item = Result<LocalEvents, SubscribeError>> + Send + Sync>>,
    init: OnceCell<()>,

    handle: Arc<WorkerHandle<SyncMetric>>,
}

impl<ApiClient, V> SyncWorker<ApiClient, V>
where
    ApiClient: XmtpApi + Send + Sync + 'static,
    V: SmartContractSignatureVerifier + Send + Sync + 'static,
{
    pub(super) fn new(client: Client<ApiClient, V>) -> Self {
        let receiver = client.local_events.subscribe();
        let stream = Box::pin(receiver.stream_sync_messages());

        Self {
            client,
            stream,
            init: OnceCell::new(),
            handle: Arc::new(WorkerHandle::new()),
        }
    }

    pub(super) fn spawn_worker(mut self) {
        let span = info_span!("\x1b[34mDEVICE SYNC");

        xmtp_common::spawn(
            None,
            async move {
                let inbox_id = self.client.inbox_id().to_string();
                let installation_id = hex::encode(self.client.installation_public_key());

                while let Err(err) = self.run().await {
                    tracing::info!("Running worker..");
                    if err.db_needs_connection() {
                        tracing::warn!(
                            inbox_id,
                            installation_id,
                            "Pool disconnected. task will restart on reconnect"
                        );
                        break;
                    } else {
                        tracing::error!(inbox_id, installation_id, "sync worker error {err}");
                        // Wait before restarting.
                        xmtp_common::time::sleep(WORKER_RESTART_DELAY).await;
                    }
                }
            }
            .instrument(span),
        );
    }
}

impl<ApiClient, V> SyncWorker<ApiClient, V>
where
    ApiClient: XmtpApi + 'static,
    V: SmartContractSignatureVerifier + 'static,
{
    pub(super) fn handle(&self) -> &Arc<WorkerHandle<SyncMetric>> {
        &self.handle
    }

    async fn run(&mut self) -> Result<(), DeviceSyncError> {
        // Wait for the identity to be ready & verified before doing anything
        while !self.client.identity().is_ready() {
            xmtp_common::yield_().await
        }
        self.sync_init().await?;
        self.handle.increment_metric(SyncMetric::Init);

        while let Some(event) = self.stream.next().await {
            let event = event?;

            if let LocalEvents::SyncEvent(msg) = event {
                match msg {
                    SyncEvent::NewSyncGroupFromWelcome => {
                        self.evt_new_sync_group_from_welcome().await?;
                    }
                    SyncEvent::NewSyncGroupMsg => {
                        self.evt_new_sync_group_msg().await?;
                    }
                    SyncEvent::PreferencesOutgoing(preference_updates) => {
                        self.evt_preferences_outgoing(preference_updates).await?;
                    }
                    SyncEvent::PreferencesChanged(_) => {
                        // Intentionally left blank. This event is for streaming to consume.
                    }
                    // Device Sync V1 events
                    SyncEvent::Reply { message_id } => {
                        self.evt_v1_device_sync_reply(message_id).await?;
                    }
                    SyncEvent::Request { message_id } => {
                        self.evt_v1_device_sync_request(message_id).await?;
                    }
                }
            };
        }
        Ok(())
    }

    //// Ideally called when the client is registered.
    //// Will auto-send a sync request if sync group is created.
    #[instrument(level = "trace", skip_all)]
    async fn sync_init(&mut self) -> Result<(), DeviceSyncError> {
        let Self {
            ref init,
            ref client,
            ..
        } = self;

        init.get_or_try_init(|| async {
            let provider = self.client.mls_provider()?;
            tracing::info!(
                inbox_id = client.inbox_id(),
                installation_id = hex::encode(client.installation_public_key()),
                "Initializing device sync... url: {:?}",
                client.device_sync.server_url
            );

            // The only thing that sync init really does right now is ensures that there's a sync group.
            client.ensure_sync_group(&provider).await?;

            tracing::info!(
                inbox_id = client.inbox_id(),
                installation_id = hex::encode(client.installation_public_key()),
                "Device sync initialized."
            );

            Ok(())
        })
        .await
        .copied()
    }

    async fn evt_new_sync_group_from_welcome(&self) -> Result<(), DeviceSyncError> {
        tracing::info!("New sync group from welcome detected.");
        // A new sync group from a welcome indicates a new installation.
        // We need to add that installation to the groups.
        let provider = self.client.mls_provider()?;
        if self
            .client
            .acknowledge_new_sync_group(&provider)
            .await
            .is_err()
        {
            // We do not want to process the new installation if another installation is already processing it.
            self.handle
                .increment_metric(SyncMetric::SyncGroupWelcomesProcessed);
            return Ok(());
        }
        self.client.add_new_installation_to_groups().await?;
        self.handle
            .increment_metric(SyncMetric::SyncGroupWelcomesProcessed);

        self.client
            .send_sync_payload(
                None,
                || async { self.client.acknowledge_new_sync_group(&provider).await },
                &self.handle,
            )
            .await?;

        // Cycle the HMAC
        UserPreferenceUpdate::cycle_hmac(&self.client).await?;

        Ok(())
    }

    async fn evt_new_sync_group_msg(&self) -> Result<(), DeviceSyncError> {
        let provider = self.client.mls_provider()?;
        self.client
            .process_new_sync_group_messages(&provider, &self.handle)
            .await?;
        Ok(())
    }

    async fn evt_preferences_outgoing(
        &self,
        preference_updates: Vec<UserPreferenceUpdate>,
    ) -> Result<(), DeviceSyncError> {
        UserPreferenceUpdate::sync(preference_updates, &self.client).await?;
        Ok(())
    }

    async fn evt_v1_device_sync_reply(&self, message_id: Vec<u8>) -> Result<(), DeviceSyncError> {
        let provider = self.client.mls_provider()?;
        if let Some(msg) = provider.conn_ref().get_group_message(&message_id)? {
            let content: OldDeviceSyncContent = serde_json::from_slice(&msg.decrypted_message_bytes)?;
            if let OldDeviceSyncContent::Payload(reply) = content {
                self.client
                    .v1_process_sync_reply(&provider, reply, &self.handle)
                    .await?;
            }
        }
        Ok(())
    }

    async fn evt_v1_device_sync_request(&self, message_id: Vec<u8>) -> Result<(), DeviceSyncError> {
        let provider = self.client.mls_provider()?;
        if let Some(msg) = provider.conn_ref().get_group_message(&message_id)? {
            let content: OldDeviceSyncContent = serde_json::from_slice(&msg.decrypted_message_bytes)?;
            if let OldDeviceSyncContent::Request(request) = content {
                self.client
                    .v1_reply_to_sync_request(&provider, request, &self.handle)
                    .await?;
            }
        }
        Ok(())
    }
}
