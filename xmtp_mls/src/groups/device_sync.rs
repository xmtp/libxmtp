use super::{scoped_client::ScopedGroupClient, GroupError, MlsGroup};
#[cfg(any(test, feature = "test-utils"))]
use crate::{
    client::ClientError,
    configuration::NS_IN_HOUR,
    storage::{
        group::{ConversationType, GroupQueryArgs},
        group_message::{GroupMessageKind, MsgQueryArgs, StoredGroupMessage},
        xmtp_openmls_provider::XmtpOpenMlsProvider,
        DbConnection, NotFound, StorageError,
    },
    subscriptions::{LocalEvents, StreamMessages, SubscribeError},
    Client, Store,
};
use crate::{configuration::WORKER_RESTART_DELAY, subscriptions::SyncEvent};
use aes_gcm::aead::generic_array::GenericArray;
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm,
};
use backup::BackupImporter;
#[cfg(not(target_arch = "wasm32"))]
use backup::{backup_exporter::BackupExporter, BackupError};
use futures::{future::join_all, Stream, StreamExt};
use futures_util::StreamExt;
use handle::{SyncWorkerMetric, WorkerHandle};
use preference_sync::UserPreferenceUpdate;
use rand::{Rng, RngCore};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, pin::Pin, sync::Arc};
use thiserror::Error;
use tokio::sync::OnceCell;
use tokio_util::compat::{FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};
use tokio_util::io::{ReaderStream, StreamReader};
use tracing::{instrument, warn};
use xmtp_common::{retry_async, Retry, RetryableError};
use xmtp_common::{
    time::{now_ns, Duration},
    ExponentialBackoff,
};
use xmtp_cryptography::utils as crypto_utils;
use xmtp_id::{associations::DeserializationError, scw_verifier::SmartContractSignatureVerifier};
use xmtp_proto::api_client::trait_impls::XmtpApi;
use xmtp_proto::xmtp::mls::message_contents::device_sync_key_type::Key as EncKeyProto;
use xmtp_proto::xmtp::mls::message_contents::plaintext_envelope::Content;
use xmtp_proto::xmtp::mls::message_contents::{
    plaintext_envelope::v2::MessageType,
    plaintext_envelope::{V1, V2},
    DeviceSyncKeyType as DeviceSyncKeyTypeProto, DeviceSyncKind, PlaintextEnvelope,
};
use xmtp_proto::xmtp::mls::message_contents::{
    DeviceSyncReply as DeviceSyncReplyProto, DeviceSyncRequest as DeviceSyncRequestProto,
};

pub mod backup;
pub mod consent_sync;
pub mod handle;
pub mod message_sync;
pub mod preference_sync;

pub const ENC_KEY_SIZE: usize = 32; // 256-bit key
pub const NONCE_SIZE: usize = 12; // 96-bit nonce

#[derive(Debug, Error)]
pub enum DeviceSyncError {
    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),
    #[error("Serialization/Deserialization Error {0}")]
    Serde(#[from] serde_json::Error),
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
    #[cfg(not(target_arch = "wasm32"))]
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
}

const LOG_PREFIX: &str = "Device Sync: ";

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

    /// This should be triggered when a new sync group appears,
    /// indicating the presence of a new installation.
    pub async fn add_new_installation_to_groups(&self) -> Result<(), DeviceSyncError> {
        let provider = self.mls_provider()?;
        let groups = self.find_groups(GroupQueryArgs::default())?;

        for chunk in groups.chunks(20) {
            let mut add_futs = vec![];
            for group in chunk {
                add_futs.push(group.add_missing_installations(&provider));
            }
            let results = join_all(add_futs).await;
            for result in results {
                if let Err(err) = result {
                    tracing::warn!("{LOG_PREFIX}Unable to add new installation to group. {err:?}");
                }
            }
        }

        Ok(())
    }
}

