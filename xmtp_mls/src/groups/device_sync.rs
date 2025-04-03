use super::{scoped_client::ScopedGroupClient, GroupError, MlsGroup};
use crate::{
    client::ClientError,
    subscriptions::{LocalEvents, StreamMessages, SubscribeError},
    Client,
};
use crate::{configuration::WORKER_RESTART_DELAY, subscriptions::SyncEvent};
use backup::BackupImporter;
use backup::{exporter::BackupExporter, BackupError};
use futures::{future::join_all, Stream, StreamExt};
use handle::{SyncMetric, WorkerHandle};
use preference_sync::UserPreferenceUpdate;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, pin::Pin, sync::Arc};
use thiserror::Error;
use tokio::sync::OnceCell;
use tokio_util::compat::TokioAsyncReadCompatExt;
use tracing::instrument;
use xmtp_common::{retry_async, time::Duration, ExponentialBackoff};
use xmtp_common::{Retry, RetryableError};
use xmtp_db::{
    group::GroupQueryArgs,
    group_message::{MsgQueryArgs, StoredGroupMessage},
    xmtp_openmls_provider::XmtpOpenMlsProvider,
    NotFound, StorageError,
};
use xmtp_db::{user_preferences::StoredUserPreferences, Store};
use xmtp_id::{associations::DeserializationError, scw_verifier::SmartContractSignatureVerifier};
use xmtp_proto::xmtp::mls::message_contents::plaintext_envelope::Content;
use xmtp_proto::xmtp::mls::message_contents::{
    plaintext_envelope::v2::MessageType,
    plaintext_envelope::{V1, V2},
    PlaintextEnvelope,
};
use xmtp_proto::xmtp::mls::message_contents::{
    DeviceSyncReply as DeviceSyncReplyProto, DeviceSyncRequest as DeviceSyncRequestProto,
};
use xmtp_proto::{
    api_client::trait_impls::XmtpApi,
    xmtp::device_sync::{BackupElementSelection, BackupOptions},
};

pub mod backup;
pub mod handle;
pub mod preference_sync;

#[cfg(test)]
mod tests;

pub const ENC_KEY_SIZE: usize = 32; // 256-bit key
pub const NONCE_SIZE: usize = 12; // 96-bit nonce

