use super::{GroupError, MlsGroup};
use crate::{client::ClientError, subscriptions::SubscribeError, Client};
use backup::BackupError;
use futures::future::join_all;
use handle::{SyncMetric, WorkerHandle};
use preference_sync::UserPreferenceUpdate;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use tracing::instrument;
use worker::SyncWorker;
use xmtp_common::RetryableError;
use xmtp_db::user_preferences::SyncCursor;
use xmtp_db::Store;
use xmtp_db::{
    group::GroupQueryArgs, group_message::StoredGroupMessage,
    xmtp_openmls_provider::XmtpOpenMlsProvider, NotFound, StorageError,
};
use xmtp_id::{associations::DeserializationError, scw_verifier::SmartContractSignatureVerifier};
use xmtp_proto::{
    api_client::trait_impls::XmtpApi,
    xmtp::{
        device_sync::{BackupElementSelection, BackupOptions},
        mls::message_contents::{
            plaintext_envelope::v2::MessageType,
            plaintext_envelope::{Content, V1, V2},
            DeviceSyncReply as DeviceSyncReplyProto, DeviceSyncRequest as DeviceSyncRequestProto,
            PlaintextEnvelope,
        },
    },
};

pub mod backup;
pub mod handle;
pub mod preference_sync;
pub mod worker;

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
        if !self.device_sync_worker_enabled() {
            tracing::info!("Sync worker is disabled.");
            return;
        }

        let client = self.clone();
        tracing::debug!(
            inbox_id = client.inbox_id(),
            installation_id = hex::encode(client.installation_public_key()),
            "starting sync worker"
        );

        let worker = SyncWorker::new(client);
        *self.device_sync.worker_handle.lock() = Some(worker.handle().clone());
        worker.spawn_worker();
    }
}

impl<ApiClient, V> Client<ApiClient, V>
where
    ApiClient: XmtpApi,
    V: SmartContractSignatureVerifier,
{
    /// Blocks until the sync worker notifies that it is initialized and running.
    pub async fn wait_for_sync_worker_init(&self) {
        if let Some(handle) = self.worker_handle() {
            let _ = handle.wait_for_init().await;
        }
    }

    async fn send_device_sync_message(
        &self,
        provider: &XmtpOpenMlsProvider,
        content: DeviceSyncContent,
    ) -> Result<Vec<u8>, ClientError> {
        let sync_group = self.get_sync_group(provider).await?;
        tracing::info!(
            "Sending sync message to group {:?}: {content:?}",
            &sync_group.group_id[..4]
        );

        let content_bytes =
            serde_json::to_vec(&content).map_err(|err| ClientError::Generic(err.to_string()))?;
        let message_id =
            sync_group.prepare_message(&content_bytes, provider, |now| PlaintextEnvelope {
                content: Some(Content::V1(V1 {
                    content: content_bytes.clone(),
                    idempotency_key: now.to_string(),
                })),
            })?;

        sync_group.sync_until_last_intent_resolved(provider).await?;

        Ok(message_id)
    }

    #[instrument(level = "trace", skip_all)]
    pub async fn get_sync_group(
        &self,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<MlsGroup<Self>, GroupError> {
        let conn = provider.conn_ref();
        let sync_group = match conn.latest_sync_group()? {
            Some(sync_group) => self.group_with_conn(conn, &sync_group.id)?,
            None => {
                let sync_group =
                    MlsGroup::create_and_insert_sync_group(Arc::new(self.clone()), provider)?;
                tracing::info!("Creating sync group: {:?}", sync_group.group_id);
                SyncCursor::reset(&sync_group.group_id, provider.conn_ref())?;

                sync_group.add_missing_installations(provider).await?;
                sync_group.sync_with_conn(provider).await?;

                if let Some(handle) = self.worker_handle() {
                    handle.increment_metric(SyncMetric::SyncGroupCreated);
                }
                sync_group
            }
        };

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
pub enum DeviceSyncContent {
    Request(DeviceSyncRequestProto),
    Payload(DeviceSyncReplyProto),
    Acknowledge(AcknowledgeKind),
    PreferenceUpdates(Vec<UserPreferenceUpdate>),
}
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub enum AcknowledgeKind {
    SyncGroupPresence,
    Request { request_id: String },
}

pub trait IterWithContent<A, B> {
    fn iter_with_content(self) -> impl Iterator<Item = (A, B)>;
}

impl IterWithContent<StoredGroupMessage, DeviceSyncContent> for Vec<StoredGroupMessage> {
    fn iter_with_content(self) -> impl Iterator<Item = (StoredGroupMessage, DeviceSyncContent)> {
        self.into_iter().filter_map(|msg| {
            let content = serde_json::from_slice(&msg.decrypted_message_bytes).ok()?;
            Some((msg, content))
        })
    }
}