pub struct SyncWorker<ApiClient, V> {
    client: Client<ApiClient, V>,
    /// The sync events stream
    #[allow(clippy::type_complexity)]
    stream: Pin<Box<dyn Stream<Item = Result<LocalEvents, SubscribeError>> + Send>>,
    init: OnceCell<()>,
    retry: Retry,

    handle: Arc<WorkerHandle<SyncWorkerMetric>>,
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
                            .acknowledge_new_sync_group(&provider)
                            .await
                            .is_err()
                        {
                            // We do not want to process the new installation if another installation is already processing it.
                            self.handle
                                .increment_metric(SyncWorkerMetric::SyncGroupWelcomesProcessed);
                            continue;
                        }
                        self.client.add_new_installation_to_groups().await?;
                        self.handle
                            .increment_metric(SyncWorkerMetric::SyncGroupWelcomesProcessed);
                    }
                    SyncEvent::NewSyncGroupMsg(msg_id) => {
                        let provider = self.client.mls_provider()?;
                        let conn = provider.conn_ref();

                        let Some(msg) = conn.get_group_message(&msg_id)? else {
                            tracing::error!("{LOG_PREFIX}Worker was notified of a new sync group message, but none was found.");
                            continue;
                        };
                        let Ok(content) = serde_json::from_slice::<DeviceSyncContent>(
                            &msg.decrypted_message_bytes,
                        ) else {
                            // Ignore messages that don't deserialize
                            continue;
                        };

                        match content {
                            DeviceSyncContent::Request(request) => {
                                self.client.process_sync_request(request).await?;
                            }
                            DeviceSyncContent::Reply(reply) => {
                                self.client.process_sync_reply(reply).await?;
                            }
                            DeviceSyncContent::Acknowledge(_) => {
                                // intentionally left blank
                            }
                        }
                    }
                },
                LocalEvents::OutgoingPreferenceUpdates(preference_updates) => {
                    tracing::info!("Outgoing preference update {preference_updates:?}");
                    retry_async!(
                        self.retry,
                        (async {
                            UserPreferenceUpdate::sync_across_devices(
                                preference_updates.clone(),
                                &self.client,
                            )
                            .await
                        })
                    )?;
                }
                LocalEvents::IncomingPreferenceUpdate(_) => {
                    tracing::info!("{LOG_PREFIX}Incoming preference update");
                }
                _ => {}
            }
        }
        Ok(())
    }

    async fn on_reply(
        &mut self,
        message_id: Vec<u8>,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<(), DeviceSyncError> {
        let conn = provider.conn_ref();
        let Self {
            ref client,
            ref retry,
            ..
        } = self;

        let msg = retry_async!(
            retry,
            (async {
                conn.get_group_message(&message_id)?
                    .ok_or(DeviceSyncError::from(NotFound::MessageById(
                        message_id.clone(),
                    )))
            })
        )?;

        let msg_content: DeviceSyncContent = serde_json::from_slice(&msg.decrypted_message_bytes)?;
        let DeviceSyncContent::Reply(reply) = msg_content else {
            unreachable!();
        };

        client.process_sync_reply(provider, reply).await?;
        Ok(())
    }

    //// Ideally called when the client is registered.
    //// Will auto-send a sync request if sync group is created.
    #[instrument(level = "trace", skip_all)]
    pub async fn sync_init(&mut self) -> Result<(), DeviceSyncError> {
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
                client.device_sync.history_sync_url
            );
            if client.get_sync_group(provider.conn_ref()).is_err() {
                // The only thing that sync init really does right now is ensures that there's a sync group.
                client.ensure_sync_group(&provider).await?;
            }
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
                    // Wait 2 seconds before restarting.
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
    #[instrument(level = "trace", skip_all)]
    async fn ensure_sync_group(
        &self,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<MlsGroup<Self>, GroupError> {
        let sync_group = match self.get_sync_group(provider.conn_ref()) {
            Ok(group) => group,
            Err(_) => self.create_sync_group(provider).await?,
        };
        sync_group.sync_with_conn(provider).await?;

        Ok(sync_group)
    }

    async fn send_device_sync_message(
        &self,
        provider: &XmtpOpenMlsProvider,
        content: DeviceSyncContent,
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

        sync_group.publish_intents(provider).await?;

        Ok(message_id)
    }

    /// Acknowledge the existence of a new sync group.
    /// Returns an error if sync group is already acknowledged by another installation.
    /// The first installation to acknowledge a sync group will the the installation to handle the sync.
    pub async fn acknowledge_new_sync_group(
        &self,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<(), DeviceSyncError> {
        let sync_group = self.get_sync_group(provider)?;
        // Pull down any new messages
        sync_group.sync_with_conn(provider).await?;

        let messages = sync_group.find_messages(&MsgQueryArgs::default())?;

        let acknowledgement = messages
            .into_iter()
            .find(|m| m.decrypted_message_bytes.is_empty());
        let Some(acknowledgement) = acknowledgement else {
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
                    )
                    .await?;

                    break;
                }
                _ => {}
            }
        }

        Ok(())
    }

    #[instrument(level = "trace", skip_all)]
    pub async fn send_sync_request(
        &self,
        provider: &XmtpOpenMlsProvider,
        kind: DeviceSyncKind,
    ) -> Result<DeviceSyncRequestProto, DeviceSyncError> {
        tracing::info!(
            inbox_id = self.inbox_id(),
            installation_id = hex::encode(self.installation_public_key()),
            "Sending a sync request for {kind:?}"
        );
        let request = DeviceSyncRequest::new(kind);

        // find the sync group
        let sync_group = self.get_sync_group(provider.conn_ref())?;

        // sync the group
        sync_group.sync_with_conn(provider).await?;

        // lookup if a request has already been made
        if let Ok((_msg, request)) = self.get_pending_sync_request(provider, request.kind).await {
            return Ok(request);
        }

        // build the request
        let request: DeviceSyncRequestProto = request.into();

        let content = DeviceSyncContent::Request(request.clone());
        let content_bytes = serde_json::to_vec(&content)?;

        let _message_id = sync_group.prepare_message(&content_bytes, provider, {
            let request = request.clone();
            move |now| PlaintextEnvelope {
                content: Some(Content::V2(V2 {
                    message_type: Some(MessageType::DeviceSyncRequest(request)),
                    idempotency_key: now.to_string(),
                })),
            }
        })?;

        // publish the intent
        sync_group.publish_intents(provider).await?;

        Ok(request)
    }

    pub(crate) async fn process_sync_request(
        &self,
        request: DeviceSyncRequestProto,
    ) -> Result<(), DeviceSyncError> {
        tracing::info!("{LOG_PREFIX}Responding to sync request.");
        let provider = Arc::new(self.mls_provider()?);

        if let Err(err) = self.acknowledge_sync_request(&provider).await {
            // Absorb the error and log it as an info, since it's not a real error.
            // This just means that another installation is handling it.
            tracing::info!("{LOG_PREFIX}{err}");
            return Ok(());
        };

        let Some(history_sync_url) = &self.device_sync.history_sync_url else {
            tracing::info!(
                "{LOG_PREFIX}Unable to process sync request due to not having a sync server url present."
            );
            return Err(DeviceSyncError::MissingSyncServerUrl);
        };
        let Some(options) = request.options else {
            return Err(DeviceSyncError::MissingOptions);
        };

        // Generate a random encryption key
        let key = xmtp_common::rand_vec::<32>();

        // Now we want to create an encrypted stream from our database to the history server.
        //
        // 1. Build the exporter
        let exporter = BackupExporter::new(options, &provider, &key);
        let metadata = exporter.metadata().clone();
        // 2. A compat layer to have futures AsyncRead play nice with tokio's AsyncRead
        let exporter_compat = tokio_util::compat::FuturesAsyncReadCompatExt::compat(exporter);
        // 3. Add a stream layer over the async read
        let stream = ReaderStream::new(exporter_compat);
        // 4. Pipe that stream as the body to the request to the history server
        let body = reqwest::Body::wrap_stream(stream);
        // 5. Make the request
        let url = format!("{history_sync_url}/upload");
        tracing::info!("{LOG_PREFIX}Uploading sync payload to history server...");
        let response = reqwest::Client::new().post(url).body(body).send().await?;
        tracing::info!("{LOG_PREFIX}Done uploading sync payload to history server.");

        if let Err(err) = response.error_for_status_ref() {
            tracing::error!(
                inbox_id = self.inbox_id(),
                installation_id = hex::encode(self.installation_public_key()),
                "{LOG_PREFIX}Failed to upload file. Status code: {:?}",
                err.status()
            );
            return Err(DeviceSyncError::Reqwest(err));
        }

        // Build a sync reply message that the new installation will consume
        let reply = DeviceSyncReplyProto {
            key,
            request_id: request.request_id,
            url: format!("{history_sync_url}/files/{}", response.text().await?),
            metadata: Some(metadata),
            ..Default::default()
        };

        // Send the message out over the network
        let content = DeviceSyncContent::Reply(reply);
        self.send_device_sync_message(&provider, content).await?;

        Ok(())
    }

    #[cfg(test)]
    async fn get_latest_sync_reply(
        &self,
        provider: &XmtpOpenMlsProvider,
        kind: DeviceSyncKind,
    ) -> Result<Option<(StoredGroupMessage, DeviceSyncReplyProto)>, DeviceSyncError> {
        let sync_group = self.get_sync_group(provider.conn_ref())?;
        sync_group.sync_with_conn(provider).await?;

        let messages = sync_group.find_messages(&MsgQueryArgs {
            kind: Some(GroupMessageKind::Application),
            ..Default::default()
        })?;

        for msg in messages.into_iter().rev() {
            let Ok(msg_content) =
                serde_json::from_slice::<DeviceSyncContent>(&msg.decrypted_message_bytes)
            else {
                continue;
            };
            match msg_content {
                DeviceSyncContent::Reply(reply) if reply.kind() == kind => {
                    return Ok(Some((msg, reply)));
                }
                _ => {}
            }
        }

        Ok(None)
    }

    pub async fn process_sync_reply(
        &self,
        reply: DeviceSyncReplyProto,
    ) -> Result<(), DeviceSyncError> {
        tracing::info!("{LOG_PREFIX}Inspecting sync response.");
        let provider = Arc::new(self.mls_provider()?);

        // First let's check if this installation asked for this sync payload.
        let sync_group = self.get_sync_group(&provider)?;
        let messages = sync_group.find_messages(&MsgQueryArgs::default())?;

        // Find the request corresponding to this reply.
        let Some((msg, DeviceSyncContent::Request(_))) = messages.iter_with_content().find(|(_msg, content)| matches!(content, DeviceSyncContent::Request(DeviceSyncRequestProto { request_id, .. }) if *request_id == reply.request_id)) else {
            tracing::info!("{LOG_PREFIX}Unable to find a sync request for the provided sync response.");
            return Ok(());
        };

        if msg.sender_installation_id != self.installation_id() {
            // We didn't ask for this sync reply.
            return Ok(());
        }

        let response = reqwest::Client::new().get(reply.url).send().await?;
        if let Err(err) = response.error_for_status_ref() {
            tracing::error!(
                "Failed to download file. Status code: {} Response: {:?}",
                response.status(),
                response
            );
            return Err(DeviceSyncError::Reqwest(err));
        }

        let stream = response
            .bytes_stream()
            .map(|result| result.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)));
        let tokio_reader = StreamReader::new(stream);
        let futures_reader = tokio_reader.compat();
        let reader = Box::pin(futures_reader);
        let mut importer = BackupImporter::load(reader, &reply.key).await?;

        importer.run(&provider).await?;

        Ok(())
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
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub enum DeviceSyncContent {
    Request(DeviceSyncRequestProto),
    Reply(DeviceSyncReplyProto),
    Acknowledge(AcknowledgeKind),
}
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub enum AcknowledgeKind {
    SyncGroupPresence,
    Request { request_id: String },
}

