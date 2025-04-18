use super::{GroupError, MlsGroup};
use crate::configuration::WORKER_RESTART_DELAY;
use crate::groups::disappearing_messages::DisappearingMessagesCleanerWorker;
use crate::groups::scoped_client::ScopedGroupClient;
use crate::{
    client::ClientError,
    configuration::NS_IN_HOUR,
    subscriptions::{LocalEvents, StreamMessages, SubscribeError, SyncMessage},
    Client,
};
use aes_gcm::aead::generic_array::GenericArray;
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm,
};
#[cfg(not(target_arch = "wasm32"))]
use backup::BackupError;
use futures::{Stream, StreamExt};
use handle::{SyncMetric, WorkerHandle};
use preference_sync::UserPreferenceUpdate;
use rand::{Rng, RngCore};
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::OnceCell;
use tracing::{info_span, instrument, warn, Instrument};
use xmtp_common::{retry_async, Retry, RetryableError};
use xmtp_common::{
    time::{now_ns, Duration},
    ExponentialBackoff,
};
use xmtp_cryptography::utils as crypto_utils;
use xmtp_db::{
    consent_record::StoredConsentRecord,
    group::{ConversationType, GroupQueryArgs, StoredGroup},
    group_message::{GroupMessageKind, MsgQueryArgs, StoredGroupMessage},
    xmtp_openmls_provider::XmtpOpenMlsProvider,
    DbConnection, NotFound, StorageError,
};
use xmtp_db::{Store, StoreOrIgnore};
use xmtp_id::{associations::DeserializationError, scw_verifier::SmartContractSignatureVerifier};
use xmtp_proto::api_client::trait_impls::XmtpApi;
use xmtp_proto::xmtp::mls::message_contents::device_sync_key_type::Key as EncKeyProto;
use xmtp_proto::xmtp::mls::message_contents::plaintext_envelope::Content;
use xmtp_proto::xmtp::mls::message_contents::{
    plaintext_envelope::v2::MessageType, plaintext_envelope::V2,
    DeviceSyncKeyType as DeviceSyncKeyTypeProto, DeviceSyncKind, PlaintextEnvelope,
};
use xmtp_proto::xmtp::mls::message_contents::{
    DeviceSyncReply as DeviceSyncReplyProto, DeviceSyncRequest as DeviceSyncRequestProto,
};
#[cfg(not(target_arch = "wasm32"))]
pub mod backup;
pub mod consent_sync;
pub mod handle;
pub mod message_sync;
pub mod preference_sync;

pub const ENC_KEY_SIZE: usize = 32; // 256-bit key
pub const NONCE_SIZE: usize = 12; // 96-bit nonce

#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(untagged)]
enum Syncable {
    Group(StoredGroup),
    GroupMessage(StoredGroupMessage),
    ConsentRecord(StoredConsentRecord),
}

#[derive(Debug, Error)]
pub enum DeviceSyncError {
    #[error("pin not found")]
    PinNotFound,
    #[error("pin does not match the expected value")]
    PinMismatch,
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
    #[error("missing history sync url")]
    MissingHistorySyncUrl,
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
        if !self.device_sync_worker_enabled() {
            return;
        }

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

