use super::{
    handle::{SyncMetric, WorkerHandle},
    preference_sync::UserPreferenceUpdate,
    DeviceSyncContent, DeviceSyncError, IterWithContent, ENC_KEY_SIZE,
};
use crate::{
    configuration::WORKER_RESTART_DELAY,
    groups::{
        device_sync::{
            backup::{exporter::BackupExporter, BackupImporter},
            default_backup_options, AcknowledgeKind,
        },
        scoped_client::ScopedGroupClient,
    },
    subscriptions::{LocalEvents, StreamMessages, SubscribeError, SyncEvent},
    Client,
};
use futures::{Stream, StreamExt};
use std::collections::HashMap;
use std::{pin::Pin, sync::Arc};
use tokio::sync::OnceCell;
#[cfg(not(target_arch = "wasm32"))]
use tokio_util::compat::TokioAsyncReadCompatExt;
use tracing::{info_span, instrument, Instrument};
use xmtp_db::{
    group_message::{MsgQueryArgs, StoredGroupMessage},
    user_preferences::StoredUserPreferences,
    XmtpOpenMlsProvider,
};
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_proto::{
    api_client::trait_impls::XmtpApi,
    xmtp::{
        device_sync::{BackupElementSelection, BackupOptions},
        mls::message_contents::{
            DeviceSyncReply as DeviceSyncReplyProto, DeviceSyncRequest as DeviceSyncRequestProto,
        },
    },
};

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
            client.get_sync_group(&provider).await?;

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
        UserPreferenceUpdate::cycle_hmac(&self.client, &provider).await?;

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
        let provider = self.client.mls_provider()?;
        UserPreferenceUpdate::sync(preference_updates, &self.client, &provider).await?;
        Ok(())
    }

    async fn evt_v1_device_sync_reply(&self, message_id: Vec<u8>) -> Result<(), DeviceSyncError> {
        let provider = self.client.mls_provider()?;
        if let Some(msg) = provider.conn_ref().get_group_message(&message_id)? {
            let content: DeviceSyncContent = serde_json::from_slice(&msg.decrypted_message_bytes)?;
            if let DeviceSyncContent::Payload(reply) = content {
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
            let content: DeviceSyncContent = serde_json::from_slice(&msg.decrypted_message_bytes)?;
            if let DeviceSyncContent::Request(request) = content {
                self.client
                    .v1_reply_to_sync_request(&provider, request, &self.handle)
                    .await?;
            }
        }
        Ok(())
    }
}

