use super::{
    handle::{SyncMetric, WorkerHandle},
    preference_sync::{store_preference_updates, PreferenceUpdate},
    DeviceSyncClient, DeviceSyncError, IterWithContent, ENC_KEY_SIZE,
};
use crate::{
    client::ClientError,
    configuration::WORKER_RESTART_DELAY,
    context::{XmtpContextProvider, XmtpMlsLocalContext},
    groups::{
        device_sync::{
            archive::{exporter::ArchiveExporter, ArchiveImporter},
            default_archive_options,
        },
        device_sync_legacy::DeviceSyncContent,
        GroupError,
    },
    subscriptions::{LocalEvents, StreamMessages, SubscribeError, SyncWorkerEvent},
};
use futures::{Stream, StreamExt};
use std::{pin::Pin, sync::Arc};
use tokio::sync::OnceCell;
#[cfg(not(target_arch = "wasm32"))]
use tokio_util::compat::TokioAsyncReadCompatExt;
use tracing::{info_span, instrument, Instrument};
use xmtp_db::{
    group_message::{MsgQueryArgs, StoredGroupMessage},
    processed_device_sync_messages::StoredProcessedDeviceSyncMessages,
    Store, XmtpDb,
};
use xmtp_proto::{
    api_client::trait_impls::XmtpApi,
    xmtp::device_sync::{
        content::{
            device_sync_content::Content as ContentProto, device_sync_key_type::Key,
            DeviceSyncAcknowledge, DeviceSyncKeyType, DeviceSyncReply as DeviceSyncReplyProto,
            DeviceSyncRequest as DeviceSyncRequestProto,
            PreferenceUpdates as PreferenceUpdatesProto,
        },
        BackupElementSelection, BackupOptions,
    },
    ConversionError,
};

pub struct SyncWorker<ApiClient, Db> {
    client: DeviceSyncClient<ApiClient, Db>,
    /// The sync events stream
    #[allow(clippy::type_complexity)]
    stream: Pin<Box<dyn Stream<Item = Result<LocalEvents, SubscribeError>> + Send + Sync>>,
    init: OnceCell<()>,

    handle: Arc<WorkerHandle<SyncMetric>>,
}

impl<ApiClient, Db> SyncWorker<ApiClient, Db>
where
    ApiClient: XmtpApi + Send + Sync + 'static,
    Db: XmtpDb + Send + Sync + 'static,
{
    pub(super) fn new(context: Arc<XmtpMlsLocalContext<ApiClient, Db>>) -> Self {
        let receiver = context.local_events.subscribe();
        let stream = Box::pin(receiver.stream_sync_messages());
        let client = DeviceSyncClient::new(context.clone());
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
                let inbox_id = self.client.context.identity.inbox_id().to_string();
                let installation_id = hex::encode(self.client.context.installation_id());

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
                        tracing::error!(inbox_id, installation_id, "Sync worker error: {err}");
                        // Wait before restarting.
                        xmtp_common::time::sleep(WORKER_RESTART_DELAY).await;
                        tracing::info!("Restarting sync worker...");
                    }
                }
            }
            .instrument(span),
        );
    }
}