    #[instrument(level = "trace", skip_all)]
    pub fn start_disappearing_messages_cleaner_worker(&self) {
        let client = self.clone();
        tracing::trace!(
            inbox_id = client.inbox_id(),
            installation_id = hex::encode(client.installation_public_key()),
            "starting expired messages cleaner worker"
        );

        let worker = DisappearingMessagesCleanerWorker::new(client);
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

    // Number of events processed
    handle: std::sync::Arc<WorkerHandle<SyncMetric>>,
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
                LocalEvents::SyncMessage(msg) => match msg {
                    SyncMessage::Reply { message_id } => {
                        let provider = self.client.mls_provider()?;
                        self.on_reply(message_id, &provider).await?;
                        self.handle.increment_metric(SyncMetric::V1PayloadProcessed);
                    }
                    SyncMessage::Request { message_id } => {
                        let provider = self.client.mls_provider()?;
                        self.on_request(message_id, &provider).await?;
                    }
                },
                LocalEvents::OutgoingPreferenceUpdates(preference_updates) => {
                    if preference_updates.is_empty() {
                        continue;
                    }

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
                    // Intentionally blank.
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

    async fn on_request(
        &mut self,
        message_id: Vec<u8>,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<(), DeviceSyncError> {
        let conn = provider.conn_ref();
        let Self {
            ref client, retry, ..
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

        if msg.sender_installation_id == self.client.installation_id() {
            return Ok(());
        }

        let msg_content: DeviceSyncContent = serde_json::from_slice(&msg.decrypted_message_bytes)?;
        let DeviceSyncContent::Request(request) = msg_content else {
            unreachable!();
        };

        client.reply_to_sync_request(provider, request).await?;

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
                client.device_sync.server_url
            );
            if client.get_sync_group(&provider).is_err() {
                client.ensure_sync_group(&provider).await?;

                client
                    .send_sync_request(&provider, DeviceSyncKind::Consent)
                    .await?;
                client
                    .send_sync_request(&provider, DeviceSyncKind::MessageHistory)
                    .await?;
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
            handle: std::sync::Arc::new(WorkerHandle::new()),
        }
    }

    fn spawn_worker(mut self) {
        let span = info_span!("\x1b[34mSYNC WORKER");

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
                        // Wait 2 seconds before restarting.
                        xmtp_common::time::sleep(WORKER_RESTART_DELAY).await;
                    }
                }
            }
            .instrument(span),
        );
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
        let sync_group = match self.get_sync_group(provider) {
            Ok(group) => group,
            Err(_) => {
                let sync_group =
                    MlsGroup::create_and_insert_sync_group(Arc::new(self.clone()), provider)?;
                tracing::info!("Creating sync group: {:?}", sync_group.group_id);
                sync_group.add_missing_installations(provider).await?;
                sync_group.sync_with_conn(provider).await?;

                sync_group
            }
        };

        Ok(sync_group)
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
        let sync_group = self.get_sync_group(provider)?;
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

        sync_group.prepare_message(&content_bytes, provider, {
            let request = request.clone();
            move |now| PlaintextEnvelope {
                content: Some(Content::V2(V2 {
                    message_type: Some(MessageType::DeviceSyncRequest(request)),
                    idempotency_key: now.to_string(),
                })),
            }
        })?;

        // publish the intent
        sync_group.sync_until_last_intent_resolved(provider).await?;

        if let Some(handle) = self.worker_handle() {
            handle.increment_metric(SyncMetric::V1RequestSent);
        }