impl<ApiClient, V> Client<ApiClient, V>
where
    ApiClient: XmtpApi,
    V: SmartContractSignatureVerifier,
{
    async fn process_new_sync_group_messages(
        &self,
        provider: &XmtpOpenMlsProvider,
        handle: &WorkerHandle<SyncMetric>,
    ) -> Result<(), DeviceSyncError> {
        let sync_group = self.get_sync_group(provider).await?;
        let Some(mut cursor) = StoredUserPreferences::sync_cursor(provider.conn_ref())? else {
            tracing::warn!("Tried to process sync group message, but sync cursor is missing, and should havae been set upon group creation.");
            return Ok(());
        };

        let messages = sync_group.get_sync_group_messages(cursor.offset)?;
        let installation_id = self.installation_id();
        let external_count = messages
            .iter()
            .filter(|msg| msg.sender_installation_id != installation_id)
            .count();

        tracing::info!(
            "Processing {} sync group messages that were sent after {}. ({external_count} external, group_id {:?})",
            messages.len(),
            cursor.offset,
            &sync_group.group_id[..4]
        );

        for (msg, content) in messages.iter_with_content() {
            tracing::info!("Message content: {content:?}");
            if let Err(err) = self.process_message(provider, handle, &msg, content).await {
                tracing::error!("Message processing: {err:?}");
            };

            // Move the cursor
            cursor.offset += 1;
            StoredUserPreferences::store_sync_cursor(provider.conn_ref(), &cursor)?;
        }

        Ok(())
    }

    async fn process_message(
        &self,
        provider: &XmtpOpenMlsProvider,
        handle: &WorkerHandle<SyncMetric>,
        msg: &StoredGroupMessage,
        content: DeviceSyncContent,
    ) -> Result<(), DeviceSyncError> {
        let installation_id = self.installation_id();
        let is_external = msg.sender_installation_id != installation_id;
        match content {
            DeviceSyncContent::Request(request) => {
                if msg.sender_installation_id == self.installation_id() {
                    // Ignore our own messages
                    return Ok(());
                }

                self.send_sync_payload(
                    Some(request),
                    || async { self.acknowledge_sync_request(provider).await },
                    handle,
                )
                .await?;
            }
            DeviceSyncContent::Payload(payload) => {
                if msg.sender_installation_id == self.installation_id() {
                    // Ignore our own messages
                    return Ok(());
                }

                self.process_sync_payload(payload).await?;
                handle.increment_metric(SyncMetric::PayloadProcessed);
            }
            DeviceSyncContent::PreferenceUpdates(updates) => {
                if is_external {
                    tracing::info!("Incoming preference updates: {updates:?}");
                }

                // We'll process even our own messages here. The sync group message ordering takes authority over our own here.
                let mut updated = vec![];
                for update in updates.clone() {
                    updated.extend(update.store(provider, handle)?);
                }

                if !updated.is_empty() {
                    let _ = self.local_events.send(LocalEvents::SyncEvent(
                        SyncEvent::PreferencesChanged(updated),
                    ));
                }
            }
            DeviceSyncContent::Acknowledge(_) => {
                return Ok(());
            }
        }

        Ok(())
    }

    /// Acknowledge the existence of a new sync group.
    /// Returns an error if sync group is already acknowledged by another installation.
    /// The first installation to acknowledge a sync group will the the installation to handle the sync.
    pub async fn acknowledge_new_sync_group(
        &self,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<(), DeviceSyncError> {
        let sync_group = self.get_sync_group(provider).await?;
        // Pull down any new messages
        sync_group.sync_with_conn(provider).await?;

        let messages = sync_group.find_messages(&MsgQueryArgs::default())?;

        let acknowledgement = messages.iter_with_content().find(|(_msg, content)| {
            matches!(
                content,
                DeviceSyncContent::Acknowledge(AcknowledgeKind::SyncGroupPresence)
            )
        });
        let Some((acknowledgement, _content)) = acknowledgement else {
            // Send an acknowledgement if there is none.
            self.send_device_sync_message(
                provider,
                DeviceSyncContent::Acknowledge(AcknowledgeKind::SyncGroupPresence),
            )
            .await?;
            return Ok(());
        };

        let installation_id = self.installation_id();
        if installation_id != acknowledgement.sender_installation_id {
            // Another device acknowledged the group. They're handling it.
            tracing::info!("Another installation already acknowledged the new sync group.");
            return Err(DeviceSyncError::AlreadyAcknowledged);
        }

        Ok(())
    }

    /// Acknowledge a sync request.
    /// Returns an error if request is already acknowledged by another installation.
    /// The first installation to acknowledge the sync request will be the installation to handle the response.
    pub async fn acknowledge_sync_request(
        &self,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<(), DeviceSyncError> {
        let sync_group = self.get_sync_group(provider).await?;
        // Pull down any new messages
        sync_group.sync_with_conn(provider).await?;

        let messages = sync_group.find_messages(&MsgQueryArgs::default())?;

        let mut acknowledged = HashMap::new();
        // Look in reverse for a request, and ensure it was not acknowledged by someone else.
        for message in messages.iter().rev() {
            let Some(content) =
                serde_json::from_slice::<DeviceSyncContent>(&message.decrypted_message_bytes).ok()
            else {
                continue;
            };

            match content {
                DeviceSyncContent::Acknowledge(AcknowledgeKind::Request { request_id }) => {
                    acknowledged.insert(request_id, message.sender_installation_id.clone());
                }
                DeviceSyncContent::Request(req) => {
                    if let Some(installation_id) = acknowledged.get(&req.request_id) {
                        if installation_id != self.installation_id() {
                            // Request has already been acknowledged by another installation.
                            // Let that installation handle it.
                            return Err(DeviceSyncError::AlreadyAcknowledged);
                        }

                        // We've already acknowledged it. Return here.
                        return Ok(());
                    }

                    // Acknowledge and break.
                    self.send_device_sync_message(
                        provider,
                        DeviceSyncContent::Acknowledge(AcknowledgeKind::Request {
                            request_id: req.request_id,
                        }),
                    )
                    .await?;

                    break;
                }
                _ => {}
            }
        }

        Ok(())
    }

    pub(crate) async fn send_sync_payload<F, Fut>(
        &self,
        request: Option<DeviceSyncRequestProto>,
        acknowledge: F,
        handle: &WorkerHandle<SyncMetric>,
    ) -> Result<(), DeviceSyncError>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<(), DeviceSyncError>>,
    {
        let provider = Arc::new(self.mls_provider()?);

        match acknowledge().await {
            Err(DeviceSyncError::AlreadyAcknowledged) => {
                tracing::info!("Sync group was already acknowledged by another installation.");
                return Ok(());
            }
            result => result?,
        }

        let Some(device_sync_server_url) = &self.device_sync.server_url else {
            tracing::info!("No message history payload sent - server url not present.");
            return Ok(());
        };
        tracing::info!("Sending sync payload.");

        let mut request_id = "".to_string();
        let options = if let Some(request) = request {
            let Some(options) = request.options else {
                return Err(DeviceSyncError::MissingOptions);
            };
            request_id = request.request_id;
            options
        } else {
            default_backup_options()
        };

        // Generate a random encryption key
        let key = xmtp_common::rand_vec::<32>();

        // Now we want to create an encrypted stream from our database to the history server.
        //
        // 1. Build the exporter
        let exporter = BackupExporter::new(options, &provider, &key);
        let metadata = exporter.metadata().clone();

        #[cfg(not(target_arch = "wasm32"))]
        let body = {
            // 2. A compat layer to have futures AsyncRead play nice with tokio's AsyncRead
            let exporter_compat = tokio_util::compat::FuturesAsyncReadCompatExt::compat(exporter);
            // 3. Add a stream layer over the async read
            let stream = tokio_util::io::ReaderStream::new(exporter_compat);
            // 4. Pipe that stream as the body to the request to the history server
            reqwest::Body::wrap_stream(stream)
        };
        #[cfg(target_arch = "wasm32")]
        let body = {
            use futures::AsyncReadExt;
            // Make exporter mutable
            let mut exporter = exporter;

            // Wasm does not support stream uploads. So we'll just consume the stream into a vec.
            let mut buffer = Vec::new();
            exporter.read_to_end(&mut buffer).await?;
            buffer
        };

        // 5. Make the request
        let url = format!("{device_sync_server_url}/upload");
        tracing::info!("Uploading sync payload to history server...");
        let response = reqwest::Client::new().post(url).body(body).send().await?;
        tracing::info!("Done uploading sync payload to history server.");

        if let Err(err) = response.error_for_status_ref() {
            tracing::error!(
                inbox_id = self.inbox_id(),
                installation_id = hex::encode(self.installation_public_key()),
                "Failed to upload file. Status code: {:?}",
                err.status()
            );
            return Err(DeviceSyncError::Reqwest(err));
        }

        // Build a sync reply message that the new installation will consume
        let reply = DeviceSyncReplyProto {
            key,
            request_id,
            url: format!("{device_sync_server_url}/files/{}", response.text().await?),
            metadata: Some(metadata),

            // Deprecated fields
            ..Default::default()
        };

        // Check acknowledgement one more time before responding to try to avoid double-responses
        // from two or more old installations.
        match acknowledge().await {
            Err(DeviceSyncError::AlreadyAcknowledged) => {
                return Ok(());
            }
            result => result?,
        }

        // Send the message out over the network
        self.send_device_sync_message(&provider, DeviceSyncContent::Payload(reply))
            .await?;

        handle.increment_metric(SyncMetric::PayloadSent);

        Ok(())
    }

    pub async fn send_sync_request(
        &self,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<(), DeviceSyncError> {
        tracing::info!("Sending a sync request.");

        let sync_group = self.get_sync_group(provider).await?;
        sync_group.sync_with_conn(provider).await?;

        let request = DeviceSyncRequestProto {
            request_id: xmtp_common::rand_string::<ENC_KEY_SIZE>(),
            options: Some(BackupOptions {
                elements: vec![
                    BackupElementSelection::Messages as i32,
                    BackupElementSelection::Consent as i32,
                ],
                ..Default::default()
            }),

            // Deprecated fields
            ..Default::default()
        };

        self.send_device_sync_message(provider, DeviceSyncContent::Request(request))
            .await?;

        Ok(())
    }

    async fn is_reply_requested_by_installation(
        &self,
        provider: &XmtpOpenMlsProvider,
        reply: &DeviceSyncReplyProto,
    ) -> Result<bool, DeviceSyncError> {
        let sync_group = self.get_sync_group(provider).await?;
        let stored_group = provider.conn_ref().find_group(&sync_group.group_id)?;
        let Some(stored_group) = stored_group else {
            return Err(DeviceSyncError::MissingSyncGroup);
        };

        if reply.request_id == stored_group.added_by_inbox_id {
            return Ok(true);
        }

        let messages = sync_group.find_messages(&MsgQueryArgs::default())?;

        for (msg, content) in messages.iter_with_content() {
            if let DeviceSyncContent::Request(DeviceSyncRequestProto { request_id, .. }) = content {
                if *request_id == reply.request_id
                    && msg.sender_installation_id == self.installation_id()
                {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    pub async fn process_sync_payload(
        &self,
        reply: DeviceSyncReplyProto,
    ) -> Result<(), DeviceSyncError> {
        tracing::info!("Inspecting sync payload.");
        let provider = Arc::new(self.mls_provider()?);

        // Check if this reply was asked for by this installation.
        if !self
            .is_reply_requested_by_installation(&provider, &reply)
            .await?
        {
            // This installation didn't ask for it. Ignore the reply.
            tracing::info!("Sync response was not intended for this installation.");
            return Ok(());
        }

        // If a payload was sent to this installation,
        // that means they also sent this installation a bunch of welcomes.
        tracing::info!("Sync response is for this installation. Syncing welcomes.");
        self.sync_welcomes(&provider).await?;

        // Get a download stream of the payload.
        tracing::info!("Downloading sync payload.");
        let response = reqwest::Client::new().get(reply.url).send().await?;
        if let Err(err) = response.error_for_status_ref() {
            tracing::error!(
                "Failed to download file. Status code: {} Response: {:?}",
                response.status(),
                response
            );
            return Err(DeviceSyncError::Reqwest(err));
        }

        #[cfg(not(target_arch = "wasm32"))]
        let reader = {
            use futures::StreamExt;
            let stream = response.bytes_stream().map(|result| {
                result.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
            });

            // Convert that stream into a reader
            let tokio_reader = tokio_util::io::StreamReader::new(stream);
            // Convert that tokio reader into a futures reader.
            // We use futures reader for WASM compat.
            tokio_reader.compat()
        };
        #[cfg(target_arch = "wasm32")]
        let reader = {
            // WASM doesn't support request streaming. Consume the response instead.
            futures::io::Cursor::new(response.bytes().await?)
        };

        // Create an importer around that futures_reader.
        let mut importer = BackupImporter::load(Box::pin(reader), &reply.key).await?;

        tracing::info!("Importing the sync payload.");
        // Run the import.
        importer.run(self).await?;

        Ok(())
    }
}