impl<ApiClient, Db> SyncWorker<ApiClient, Db>
where
    ApiClient: XmtpApi + 'static,
    Db: XmtpDb + 'static,
{
    pub(super) fn handle(&self) -> &Arc<WorkerHandle<SyncMetric>> {
        &self.handle
    }

    async fn run(&mut self) -> Result<(), DeviceSyncError> {
        // Wait for the identity to be ready & verified before doing anything
        while !self.client.context.identity().is_ready() {
            xmtp_common::yield_().await
        }
        self.sync_init().await?;
        self.handle.increment_metric(SyncMetric::Init);

        while let Some(event) = self.stream.next().await {
            let event = event?;

            tracing::info!("New event: {event:?}");

            if let LocalEvents::SyncWorkerEvent(msg) = event {
                match msg {
                    SyncWorkerEvent::NewSyncGroupFromWelcome(_group_id) => {
                        self.evt_new_sync_group_from_welcome().await?;
                    }
                    SyncWorkerEvent::NewSyncGroupMsg => {
                        self.evt_new_sync_group_msg().await?;
                    }
                    SyncWorkerEvent::SyncPreferences(preference_updates) => {
                        self.evt_sync_preferences(preference_updates).await?;
                    }

                    // Device Sync V1 events
                    SyncWorkerEvent::Reply { message_id } => {
                        self.evt_v1_device_sync_reply(message_id).await?;
                    }
                    SyncWorkerEvent::Request { message_id } => {
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
            let provider = self.client.context.mls_provider();
            tracing::info!(
                inbox_id = client.inbox_id(),
                installation_id = hex::encode(client.context.installation_public_key()),
                "Initializing device sync... url: {:?}",
                client.context.device_sync.server_url
            );

            // The only thing that sync init really does right now is ensures that there's a sync group.
            if provider.db().primary_sync_group()?.is_none() {
                client.get_sync_group().await?;

                // Ask the sync group for a sync payload if the url is present.
                if self.client.context.device_sync_server_url().is_some() {
                    self.client.send_sync_request().await?;
                }
            }

            tracing::info!(
                inbox_id = client.inbox_id(),
                installation_id = hex::encode(client.context.installation_public_key()),
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
        self.client.add_new_installation_to_groups().await?;

        self.handle
            .increment_metric(SyncMetric::SyncGroupWelcomesProcessed);

        // Cycle the HMAC
        self.client.preference_sync.cycle_hmac().await?;

        Ok(())
    }

    async fn evt_new_sync_group_msg(&self) -> Result<(), DeviceSyncError> {
        self.client
            .process_new_sync_group_messages(&self.handle)
            .await?;
        Ok(())
    }

    async fn evt_sync_preferences(
        &self,
        updates: Vec<PreferenceUpdate>,
    ) -> Result<(), DeviceSyncError> {
        self.client
            .preference_sync
            .sync_preferences(updates)
            .await?;
        Ok(())
    }

    /// Called when this device has received a device sync v1 sync reply
    async fn evt_v1_device_sync_reply(&self, message_id: Vec<u8>) -> Result<(), DeviceSyncError> {
        let provider = self.client.context.mls_provider();
        if let Some(msg) = provider.db().get_group_message(&message_id)? {
            let content: DeviceSyncContent = serde_json::from_slice(&msg.decrypted_message_bytes)?;
            if let DeviceSyncContent::Reply(reply) = content {
                self.client.v1_process_sync_reply(reply).await?;
            };
        }
        Ok(())
    }

    /// Called when this device has received a device sync v1 sync request
    async fn evt_v1_device_sync_request(&self, message_id: Vec<u8>) -> Result<(), DeviceSyncError> {
        let provider = self.client.context.mls_provider();
        if let Some(msg) = provider.db().get_group_message(&message_id)? {
            let content: DeviceSyncContent = serde_json::from_slice(&msg.decrypted_message_bytes)?;
            if let DeviceSyncContent::Request(request) = content {
                self.client
                    .v1_reply_to_sync_request(request, &self.handle)
                    .await?;
            }
        }
        Ok(())
    }
}

impl<ApiClient, Db> DeviceSyncClient<ApiClient, Db>
where
    ApiClient: XmtpApi,
    Db: XmtpDb,
{
    async fn process_new_sync_group_messages(
        &self,
        handle: &WorkerHandle<SyncMetric>,
    ) -> Result<(), DeviceSyncError>
    where
        <Db as xmtp_db::XmtpDb>::Connection: 'static,
    {
        let unprocessed_messages = self.context.db().unprocessed_sync_group_messages()?;
        let installation_id = self.installation_id();

        tracing::info!("Processing {} messages.", unprocessed_messages.len());

        for (msg, content) in unprocessed_messages.clone().iter_with_content() {
            let is_external = msg.sender_installation_id != installation_id;

            tracing::info!(
                "Message content: (external: {is_external}) id={}, {content:?}",
                xmtp_common::fmt::truncate_hex(hex::encode(&msg.id))
            );

            if let Err(err) = self.process_message(handle, &msg, content).await {
                tracing::error!("Message processing: {err:?}");
            };
        }

        for msg in unprocessed_messages {
            StoredProcessedDeviceSyncMessages { message_id: msg.id }.store(&self.context.db())?;
        }

        Ok(())
    }

    async fn process_message(
        &self,
        handle: &WorkerHandle<SyncMetric>,
        msg: &StoredGroupMessage,
        content: ContentProto,
    ) -> Result<(), DeviceSyncError>
    where
        <Db as xmtp_db::XmtpDb>::Connection: 'static,
    {
        let provider = self.context.mls_provider();
        let installation_id = self.installation_id();
        let is_external = msg.sender_installation_id != installation_id;

        match content {
            ContentProto::Request(request) => {
                if !is_external {
                    // Ignore our own messages
                    return Ok(());
                }

                self.send_sync_reply(
                    Some(request.clone()),
                    || async { self.acknowledge_sync_request(msg, &request).await },
                    handle,
                )
                .await?;
            }
            ContentProto::Reply(reply) => {
                if !is_external {
                    // Ignore our own messages
                    return Ok(());
                }
                self.process_sync_payload(reply).await?;
                handle.increment_metric(SyncMetric::PayloadProcessed);
            }
            ContentProto::PreferenceUpdates(PreferenceUpdatesProto { updates }) => {
                if is_external {
                    tracing::info!("Incoming preference updates: {updates:?}");
                }

                // We'll process even our own messages here. The sync group message ordering takes authority over our own here.
                let updated = store_preference_updates(updates.clone(), provider, handle)?;
                if !updated.is_empty() {
                    let _ = self
                        .context
                        .local_events
                        .send(LocalEvents::PreferencesChanged(updated));
                }
            }
            ContentProto::Acknowledge(DeviceSyncAcknowledge { .. }) => {
                return Ok(());
            }
        }

        Ok(())
    }

    /// Acknowledge a sync request.
    /// Returns an error if request is already acknowledged by another installation.
    /// The first installation to acknowledge the sync request will be the installation to handle the response.
    pub async fn acknowledge_sync_request(
        &self,
        message: &StoredGroupMessage,
        request: &DeviceSyncRequestProto,
    ) -> Result<(), DeviceSyncError> {
        let sync_group = self.mls_store.group(&message.group_id)?;
        // Pull down any new messages
        sync_group.sync_with_conn().await?;

        let messages = sync_group.find_messages(&MsgQueryArgs::default())?;

        // Look in reverse for a request, and ensure it was not acknowledged by someone else.
        for (message, content) in messages.iter_with_content().rev() {
            let ContentProto::Acknowledge(acknowledge) = content else {
                continue;
            };
            if acknowledge.request_id != request.request_id {
                continue;
            }

            if message.sender_installation_id != self.installation_id() {
                // Request has already been acknowledged by another installation.
                // Let that installation handle it.
                return Err(DeviceSyncError::AlreadyAcknowledged);
            }

            return Ok(());
        }

        // Acknowledge and break.
        self.send_device_sync_message(ContentProto::Acknowledge(DeviceSyncAcknowledge {
            request_id: request.request_id.clone(),
        }))
        .await?;

        Ok(())
    }

    pub(crate) async fn send_sync_reply<F, Fut>(
        &self,
        request: Option<DeviceSyncRequestProto>,
        acknowledge: F,
        handle: &WorkerHandle<SyncMetric>,
    ) -> Result<(), DeviceSyncError>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<(), DeviceSyncError>>,
        <Db as xmtp_db::XmtpDb>::Connection: 'static,
    {
        if let Some(request) = &request {
            if request.kind() != BackupElementSelection::Unspecified {
                // This is a v1 request
                return Ok(());
            }
        }

        let provider = Arc::new(self.context.mls_provider());

        match acknowledge().await {
            Err(DeviceSyncError::AlreadyAcknowledged) => {
                tracing::info!("Request was already acknowledged by another installation.");
                return Ok(());
            }
            result => result?,
        }

        let Some(device_sync_server_url) = &self.context.device_sync.server_url else {
            tracing::info!("No message history payload sent - server url not present.");
            return Ok(());
        };
        tracing::info!("\x1b[33mSending sync payload.");

        let mut request_id = "".to_string();
        let options = if let Some(request) = request {
            let Some(options) = request.options else {
                return Err(DeviceSyncError::MissingOptions);
            };
            request_id = request.request_id;
            options
        } else {
            default_archive_options()
        };

        // Generate a random encryption key
        let key = xmtp_common::rand_vec::<32>();

        // Now we want to create an encrypted stream from our database to the history server.
        //
        // 1. Build the exporter
        let exporter = ArchiveExporter::new(options, provider.clone(), &key);
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
                installation_id = hex::encode(self.context.installation_public_key()),
                "Failed to upload file. Status code: {:?}",
                err.status()
            );
            return Err(DeviceSyncError::Reqwest(err));
        }

        // Build a sync reply message that the new installation will consume
        let reply = DeviceSyncReplyProto {
            encryption_key: Some(DeviceSyncKeyType {
                key: Some(Key::Aes256Gcm(key)),
            }),
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
        self.send_device_sync_message(ContentProto::Reply(reply))
            .await?;

        handle.increment_metric(SyncMetric::PayloadSent);

        Ok(())
    }

    pub async fn send_sync_request(&self) -> Result<(), ClientError> {
        tracing::info!("\x1b[33mSending a sync request.");

        let sync_group = self.get_sync_group().await?;
        sync_group
            .sync_with_conn()
            .await
            .map_err(GroupError::from)?;

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

        self.send_device_sync_message(ContentProto::Request(request))
            .await?;

        Ok(())
    }

    async fn is_reply_requested_by_installation(
        &self,
        reply: &DeviceSyncReplyProto,
    ) -> Result<bool, DeviceSyncError> {
        let sync_group = self.get_sync_group().await?;
        let stored_group = self.context.db().find_group(&sync_group.group_id)?;
        let Some(stored_group) = stored_group else {
            return Err(DeviceSyncError::MissingSyncGroup);
        };

        if reply.request_id == stored_group.added_by_inbox_id {
            return Ok(true);
        }

        let messages = sync_group.find_messages(&MsgQueryArgs::default())?;

        for (msg, content) in messages.iter_with_content() {
            if let ContentProto::Request(DeviceSyncRequestProto { request_id, .. }) = content {
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
        if reply.kind() != BackupElementSelection::Unspecified {
            // This is a legacy payload, the legacy function will process it.
            return Ok(());
        }

        tracing::info!("Inspecting sync payload.");

        // Check if this reply was asked for by this installation.
        if !self.is_reply_requested_by_installation(&reply).await? {
            // This installation didn't ask for it. Ignore the reply.
            tracing::info!("Sync response was not intended for this installation.");
            return Ok(());
        }

        // If a payload was sent to this installation,
        // that means they also sent this installation a bunch of welcomes.
        tracing::info!("Sync response is for this installation. Syncing welcomes.");
        self.welcome_service.sync_welcomes().await?;

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

        let Some(DeviceSyncKeyType {
            key: Some(Key::Aes256Gcm(key)),
        }) = reply.encryption_key
        else {
            return Err(ConversionError::Unspecified("encryption_key"))?;
        };

        let mut importer = ArchiveImporter::load(Box::pin(reader), &key).await?;

        tracing::info!("Importing the sync payload.");
        // Run the import.
        importer.run(self.context.clone()).await?;

        Ok(())
    }
}