        Ok(request)
    }

    pub(crate) async fn reply_to_sync_request(
        &self,
        provider: &XmtpOpenMlsProvider,
        request: DeviceSyncRequestProto,
    ) -> Result<Option<DeviceSyncReplyProto>, DeviceSyncError> {
        let conn = provider.conn_ref();

        let records = match request.kind() {
            DeviceSyncKind::Consent => vec![self.syncable_consent_records(conn)?],
            DeviceSyncKind::MessageHistory => {
                vec![self.syncable_groups(conn)?, self.syncable_messages(conn)?]
            }
            DeviceSyncKind::Unspecified => return Err(DeviceSyncError::UnspecifiedDeviceSyncKind),
        };

        let reply = self
            .create_sync_reply(&request.request_id, &records, request.kind())
            .await?;

        if let Some(reply) = reply.clone() {
            self.send_sync_reply(provider, reply).await?;
        }

        Ok(reply)
    }

    async fn send_sync_reply(
        &self,
        provider: &XmtpOpenMlsProvider,
        contents: DeviceSyncReplyProto,
    ) -> Result<(), DeviceSyncError> {
        // find the sync group
        let sync_group = self.get_sync_group(provider)?;

        // sync the group
        sync_group.sync_with_conn(provider).await?;

        let (msg, _request) = self
            .get_pending_sync_request(provider, contents.kind())
            .await?;

        // add original sender to all groups on this device on the node
        self.ensure_member_of_all_groups(provider, &msg.sender_inbox_id)
            .await?;

        // the reply message
        let (content_bytes, contents) = {
            let content = DeviceSyncContent::Reply(contents);
            let content_bytes = serde_json::to_vec(&content)?;
            let DeviceSyncContent::Reply(contents) = content else {
                unreachable!("This is a reply.");
            };

            (content_bytes, contents)
        };

        sync_group.prepare_message(&content_bytes, provider, |now| PlaintextEnvelope {
            content: Some(Content::V2(V2 {
                message_type: Some(MessageType::DeviceSyncReply(contents)),
                idempotency_key: now.to_string(),
            })),
        })?;

        sync_group.sync_until_last_intent_resolved(provider).await?;

        if let Some(handle) = self.worker_handle() {
            handle.increment_metric(SyncMetric::V1PayloadSent);
        }

        tracing::info!(
            "Backup payload sent to sync group {:?}",
            sync_group.group_id
        );

        Ok(())
    }

    async fn get_pending_sync_request(
        &self,
        provider: &XmtpOpenMlsProvider,
        kind: DeviceSyncKind,
    ) -> Result<(StoredGroupMessage, DeviceSyncRequestProto), DeviceSyncError> {
        let sync_group = self.get_sync_group(provider)?;
        sync_group.sync_with_conn(provider).await?;

        let messages = provider.conn_ref().get_group_messages(
            &sync_group.group_id,
            &MsgQueryArgs {
                kind: Some(GroupMessageKind::Application),
                ..Default::default()
            },
        )?;

        for msg in messages.into_iter().rev() {
            let Ok(msg_content) =
                serde_json::from_slice::<DeviceSyncContent>(&msg.decrypted_message_bytes)
            else {
                continue;
            };

            match msg_content {
                DeviceSyncContent::Reply(reply) if reply.kind() == kind => {
                    return Err(DeviceSyncError::NoPendingRequest);
                }
                DeviceSyncContent::Request(request) if request.kind() == kind => {
                    return Ok((msg, request));
                }
                _ => {}
            }
        }

        Err(DeviceSyncError::NoPendingRequest)
    }

    #[cfg(test)]
    async fn get_latest_sync_reply(
        &self,
        provider: &XmtpOpenMlsProvider,
        kind: DeviceSyncKind,
    ) -> Result<Option<(StoredGroupMessage, DeviceSyncReplyProto)>, DeviceSyncError> {
        let sync_group = self.get_sync_group(provider)?;
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
        provider: &XmtpOpenMlsProvider,
        reply: DeviceSyncReplyProto,
    ) -> Result<(), DeviceSyncError> {
        let conn = provider.conn_ref();

        let time_diff = reply.timestamp_ns.abs_diff(now_ns() as u64);
        if time_diff > NS_IN_HOUR as u64 {
            // time discrepancy is too much
            return Err(DeviceSyncError::SyncPayloadTooOld);
        }

        let Some(enc_key) = reply.encryption_key.clone() else {
            return Err(DeviceSyncError::InvalidPayload);
        };

        let enc_payload = download_history_payload(&reply.url).await?;
        self.insert_encrypted_syncables(provider, enc_payload, &enc_key.try_into()?)
            .await?;

        self.sync_welcomes(provider).await?;

        let groups =
            conn.find_groups(GroupQueryArgs::default().conversation_type(ConversationType::Group))?;
        for xmtp_db::group::StoredGroup { id, .. } in groups.into_iter() {
            let group = self.group_with_conn(provider.conn_ref(), &id)?;
            group.maybe_update_installations(provider, None).await?;
            Box::pin(group.sync_with_conn(provider)).await?;
        }

        Ok(())
    }

    async fn ensure_member_of_all_groups(
        &self,
        provider: &XmtpOpenMlsProvider,
        inbox_id: &str,
    ) -> Result<(), GroupError> {
        let conn = provider.conn_ref();
        let groups =
            conn.find_groups(GroupQueryArgs::default().conversation_type(ConversationType::Group))?;
        for group in groups {
            let group = self.group_with_conn(conn, &group.id)?;
            Box::pin(
                group.add_members_by_inbox_id_with_provider(provider, &[inbox_id.to_string()]),
            )
            .await?;
        }

        Ok(())
    }

    async fn create_sync_reply(
        &self,
        request_id: &str,
        syncables: &[Vec<Syncable>],
        kind: DeviceSyncKind,
    ) -> Result<Option<DeviceSyncReplyProto>, DeviceSyncError> {
        let (payload, enc_key) = encrypt_syncables(syncables)?;

        // upload the payload
        let Some(url) = &self.device_sync.server_url else {
            return Ok(None);
        };
        let upload_url = format!("{url}/upload");
        tracing::info!(
            inbox_id = self.inbox_id(),
            installation_id = hex::encode(self.installation_public_key()),
            "Using upload url {upload_url}",
        );

        let response = reqwest::Client::new()
            .post(upload_url)
            .body(payload)
            .send()
            .await?;

        if !response.status().is_success() {
            tracing::error!(
                inbox_id = self.inbox_id(),
                installation_id = hex::encode(self.installation_public_key()),
                "Failed to upload file. Status code: {} Response: {response:?}",
                response.status()
            );
            response.error_for_status()?;
            // checked for error, the above line bubbled up
            unreachable!();
        }

        let url = format!("{url}/files/{}", response.text().await?);

        let sync_reply = DeviceSyncReplyProto {
            encryption_key: Some(enc_key.into()),
            request_id: request_id.to_string(),
            url,
            timestamp_ns: now_ns() as u64,
            kind: kind as i32,
        };

        Ok(Some(sync_reply))
    }

    async fn insert_encrypted_syncables(
        &self,
        provider: &XmtpOpenMlsProvider,
        payload: Vec<u8>,
        enc_key: &DeviceSyncKeyType,
    ) -> Result<(), DeviceSyncError> {
        let conn = provider.conn_ref();
        let enc_key = enc_key.as_bytes();

        // Split the nonce and ciphertext
        let (nonce, ciphertext) = payload.split_at(NONCE_SIZE);

        // Create a cipher instance
        let cipher = Aes256Gcm::new(GenericArray::from_slice(enc_key));
        let nonce_array = GenericArray::from_slice(nonce);

        // Decrypt the ciphertext
        let payload = cipher.decrypt(nonce_array, ciphertext)?;
        let payload: Vec<Syncable> = serde_json::from_slice(&payload)?;

        for syncable in payload {
            match syncable {
                Syncable::Group(group) => {
                    conn.insert_or_replace_group(group)?;
                }
                Syncable::GroupMessage(group_message) => {
                    group_message.store_or_ignore(conn)?;
                }
                Syncable::ConsentRecord(consent_record) => {
                    if let Some(existing_consent_record) =
                        conn.maybe_insert_consent_record_return_existing(&consent_record)?
                    {
                        if existing_consent_record.state != consent_record.state {
                            warn!("Existing consent record exists and does not match payload state. Streaming consent_record update to sync group.");
                            self.local_events
                                .send(LocalEvents::OutgoingPreferenceUpdates(vec![
                                    UserPreferenceUpdate::ConsentUpdate(existing_consent_record),
                                ]))
                                .map_err(|e| DeviceSyncError::Generic(e.to_string()))?;
                        }
                    }
                }
            };
        }

        Ok(())
    }

    #[instrument(level = "trace", skip_all)]
    pub fn get_sync_group(
        &self,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<MlsGroup<Self>, GroupError> {
        let sync_group_id = provider
            .conn_ref()
            .latest_sync_group()?
            .ok_or(NotFound::SyncGroup(self.installation_public_key()))?
            .id;
        let sync_group = self.group_with_conn(provider.conn_ref(), &sync_group_id)?;

        Ok(sync_group)
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub enum DeviceSyncContent {
    Request(DeviceSyncRequestProto),
    Reply(DeviceSyncReplyProto),
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

    if !response.status().is_success() {
        tracing::error!(
            "Failed to download file. Status code: {} Response: {:?}",
            response.status(),
            response
        );
        response.error_for_status()?;
        unreachable!("Checked for error");
    }

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

pub(super) fn new_request_id() -> String {
    xmtp_common::rand_string::<ENC_KEY_SIZE>()
}

pub(super) fn generate_nonce() -> [u8; NONCE_SIZE] {
    xmtp_common::rand_array::<NONCE_SIZE>()
}

pub(super) fn new_pin() -> String {
    let mut rng = crypto_utils::rng();
    let pin: u32 = rng.gen_range(0..10000);
    format!("{:04}", pin)
}

fn encrypt_syncables(
    syncables: &[Vec<Syncable>],
) -> Result<(Vec<u8>, DeviceSyncKeyType), DeviceSyncError> {
    let enc_key = DeviceSyncKeyType::new_aes_256_gcm_key();
    encrypt_syncables_with_key(syncables, enc_key)
}

fn encrypt_syncables_with_key(
    syncables: &[Vec<Syncable>],
    enc_key: DeviceSyncKeyType,
) -> Result<(Vec<u8>, DeviceSyncKeyType), DeviceSyncError> {
    let syncables: Vec<&Syncable> = syncables.iter().flat_map(|s| s.iter()).collect();
    let payload = serde_json::to_vec(&syncables)?;

    let enc_key_bytes = enc_key.as_bytes();
    let mut result = generate_nonce().to_vec();

    // create a cipher instance
    let cipher = Aes256Gcm::new(GenericArray::from_slice(enc_key_bytes));
    let nonce_array = GenericArray::from_slice(&result);

    // encrypt the payload and append to the result
    result.append(&mut cipher.encrypt(nonce_array, &*payload)?);

    Ok((result, enc_key))
}
