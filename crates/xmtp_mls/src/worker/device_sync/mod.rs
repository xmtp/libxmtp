use crate::{
    client::ClientError,
    context::XmtpSharedContext,
    groups::{
        GroupError, MlsGroup, PreconfiguredPolicies, send_message_opts, summary::SyncSummary,
    },
    identity::parse_credential,
    mls_store::{MlsStore, MlsStoreError},
    subscriptions::{SubscribeError, internal::InternalEvent},
    worker::{NeedsDbReconnect, metrics::WorkerMetrics},
};
use openmls::prelude::{BasicCredential, Member};
use owo_colors::OwoColorize;
use prost::Message;
use std::{collections::HashMap, sync::Arc};
use thiserror::Error;
use tokio::sync::broadcast::error::RecvError;
use tracing::instrument;
use worker::SyncMetric;
use xmtp_archive::ArchiveError;
use xmtp_common::ErrorCode;
use xmtp_common::{NS_IN_DAY, RetryableError, time::now_ns};
use xmtp_cryptography::secret::{Zeroize, Zeroizing};
use xmtp_db::tasks::NewTask;
use xmtp_db::{NotFound, StorageError, consent_record::ConsentState, group::GroupQueryArgs};
use xmtp_db::{XmtpDb, group::ConversationType, prelude::*};
use xmtp_id::{InboxIdRef, associations::DeserializationError};
use xmtp_mls_common::group::GroupMetadataOptions;
use xmtp_proto::types::{GroupId, InstallationId};
use xmtp_proto::xmtp::{
    device_sync::content::{
        DeviceSyncContent as DeviceSyncContentProto, device_sync_content::Content as ContentProto,
        preference_update::Update as PreferenceUpdateProto,
    },
    mls::{
        database::{
            AddMissingInstallations as AddMissingInstallationsProto, Task as TaskProto,
            task::Task as TaskKindProto,
        },
        message_contents::{ContentTypeId, EncodedContent},
    },
};

pub mod archive;
pub mod preference_sync;
pub mod worker;

pub use xmtp_archive::archive_options::{ArchiveOptions, BackupElementSelection};

#[cfg(test)]
mod tests;

/// Encode sync content as the `EncodedContent` bytes of a sync message.
///
/// Sync content can hold an HMAC root key. Every buffer that holds it here is
/// wiped: the key fields of `content` after encoding, the inner encoding, and
/// the returned bytes when the caller drops them.
fn sync_message_bytes(content: ContentProto) -> Zeroizing<Vec<u8>> {
    let mut content = DeviceSyncContentProto {
        content: Some(content),
    };
    // `encode_to_vec` sizes its buffer up front, so it never frees a partial copy.
    let inner = content.encode_to_vec();
    if let Some(content) = &mut content.content {
        wipe_hmac_keys(content);
    }
    let mut encoded = EncodedContent {
        r#type: Some(ContentTypeId {
            authority_id: "xmtp.org".to_string(),
            type_id: "application/x-protobuf".to_string(),
            version_major: 1,
            version_minor: 0,
        }),
        parameters: HashMap::new(),
        fallback: None,
        compression: None,
        content: inner,
    };
    let bytes = Zeroizing::new(encoded.encode_to_vec());
    encoded.content.zeroize();
    bytes
}

/// Wipe the HMAC root key of every preference update in `content`.
fn wipe_hmac_keys(content: &mut ContentProto) {
    if let ContentProto::PreferenceUpdates(updates) = content {
        for update in &mut updates.updates {
            if let Some(PreferenceUpdateProto::Hmac(hmac)) = &mut update.update {
                hmac.key.zeroize();
            }
        }
    }
}

/// Sync content decoded from a received message. Its HMAC root keys are wiped
/// on drop, on every path out of processing.
pub(super) struct ReceivedContent(pub(super) ContentProto);

impl Drop for ReceivedContent {
    fn drop(&mut self) {
        wipe_hmac_keys(&mut self.0);
    }
}

