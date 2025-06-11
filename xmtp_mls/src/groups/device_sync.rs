use super::{summary::SyncSummary, welcome_sync::WelcomeService, GroupError, MlsGroup};
use crate::{
    client::ClientError,
    context::XmtpMlsLocalContext,
    mls_store::{MlsStore, MlsStoreError},
    subscriptions::{SubscribeError, SyncWorkerEvent},
    worker::{metrics::WorkerMetrics, NeedsDbReconnect},
};
use futures::future::join_all;
use prost::Message;
use std::{collections::HashMap, sync::Arc};
use thiserror::Error;
use tokio::sync::broadcast::error::RecvError;
use tracing::instrument;
use worker::SyncMetric;
use xmtp_archive::ArchiveError;
use xmtp_common::{time::now_ns, types::InstallationId, RetryableError, NS_IN_DAY};
use xmtp_content_types::encoded_content_to_bytes;
use xmtp_db::{
    consent_record::ConsentState, group::GroupQueryArgs, group_message::StoredGroupMessage,
    NotFound, StorageError,
};
use xmtp_db::{DbConnection, XmtpDb};
use xmtp_id::{associations::DeserializationError, InboxIdRef};
use xmtp_proto::{
    api_client::trait_impls::XmtpApi,
    xmtp::{
        device_sync::{
            content::{
                device_sync_content::Content as ContentProto,
                DeviceSyncContent as DeviceSyncContentProto,
            },
            BackupElementSelection, BackupOptions,
        },
        mls::message_contents::{
            plaintext_envelope::{Content, V1},
            ContentTypeId, EncodedContent, PlaintextEnvelope,
        },
    },
};

pub mod archive;
pub mod preference_sync;
pub mod worker;