pub struct MessageHistoryUrls;

impl MessageHistoryUrls {
    pub const LOCAL_ADDRESS: &'static str = "http://0.0.0.0:5558";
    pub const DEV_ADDRESS: &'static str = "https://message-history.dev.ephemera.network/";
    pub const PRODUCTION_ADDRESS: &'static str = "https://message-history.ephemera.network/";
}

pub(crate) async fn download_history_payload(url: &str) -> Result<Vec<u8>, DeviceSyncError> {
    tracing::info!("downloading history bundle from {:?}", url);
    let response = reqwest::Client::new().get(url).send().await?;

    if let Err(err) = response.error_for_status() {}

    Ok(response.bytes().await?.to_vec())
}

#[derive(Clone, Debug)]
pub(super) struct DeviceSyncRequest {
    pub pin_code: String,
    pub request_id: String,
    pub kind: DeviceSyncKind,
}

impl DeviceSyncRequest {
    pub(crate) fn new(kind: DeviceSyncKind) -> Self {
        Self {
            pin_code: new_pin(),
            request_id: new_request_id(),
            kind,
        }
    }
}

impl From<DeviceSyncRequest> for DeviceSyncRequestProto {
    fn from(req: DeviceSyncRequest) -> Self {
        DeviceSyncRequestProto {
            pin_code: req.pin_code,
            request_id: req.request_id,
            kind: req.kind as i32,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DeviceSyncReply {
    /// Unique ID for each client Message History Request
    request_id: String,
    /// URL to download the backup bundle
    url: String,
    /// Encryption key for the backup bundle
    encryption_key: DeviceSyncKeyType,
    /// UNIX timestamp of when the reply was sent in ns
    timestamp_ns: u64,
    // sync kind
    kind: DeviceSyncKind,
}

impl From<DeviceSyncReply> for DeviceSyncReplyProto {
    fn from(reply: DeviceSyncReply) -> Self {
        DeviceSyncReplyProto {
            request_id: reply.request_id,
            url: reply.url,
            encryption_key: Some(reply.encryption_key.into()),
            timestamp_ns: reply.timestamp_ns,
            kind: reply.kind as i32,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) enum DeviceSyncKeyType {
    Aes256Gcm([u8; ENC_KEY_SIZE]),
}

impl DeviceSyncKeyType {
    fn new_aes_256_gcm_key() -> Self {
        let mut rng = crypto_utils::rng();
        let mut key = [0u8; ENC_KEY_SIZE];
        rng.fill_bytes(&mut key);
        DeviceSyncKeyType::Aes256Gcm(key)
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        match self {
            DeviceSyncKeyType::Aes256Gcm(key) => key.len(),
        }
    }

    fn as_bytes(&self) -> &[u8; ENC_KEY_SIZE] {
        match self {
            DeviceSyncKeyType::Aes256Gcm(key) => key,
        }
    }
}

impl From<DeviceSyncKeyType> for DeviceSyncKeyTypeProto {
    fn from(key: DeviceSyncKeyType) -> Self {
        match key {
            DeviceSyncKeyType::Aes256Gcm(key) => DeviceSyncKeyTypeProto {
                key: Some(EncKeyProto::Aes256Gcm(key.to_vec())),
            },
        }
    }
}

impl TryFrom<DeviceSyncKeyTypeProto> for DeviceSyncKeyType {
    type Error = DeviceSyncError;
    fn try_from(key: DeviceSyncKeyTypeProto) -> Result<Self, Self::Error> {
        let DeviceSyncKeyTypeProto { key } = key;
        match key {
            Some(k) => {
                let EncKeyProto::Aes256Gcm(key) = k;
                match key.try_into() {
                    Ok(array) => Ok(DeviceSyncKeyType::Aes256Gcm(array)),
                    Err(_) => Err(DeviceSyncError::Conversion),
                }
            }
            None => Err(DeviceSyncError::Conversion),
        }
    }
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
