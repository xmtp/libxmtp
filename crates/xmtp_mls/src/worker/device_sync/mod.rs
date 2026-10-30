use crate::state_tx::StateTx;
use crate::{
    client::ClientError,
    context::XmtpSharedContext,
    groups::{
        GroupError, MlsGroup, PreconfiguredPolicies, send_message_opts, summary::SyncSummary,
    },
    identity::parse_credential,
    identity_updates::IdentityUpdates,
    mls_store::{MlsStore, MlsStoreError},
    subscriptions::{SubscribeError, internal::InternalEvent},
    worker::{NeedsDbReconnect, metrics::WorkerMetrics},
};
use openmls::prelude::{BasicCredential, Member};
use prost::Message;
use std::{collections::HashMap, sync::Arc};
use thiserror::Error;
use tokio::sync::broadcast::error::RecvError;
use tracing::instrument;
use worker::SyncMetric;
use xmtp_archive::ArchiveError;
use xmtp_common::ErrorCode;
use xmtp_common::{NS_IN_DAY, RetryableError, time::now_ns};
use xmtp_configuration::{
    INSTALLATION_RETRY_MIN_PROTOCOL_VERSION, SYNC_UPDATE_INSTALLATIONS_INTERVAL_NS,
};
use xmtp_cryptography::secret::{Zeroize, Zeroizing};
use xmtp_db::TransactionalKeyStore;
use xmtp_db::tasks::NewTask;
use xmtp_db::{
    NotFound, StorageError,
    consent_record::ConsentState,
    group::{GroupMembershipState, GroupQueryArgs},
    group_message::{DeliveryStatus, StoredGroupMessage},
    sync_outbox::{OwedPage, clear_owed, owed_page},
};
use xmtp_db::{XmtpDb, group::ConversationType, prelude::*};
use xmtp_events::EventWriter;
use xmtp_id::{InboxIdRef, associations::DeserializationError};
use xmtp_mls_common::app_data::protocol_floor::committed_floor_at_least;
use xmtp_mls_common::group::GroupMetadataOptions;
use xmtp_mls_common::libxmtp_version::LibXMTPVersion;
use xmtp_proto::ShortHex;
use xmtp_proto::types::{GroupId, InstallationId};
use xmtp_proto::xmtp::{
    device_sync::content::{
        DeviceSyncContent as DeviceSyncContentProto, HmacKeyUpdate,
        PreferenceUpdate as PreferenceUpdateMessage, PreferenceUpdates,
        device_sync_content::Content as ContentProto,
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

/// Move a stored sync message's plaintext into a buffer wiped on drop. It
/// can hold an HMAC root key, so it moves as the row loads, before any
/// path out can drop it.
fn take_plaintext(msg: &mut StoredGroupMessage) -> Zeroizing<Vec<u8>> {
    Zeroizing::new(std::mem::take(&mut msg.decrypted_message_bytes))
}

/// [`take_plaintext`] for every loaded row.
fn with_plaintext(rows: Vec<StoredGroupMessage>) -> Vec<(StoredGroupMessage, Zeroizing<Vec<u8>>)> {
    rows.into_iter()
        .map(|mut msg| {
            let plaintext = take_plaintext(&mut msg);
            (msg, plaintext)
        })
        .collect()
}

/// Sync content decoded from a received message. Its HMAC root keys are wiped
/// on drop, on every path out of processing.
pub(super) struct ReceivedContent(pub(super) ContentProto);

impl Drop for ReceivedContent {
    fn drop(&mut self) {
        wipe_hmac_keys(&mut self.0);
    }
}

/// The id of the group [`DeviceSyncClient::primary_sync_group`] selects,
/// read in the caller's write.
// implements: SYNC-002, SYNC-010
pub(super) fn selected_sync_group_in<T: TransactionalKeyStore>(
    tx: &mut StateTx<'_, T>,
    own_inbox_id: &str,
) -> Result<Option<GroupId>, GroupError> {
    let groups = tx.storage().db().all_sync_groups()?;
    for stored in groups {
        let own = tx.with_group(stored.id, |group, _| {
            Ok::<_, GroupError>(is_own_sync_group(
                own_inbox_id,
                &stored.added_by_inbox_id,
                group.members(),
            ))
        })?;
        if own {
            return Ok(Some(stored.id));
        }
    }
    Ok(None)
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
    // The own identity state could not be read; a sync publish waits for it.
    #[error("own identity state unavailable: {0}")]
    #[error_code(inherit)]
    OwnStateUnavailable(#[source] ClientError),
    /// Outbox page unpublished.
    ///
    /// A queued sync message is not published yet; its values stay owed and
    /// the outbox retries. Retryable.
    #[error("a queued sync message is not published yet")]
    OutboxPageUnpublished,
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
    fn restart_delay(&self) -> Option<xmtp_common::time::Duration> {
        RetryableError::rate_limit_delay(self)
    }

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

    fn rate_limit_delay(&self) -> Option<xmtp_common::time::Duration> {
        match self {
            Self::Client(e) => e.rate_limit_delay(),
            Self::Group(e) => e.rate_limit_delay(),
            Self::Subscribe(e) => e.rate_limit_delay(),
            Self::Sync(e) => e.rate_limit_delay(),
            Self::MlsStore(e) => e.rate_limit_delay(),
            Self::OwnStateUnavailable(e) => e.rate_limit_delay(),
            _ => None,
        }
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

    /// Publish every owed value to the selected sync group, a page at
    /// a time. A page's marks clear only after the message carrying it is
    /// published.
    // implements: SYNC-004, SYNC-034, SYNC-029, SYNC-033
    #[tracing::instrument(level = "debug", skip_all, fields(operation = "publish_outbox"))]
    pub(super) async fn publish_outbox(&self) -> Result<(), DeviceSyncError> {
        // A quarter of the deployment's envelope, so MLS and envelope framing fit.
        let max_bytes = (self.context.api().limits().max_envelope_bytes / 4).min(PAGE_BYTES);
        self.get_sync_group().await?;
        loop {
            // Values first, target second. A sync Welcome that commits after
            // this read marks every key again, so the clear below skips them.
            // One that committed before it is already the selected group.
            let page = owed_page(&self.db(), PAGE_UPDATES, max_bytes)?;
            if page.is_empty() {
                return Ok(());
            }
            let group = self
                .primary_sync_group()?
                .ok_or(DeviceSyncError::MissingSyncGroup)?;
            // implements: SYNC-034
            let gap = self.own_membership_gap(&group).await?;
            if gap.missing {
                // A failed add holds no page back, unless a revoked leaf may
                // stay or the add must wait out a rate limit.
                // implements: RATE-041
                if let Err(e) = group.add_missing_installations().await {
                    if gap.revoked || e.needs_db_reconnect() || e.rate_limit_delay().is_some() {
                        return Err(e.into());
                    }
                    tracing::warn!(error = %e, "adding own installations failed");
                }
            } else if gap.revoked {
                group.remove_revoked_installations().await?;
            }
            if gap.missing || gap.revoked {
                let gap = self.own_membership_gap(&group).await?;
                if gap.revoked {
                    return Err(GroupError::InvalidGroupMembership.into());
                }
                // The leaves present still get this page; the task retries
                // the add once the retry interval has passed.
                if gap.retriable {
                    let interval = SYNC_UPDATE_INSTALLATIONS_INTERVAL_NS as u64;
                    let delay = xmtp_common::time::Duration::from_nanos(interval);
                    self.schedule_add_missing_installations_task(group.group_id, Some(delay))?;
                }
            }
            // One intent at a time: a page an earlier pass queued is
            // published, or fails for good, before this one is queued.
            group.sync_until_last_intent_resolved().await?;
            tracing::info!(
                group_id = group.group_id.short_hex(),
                "publishing owed preferences"
            );
            let id = group.prepare_message(
                &sync_message_bytes(page_content(&page)),
                send_message_opts::SendMessageOpts {
                    should_push: false,
                    idempotency_key: None,
                },
            )?;
            group.sync_until_last_intent_resolved().await?;
            // Only a published page clears its marks. Otherwise they stay
            // owed, the pass backs off, and a later pass sends them again.
            let status = self.db().get_group_message(&id)?.map(|mut sent| {
                drop(take_plaintext(&mut sent));
                sent.delivery_status
            });
            #[cfg(test)]
            let status = UNCONFIRMED_NEXT_PAGE
                .lock()
                .take_if(|installation| *installation == self.installation_id().as_slice())
                .map_or(status, |_| None);
            if status != Some(DeliveryStatus::Published) {
                return Err(DeviceSyncError::OutboxPageUnpublished);
            }
            clear_owed(&self.db(), &page)?;
            for _ in &page.consent {
                self.metrics.increment_metric(SyncMetric::ConsentSent);
            }
            if page.root_key.is_some() {
                self.metrics.increment_metric(SyncMetric::HmacSent);
            }
        }
    }

    /// One outbox pass, then one receive pass, as the worker runs them.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn sync_passes_for_test(&self) -> Result<(), DeviceSyncError> {
        self.publish_outbox().await?;
        self.process_sync_group_messages().await
    }

    /// How `group`'s leaves differ from the newest own association state held
    /// locally: an associated installation without a leaf, or a leaf other
    /// than this installation's that the state does not associate.
    async fn own_membership_gap(
        &self,
        group: &MlsGroup<Context>,
    ) -> Result<MembershipGap, DeviceSyncError> {
        // implements: SYNC-034. An unreadable own state, such as a smart-wallet
        // signature that needs the verifier, means not yet (as the receive path does).
        let own = match IdentityUpdates::new(&self.context)
            .get_association_state(&self.db(), self.inbox_id(), None)
            .await
        {
            Ok(state) => state.installation_ids(),
            Err(e) if e.db_needs_connection() => return Err(e.into()),
            Err(e) => return Err(DeviceSyncError::OwnStateUnavailable(e)),
        };
        let me = self.installation_id();
        Ok(group.with_group_snapshot(|mls_group| {
            let missing = own.iter().any(|installation| {
                !mls_group
                    .members()
                    .any(|member| member.signature_key == *installation)
            });
            let gate = LibXMTPVersion::parse(INSTALLATION_RETRY_MIN_PROTOCOL_VERSION).ok();
            Ok(MembershipGap {
                missing,
                revoked: mls_group.members().any(|member| {
                    member.signature_key != me.as_slice() && !own.contains(&member.signature_key)
                }),
                retriable: missing
                    && gate.is_some_and(|gate| {
                        committed_floor_at_least(mls_group.extensions(), &gate)
                    }),
            })
        })?)
    }

    /// Add the own installations that sync group `group` lacks. `true` while
    /// it is the selected group and one it can retry still has no leaf.
    pub(crate) async fn retry_own_installations(
        &self,
        group: &MlsGroup<Context>,
    ) -> Result<bool, DeviceSyncError> {
        group.add_missing_installations().await?;
        if self
            .primary_sync_group()?
            .is_none_or(|selected| selected.group_id != group.group_id)
        {
            return Ok(false);
        }
        Ok(self.own_membership_gap(group).await?.retriable)
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
                    // A rate-limited add is not resent before its delay.
                    // implements: RATE-041
                    self.schedule_add_missing_installations_task(
                        sync_group.group_id,
                        inline_err.rate_limit_delay(),
                    )
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

    /// Arm the sync group's reconcile task for `group_id`, due once `delay`
    /// has passed. It is kept until it finishes, whatever its failures.
    pub(crate) fn schedule_add_missing_installations_task(
        &self,
        group_id: GroupId,
        delay: Option<xmtp_common::time::Duration>,
    ) -> Result<(), DeviceSyncError> {
        Ok(schedule_add_missing_installations(
            &self.context.db(),
            self.context.events(),
            group_id,
            delay,
            true,
        )?)
    }

    /// Schedule the adds to the user's conversations for a new installation.
    #[cfg(test)]
    pub(crate) fn schedule_add_installations_to_groups(&self) -> Result<usize, DeviceSyncError> {
        Ok(schedule_newcomer_adds(
            &self.context.db(),
            self.context.events(),
        )?)
    }
}

/// How a sync group's leaves differ from the own inbox's installations.
struct MembershipGap {
    missing: bool,
    revoked: bool,
    /// Missing, and the group's floor admits own-installation retries.
    retriable: bool,
}

/// Test-only fault: the next outbox page of this installation reads back as
/// not published, as when its confirmation fails after the backend took it.
#[cfg(test)]
pub(super) static UNCONFIRMED_NEXT_PAGE: parking_lot::Mutex<Option<Vec<u8>>> =
    parking_lot::Mutex::new(None);

/// Records per sync message the outbox sends.
const PAGE_UPDATES: usize = 1_000;
/// Bytes of encoded updates per sync message.
const PAGE_BYTES: usize = 256 * 1024;

/// The sync content of an outbox page. It holds a plain copy of the root key;
/// `sync_message_bytes` wipes it.
fn page_content(page: &OwedPage) -> ContentProto {
    let root_key = page.root_key.iter().map(|root| {
        PreferenceUpdateProto::Hmac(HmacKeyUpdate {
            key: root.key.as_slice().to_vec(),
            cycled_at_ns: root.cycled_at_ns,
        })
    });
    let consent = page
        .consent
        .iter()
        .map(|record| PreferenceUpdateProto::Consent(record.clone().into()));
    ContentProto::PreferenceUpdates(PreferenceUpdates {
        updates: root_key
            .chain(consent)
            .map(|update| PreferenceUpdateMessage {
                update: Some(update),
            })
            .collect(),
    })
}

/// A new sync group from a Welcome means a new installation: schedule one
/// durable AddMissingInstallations task per eligible group (deduped by
/// payload hash), in the transaction that installs the Welcome, so a restart
/// cannot lose them.
// implements: SYNC-014
pub(crate) fn schedule_newcomer_adds(
    db: &impl DbQuery,
    events: &impl EventWriter<InternalEvent>,
) -> Result<usize, StorageError> {
    let groups = db.find_groups(GroupQueryArgs {
        last_activity_after_ns: Some(now_ns() - NS_IN_DAY * 90),
        consent_states: Some(vec![ConsentState::Allowed, ConsentState::Unknown]),
        // A Restored placeholder takes no commit until a Welcome activates
        // it, so its task could never finish.
        // implements: ARCH-015
        allowed_states: Some(vec![
            GroupMembershipState::Allowed,
            GroupMembershipState::Rejected,
            GroupMembershipState::Pending,
            GroupMembershipState::PendingRemove,
        ]),
        ..Default::default()
    })?;
    for group in &groups {
        schedule_add_missing_installations(db, events, group.id, None, false)?;
    }
    Ok(groups.len())
}

/// Durably enqueue one AddMissingInstallations task for `group_id` (deduped
/// by payload hash against any pending row for the same group) and wake the
/// TaskRunner. The task is due once `delay`, a rate-limit delay, has passed.
/// A task `until_done` never expires, and a failing one backs off to at most
/// one run an interval.
fn schedule_add_missing_installations(
    db: &impl DbQuery,
    events: &impl EventWriter<InternalEvent>,
    group_id: GroupId,
    delay: Option<xmtp_common::time::Duration>,
    until_done: bool,
) -> Result<(), StorageError> {
    let due = now_ns().saturating_add(delay.map_or(0, |delay| {
        i64::try_from(delay.as_nanos()).unwrap_or(i64::MAX)
    }));
    let mut task = NewTask::builder();
    task.originating_message_sequence_id(0)
        .next_attempt_at_ns(due);
    if until_done {
        task.expires_at_ns(i64::MAX)
            .max_attempts(i32::MAX)
            .max_backoff_duration_ns(SYNC_UPDATE_INSTALLATIONS_INTERVAL_NS);
    } else {
        // The usual three days to succeed count from when the task is due.
        task.expires_at_ns(due.saturating_add(NS_IN_DAY * 3));
    }
    let task = task.build(TaskProto {
        task: Some(TaskKindProto::AddMissingInstallations(
            AddMissingInstallationsProto {
                group_id: group_id.to_vec(),
            },
        )),
    })?;
    db.create_or_ignore_task(task)?;
    events.emit(None, Some(InternalEvent::TaskScheduled));
    Ok(())
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
    let payload = SyncPayload::decode(decode_buffer(plaintext)).ok()?;
    decode_supported_content(&payload.content)
}

/// A copy of `bytes` for prost to decode. prost decodes a bytes field of a
/// `&[u8]` through a temporary copy that it frees unwiped; from `Bytes` it
/// takes a slice instead. This copy is wiped on drop.
fn decode_buffer(bytes: &[u8]) -> bytes::Bytes {
    bytes::Bytes::from_owner(Zeroizing::new(bytes.to_vec()))
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
    DeviceSyncContentProto::decode(decode_buffer(bytes))
        .ok()?
        .content
        .map(ReceivedContent)
}