#[derive(Debug, Error)]
pub enum DeviceSyncError {
    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),
    #[error("Serialization/Deserialization Error {0}")]
    Serde(#[from] serde_json::Error),
    #[error(transparent)]
    ProtoConversion(#[from] xmtp_proto::ConversionError),
    #[error("AES-GCM encryption error")]
    AesGcm(#[from] aes_gcm::Error),
    #[error("reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("type conversion error")]
    Conversion,
    #[error("utf-8 error: {0}")]
    UTF8(#[from] std::str::Utf8Error),
    #[error("client error: {0}")]
    Client(#[from] ClientError),
    #[error("group error: {0}")]
    Group(#[from] GroupError),
    #[error("unable to find sync request with provided request_id")]
    ReplyRequestIdMissing,
    #[error("reply already processed")]
    ReplyAlreadyProcessed,
    #[error("no pending request to reply to")]
    NoPendingRequest,
    #[error("no reply to process")]
    NoReplyToProcess,
    #[error("generic: {0}")]
    Generic(String),
    #[error("invalid history message payload")]
    InvalidPayload,
    #[error("invalid history bundle url")]
    InvalidBundleUrl,
    #[error("unspecified device sync kind")]
    UnspecifiedDeviceSyncKind,
    #[error("sync reply is too old")]
    SyncPayloadTooOld,
    #[error(transparent)]
    Subscribe(#[from] SubscribeError),
    #[error(transparent)]
    Bincode(#[from] bincode::Error),
    #[error(transparent)]
    Backup(#[from] BackupError),
    #[error(transparent)]
    Decode(#[from] prost::DecodeError),
    #[error(transparent)]
    Deserialization(#[from] DeserializationError),
    #[error("Sync interaction is already acknowledged by another installation")]
    AlreadyAcknowledged,
    #[error("Sync request is missing options")]
    MissingOptions,
    #[error("Missing sync server url")]
    MissingSyncServerUrl,
    #[error("Missing sync group")]
    MissingSyncGroup,
}

impl DeviceSyncError {
    pub fn db_needs_connection(&self) -> bool {
        match self {
            Self::Client(s) => s.db_needs_connection(),
            _ => false,
        }
    }
}

impl RetryableError for DeviceSyncError {
    fn is_retryable(&self) -> bool {
        true
    }
}

impl From<NotFound> for DeviceSyncError {
    fn from(value: NotFound) -> Self {
        DeviceSyncError::Storage(StorageError::NotFound(value))
    }
}

impl<ApiClient, V> Client<ApiClient, V>
where
    ApiClient: XmtpApi + Send + Sync + 'static,
    V: SmartContractSignatureVerifier + Send + Sync + 'static,
{
    #[instrument(level = "trace", skip_all)]
    pub fn start_sync_worker(&self) {
        let client = self.clone();
        tracing::debug!(
            inbox_id = client.inbox_id(),
            installation_id = hex::encode(client.installation_public_key()),
            "starting sync worker"
        );

        let worker = SyncWorker::new(client);
        *self.device_sync.worker_handle.lock() = Some(worker.handle.clone());
        worker.spawn_worker();
    }
}

pub struct SyncWorker<ApiClient, V> {
    client: Client<ApiClient, V>,
    /// The sync events stream
    #[allow(clippy::type_complexity)]
    stream: Pin<Box<dyn Stream<Item = Result<LocalEvents, SubscribeError>> + Send>>,
    init: OnceCell<()>,
    retry: Retry,

    handle: Arc<WorkerHandle<SyncMetric>>,
}

impl<ApiClient, V> SyncWorker<ApiClient, V>
where
    ApiClient: XmtpApi + 'static,
    V: SmartContractSignatureVerifier + 'static,
{
    async fn run(&mut self) -> Result<(), DeviceSyncError> {
        // Wait for the identity to be ready & verified before doing anything
        while !self.client.identity().is_ready() {
            xmtp_common::yield_().await
        }
        self.sync_init().await?;
        self.handle.increment_metric(SyncMetric::Init);

        while let Some(event) = self.stream.next().await {
            let event = event?;
            match event {
                LocalEvents::SyncEvent(msg) => match msg {
                    SyncEvent::NewSyncGroupFromWelcome => {
                        // A new sync group from a welcome indicates a new installation.
                        // We need to add that installation to the groups.
                        let provider = self.client.mls_provider()?;
                        if self
                            .client
                            .acknowledge_new_sync_group(&provider, &self.retry)
                            .await
                            .is_err()
                        {
                            // We do not want to process the new installation if another installation is already processing it.
                            self.handle
                                .increment_metric(SyncMetric::SyncGroupWelcomesProcessed);
                            continue;
                        }
                        self.client.add_new_installation_to_groups().await?;
                        self.handle
                            .increment_metric(SyncMetric::SyncGroupWelcomesProcessed);

                        self.client
                            .send_sync_payload(
                                None,
                                || async {
                                    self.client
                                        .acknowledge_new_sync_group(&provider, &self.retry)
                                        .await
                                },
                                &self.handle,
                                &self.retry,
                            )
                            .await?;

                        // Send the HMAC as well
                        UserPreferenceUpdate::sync_hmac(&self.client, &self.handle, &self.retry)
                            .await?;
                    }
                    SyncEvent::NewSyncGroupMsg => {
                        let provider = self.client.mls_provider()?;
                        self.client
                            .process_new_sync_group_messages(&provider, &self.handle, &self.retry)
                            .await?;
                    }

                    SyncEvent::PreferencesOutgoing(preference_updates) => {
                        UserPreferenceUpdate::sync(
                            preference_updates,
                            &self.client,
                            &self.handle,
                            &self.retry,
                        )
                        .await?;
                    }

                    SyncEvent::PreferencesChanged(_) => {
                        // Intentionally left blank. This event is for streaming to consume.
                    }

                    // Device Sync V1 events
                    SyncEvent::Reply { message_id } => {
                        let provider = self.client.mls_provider()?;
                        if let Some(msg) = provider.conn_ref().get_group_message(&message_id)? {
                            let content: DeviceSyncContent =
                                serde_json::from_slice(&msg.decrypted_message_bytes)?;
                            if let DeviceSyncContent::Payload(reply) = content {
                                self.client
                                    .v1_process_sync_reply(&provider, reply, &self.handle)
                                    .await;
                            }
                        }
                    }
                    SyncEvent::Request { message_id } => {
                        let provider = self.client.mls_provider()?;
                        if let Some(msg) = provider.conn_ref().get_group_message(&message_id)? {
                            let content: DeviceSyncContent =
                                serde_json::from_slice(&msg.decrypted_message_bytes)?;
                            if let DeviceSyncContent::Request(request) = content {
                                self.client
                                    .v1_reply_to_sync_request(&provider, request, &self.handle)
                                    .await?;
                            }
                        }
                    }
                },
                _ => {}
            }
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
}

impl<ApiClient, V> SyncWorker<ApiClient, V>
where
    ApiClient: XmtpApi + Send + Sync + 'static,
    V: SmartContractSignatureVerifier + Send + Sync + 'static,
{
    fn new(client: Client<ApiClient, V>) -> Self {
        let strategy = ExponentialBackoff::builder()
            .duration(Duration::from_millis(20))
            .build();
        let retry = Retry::builder().retries(5).with_strategy(strategy).build();

        let receiver = client.local_events.subscribe();
        let stream = Box::pin(receiver.stream_sync_messages());

        Self {
            client,
            stream,
            init: OnceCell::new(),
            retry,
            handle: Arc::new(WorkerHandle::new()),
        }
    }

    fn spawn_worker(mut self) {
        xmtp_common::spawn(None, async move {
            let inbox_id = self.client.inbox_id().to_string();
            let installation_id = hex::encode(self.client.installation_public_key());
            // let span = info_span!("\x1b[35mDevice sync: ");
            // let _guard = span.enter();

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
        });
    }
}

impl<ApiClient, V> Client<ApiClient, V>
where
    ApiClient: XmtpApi,
    V: SmartContractSignatureVerifier,
{
    /// Returns number of new messages processed
    async fn process_new_sync_group_messages(
        &self,
        provider: &XmtpOpenMlsProvider,
        handle: &WorkerHandle<SyncMetric>,
        retry: &Retry,
    ) -> Result<usize, DeviceSyncError> {
        let sync_group = self.get_sync_group(provider)?;
        let mut cursor =
            StoredUserPreferences::sync_cursor(provider.conn_ref(), &sync_group.group_id)?;

        let messages = sync_group.sync_messages(cursor.cursor)?;
        let mut num_processed = 0;

        for (msg, content) in messages.iter_with_content() {
            match content {
                DeviceSyncContent::Request(request) => {
                    if msg.sender_installation_id == self.installation_id() {
                        // Ignore our own messages
                        continue;
                    }

                    self.send_sync_payload(
                        Some(request),
                        || async { self.acknowledge_sync_request(&provider, retry).await },
                        &handle,
                        retry,
                    )
                    .await?;
                }
                DeviceSyncContent::Payload(payload) => {
                    if msg.sender_installation_id == self.installation_id() {
                        // Ignore our own messages
                        continue;
                    }

                    self.process_sync_payload(payload).await?;
                    handle.increment_metric(SyncMetric::PayloadProcessed);
                }
                DeviceSyncContent::PreferenceUpdates(preference_updates) => {
                    // We'll process even our own messages here. The sync group message ordering takes authority over our own here.
                    for update in preference_updates {
                        update.store(provider, handle)?;
                    }
                }
                DeviceSyncContent::Acknowledge(_) => {
                    continue;
                }
            }

            // Move the cursor
            cursor.cursor += 1;
            StoredUserPreferences::store_sync_cursor(provider.conn_ref(), &cursor)?;
            num_processed += 1;
        }

        Ok(num_processed)
    }

    /// Blocks until the sync worker notifies that it is initialized and running.
    pub async fn wait_for_sync_worker_init(&self) {
        if let Some(handle) = self.device_sync.worker_handle() {
            let _ = handle.wait_for_init().await;
        }
    }

    /// Acknowledge the existence of a new sync group.
    /// Returns an error if sync group is already acknowledged by another installation.
    /// The first installation to acknowledge a sync group will the the installation to handle the sync.
    pub async fn acknowledge_new_sync_group(
        &self,
        provider: &XmtpOpenMlsProvider,
        retry: &Retry,
    ) -> Result<(), DeviceSyncError> {
        let sync_group = self.get_sync_group(provider)?;
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
                retry,
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
        retry: &Retry,
    ) -> Result<(), DeviceSyncError> {
        let sync_group = self.get_sync_group(provider)?;
        // Pull down any new messages
        sync_group.sync_with_conn(provider).await?;

        let messages = sync_group.find_messages(&MsgQueryArgs::default())?;

        let mut acknowledged = HashMap::new();
        // Look in reverse for a request, and ensure it was not acknowledged by someone else.
        for message in messages.iter().rev() {
            let Ok(content) =
                serde_json::from_slice::<DeviceSyncContent>(&message.decrypted_message_bytes)
            else {
                continue;
            };

            match content {
                DeviceSyncContent::Acknowledge(kind) => match kind {
                    AcknowledgeKind::Request { request_id } => {
                        acknowledged.insert(request_id, message.sender_installation_id.clone());
                    }
                    _ => {}
                },
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
                        retry,
                    )
                    .await?;

                    break;
                }
                _ => {}
            }
        }

        Ok(())
    }

    pub async fn send_sync_request(
        &self,
        provider: &XmtpOpenMlsProvider,
        retry: &Retry,
    ) -> Result<(), DeviceSyncError> {
        tracing::info!("Sending a sync request.");

        let sync_group = self.get_sync_group(provider)?;
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
        let content = DeviceSyncContent::Request(request);
        self.send_device_sync_message(provider, content, retry)
            .await?;

        Ok(())
    }

    pub(crate) async fn send_sync_payload<F, Fut>(
        &self,
        request: Option<DeviceSyncRequestProto>,
        acknowledge: F,
        handle: &WorkerHandle<SyncMetric>,
        retry: &Retry,
    ) -> Result<(), DeviceSyncError>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<(), DeviceSyncError>>,
    {
        tracing::info!("Sending sync payload.");
        let provider = Arc::new(self.mls_provider()?);

        match acknowledge().await {
            Err(DeviceSyncError::AlreadyAcknowledged) => {
                return Ok(());
            }
            result => result?,
        }

        let Some(device_sync_server_url) = &self.device_sync.server_url else {
            tracing::info!("Unable to send sync payload - no sync server url present.");
            return Err(DeviceSyncError::MissingSyncServerUrl);
        };

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
        let content = DeviceSyncContent::Payload(reply);
        self.send_device_sync_message(&provider, content, retry)
            .await?;

        handle.increment_metric(SyncMetric::PayloadSent);

        Ok(())
    }

    fn did_this_installation_ask_for_this_reply(
        &self,
        provider: &XmtpOpenMlsProvider,
        reply: &DeviceSyncReplyProto,
    ) -> Result<bool, DeviceSyncError> {
        let sync_group = self.get_sync_group(&provider)?;
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
        tracing::info!("Inspecting sync response.");
        let provider = Arc::new(self.mls_provider()?);

        // Check if this reply was asked for by this installation.
        if !self.did_this_installation_ask_for_this_reply(&provider, &reply)? {
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
        importer.run(&provider).await?;

        Ok(())
    }

    async fn send_device_sync_message(
        &self,
        provider: &XmtpOpenMlsProvider,
        content: DeviceSyncContent,
        retry: &Retry,
    ) -> Result<Vec<u8>, GroupError> {
        let sync_group = self.get_sync_group(provider)?;
        let content_bytes = serde_json::to_vec(&content).unwrap();
        let message_id =
            sync_group.prepare_message(&content_bytes, provider, |now| PlaintextEnvelope {
                content: Some(Content::V1(V1 {
                    content: content_bytes.clone(),
                    idempotency_key: now.to_string(),
                })),
            })?;

        retry_async!(
            retry,
            (async { sync_group.publish_intents(provider).await })
        )?;

        Ok(message_id)
    }

    #[instrument(level = "trace", skip_all)]
    pub fn get_sync_group(
        &self,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<MlsGroup<Self>, GroupError> {
        let conn = provider.conn_ref();
        let sync_group_id = conn
            .latest_sync_group()?
            .ok_or(NotFound::SyncGroup(self.installation_public_key()))?
            .id;
        let sync_group = self.group_with_conn(conn, &sync_group_id)?;

        Ok(sync_group)
    }

    #[instrument(level = "trace", skip_all)]
    async fn ensure_sync_group(
        &self,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<MlsGroup<Self>, GroupError> {
        let sync_group = match self.get_sync_group(provider) {
            Ok(group) => group,
            Err(_) => self.create_sync_group(provider).await?,
        };
        sync_group.sync_with_conn(provider).await?;

        Ok(sync_group)
    }

    /// This should be triggered when a new sync group appears,
    /// indicating the presence of a new installation.
    pub async fn add_new_installation_to_groups(&self) -> Result<(), DeviceSyncError> {
        let provider = self.mls_provider()?;
        let groups = self.find_groups(GroupQueryArgs::default())?;

        // Add the new installation to groups in batches
        for chunk in groups.chunks(20) {
            let mut add_futs = vec![];
            for group in chunk {
                add_futs.push(group.add_missing_installations(&provider));
            }
            let results = join_all(add_futs).await;
            for result in results {
                if let Err(err) = result {
                    tracing::warn!("Unable to add new installation to group. {err:?}");
                }
            }
        }

        Ok(())
    }
}

fn default_backup_options() -> BackupOptions {
    BackupOptions {
        elements: vec![
            BackupElementSelection::Messages as i32,
            BackupElementSelection::Consent as i32,
        ],
        ..Default::default()
    }
}

// These are the messages that get sent out to the sync group
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[repr(i32)]
pub enum DeviceSyncContent {
    Request(DeviceSyncRequestProto) = 0,
    Payload(DeviceSyncReplyProto) = 1,
    Acknowledge(AcknowledgeKind) = 2,
    PreferenceUpdates(Vec<UserPreferenceUpdate>) = 3,
}
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub enum AcknowledgeKind {
    SyncGroupPresence,
    Request { request_id: String },
}

pub trait ZipContent<A, B> {
    fn iter_with_content(self) -> impl Iterator<Item = (A, B)>;
}

impl ZipContent<StoredGroupMessage, DeviceSyncContent> for Vec<StoredGroupMessage> {
    fn iter_with_content(self) -> impl Iterator<Item = (StoredGroupMessage, DeviceSyncContent)> {
        self.into_iter().filter_map(|msg| {
            let content: DeviceSyncContent =
                serde_json::from_slice(&msg.decrypted_message_bytes).ok()?;
            Some((msg, content))
        })
    }
}