#[cfg(test)]
mod tests;

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
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("type conversion error")]
    Conversion,
    #[error("utf-8 error: {0}")]
    UTF8(#[from] std::str::Utf8Error),
    #[error("client error: {0}")]
    Client(#[from] ClientError),
    #[error("group error: {0}")]
    Group(#[from] GroupError),
    #[error("no pending request to reply to")]
    NoPendingRequest,
    #[error("invalid history message payload")]
    InvalidPayload,
    #[error("unspecified device sync kind")]
    UnspecifiedDeviceSyncKind,
    #[error("sync reply is too old")]
    SyncPayloadTooOld,
    #[error(transparent)]
    Subscribe(#[from] SubscribeError),
    #[error(transparent)]
    Bincode(#[from] bincode::Error),
    #[error(transparent)]
    Archive(#[from] ArchiveError),
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
    #[error(transparent)]
    Db(#[from] xmtp_db::ConnectionError),
    #[error("{}", _0.to_string())]
    Sync(Box<SyncSummary>),
    #[error(transparent)]
    MlsStore(#[from] MlsStoreError),
    #[error(transparent)]
    Recv(#[from] RecvError),
}

impl From<SyncSummary> for DeviceSyncError {
    fn from(value: SyncSummary) -> Self {
        DeviceSyncError::Sync(Box::new(value))
    }
}

impl NeedsDbReconnect for DeviceSyncError {
    fn needs_db_reconnect(&self) -> bool {
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

#[derive(Clone)]
pub struct DeviceSyncClient<ApiClient, Db> {
    pub(crate) context: Arc<XmtpMlsLocalContext<ApiClient, Db>>,
    pub(crate) welcome_service: WelcomeService<ApiClient, Db>,
    pub(crate) mls_store: MlsStore<ApiClient, Db>,
    pub(crate) metrics: Arc<WorkerMetrics<SyncMetric>>,
}

impl<ApiClient, Db> DeviceSyncClient<ApiClient, Db> {
    pub fn new(
        context: &Arc<XmtpMlsLocalContext<ApiClient, Db>>,
        metrics: Arc<WorkerMetrics<SyncMetric>>,
    ) -> Self {
        Self {
            context: context.clone(),
            welcome_service: WelcomeService::new(context.clone()),
            mls_store: MlsStore::new(context.clone()),
            metrics,
        }
    }
}

impl<ApiClient, Db> DeviceSyncClient<ApiClient, Db>
where
    ApiClient: XmtpApi,
    Db: XmtpDb,
{
    pub fn inbox_id(&self) -> InboxIdRef<'_> {
        self.context.identity.inbox_id()
    }

    pub fn installation_id(&self) -> InstallationId {
        self.context.installation_id()
    }

    pub fn db(&self) -> DbConnection<<Db as XmtpDb>::Connection> {
        self.context.db()
    }

    /// Blocks until the sync worker notifies that it is initialized and running.
    pub async fn wait_for_sync_worker_init(&self) -> Result<(), xmtp_common::time::Expired> {
        self.metrics.wait_for_init().await
    }

    /// Sends a device sync message.
    /// If the `group_id` is `None`, the message will be sent
    /// to the primary sync group ID.
    async fn send_device_sync_message(
        &self,
        content: ContentProto,
    ) -> Result<Vec<u8>, ClientError> {
        let content = DeviceSyncContentProto {
            content: Some(content),
        };

        let sync_group = self.get_sync_group().await?;

        tracing::info!(
            "\x1b[33mSending sync message to group {:?}: \x1b[0m{content:?}",
            &sync_group.group_id[..4]
        );

        let mut content_bytes = vec![];
        content
            .encode(&mut content_bytes)
            .map_err(|err| ClientError::Generic(err.to_string()))?;

        let encoded_content = EncodedContent {
            r#type: Some(ContentTypeId {
                authority_id: "xmtp.org".to_string(),
                type_id: "application/x-protobuf".to_string(),
                version_major: 1,
                version_minor: 0,
            }),
            parameters: HashMap::new(),
            fallback: None,
            compression: None,
            content: content_bytes,
        };
        let content_bytes = encoded_content_to_bytes(encoded_content);

        let message_id = sync_group.prepare_message(&content_bytes, |now| PlaintextEnvelope {
            content: Some(Content::V1(V1 {
                content: content_bytes.clone(),
                idempotency_key: now.to_string(),
            })),
        })?;

        sync_group.sync_until_last_intent_resolved().await?;

        // Notify our own worker of our own message so it can process it.
        let _ = self
            .context
            .worker_events
            .send(SyncWorkerEvent::NewSyncGroupMsg);

        Ok(message_id)
    }

    #[instrument(level = "trace", skip_all)]
    pub async fn get_sync_group(&self) -> Result<MlsGroup<ApiClient, Db>, GroupError> {
        let db = self.context.db();
        let sync_group = match db.primary_sync_group()? {
            Some(sync_group) => self.mls_store.group(&sync_group.id)?,
            None => {
                let sync_group = MlsGroup::create_and_insert_sync_group(self.context.clone())?;
                tracing::info!("Creating sync group: {:?}", sync_group.group_id);
                sync_group.add_missing_installations().await?;
                sync_group.sync_with_conn().await?;

                self.metrics.increment_metric(SyncMetric::SyncGroupCreated);

                sync_group
            }
        };

        Ok(sync_group)
    }

    /// This should be triggered when a new sync group appears,
    /// indicating the presence of a new installation.
    pub async fn add_new_installation_to_groups(&self) -> Result<(), DeviceSyncError> {
        let groups = self.mls_store.find_groups(GroupQueryArgs {
            activity_after_ns: Some(now_ns() - NS_IN_DAY * 90),
            consent_states: Some(vec![ConsentState::Allowed]),
            ..Default::default()
        })?;

        // Add the new installation to groups in batches
        for chunk in groups.chunks(10) {
            let mut add_futs = vec![];
            for group in chunk {
                add_futs.push(group.add_missing_installations());
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

fn default_archive_options() -> BackupOptions {
    BackupOptions {
        elements: vec![
            BackupElementSelection::Messages as i32,
            BackupElementSelection::Consent as i32,
        ],
        ..Default::default()
    }
}

pub trait IterWithContent<A, B> {
    fn iter_with_content(self) -> impl DoubleEndedIterator<Item = (A, B)>;
}

impl IterWithContent<StoredGroupMessage, ContentProto> for Vec<StoredGroupMessage> {
    fn iter_with_content(
        self,
    ) -> impl DoubleEndedIterator<Item = (StoredGroupMessage, ContentProto)> {
        self.into_iter().flat_map(|msg| {
            let result = (|| {
                let encoded_content = EncodedContent::decode(&*msg.decrypted_message_bytes).ok()?;
                let content = DeviceSyncContentProto::decode(&*encoded_content.content).ok()?;
                content.content.map(|c| (msg, c))
            })();

            result.into_iter()
        })
    }
}