/// A sync group belongs to the own inbox only when that inbox added this
/// client and every leaf node's credential names it.
// implements: SYNC-010
pub(crate) fn is_own_sync_group(
    own_inbox_id: &str,
    added_by_inbox_id: &str,
    members: impl IntoIterator<Item = Member>,
) -> bool {
    added_by_inbox_id == own_inbox_id
        && members.into_iter().all(|member| {
            BasicCredential::try_from(member.credential)
                .ok()
                .and_then(|credential| parse_credential(credential.identity()).ok())
                .is_some_and(|inbox_id| inbox_id == own_inbox_id)
        })
}

#[derive(Debug, Error, ErrorCode)]
pub enum DeviceSyncError {
    /// I/O error.
    ///
    /// File system or network I/O failed. May be retryable.
    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),
    /// Serialization error.
    ///
    /// JSON serialization/deserialization failed. Retryable.
    #[error("Serialization/Deserialization Error {0}")]
    Serde(#[from] serde_json::Error),
    #[error(transparent)]
    #[error_code(inherit)]
    ProtoConversion(#[from] xmtp_proto::ConversionError),
    /// AES-GCM encryption error.
    ///
    /// Encryption/decryption of sync payload failed. Retryable.
    #[error("AES-GCM encryption error")]
    AesGcm(#[from] aes_gcm::Error),
    #[error("storage error: {0}")]
    #[error_code(inherit)]
    Storage(#[from] StorageError),
    /// Type conversion error.
    ///
    /// Internal type conversion failed. Retryable.
    #[error("type conversion error")]
    Conversion,
    /// UTF-8 error.
    ///
    /// String is not valid UTF-8. Retryable.
    #[error("utf-8 error: {0}")]
    UTF8(#[from] std::str::Utf8Error),
    #[error("client error: {0}")]
    #[error_code(inherit)]
    Client(#[from] ClientError),
    #[error("group error: {0}")]
    #[error_code(inherit)]
    Group(#[from] GroupError),
    /// Invalid payload.
    ///
    /// Sync message payload is malformed. Retryable.
    #[error("invalid history message payload")]
    InvalidPayload,
    /// Unspecified sync kind.
    ///
    /// Device sync kind not specified. Not retryable.
    #[error("unspecified device sync kind")]
    UnspecifiedDeviceSyncKind,
    #[error(transparent)]
    #[error_code(inherit)]
    Subscribe(#[from] SubscribeError),
    /// Bincode error.
    ///
    /// Binary serialization failed. Retryable.
    #[error(transparent)]
    Bincode(#[from] bincode::Error),
    /// Archive error.
    ///
    /// Sync archive operation failed. Retryable.
    #[error(transparent)]
    Archive(#[from] ArchiveError),
    /// Decode error.
    ///
    /// Protobuf decoding failed. Retryable.
    #[error(transparent)]
    Decode(#[from] prost::DecodeError),
    #[error(transparent)]
    #[error_code(inherit)]
    Deserialization(#[from] DeserializationError),
    /// Missing sync group.
    ///
    /// Sync group not found. Not retryable.
    #[error("Missing sync group")]
    MissingSyncGroup,
    #[error(transparent)]
    #[error_code(inherit)]
    Db(#[from] xmtp_db::ConnectionError),
    /// Sync summary.
    ///
    /// Sync completed with errors. May be retryable.
    #[error("{}", _0.to_string())]
    Sync(#[source] Box<SyncSummary>),
    /// MLS store error.
    ///
    /// OpenMLS key store operation failed. Retryable.
    #[error(transparent)]
    MlsStore(#[from] MlsStoreError),
    /// Receive error.
    ///
    /// Channel receive failed. Retryable.
    #[error(transparent)]
    Recv(#[from] RecvError),
    /// Missing field.
    ///
    /// Required field not present. Retryable.
    #[error("Missing Field: {0:?} {1}")]
    MissingField(MissingField, String),
}

#[derive(Debug)]
pub enum MissingField {
    Conversation(ConversationField),
}
#[derive(Debug)]
pub enum ConversationField {
    DmId,
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
            Self::Storage(s) => s.db_needs_connection(),
            // A dropped pool can hide in these wrapped errors; forward so the
            // worker stops instead of hot-looping (was `_ => false`).
            Self::Db(c) => c.db_needs_connection(),
            Self::Group(e) => e.needs_db_reconnect(),
            Self::MlsStore(e) => e.needs_db_reconnect(),
            Self::Subscribe(e) => e.needs_db_reconnect(),
            _ => false,
        }
    }
}

impl RetryableError for DeviceSyncError {
    fn is_retryable(&self) -> bool {
        !matches!(
            self,
            Self::MissingSyncGroup | Self::UnspecifiedDeviceSyncKind
        )
    }
}

impl From<NotFound> for DeviceSyncError {
    fn from(value: NotFound) -> Self {
        DeviceSyncError::Storage(StorageError::NotFound(value))
    }
}

#[derive(Clone)]
pub struct DeviceSyncClient<Context> {
    pub(crate) context: Context,
    pub(crate) mls_store: MlsStore<Context>,
    pub(crate) metrics: Arc<WorkerMetrics<SyncMetric>>,
}

impl<Context: XmtpSharedContext> DeviceSyncClient<Context> {
    pub fn new(context: Context, metrics: Arc<WorkerMetrics<SyncMetric>>) -> Self {
        Self {
            context: context.clone(),
            mls_store: MlsStore::new(context),
            metrics,
        }
    }
}

impl<Context> DeviceSyncClient<Context>
where
    Context: XmtpSharedContext,
{
    pub fn inbox_id(&self) -> InboxIdRef<'_> {
        self.context.identity().inbox_id()
    }

    pub fn installation_id(&self) -> InstallationId {
        self.context.installation_id()
    }

    pub fn db(&self) -> <Context::Db as XmtpDb>::DbQuery {
        self.context.db()
    }

    /// Blocks until the sync worker notifies that it is initialized and running.
    pub async fn wait_for_sync_worker_init(&self) -> Result<(), xmtp_common::time::Expired> {
        self.metrics.wait_for_init().await
    }

    /// Sends a device sync message.
    /// If the `group_id` is `None`, the message will be sent
    /// to the primary sync group ID.
    #[cfg_attr(any(test, feature = "test-utils"), tracing::instrument(level = "info", fields(inbox_id = self.context.inbox_id()), skip(self, content)))]
    #[cfg_attr(
        not(any(test, feature = "test-utils")),
        tracing::instrument(level = "trace", skip(self, content))
    )]
    // implements: SYNC-004
    async fn send_device_sync_message(
        &self,
        content: ContentProto,
    ) -> Result<Vec<u8>, ClientError> {
        // Encode first: the content can hold an HMAC root key, and only the
        // encoding wipes it, so every later return drops wiped bytes.
        let content_bytes = sync_message_bytes(content);
        let sync_group = self.get_sync_group().await?;

        let msg = format!(
            "Sending sync message to group {:?}",
            xmtp_common::fmt::debug_hex(sync_group.group_id)
        );
        tracing::info!("{}", msg.yellow());

        let message_id = sync_group.prepare_message(
            &content_bytes,
            send_message_opts::SendMessageOpts {
                should_push: false,
                idempotency_key: None,
            },
        )?;

        sync_group.sync_until_last_intent_resolved().await?;

        xmtp_events::EventWriter::emit(
            self.context.events(),
            None,
            Some(InternalEvent::SyncMessagePublished),
        );

        Ok(message_id)
    }

    /// The newest sync group that only the own inbox is in.
    /// Older builds stored sync groups from other inboxes; this skips them.
    // implements: SYNC-002, SYNC-010
    pub(crate) fn primary_sync_group(&self) -> Result<Option<MlsGroup<Context>>, GroupError> {
        let own_inbox_id = self.context.inbox_id();
        for stored in self.context.db().all_sync_groups()? {
            let group = self.mls_store.group(&stored.id)?;
            let own = group.with_group_snapshot(|mls_group| {
                Ok(is_own_sync_group(
                    own_inbox_id,
                    &stored.added_by_inbox_id,
                    mls_group.members(),
                ))
            })?;
            if own {
                return Ok(Some(group));
            }
            tracing::debug!(
                group_id = %stored.id,
                "skipping a sync group that names another inbox"
            );
        }
        Ok(None)
    }

    // implements: SYNC-001
    #[instrument(level = "trace", skip_all)]
    pub async fn get_sync_group(&self) -> Result<MlsGroup<Context>, GroupError> {
        let sync_group = match self.primary_sync_group()? {
            Some(sync_group) => sync_group,
            None => {
                let sync_group = MlsGroup::create_and_insert(
                    self.context.clone(),
                    ConversationType::Sync,
                    PreconfiguredPolicies::default().to_policy_set(),
                    GroupMetadataOptions::default(),
                    None,
                )?;
                tracing::info!(
                    "Creating sync group: {}",
                    xmtp_common::fmt::debug_hex(sync_group.group_id)
                );
                if let Err(inline_err) = sync_group.add_missing_installations().await {
                    // The group row is already persisted, so this add is never
                    // re-attempted (later calls take the `Some` branch) — arm
                    // the durable reconcile task so the TaskRunner heals it,
                    // then surface the original error. Armed only on failure:
                    // enqueue-first duplicated the reconcile (and its identity
                    // fetch) on every sync-group creation.
                    self.schedule_add_missing_installations_task(sync_group.group_id)
                        .map_err(Box::new)?;
                    return Err(inline_err);
                }
                sync_group.sync_with_conn().await?;

                self.metrics.increment_metric(SyncMetric::SyncGroupCreated);

                sync_group
            }
        };

        Ok(sync_group)
    }

    /// This should be triggered when a new sync group appears,
    /// indicating the presence of a new installation.
    ///
    /// Schedules one durable AddMissingInstallations task per eligible group
    /// on the TaskRunner (deduped by payload hash) so the membership add is
    /// retried with backoff instead of being lost if a single inline attempt
    /// fails (e.g. identity propagation lag → MissingSequenceId).
    #[cfg_attr(
        any(test, feature = "test-utils"),
        tracing::instrument(level = "info", skip_all)
    )]
    // implements: SYNC-014
    pub fn schedule_add_installations_to_groups(&self) -> Result<usize, DeviceSyncError> {
        let groups = self.mls_store.find_groups(GroupQueryArgs {
            last_activity_after_ns: Some(now_ns() - NS_IN_DAY * 90),
            consent_states: Some(vec![ConsentState::Allowed, ConsentState::Unknown]),
            ..Default::default()
        })?;

        for group in &groups {
            self.schedule_add_missing_installations_task(group.group_id)?;
        }
        Ok(groups.len())
    }

    /// Durably enqueue one AddMissingInstallations task for `group_id`
    /// (deduped by payload hash against any pending row for the same group)
    /// and wake the TaskRunner.
    pub(crate) fn schedule_add_missing_installations_task(
        &self,
        group_id: GroupId,
    ) -> Result<(), DeviceSyncError> {
        let task = NewTask::builder()
            .originating_message_sequence_id(0)
            .build(TaskProto {
                task: Some(TaskKindProto::AddMissingInstallations(
                    AddMissingInstallationsProto {
                        group_id: group_id.to_vec(),
                    },
                )),
            })?;
        self.context.db().create_or_ignore_task(task)?;
        xmtp_events::EventWriter::emit(
            self.context.events(),
            None,
            Some(InternalEvent::TaskScheduled),
        );
        Ok(())
    }
}

/// The payload of a sync message's outer `EncodedContent`, the only field the
/// worker reads. It can hold an HMAC root key, so it is wiped on drop, also
/// when prost drops a value whose decode fails part way.
#[derive(Clone, PartialEq, prost::Message)]
struct SyncPayload {
    #[prost(bytes = "vec", tag = "4")]
    content: Vec<u8>,
}

impl Drop for SyncPayload {
    fn drop(&mut self) {
        self.content.zeroize();
    }
}

/// Decode a received sync message: the outer `EncodedContent`, then the
/// supported device-sync content inside it.
pub(super) fn decode_sync_message(plaintext: &[u8]) -> Option<ReceivedContent> {
    let payload = SyncPayload::decode(plaintext).ok()?;
    decode_supported_content(&payload.content)
}

/// Decode only device-sync content that this client supports.
///
/// Old archive transfer fields decode as an unset oneof after their field
/// numbers became reserved. Ignore them so older installations do not make
/// the sync worker fail or retry the message.
///
/// `bytes` can hold an HMAC root key: the caller wipes them, and the content
/// wipes its keys on drop.
fn decode_supported_content(bytes: &[u8]) -> Option<ReceivedContent> {
    DeviceSyncContentProto::decode(bytes)
        .ok()?
        .content
        .map(ReceivedContent)
}
