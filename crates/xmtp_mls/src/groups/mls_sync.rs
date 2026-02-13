use super::{
    GroupError, HmacKey, MlsGroup, build_extensions_for_admin_lists_update,
    build_extensions_for_membership_update, build_extensions_for_metadata_update,
    build_extensions_for_permissions_update,
    group_permissions::extract_group_permissions,
    intents::{
        CommitPendingProposalsIntentData, Installation, IntentError, PostCommitAction,
        ProposeGroupContextExtensionsIntentData, ProposeMemberUpdateIntentData,
        SendMessageIntentData, SendWelcomesAction, UpdateAdminListIntentData,
        UpdateGroupMembershipIntentData, UpdatePermissionIntentData,
    },
    summary::{MessageIdentifier, MessageIdentifierBuilder, ProcessSummary, SyncSummary},
    validated_commit::{
        CommitValidationError, LibXMTPVersion, extract_group_membership, validate_proposal,
    },
};
use crate::{
    client::ClientError,
    context::XmtpSharedContext,
    groups::{
        group_membership::{GroupMembership, MembershipDiffWithKeyPackages},
        intents::{QueueIntent, ReaddInstallationsIntentData, UpdateMetadataIntentData},
        mls_ext::{CommitLogStorer, MlsGroupReload, WrapWelcomeError, wrap_welcome},
        mls_sync::{
            GroupMessageProcessingError::OpenMlsProcessMessage,
            update_group_membership::apply_readd_installations_intent,
        },
        validated_commit::{Inbox, MutableMetadataValidationInfo, ValidatedCommit},
    },
    identity::{IdentityError, parse_credential},
    identity_updates::{IdentityUpdates, load_identity_updates},
    intents::ProcessIntentError,
    messages::{decoded_message::MessageBody, enrichment::EnrichMessageError},
    mls_store::MlsStore,
    subscriptions::{LocalEvents, SyncWorkerEvent},
    traits::IntoWith,
    utils::{
        self,
        hash::sha256,
        id::{calculate_message_id, calculate_message_id_for_intent},
        time::hmac_epoch,
    },
};
use futures::future::try_join_all;
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use openmls::group::{ProcessMessageError, ValidationError};
use openmls::prelude::BasicCredentialError;
use openmls::{
    credentials::BasicCredential,
    extensions::Extensions,
    framing::ProtocolMessage,
    group::{GroupContext, GroupEpoch, StagedCommit},
    key_packages::KeyPackage,
    messages::proposals::Proposal,
    prelude::{
        ExtensionType, LeafNodeIndex, MlsGroup as OpenMlsGroup, ProcessedMessage,
        ProcessedMessageContent, Sender,
        tls_codec::{Error as TlsCodecError, Serialize},
    },
    treesync::LeafNodeParameters,
};
use openmls_traits::OpenMlsProvider;
use prost::Message;
use prost::bytes::Bytes;
use sha2::Sha256;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    mem::{Discriminant, discriminant},
    ops::RangeInclusive,
    time::Duration,
};
use thiserror::Error;
use tracing::debug;
use update_group_membership::apply_update_group_membership_intent;
use xmtp_common::{
    Event, ExponentialBackoff, Retry, RetryableError, Strategy, log_event, retry_async,
    time::now_ns,
};
use xmtp_configuration::{
    GRPC_PAYLOAD_LIMIT, HMAC_SALT, MAX_GROUP_SIZE, MAX_GROUP_SYNC_RETRIES,
    MAX_INTENT_PUBLISH_ATTEMPTS, MAX_PAST_EPOCHS, PROPOSAL_SUPPORT_EXTENSION_ID,
    SYNC_BACKOFF_TOTAL_WAIT_MAX_SECS, SYNC_BACKOFF_WAIT_MS, SYNC_JITTER_MS,
    SYNC_UPDATE_INSTALLATIONS_INTERVAL_NS,
};
use xmtp_content_types::{CodecError, ContentCodec, group_updated::GroupUpdatedCodec};
use xmtp_db::message_deletion::{QueryMessageDeletion, StoredMessageDeletion};
use xmtp_db::{
    Fetch, MlsProviderExt, StorageError, StoreOrIgnore,
    group::{ConversationType, StoredGroup},
    group_intent::{ID, IntentKind, IntentState, StoredGroupIntent},
    group_message::{ContentType, DeliveryStatus, GroupMessageKind, StoredGroupMessage},
    remote_commit_log::CommitResult,
    sql_key_store,
    user_preferences::StoredUserPreferences,
};
use xmtp_db::{NotFound, group_intent::IntentKind::MetadataUpdate};
use xmtp_db::{TransactionalKeyStore, XmtpMlsStorageProvider, refresh_state::HasEntityKind};
use xmtp_db::{XmtpOpenMlsProvider, XmtpOpenMlsProviderRef, prelude::*};
use xmtp_db::{group::GroupMembershipState, group_message::Deletable};
use xmtp_db::{
    group_message::MsgQueryArgs,
    pending_remove::{PendingRemove, QueryPendingRemove},
};
use xmtp_id::{InboxId, InboxIdRef};
use xmtp_mls_common::group_metadata::extract_group_metadata;
use xmtp_mls_common::group_mutable_metadata::{MetadataField, extract_group_mutable_metadata};
use xmtp_proto::xmtp::mls::message_contents::content_types::DeleteMessage;
use xmtp_proto::xmtp::mls::{
    api::v1::{
        GroupMessageInput, WelcomeMessageInput, WelcomeMetadata,
        group_message_input::{V1 as GroupMessageInputV1, Version as GroupMessageInputVersion},
        welcome_message_input::{
            V1 as WelcomeMessageInputV1, Version as WelcomeMessageInputVersion,
            WelcomePointer as WelcomePointerInput,
        },
    },
    message_contents::{
        GroupUpdated, PlaintextEnvelope, WelcomePointer as WelcomePointerProto, group_updated,
        plaintext_envelope::{Content, V1, V2},
    },
};
use xmtp_proto::{
    GroupUpdateDeduper,
    types::{Cursor, GroupMessage},
};
use xmtp_proto::{ShortHex, xmtp::mls::message_contents::EncodedContent};
use zeroize::Zeroizing;

pub mod update_group_membership;

#[derive(Debug, Error)]
pub enum GroupMessageProcessingError {
    #[error("intent already processed")]
    IntentAlreadyProcessed,
    #[error("message with cursor [{}] for group [{}] already processed", _0.cursor, xmtp_common::fmt::debug_hex(&_0.group_id)
    )]
    MessageAlreadyProcessed(MessageIdentifier),
    #[error("message identifier not found")]
    MessageIdentifierNotFound,
    #[error("welcome with cursor [{0}] already processed")]
    WelcomeAlreadyProcessed(u64),
    #[error("[{message_time_ns:?}] invalid sender with credential: {credential:?}")]
    InvalidSender {
        message_time_ns: u64,
        credential: Vec<u8>,
    },
    #[error("invalid payload")]
    InvalidPayload,
    #[error("storage error: {0}")]
    Storage(#[from] xmtp_db::StorageError),
    #[error(transparent)]
    Identity(#[from] IdentityError),
    #[error("openmls process message error: {0}")]
    OpenMlsProcessMessage(
        #[from] openmls::prelude::ProcessMessageError<sql_key_store::SqlKeyStoreError>,
    ),
    #[error("merge staged commit: {0}")]
    MergeStagedCommit(#[from] openmls::group::MergeCommitError<sql_key_store::SqlKeyStoreError>),
    #[error("TLS Codec error: {0}")]
    TlsError(#[from] TlsCodecError),
    #[error("unsupported message type: {0:?}")]
    UnsupportedMessageType(Discriminant<ProtocolMessage>),
    #[error("commit validation")]
    CommitValidation(#[from] CommitValidationError),
    #[error("epoch increment not allowed")]
    EpochIncrementNotAllowed,
    #[error("clear pending commit error: {0}")]
    ClearPendingCommit(#[from] sql_key_store::SqlKeyStoreError),
    #[error("Serialization/Deserialization Error {0}")]
    Serde(#[from] serde_json::Error),
    #[error("intent is missing staged_commit field")]
    IntentMissingStagedCommit,
    #[error("encode proto: {0}")]
    EncodeProto(#[from] prost::EncodeError),
    #[error("proto decode error: {0}")]
    DecodeProto(#[from] prost::DecodeError),
    #[error(transparent)]
    Intent(#[from] IntentError),
    #[error(transparent)]
    Codec(#[from] CodecError),
    #[error("wrong credential type")]
    WrongCredentialType(#[from] BasicCredentialError),
    #[error(transparent)]
    ProcessIntent(#[from] ProcessIntentError),
    #[error(transparent)]
    AssociationDeserialization(#[from] xmtp_id::associations::DeserializationError),
    #[error(transparent)]
    Client(#[from] ClientError),
    #[error("Group paused due to minimum protocol version requirement")]
    GroupPaused,
    #[error("Message epoch [{0}] is too old [{1}]")]
    OldEpoch(u64, u64),
    #[error("Message epoch [{0}] is greater than group epoch [{1}]")]
    FutureEpoch(u64, u64),
    #[error(transparent)]
    Db(#[from] xmtp_db::ConnectionError),
    #[error(transparent)]
    Builder(#[from] derive_builder::UninitializedFieldError),
    #[error(transparent)]
    Diesel(#[from] xmtp_db::diesel::result::Error),
    #[error(transparent)]
    EnrichMessage(#[from] EnrichMessageError),
}

impl RetryableError for GroupMessageProcessingError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Storage(err) => err.is_retryable(),
            Self::Diesel(err) => err.is_retryable(),
            Self::Identity(err) => err.is_retryable(),
            Self::OpenMlsProcessMessage(err) => err.is_retryable(),
            Self::MergeStagedCommit(err) => err.is_retryable(),
            Self::ProcessIntent(err) => err.is_retryable(),
            Self::CommitValidation(err) => err.is_retryable(),
            Self::ClearPendingCommit(err) => err.is_retryable(),
            Self::Client(err) => err.is_retryable(),
            Self::Db(e) => e.is_retryable(),
            Self::EnrichMessage(e) => e.is_retryable(),
            Self::IntentAlreadyProcessed
            | Self::MessageIdentifierNotFound
            | Self::WrongCredentialType(_)
            | Self::Codec(_)
            | Self::MessageAlreadyProcessed(_)
            | Self::WelcomeAlreadyProcessed(_)
            | Self::InvalidSender { .. }
            | Self::DecodeProto(_)
            | Self::InvalidPayload
            | Self::Intent(_)
            | Self::EpochIncrementNotAllowed
            | Self::EncodeProto(_)
            | Self::IntentMissingStagedCommit
            | Self::Serde(_)
            | Self::AssociationDeserialization(_)
            | Self::TlsError(_)
            | Self::UnsupportedMessageType(_)
            | Self::GroupPaused
            | Self::FutureEpoch(_, _)
            | Self::OldEpoch(_, _) => false,
            Self::Builder(_) => false,
        }
    }
}

impl GroupMessageProcessingError {
    pub(crate) fn commit_result(&self) -> CommitResult {
        match self {
            GroupMessageProcessingError::OpenMlsProcessMessage(
                ProcessMessageError::ValidationError(ValidationError::WrongEpoch),
            ) => CommitResult::WrongEpoch,
            GroupMessageProcessingError::OldEpoch(_, _) => CommitResult::WrongEpoch,
            GroupMessageProcessingError::FutureEpoch(_, _) => CommitResult::WrongEpoch,
            GroupMessageProcessingError::CommitValidation(_) => CommitResult::Invalid,
            GroupMessageProcessingError::OpenMlsProcessMessage(_) => CommitResult::Undecryptable,
            _ => CommitResult::Unknown,
        }
    }
}

#[derive(Debug, Error)]
pub struct IntentResolutionError {
    processing_error: GroupMessageProcessingError,
    // The next intent state to transition to, if the error is non-retriable.
    // Should not be used for retryable errors.
    next_intent_state: IntentState,
}

impl std::fmt::Display for IntentResolutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "IntentValidationError: {}", self.processing_error)
    }
}

impl RetryableError for IntentResolutionError {
    fn is_retryable(&self) -> bool {
        self.processing_error.is_retryable()
    }
}

#[derive(Debug)]
pub(crate) struct PublishIntentData {
    staged_commit: Option<Vec<u8>>,
    post_commit_action: Option<Vec<u8>>,
    /// One or more payloads to publish. Most intents have a single payload (commit or message),
    /// but proposal intents may have multiple payloads (one per proposal).
    payloads_to_publish: Vec<Vec<u8>>,
    should_send_push_notification: bool,
    group_epoch: u64,
}

#[cfg(any(test, feature = "test-utils"))]
impl PublishIntentData {
    #[allow(dead_code)]
    pub fn post_commit_data(&self) -> Option<Vec<u8>> {
        self.post_commit_action.clone()
    }

    #[allow(dead_code)]
    pub fn staged_commit(&self) -> Option<Vec<u8>> {
        self.staged_commit.clone()
    }
}

impl<Context> MlsGroup<Context>
where
    Context: XmtpSharedContext,
{
    #[tracing::instrument]
    pub async fn sync(&self) -> Result<SyncSummary, GroupError> {
        let conn = self.context.db();

        let epoch = self.epoch().await?;
        tracing::info!(
            inbox_id = self.context.inbox_id(),
            installation_id = %self.context.installation_id(),
            group_id = self.group_id.short_hex(),
            "[{}] syncing group, epoch = {epoch}",
            self.context.inbox_id(),
        );

        // Also sync the "stitched DMs", if any...
        for other_dm in conn.other_dms(&self.group_id)? {
            let other_dm = Self::new_from_arc(
                self.context.clone(),
                other_dm.id,
                other_dm.dm_id.clone(),
                other_dm.conversation_type,
                other_dm.created_at_ns,
            );

            other_dm.sync_with_conn().await?;
            other_dm.maybe_update_installations(None).await?;
        }

        let sync_summary = self.sync_with_conn().await.map_err(GroupError::from)?;
        self.maybe_update_installations(None).await?;
        Ok(sync_summary)
    }

    fn handle_group_paused(&self) -> Result<(), GroupError> {
        // Check if group is paused and try to unpause if version requirements are met
        if let Some(required_min_version_str) =
            self.context.db().get_group_paused_version(&self.group_id)?
        {
            tracing::info!(
                "Group is paused until version: {}",
                required_min_version_str
            );
            let current_version_str = self.context.version_info().pkg_version();
            let current_version = LibXMTPVersion::parse(current_version_str)?;
            let required_min_version = LibXMTPVersion::parse(&required_min_version_str)?;

            if required_min_version <= current_version {
                tracing::info!(
                    "Unpausing group since version requirements are met. \
                     Group ID: {}",
                    hex::encode(&self.group_id),
                );
                self.context.db().unpause_group(&self.group_id)?;
            } else {
                tracing::warn!(
                    "Skipping sync for paused group since version requirements are not met. \
                    Group ID: {}, \
                    Required version: {}, \
                    Current version: {}",
                    hex::encode(&self.group_id),
                    required_min_version_str,
                    current_version_str
                );
                // Skip sync for paused groups
                return Err(GroupError::GroupPausedUntilUpdate(required_min_version_str));
            }
        }
        Ok(())
    }

    /// Sync from the network with the 'conn' (local database).
    /// must return a summary of all messages synced, whether they were
    /// successful or not.
    #[cfg_attr(any(test, feature = "test-utils"), tracing::instrument(fields(who = %self.context.inbox_id())))]
    #[cfg_attr(not(any(test, feature = "test-utils")), tracing::instrument(skip_all))]
    pub async fn sync_with_conn(&self) -> Result<SyncSummary, SyncSummary> {
        let _mutex = self.mutex.lock().await;
        let mut summary = SyncSummary::default();

        if !self.is_active().map_err(SyncSummary::other)? {
            log_event!(
                Event::GroupSyncGroupInactive,
                self.context.installation_id(),
                group_id = self.group_id
            );
            return Err(SyncSummary::other(GroupError::GroupInactive));
        }

        if let Err(e) = self.handle_group_paused() {
            if matches!(e, GroupError::GroupPausedUntilUpdate(_)) {
                // nothing synced
                return Ok(summary);
            } else {
                return Err(SyncSummary::other(e));
            }
        }

        // Even if publish fails, continue to receiving
        let result = self.publish_intents().await;
        if let Err(e) = result {
            tracing::error!("Sync: error publishing intents {e:?}",);
            summary.add_publish_err(e);
        }

        // Even if receiving fails, we continue to post_commit
        // Errors are collected in the summary.
        let result = self.receive().await;
        match result {
            Ok(s) => summary.add_process(s),
            Err(e) => {
                summary.add_other(e);
                // We don't return an error if receive fails, because it's possible this is caused
                // by malicious data sent over the network, or messages from before the user was
                // added to the group
            }
        }

        let result = self.post_commit().await;
        if let Err(e) = result {
            tracing::error!("post commit error {e:?}",);
            summary.add_post_commit_err(e);
        }

        if summary.is_errored() {
            Err(summary)
        } else {
            Ok(summary)
        }
    }

    #[cfg_attr(any(test, feature = "test-utils"), tracing::instrument(level = "info", fields(who = %self.context.inbox_id()), skip_all))]
    #[cfg_attr(
        not(any(test, feature = "test-utils")),
        tracing::instrument(level = "trace", skip_all)
    )]
    pub(super) async fn sync_until_last_intent_resolved(&self) -> Result<SyncSummary, GroupError> {
        let intents = self.context.db().find_group_intents(
            self.group_id.clone(),
            Some(vec![IntentState::ToPublish, IntentState::Published]),
            None,
        )?;

        let Some(intent) = intents.last() else {
            return Ok(Default::default());
        };

        self.sync_until_intent_resolved(intent.id).await
    }

    /**
     * Sync the group and wait for the intent to be deleted
     * Group syncing may involve picking up messages unrelated to the intent, so simply checking for errors
     * does not give a clear signal as to whether the intent was successfully completed or not.
     *
     * This method will retry up to `xmtp_configuration::MAX_GROUP_SYNC_RETRIES` times.
     */
    #[cfg_attr(any(test, feature = "test-utils"), tracing::instrument(level = "info", fields(who = %self.context.inbox_id()), skip(self)))]
    #[cfg_attr(
        not(any(test, feature = "test-utils")),
        tracing::instrument(level = "trace", skip(self))
    )]
    pub(super) async fn sync_until_intent_resolved(
        &self,
        intent_id: ID,
    ) -> Result<SyncSummary, GroupError> {
        log_event!(
            Event::GroupSyncStart,
            self.context.installation_id(),
            group_id = self.group_id
        );

        let result = self.sync_until_intent_resolved_inner(intent_id).await;
        let summary = match &result {
            Ok(summary) => Some(summary),
            Err(GroupError::Sync(summary)) => Some(&**summary),
            Err(GroupError::SyncFailedToWait(summary)) => Some(&**summary),
            _ => None,
        };

        log_event!(
            Event::GroupSyncFinished,
            self.context.installation_id(),
            group_id = self.group_id,
            summary = ?summary,
            success = result.is_ok()
        );

        result
    }

    #[cfg_attr(any(test, feature = "test-utils"), tracing::instrument(level = "info", fields(who = %self.context.inbox_id()), skip(self)))]
    #[cfg_attr(
        not(any(test, feature = "test-utils")),
        tracing::instrument(level = "trace", skip(self))
    )]
    async fn sync_until_intent_resolved_inner(
        &self,
        intent_id: ID,
    ) -> Result<SyncSummary, GroupError> {
        let mut summary = SyncSummary::default();
        let db = self.context.db();

        let time_spent = xmtp_common::time::Instant::now();
        let backoff = ExponentialBackoff::builder()
            .duration(Duration::from_millis(SYNC_BACKOFF_WAIT_MS.into()))
            .total_wait_max(Duration::from_secs(SYNC_BACKOFF_TOTAL_WAIT_MAX_SECS.into()))
            .max_jitter(Duration::from_millis(SYNC_JITTER_MS.into()))
            .build();

        // Return the last error to the caller if we fail to sync
        for attempt in 0..MAX_GROUP_SYNC_RETRIES {
            let wait_for = backoff
                .backoff(attempt + 1, time_spent)
                .unwrap_or(Duration::from_millis(50));

            log_event!(
                Event::GroupSyncAttempt,
                self.context.installation_id(),
                group_id = self.group_id,
                attempt,
                backoff = ?wait_for
            );

            match self.sync_with_conn().await {
                Ok(s) => summary.extend(s),
                Err(s) => {
                    tracing::error!("error syncing group {s}");
                    summary.extend(s);
                }
            }
            match Fetch::<StoredGroupIntent>::fetch(&db, &intent_id) {
                Ok(Some(StoredGroupIntent {
                    state: IntentState::Processed,
                    ..
                })) => {
                    // This is expected, we mark intents as processed on success.
                    return Ok(summary);
                }
                Ok(None) => {
                    // This is somewhat expected, we used to delete intents on success.
                    tracing::warn!(
                        "Intent was deleted when it should have been marked as processed.\
                         This is still okay, but unexpected. intent_id: {intent_id}",
                    );
                    return Ok(summary);
                }

                Ok(Some(StoredGroupIntent {
                    state: IntentState::Error,
                    kind,
                    ..
                })) => {
                    log_event!(
                        Event::GroupSyncIntentErrored,
                        self.context.installation_id(),
                        level = warn,
                        group_id = self.group_id, intent_id = intent_id,
                        summary = ?summary, intent_kind = ?kind
                    );
                    return Err(GroupError::from(summary));
                }
                Ok(Some(StoredGroupIntent { state, kind, .. })) => {
                    log_event!(
                        Event::GroupSyncIntentRetry,
                        self.context.installation_id(),
                        level = warn, group_id = self.group_id,
                        intent_id = intent_id, state = ?state, intent_kind = ?kind
                    );
                }
                Err(err) => {
                    tracing::error!("database error fetching intent {err:?}");
                    summary.add_other(GroupError::Storage(err));
                }
            };
            if attempt + 1 < MAX_GROUP_SYNC_RETRIES {
                xmtp_common::time::sleep(wait_for).await;
            }
        }
        Err(GroupError::SyncFailedToWait(Box::new(summary)))
    }

    fn validate_message_epoch(
        inbox_id: InboxIdRef<'_>,
        intent_id: i32,
        group_epoch: GroupEpoch,
        message_epoch: GroupEpoch,
        max_past_epochs: usize,
    ) -> Result<(), GroupMessageProcessingError> {
        #[cfg(any(test, feature = "test-utils"))]
        utils::test_mocks_helpers::maybe_mock_future_epoch_for_tests()?;

        if message_epoch.as_u64() + max_past_epochs as u64 <= group_epoch.as_u64() {
            tracing::warn!(
                inbox_id,
                message_epoch = message_epoch.as_u64(),
                group_epoch = group_epoch.as_u64(),
                intent_id,
                "[{}] message epoch {} is {} or more less than the group epoch {} for intent {}. Retrying message",
                inbox_id,
                message_epoch,
                max_past_epochs,
                group_epoch.as_u64(),
                intent_id
            );
            return Err(GroupMessageProcessingError::OldEpoch(
                message_epoch.as_u64(),
                group_epoch.as_u64(),
            ));
        } else if message_epoch.as_u64() > group_epoch.as_u64() {
            // Should not happen, logging proactively
            tracing::error!(
                inbox_id,
                message_epoch = message_epoch.as_u64(),
                group_epoch = group_epoch.as_u64(),
                intent_id,
                "[{}] message epoch {} is greater than group epoch {} for intent {}. Retrying message",
                inbox_id,
                message_epoch,
                group_epoch,
                intent_id
            );
            return Err(GroupMessageProcessingError::FutureEpoch(
                message_epoch.as_u64(),
                group_epoch.as_u64(),
            ));
        }
        Ok(())
    }

    // This function is intended to isolate the async validation code to
    // validate the message and prepare it for database insertion synchronously.
    async fn stage_and_validate_intent(
        &self,
        mls_group: &openmls::group::MlsGroup,
        intent: &StoredGroupIntent,
        envelope: &GroupMessage,
    ) -> Result<Option<(StagedCommit, ValidatedCommit)>, IntentResolutionError> {
        let GroupMessage {
            message, cursor, ..
        } = &envelope;
        let group_epoch = mls_group.epoch();
        let message_epoch = message.epoch();

        match intent.kind {
            IntentKind::KeyUpdate
            | IntentKind::UpdateGroupMembership
            | IntentKind::UpdateAdminList
            | IntentKind::MetadataUpdate
            | IntentKind::UpdatePermission
            | IntentKind::ReaddInstallations
            | IntentKind::CommitPendingProposals => {
                if let Some(published_in_epoch) = intent.published_in_epoch {
                    let group_epoch = group_epoch.as_u64() as i64;
                    let message_epoch = message_epoch.as_u64() as i64;

                    // TODO(rich): Merge into validate_message_epoch()
                    if message_epoch != group_epoch {
                        tracing::warn!(
                            inbox_id = self.context.inbox_id(),
                            installation_id = %self.context.installation_id(),
                            group_id = hex::encode(&self.group_id),
                            cursor = %cursor,
                            intent.id,
                            intent.kind = %intent.kind,
                            "Intent for msg = [{cursor}] was published in epoch {} with local save intent epoch of {} but group is currently in epoch {}",
                            message_epoch,
                            published_in_epoch,
                            group_epoch
                        );
                        let processing_error = if message_epoch < group_epoch {
                            GroupMessageProcessingError::OldEpoch(
                                message_epoch as u64,
                                group_epoch as u64,
                            )
                        } else {
                            GroupMessageProcessingError::FutureEpoch(
                                message_epoch as u64,
                                group_epoch as u64,
                            )
                        };

                        return Err(IntentResolutionError {
                            processing_error,
                            next_intent_state: IntentState::ToPublish,
                        });
                    }

                    let staged_commit = intent
                        .staged_commit
                        .as_ref()
                        .map_or(
                            Err(GroupMessageProcessingError::IntentMissingStagedCommit),
                            |staged_commit| decode_staged_commit(staged_commit),
                        )
                        .map_err(|err| {
                            // If we can't retrieve the cached staged commit from the intent, we can't
                            // apply it. It is indeterminate whether other members were able to apply it
                            // or not - if they did apply it, then we are forked.
                            tracing::error!(
                                inbox_id = self.context.inbox_id(),
                                installation_id = %self.context.installation_id(),
                                group_id = hex::encode(&self.group_id),
                                cursor = %cursor,
                                intent_id = intent.id,
                                intent.kind = %intent.kind,
                                "Error decoding staged commit for intent, now may be forked: {err:?}",
                            );
                            IntentResolutionError {
                                processing_error: err,
                                next_intent_state: IntentState::Error,
                            }
                        })?;

                    tracing::info!(
                        "[{}] Validating commit for intent {}. Message timestamp: ({})/{}",
                        self.context.inbox_id(),
                        intent.id,
                        envelope.timestamp(),
                        envelope.created_ns
                    );

                    let maybe_validated_commit = ValidatedCommit::from_staged_commit(
                        &self.context,
                        &staged_commit,
                        mls_group,
                    )
                    .await;

                    let validated_commit = match maybe_validated_commit {
                        Err(err) => {
                            tracing::error!(
                                inbox_id = self.context.inbox_id(),
                                installation_id = %self.context.installation_id(),
                                group_id = hex::encode(&self.group_id),
                                cursor = %cursor,
                                intent.id,
                                intent.kind = %intent.kind,
                                "Error validating commit for own message. Intent ID [{}]: {err:?}",
                                intent.id,
                            );
                            return Err(IntentResolutionError {
                                processing_error: GroupMessageProcessingError::CommitValidation(
                                    err,
                                ),
                                next_intent_state: IntentState::Error,
                            });
                        }
                        Ok(validated_commit) => validated_commit,
                    };

                    return Ok(Some((staged_commit, validated_commit)));
                }
            }

            IntentKind::SendMessage
            | IntentKind::ProposeMemberUpdate
            | IntentKind::ProposeGroupContextExtensions => {
                // Proposals and messages don't produce commits, just validate epoch
                Self::validate_message_epoch(
                    self.context.inbox_id(),
                    intent.id,
                    group_epoch,
                    message_epoch,
                    MAX_PAST_EPOCHS,
                )
                .map_err(|err| IntentResolutionError {
                    processing_error: err,
                    next_intent_state: IntentState::ToPublish,
                })?;
            }
        }

        Ok(None)
    }

    // Applies the message/commit to the mls group. If it was successfully applied, return Ok(()),
    // so that the caller can mark the intent as committed.
    // If any error occurs, return an IntentResolutionError with the error, and the next intent state
    // to use in the event the error is non-retriable.
    #[allow(clippy::too_many_arguments)]
    #[tracing::instrument(level = "trace", skip_all)]
    fn process_own_message(
        &self,
        mls_group: &mut OpenMlsGroup,
        commit: Option<(StagedCommit, ValidatedCommit)>,
        intent: &StoredGroupIntent,
        envelope: &GroupMessage,
        storage: &impl XmtpMlsStorageProvider,
    ) -> Result<Option<Vec<u8>>, IntentResolutionError> {
        if intent.state == IntentState::Committed
            || intent.state == IntentState::Processed
            || intent.state == IntentState::Error
        {
            tracing::warn!(
                "Skipping already processed intent {} of kind {} because it is in state {:?}",
                intent.id,
                intent.kind,
                intent.state
            );
            return Err(IntentResolutionError {
                processing_error: GroupMessageProcessingError::IntentAlreadyProcessed,
                next_intent_state: intent.state,
            });
        }

        let message_epoch = envelope.message.epoch();
        let GroupMessage { cursor, .. } = envelope;
        let envelope_timestamp_ns = envelope.timestamp();

        tracing::debug!(
            inbox_id = self.context.inbox_id(),
            installation_id = %self.context.installation_id(),
            group_id = hex::encode(&self.group_id),
            cursor = %cursor,
            intent.id,
            intent.kind = %intent.kind,
            "[{}]-[{}] processing own message for intent {} / {}, message_epoch: {}",
            self.context.inbox_id(),
            hex::encode(self.group_id.clone()),
            intent.id,
            intent.kind,
            message_epoch.clone()
        );

        if let Some((staged_commit, validated_commit)) = commit {
            tracing::info!(
                "[{}] merging pending commit for intent {}",
                self.context.inbox_id(),
                intent.id
            );

            if let Err(err) = mls_group.merge_staged_commit_logged(
                &XmtpOpenMlsProviderRef::new(storage),
                staged_commit,
                &validated_commit,
                cursor.sequence_id as i64,
            ) {
                tracing::error!("error merging commit: {err}");
                return Err(IntentResolutionError {
                    processing_error: err,
                    // If the error is non-retriable, it means the commit failed to apply due to some
                    // issue with the commit (e.g. encryption problem). We reset the intent state to
                    // ToPublish so that we can republish it.
                    next_intent_state: IntentState::ToPublish,
                });
            }
            Self::mark_readd_requests_as_responded(
                storage,
                &self.group_id,
                &validated_commit.readded_installations,
                cursor.sequence_id as i64,
            )
            .map_err(|err| IntentResolutionError {
                processing_error: err.into(),
                next_intent_state: IntentState::Error,
            })?;

            // If no error committing the change, write a transcript message
            let msg = self
                .save_transcript_message(
                    validated_commit.clone(),
                    envelope_timestamp_ns as u64,
                    *cursor,
                    storage,
                )
                .map_err(|err| IntentResolutionError {
                    processing_error: err,
                    // If it is a non-retriable error, the commit will be applied, but the transcript message
                    // will be missing. We mark the intent state as errored and continue.
                    next_intent_state: IntentState::Error,
                })?;

            // Clean up pending_remove list for removed members
            self.clean_pending_remove_list(storage, &validated_commit.removed_inboxes);

            // Handle super_admin status changes
            self.handle_super_admin_status_change(
                storage,
                mls_group,
                &validated_commit.metadata_validation_info,
            );

            if let Some((_, payload)) = &msg {
                log_event!(
                    Event::MLSProcessedStagedCommit,
                    self.context.installation_id(),
                    group_id = self.group_id,
                    epoch = mls_group.epoch().as_u64(),
                    actor_installation_id = validated_commit.actor.installation_id,
                    added_inboxes = $payload.added_inboxes,
                    removed_inboxes = $payload.removed_inboxes,
                    left_inboxes = $payload.left_inboxes,
                    metadata_changes = $payload.metadata_field_changes
                );
            }

            return Ok(msg.map(|(m, _)| m.id));
        }

        let id: Option<Vec<u8>> = calculate_message_id_for_intent(intent)
            .map_err(GroupMessageProcessingError::Intent)
            .map_err(|err| {
                if !err.is_retryable() {
                    tracing::error!(
                        "Message identifier not found for intent {} with kind {}, {err:?}",
                        intent.id,
                        intent.kind
                    );
                }
                IntentResolutionError {
                    processing_error: err,
                    // If the error is non-retriable, it means that the optimistic message (which is already in
                    // the db) will never have its delivery status updated to published. We mark the intent state
                    // as errored and continue.
                    next_intent_state: IntentState::Error,
                }
            })?;
        let Some(id) = id else {
            // The message is likely to be a legacy envelope, probably from legacy device sync.
            // We don't need to set the delivery status for these.
            return Ok(None);
        };
        tracing::debug!("setting message @cursor=[{}] to published", envelope.cursor);
        let message_expire_at_ns = Self::get_message_expire_at_ns(mls_group);
        storage
            .db()
            .set_delivery_status_to_published(
                &id,
                envelope_timestamp_ns as u64,
                envelope.cursor,
                message_expire_at_ns,
            )
            .map_err(|err| IntentResolutionError {
                processing_error: GroupMessageProcessingError::Db(err),
                next_intent_state: IntentState::Error,
            })?;
        self.process_own_leave_request_message(mls_group, storage, &id);
        self.process_own_delete_message(storage, &id);
        Ok(Some(id))
    }

    #[tracing::instrument(level = "trace", skip(mls_group, envelope))]
    async fn validate_and_process_external_message(
        &self,
        mls_group: &mut OpenMlsGroup,
        envelope: &GroupMessage,
        allow_cursor_increment: bool,
    ) -> Result<MessageIdentifier, GroupMessageProcessingError> {
        #[cfg(any(test, feature = "test-utils"))]
        {
            use crate::utils::test_mocks_helpers::maybe_mock_wrong_epoch_for_tests;
            maybe_mock_wrong_epoch_for_tests()?;
        }

        let provider = self.context.mls_provider();

        let GroupMessage {
            cursor, message, ..
        } = envelope;
        let envelope_timestamp_ns = envelope.timestamp();
        let mut identifier = MessageIdentifierBuilder::from(envelope);

        // We need to process the message twice to avoid an async transaction.
        // We'll process for the first time, get the processed message,
        // and roll the transaction back, so we can fetch updates from the server before
        // being ready to process the message for a second time.
        let mut processed_message = None;
        let result = provider.key_store().transaction(|conn| {
            let storage = conn.key_store();
            let provider = XmtpOpenMlsProvider::new(storage);
            processed_message = Some(mls_group.process_message(&provider, message.clone()));
            // Rollback the transaction. We want to synchronize with the server before committing.
            Err::<(), StorageError>(StorageError::IntentionalRollback)
        });
        if !matches!(result, Err(StorageError::IntentionalRollback)) {
            result.inspect_err(|e| tracing::debug!("immutable process message failed {}", e))?;
        }
        let processed_message = processed_message.expect("Was just set to Some")?;

        // Reload the mlsgroup to clear the it's internal cache
        mls_group.reload(provider.storage())?;

        let (sender_inbox_id, sender_installation_id) =
            extract_message_sender(mls_group, &processed_message, envelope_timestamp_ns as u64)?;

        tracing::info!(
            inbox_id = self.context.inbox_id(),
            installation_id = %self.context.installation_id(),sender_inbox_id = sender_inbox_id,
            sender_installation_id = hex::encode(&sender_installation_id),
            group_id = hex::encode(&self.group_id),
            current_epoch = mls_group.epoch().as_u64(),
            msg_epoch = processed_message.epoch().as_u64(),
            msg_group_id = hex::encode(processed_message.group_id().as_slice()),
            cursor = %cursor,
            "[{}] extracted sender inbox id: {}",
            self.context.inbox_id(),
            sender_inbox_id
        );

        let validated_commit = match &processed_message.content() {
            ProcessedMessageContent::StagedCommitMessage(staged_commit) => {
                let result =
                    ValidatedCommit::from_staged_commit(&self.context, staged_commit, mls_group)
                        .await;

                let validated_commit = match result {
                    Err(e) if !e.is_retryable() => {
                        match &e {
                            CommitValidationError::ProtocolVersionTooLow(_) => {}
                            _ => {
                                self.maybe_update_cursor(&self.context.db(), envelope)?;
                            }
                        };

                        Err(e)
                    }
                    v => v,
                }?;

                identifier.group_context(staged_commit.group_context().clone());
                Some(validated_commit)
            }
            ProcessedMessageContent::ProposalMessage(queued_proposal) => {
                // Validate the proposal before processing it
                // This ensures that when we later commit pending proposals, they will succeed
                let extensions = mls_group.extensions();
                let policy_set =
                    extract_group_permissions(mls_group).map_err(CommitValidationError::from)?;
                let immutable_metadata =
                    extract_group_metadata(extensions).map_err(CommitValidationError::from)?;
                let mutable_metadata = extract_group_mutable_metadata(mls_group)
                    .map_err(CommitValidationError::from)?;

                let validation_result = validate_proposal(
                    queued_proposal,
                    mls_group,
                    &policy_set.policies,
                    &immutable_metadata,
                    &mutable_metadata,
                );

                if let Err(e) = validation_result {
                    tracing::warn!(
                        inbox_id = self.context.inbox_id(),
                        installation_id = %self.context.installation_id(),
                        group_id = hex::encode(&self.group_id),
                        proposal_type = ?queued_proposal.proposal().proposal_type(),
                        error = %e,
                        "Received invalid proposal, rejecting"
                    );
                    // Update cursor so we don't reprocess this invalid proposal
                    self.maybe_update_cursor(&self.context.db(), envelope)?;
                    return Err(e.into());
                }

                None
            }
            _ => None,
        };

        let mut deferred_events = DeferredEvents::new();
        let identifier = provider.key_store().transaction(|conn| {
            let storage = conn.key_store();
            let db = storage.db();
            let provider = XmtpOpenMlsProviderRef::new(&storage);
            tracing::debug!(
                inbox_id = self.context.inbox_id(),
                installation_id = %self.context.installation_id(),
                group_id = hex::encode(&self.group_id),
                current_epoch = mls_group.epoch().as_u64(),
                msg_epoch = processed_message.epoch().as_u64(),
                cursor = ?cursor,
                "[{}] processing message in transaction epoch = {}, cursor = {:?}",
                self.context.inbox_id(),
                mls_group.epoch().as_u64(),
                cursor
            );
            let requires_processing = if allow_cursor_increment {
                self.maybe_update_cursor(&db, envelope)?
            } else {
                tracing::info!(
                    "will not call update cursor for group {}, with cursor {}, allow_cursor_increment is false",
                    hex::encode(envelope.group_id.as_slice()),
                    *cursor
                );
                let current_cursor = db
                    .get_last_cursor_for_originator(&envelope.group_id, envelope.entity_kind(), envelope.originator_id())?;
                current_cursor.sequence_id < envelope.cursor.sequence_id
            };
            if !requires_processing {
                // early return if the message is already processed
                // _NOTE_: Not early returning and re-processing a message that
                // has already been processed, has the potential to result in forks.
                tracing::debug!("message @cursor=[{}] for group=[{}] created_at=[{}] no longer require processing, should be available in database",
                    envelope.cursor,
                    xmtp_common::fmt::debug_hex(&envelope.group_id),
                    envelope.created_ns
                 );
                identifier.previously_processed(true);
                return identifier.build();
            }
            // once the checks for processing pass, actually process the message
            let processed_message = mls_group.process_message(&provider, message.clone())?;
            let previous_epoch = mls_group.epoch().as_u64();
            let identifier = self.process_external_message(
                mls_group,
                processed_message,
                envelope,
                validated_commit.clone(),
                &storage,
                &mut deferred_events,
            )?;
            let new_epoch = mls_group.epoch().as_u64();
            if new_epoch > previous_epoch {
                log_event!(
                    Event::MLSGroupEpochUpdated,
                    self.context.installation_id(),
                    group_id = self.group_id,
                    cursor = cursor.sequence_id,
                    originator = cursor.originator_id,
                    epoch = new_epoch,
                    epoch_auth = mls_group.epoch_authenticator().as_slice().short_hex(),
                    previous_epoch
                );
            }
            Ok::<_, GroupMessageProcessingError>(identifier)
        })?;

        // Send all deferred events after the transaction completes
        deferred_events.send_all(&self.context);

        Ok(identifier)
    }

    /// Process an external message
    /// returns a MessageIdentifier, identifying the message processed if any.
    #[tracing::instrument(level = "trace", skip_all)]
    fn process_external_message(
        &self,
        mls_group: &mut OpenMlsGroup,
        processed_message: ProcessedMessage,
        message_envelope: &GroupMessage,
        validated_commit: Option<ValidatedCommit>,
        storage: &impl XmtpMlsStorageProvider,
        deferred_events: &mut DeferredEvents,
    ) -> Result<MessageIdentifier, GroupMessageProcessingError> {
        let GroupMessage { cursor, .. } = &message_envelope;
        let envelope_timestamp_ns = message_envelope.timestamp();
        let msg_epoch = processed_message.epoch().as_u64();
        let msg_group_id = processed_message.group_id().as_slice().to_vec();
        let (sender_inbox_id, sender_installation_id) =
            extract_message_sender(mls_group, &processed_message, envelope_timestamp_ns as u64)?;

        let mut identifier = MessageIdentifierBuilder::from(message_envelope);
        match processed_message.into_content() {
            ProcessedMessageContent::ApplicationMessage(application_message) => {
                log_event!(
                    Event::MLSReceivedApplicationMessage,
                    self.context.installation_id(),
                    inbox_id = self.context.inbox_id(),
                    sender_inbox_id,
                    sender_installation_id,
                    group_id = self.group_id,
                    epoch = mls_group.epoch().as_u64(),
                    msg_epoch,
                    msg_group_id,
                    cursor = %cursor,
                );
                let message_bytes = application_message.into_bytes();

                let mut bytes = Bytes::from(message_bytes);
                let envelope = PlaintextEnvelope::decode(&mut bytes)?;

                match envelope.content {
                    Some(Content::V1(V1 {
                        idempotency_key,
                        content,
                    })) => {
                        let message_id =
                            calculate_message_id(&self.group_id, &content, &idempotency_key);
                        let queryable_content_fields =
                            Self::extract_queryable_content_fields(&content);

                        let message = StoredGroupMessage {
                            id: message_id.clone(),
                            group_id: self.group_id.clone(),
                            decrypted_message_bytes: content,
                            sent_at_ns: envelope_timestamp_ns,
                            kind: GroupMessageKind::Application,
                            sender_installation_id,
                            sender_inbox_id: sender_inbox_id.clone(),
                            delivery_status: DeliveryStatus::Published,
                            content_type: queryable_content_fields.content_type,
                            version_major: queryable_content_fields.version_major,
                            version_minor: queryable_content_fields.version_minor,
                            authority_id: queryable_content_fields.authority_id,
                            reference_id: queryable_content_fields.reference_id,
                            sequence_id: cursor.sequence_id as i64,
                            originator_id: cursor.originator_id as i64,
                            expire_at_ns: Self::get_message_expire_at_ns(mls_group),
                            inserted_at_ns: 0, // Will be set by database
                            should_push: true,
                        };
                        message.store_or_ignore(&storage.db())?;
                        identifier.internal_id(message_id);

                        // If this message was sent by us on another installation, check if it
                        // belongs to a sync group, and if it is - notify the worker.
                        if sender_inbox_id == self.context.inbox_id() {
                            tracing::info!(
                                installation_id = hex::encode(self.context.installation_id()),
                                "new sync group message event"
                            );
                            if let Some(StoredGroup {
                                conversation_type: ConversationType::Sync,
                                ..
                            }) = storage.db().find_group(&self.group_id)?
                            {
                                // Send this event after the transaction completes
                                deferred_events.add_worker_event(SyncWorkerEvent::NewSyncGroupMsg);
                            }
                        }
                        if message.content_type == ContentType::LeaveRequest {
                            self.process_leave_request_message(mls_group, storage, &message)?;
                        }

                        if message.content_type == ContentType::DeleteMessage {
                            self.process_delete_message(mls_group, storage, &message)?;
                        }

                        Ok::<_, GroupMessageProcessingError>(())
                    }
                    Some(Content::V2(V2 { .. })) => {
                        // V2 was used for DeviceSync V1, which is now removed.
                        // Device Sync V2 reverted back to using V1 envelopes.
                        Ok::<_, GroupMessageProcessingError>(())
                    }
                    None => {
                        return Err(GroupMessageProcessingError::InvalidPayload);
                    }
                }
            }
            ProcessedMessageContent::ProposalMessage(proposal_ptr) => {
                // OpenMLS automatically stores received proposals in its internal proposal store
                // during process_message(). The CommitPendingProposals intent will consume these.
                tracing::debug!(
                    inbox_id = self.context.inbox_id(),
                    installation_id = %self.context.installation_id(),
                    group_id = hex::encode(&self.group_id),
                    proposal_type = ?proposal_ptr.proposal().proposal_type(),
                    "Received and stored proposal in proposal store"
                );
                Ok(())
            }
            ProcessedMessageContent::ExternalJoinProposalMessage(_external_proposal_ptr) => {
                Ok(())
                // intentionally left blank.
            }
            ProcessedMessageContent::StagedCommitMessage(staged_commit) => {
                let staged_commit = *staged_commit;
                let validated_commit =
                    validated_commit.expect("Needs to be present when this is a staged commit");

                log_event!(
                    Event::MLSReceivedStagedCommit,
                    self.context.installation_id(),
                    inbox_id = self.context.inbox_id(),
                    sender_inbox = sender_inbox_id,
                    sender_installation_id,
                    group_id = self.group_id,
                    epoch = mls_group.epoch().as_u64(),
                    msg_epoch,
                    msg_group_id,
                    cursor = %cursor,
                );

                identifier.group_context(staged_commit.group_context().clone());

                mls_group.merge_staged_commit_logged(
                    &XmtpOpenMlsProviderRef::new(storage),
                    staged_commit,
                    &validated_commit,
                    cursor.sequence_id as i64,
                )?;

                Self::mark_readd_requests_as_responded(
                    storage,
                    &self.group_id,
                    &validated_commit.readded_installations,
                    cursor.sequence_id as i64,
                )?;

                let transcript = self.save_transcript_message(
                    validated_commit.clone(),
                    envelope_timestamp_ns as u64,
                    *cursor,
                    storage,
                )?;

                // remove left/removed members from the pending_remove list
                self.clean_pending_remove_list(storage, &validated_commit.removed_inboxes);

                // Handle super_admin status changes for the current user
                // If promoted: check for pending remove members and mark group accordingly
                // If demoted: clear the pending leave request status
                self.handle_super_admin_status_change(
                    storage,
                    mls_group,
                    &validated_commit.metadata_validation_info,
                );

                if let Some((msg, payload)) = transcript {
                    identifier.internal_id(msg.id);

                    log_event!(
                        Event::MLSProcessedStagedCommit,
                        self.context.installation_id(),
                        group_id = self.group_id,
                        epoch = mls_group.epoch().as_u64(),
                        actor_installation_id = validated_commit.actor.installation_id,
                        added_inboxes = $payload.added_inboxes,
                        removed_inboxes = $payload.removed_inboxes,
                        left_inboxes = $payload.left_inboxes,
                        metadata_changes = $payload.metadata_field_changes
                    );
                }

                Ok(())
            }
        }?;
        identifier.build()
    }

    fn process_own_leave_request_message(
        &self,
        mls_group: &OpenMlsGroup,
        storage: &impl XmtpMlsStorageProvider,
        message_id: &[u8],
    ) {
        if let Ok(Some(message)) = storage.db().get_group_message(message_id)
            && message.content_type == ContentType::LeaveRequest
        {
            match self.process_leave_request_message(mls_group, storage, &message) {
                Ok(()) => {
                    debug!("Successfully processed leave request message");
                }
                Err(e) => {
                    debug!("Failed to process leave request message: {}", e);
                }
            }
        }
    }

    fn process_own_delete_message(&self, storage: &impl XmtpMlsStorageProvider, message_id: &[u8]) {
        let db = storage.db();

        let Ok(Some(message)) = db.get_group_message(message_id) else {
            return;
        };

        if message.content_type != ContentType::DeleteMessage {
            return;
        }

        let Ok(Some(deletion)) = db.get_message_deletion(message_id) else {
            tracing::warn!(
                message_id = hex::encode(message_id),
                "Deletion record not found for own delete message"
            );
            return;
        };

        let Ok(Some(original_msg)) = db.get_group_message(&deletion.deleted_message_id) else {
            tracing::debug!(
                deleted_message_id = hex::encode(&deletion.deleted_message_id),
                "Original message not found for deletion event (may be out-of-order)"
            );
            return;
        };

        match crate::messages::decoded_message::DecodedMessage::try_from(original_msg) {
            Ok(decoded_message) => {
                let _ = self.context.local_events().send(
                    crate::subscriptions::LocalEvents::MessageDeleted(Box::new(decoded_message)),
                );
            }
            Err(e) => {
                tracing::warn!(
                    message_id = hex::encode(&deletion.deleted_message_id),
                    error = ?e,
                    "Failed to decode deleted message for deletion event"
                );
            }
        }
    }

    fn process_leave_request_message(
        &self,
        mls_group: &OpenMlsGroup,
        storage: &impl XmtpMlsStorageProvider,
        message: &StoredGroupMessage,
    ) -> Result<(), GroupMessageProcessingError> {
        let current_inbox_id = self.context.inbox_id().to_string();

        // Process leave-request messages - only if the actor is the current user
        // changes if they were made by the same inbox-id
        if message.sender_inbox_id == current_inbox_id {
            storage
                .db()
                .update_group_membership(&self.group_id, GroupMembershipState::PendingRemove)?;
        }

        // put the user in the pending-remove list
        PendingRemove {
            group_id: message.group_id.clone(),
            inbox_id: message.sender_inbox_id.clone(),
            message_id: message.id.clone(),
        }
        .store_or_ignore(&storage.db())?;

        // If we reach here, the action was by another user or no validated commit
        // Only process admin actions if we're admin/super-admin
        self.process_admin_pending_remove_actions(mls_group, storage)?;

        Ok(())
    }

    /// Process an incoming DeleteMessage from the network.
    ///
    /// Returns `Ok(())` for invalid deletions to avoid disrupting sync.
    #[cfg_attr(test, allow(dead_code))]
    pub(crate) fn process_delete_message(
        &self,
        mls_group: &OpenMlsGroup,
        storage: &impl XmtpMlsStorageProvider,
        message: &StoredGroupMessage,
    ) -> Result<(), GroupMessageProcessingError> {
        let encoded_content =
            match EncodedContent::decode(message.decrypted_message_bytes.as_slice()) {
                Ok(content) => content,
                Err(err) => {
                    tracing::warn!(
                        error = ?err,
                        "Failed to decode EncodedContent for delete message, skipping"
                    );
                    return Ok(());
                }
            };

        let delete_msg = match DeleteMessage::decode(encoded_content.content.as_slice()) {
            Ok(msg) => msg,
            Err(err) => {
                tracing::warn!(error = ?err, "Failed to decode DeleteMessage, skipping");
                return Ok(());
            }
        };

        let target_message_id = match hex::decode(&delete_msg.message_id) {
            Ok(id) => id,
            Err(_) => {
                tracing::warn!("Invalid delete message_id: {}", delete_msg.message_id);
                return Ok(());
            }
        };

        let original_msg_opt = storage.db().get_group_message(&target_message_id)?;

        let is_super_admin_deletion = if let Some(ref original_msg) = original_msg_opt {
            if original_msg.group_id != self.group_id {
                tracing::warn!(
                    "Cross-group deletion attempt: message {} from group {}",
                    delete_msg.message_id,
                    hex::encode(&original_msg.group_id)
                );
                return Ok(());
            }

            if !original_msg.kind.is_deletable() || !original_msg.content_type.is_deletable() {
                tracing::warn!(
                    "Non-deletable message {} (kind: {:?}, content_type: {:?})",
                    delete_msg.message_id,
                    original_msg.kind,
                    original_msg.content_type
                );
                return Ok(());
            }

            let is_sender = original_msg.sender_inbox_id == message.sender_inbox_id;
            let is_super_admin_deletion = if is_sender {
                false
            } else {
                self.is_super_admin_without_lock(mls_group, message.sender_inbox_id.clone())
                    .unwrap_or(false)
            };

            let is_authorized = is_sender || is_super_admin_deletion;
            if !is_authorized {
                tracing::warn!(
                    "Unauthorized deletion by {} for message {}",
                    message.sender_inbox_id,
                    delete_msg.message_id
                );
                return Ok(());
            }

            is_super_admin_deletion
        } else {
            // Out-of-order: deletion arrived before the message.
            // Authorization is validated at enrichment time via is_deletion_valid().
            self.is_super_admin_without_lock(mls_group, message.sender_inbox_id.clone())
                .unwrap_or(false)
        };

        let deletion = StoredMessageDeletion {
            id: message.id.clone(),
            group_id: self.group_id.clone(),
            deleted_message_id: target_message_id.clone(),
            deleted_by_inbox_id: message.sender_inbox_id.clone(),
            is_super_admin_deletion,
            deleted_at_ns: message.sent_at_ns,
        };

        deletion.store_or_ignore(&storage.db())?;

        let out_of_order = original_msg_opt.is_none();
        if let Some(original_msg) = original_msg_opt {
            match crate::messages::decoded_message::DecodedMessage::try_from(original_msg) {
                Ok(decoded_message) => {
                    let _ = self.context.local_events().send(
                        crate::subscriptions::LocalEvents::MessageDeleted(Box::new(
                            decoded_message,
                        )),
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        message_id = hex::encode(&target_message_id),
                        error = ?e,
                        "Failed to decode deleted message for deletion event"
                    );
                }
            }
        }

        tracing::info!(
            "Message {} deleted by {} (super_admin: {}, out_of_order: {})",
            delete_msg.message_id,
            message.sender_inbox_id,
            is_super_admin_deletion,
            out_of_order
        );

        Ok(())
    }

    fn process_admin_pending_remove_actions(
        &self,
        mls_group: &OpenMlsGroup,
        storage: &impl XmtpMlsStorageProvider,
    ) -> Result<(), GroupMessageProcessingError> {
        let current_inbox_id = self.context.inbox_id().to_string();

        // Process admin actions based on current group state
        // If the current user is super-admin and there are pending remove requests, mark the group accordingly
        let is_super_admin = match self
            .is_super_admin_without_lock(mls_group, self.context.inbox_id().to_string())
        {
            Ok(is_admin) => is_admin,
            Err(e) => {
                debug!(
                    "Failed to check super admin status while processing LeaveRequestMessage: {}. Skipping admin pending remove actions.",
                    e
                );
                return Ok(());
            }
        };
        // Only process if we're an admin/super-admin
        if !is_super_admin {
            return Ok(());
        }
        let pending_remove_users = storage
            .db()
            .get_pending_remove_users(&mls_group.group_id().to_vec())?;
        if pending_remove_users.is_empty() {
            return Ok(());
        }

        // if the current user is in pending remove-users, then we should not mark it for the worker
        if !pending_remove_users.contains(&current_inbox_id) {
            self.update_group_pending_status(storage, true)
        }

        Ok(())
    }

    fn clean_pending_remove_list(
        &self,
        storage: &impl XmtpMlsStorageProvider,
        removed_inboxes: &[Inbox],
    ) {
        if removed_inboxes.is_empty() {
            return;
        }

        let removed_inbox_ids: Vec<String> = removed_inboxes
            .iter()
            .map(|inbox| inbox.inbox_id.clone())
            .collect();

        match storage
            .db()
            .delete_pending_remove_users(&self.group_id, removed_inbox_ids.clone())
        {
            Ok(_) => {
                tracing::info!(
                    group_id = hex::encode(&self.group_id),
                    removed_inboxes = ?removed_inbox_ids,
                    "Successfully removed left/removed members from pending_remove list"
                );
            }
            Err(e) => {
                tracing::info!(
                    group_id = hex::encode(&self.group_id),
                    removed_inboxes = ?removed_inbox_ids,
                    error = %e,
                    "Failed to clean pending_remove list for removed members"
                );
            }
        }
    }

    fn handle_super_admin_status_change(
        &self,
        storage: &impl XmtpMlsStorageProvider,
        mls_group: &OpenMlsGroup,
        metadata_info: &MutableMetadataValidationInfo,
    ) {
        let current_inbox_id = self.context.inbox_id().to_string();

        // Check if current user was promoted to super_admin
        let was_promoted = metadata_info
            .super_admins_added
            .iter()
            .any(|inbox| inbox.inbox_id == current_inbox_id);

        // Check if current user was demoted from super_admin
        let was_demoted = metadata_info
            .super_admins_removed
            .iter()
            .any(|inbox| inbox.inbox_id == current_inbox_id);

        if !was_promoted && !was_demoted {
            // No change in super_admin status for current user
            return;
        }

        if was_promoted {
            // Promoted to super_admin: check if there are pending remove users
            match storage
                .db()
                .get_pending_remove_users(&mls_group.group_id().to_vec())
            {
                Ok(pending_remove_users) => {
                    if !pending_remove_users.is_empty()
                        && !pending_remove_users.contains(&current_inbox_id)
                    {
                        self.update_group_pending_status(storage, true);
                    }
                }
                Err(e) => {
                    tracing::info!(
                        group_id = hex::encode(&self.group_id),
                        inbox_id = %current_inbox_id,
                        error = %e,
                        "Failed to get pending remove users after promotion"
                    );
                }
            }
        } else if was_demoted {
            // Demoted from super_admin: clear the pending leave request status
            self.update_group_pending_status(storage, false);
        }
    }

    pub(crate) fn update_group_pending_status(
        &self,
        storage: &impl XmtpMlsStorageProvider,
        has_pending_removes: bool,
    ) {
        // This is where we would mark the group as having/not having pending remove requests
        if has_pending_removes {
            tracing::info!(
                group_id = hex::encode(&self.group_id),
                inbox_id = %self.context.inbox_id(),
                "Group has pending remove requests requiring admin action"
            );

            if let Err(e) = storage
                .db()
                .set_group_has_pending_leave_request_status(&self.group_id, Some(true))
            {
                tracing::error!(
                    error = %e,
                    operation = "set_group_pending_status",
                    group_id = hex::encode(&self.group_id),
                    "Failed to mark group as having pending leave requests"
                );
            }
        } else {
            tracing::debug!(
                group_id = hex::encode(&self.group_id),
                inbox_id = %self.context.inbox_id(),
                "Group has no pending remove requests"
            );

            if let Err(e) = storage
                .db()
                .set_group_has_pending_leave_request_status(&self.group_id, Some(false))
            {
                tracing::error!(
                    operation = "set_group_pending_status",
                    group_id = hex::encode(&self.group_id),
                    "Failed to mark group as not having pending leave requests {}",
                    e,
                );
            }
        }
    }

    pub(crate) fn mark_readd_requests_as_responded(
        storage: &impl XmtpMlsStorageProvider,
        group_id: &Vec<u8>,
        readded_installations: &HashSet<Vec<u8>>,
        cursor: i64,
    ) -> Result<(), StorageError> {
        for installation_id in readded_installations {
            storage.db().update_responded_at_sequence_id(
                group_id.as_slice(),
                installation_id.as_slice(),
                cursor,
            )?;
        }
        Ok(())
    }

    fn get_message_expire_at_ns(mls_group: &OpenMlsGroup) -> Option<i64> {
        let mutable_metadata = extract_group_mutable_metadata(mls_group).ok()?;
        let group_disappearing_settings =
            Self::conversation_message_disappearing_settings_from_extensions(&mutable_metadata)
                .ok()?;

        if group_disappearing_settings.is_enabled() {
            Some(now_ns() + group_disappearing_settings.in_ns)
        } else {
            None
        }
    }

    /// This function is idempotent. No need to wrap in a transaction.
    ///
    /// # Parameters
    /// * `envelope` - The message envelope to process
    /// * `trust_message_order` - Controls whether to allow epoch increments from commits and msg cursor increments.
    ///   Set to `true` when processing messages from trusted ordered sources (queries), and `false` when
    ///   processing from potentially out-of-order sources like streams.
    #[cfg_attr(
        any(test, feature = "test-utils"),
        tracing::instrument(level = "info", skip(self), fields(envelope = %envelope))
    )]
    #[cfg_attr(
        not(any(test, feature = "test-utils")),
        tracing::instrument(level = "trace", skip_all)
    )]
    pub(crate) async fn process_message(
        &self,
        envelope: &GroupMessage,
        trust_message_order: bool,
    ) -> Result<MessageIdentifier, GroupMessageProcessingError> {
        if trust_message_order {
            let last_cursor = self.context.db().get_last_cursor_for_originator(
                &envelope.group_id,
                envelope.entity_kind(),
                envelope.originator_id(),
            )?;
            tracing::info!("last cursor of processed = {}", last_cursor);
            if last_cursor.sequence_id >= envelope.sequence_id() {
                tracing::info!(
                    inbox_id = self.context.inbox_id(),
                    installation_id = %self.context.installation_id(),
                    group_id = hex::encode(&envelope.group_id),
                    "Message already processed: skipped cursor:[{}] last cursor in db: [{}]",
                    envelope.cursor,
                    last_cursor
                );
                // early return if the message is already processed
                // _NOTE_: Not early returning and re-processing a message that
                // has already been processed, has the potential to result in forks.
                return MessageIdentifierBuilder::from(envelope).build();
            }
        }

        self.load_mls_group_with_lock_async(async |mut mls_group| {
            // ensure we are processing a private message
            match &envelope.message {
                ProtocolMessage::PrivateMessage(_) => (),
                other => {
                    return Err(GroupMessageProcessingError::UnsupportedMessageType(
                        discriminant(other),
                    ));
                }
            };
            let mut result = self
                .process_message_inner(&mut mls_group, envelope, trust_message_order)
                .await;
            if trust_message_order {
                result = self
                    .post_process_message(&mls_group, result, envelope)
                    .await;
            }
            result
        })
        .await
    }

    #[tracing::instrument(skip(self, mls_group, envelope), level = "trace")]
    async fn process_message_inner(
        &self,
        mls_group: &mut OpenMlsGroup,
        envelope: &GroupMessage,
        trust_message_order: bool,
    ) -> Result<MessageIdentifier, GroupMessageProcessingError> {
        let db = self.context.db();
        let allow_epoch_increment = trust_message_order;
        let allow_cursor_increment = trust_message_order;
        let cursor = envelope.cursor;
        if !allow_epoch_increment && envelope.is_commit() {
            return Err(GroupMessageProcessingError::EpochIncrementNotAllowed);
        }

        let intent = db
            .find_group_intent_by_payload_hash(envelope.payload_hash.as_slice())
            .map_err(GroupMessageProcessingError::Storage)?;

        let group_cursor = db.get_last_cursor_for_originator(
            &self.group_id,
            envelope.entity_kind(),
            envelope.originator_id(),
        )?;
        if group_cursor.sequence_id >= envelope.sequence_id() {
            // early return if the message is already processed
            // _NOTE_: Not early returning and re-processing a message that
            // has already been processed, has the potential to result in forks.
            return MessageIdentifierBuilder::from(envelope)
                .previously_processed(true)
                .build();
        }

        tracing::info!(
            inbox_id = self.context.inbox_id(),
            installation_id = %self.context.installation_id(),
            group_id = hex::encode(&self.group_id),
            cursor = %envelope.cursor,
            "Processing envelope with hash {}, cursor = {}, is_own_intent={}",
            hex::encode(&envelope.payload_hash),
            envelope.cursor,
            intent.is_some()
        );
        match intent {
            // Intent with the payload hash matches
            Some(intent) => {
                let mut identifier = MessageIdentifierBuilder::from(envelope);
                identifier.intent_kind(intent.kind);
                let intent_id = intent.id;
                tracing::info!(
                    inbox_id = self.context.inbox_id(),
                    installation_id = %self.context.installation_id(),
                    group_id = hex::encode(&self.group_id),
                    cursor = %envelope.cursor,
                    intent_id,
                    intent.kind = %intent.kind,
                    "client [{}] is about to process own envelope [{}] for intent [{}] [{}]",
                    self.context.inbox_id(),
                    envelope.cursor,
                    intent_id,
                    intent.kind
                );

                let validation_result = self
                    .stage_and_validate_intent(mls_group, &intent, envelope)
                    .await;

                self.context.mls_storage().transaction(|conn| {
                    let storage = conn.key_store();
                    let db = storage.db();
                    let provider = XmtpOpenMlsProviderRef::new(&storage);
                    let requires_processing = if allow_cursor_increment {
                        self.maybe_update_cursor(&db, envelope)?
                    } else {
                        tracing::info!(
                            "will not call update cursor for group {}, with cursor {}, allow_cursor_increment is false",
                            hex::encode(envelope.group_id.as_slice()),
                            cursor
                        );
                        let current_cursor = db
                            .get_last_cursor_for_originator(&envelope.group_id, envelope.entity_kind(), envelope.originator_id())?;
                        current_cursor.sequence_id < envelope.sequence_id()
                    };
                    if !requires_processing {
                        tracing::debug!("message @cursor=[{}] for group=[{}] created_at=[{}] no longer require processing, should be available in database",
                            envelope.cursor,
                            xmtp_common::fmt::debug_hex(&envelope.group_id),
                            envelope.created_ns
                        );

                        // early return if the message is already processed
                        // _NOTE_: Not early returning and re-processing a message that
                        // has already been processed, has the potential to result in forks.
                        // In some cases, we may want to roll back the cursor if we updated the
                        // cursor, but actually cannot process the message.
                        identifier.previously_processed(true);
                        return Ok(());
                    }
                    let result: Result<Option<Vec<u8>>, IntentResolutionError> = match validation_result {
                        Err(err) => Err(err),
                        Ok(validated_intent) => {
                            self.process_own_message(mls_group, validated_intent, &intent, envelope, &storage)
                        }
                    };
                    let (next_intent_state, internal_message_id) = match result {
                        Err(err) => {
                            if err.processing_error.is_retryable() {
                                // Rollback the transaction so that we can retry
                                return Err(err.processing_error);
                            }
                            if envelope.is_commit() && let Err(accounting_error) = mls_group.mark_failed_commit_logged(&provider, cursor.sequence_id, envelope.message.epoch(), &err.processing_error) {
                                tracing::error!("Error inserting commit entry for failed self commit: {}", accounting_error);
                            }
                            (err.next_intent_state, None)
                        }
                        Ok(internal_message_id) => (IntentState::Committed, internal_message_id)
                    };
                    identifier.internal_id(internal_message_id.clone());

                    if next_intent_state == intent.state {
                        tracing::warn!("Intent [{}] is already in state [{:?}]", intent_id, next_intent_state);
                        return Ok(());
                    }
                    match next_intent_state {
                        IntentState::ToPublish => {
                            db.set_group_intent_to_publish(intent_id)?;
                        }
                        IntentState::Committed => {
                            self.handle_metadata_update_from_intent(&intent, &storage)?;
                            db.set_group_intent_committed(intent_id, cursor)?;
                        }
                        IntentState::Published => {
                            tracing::error!("Unexpected behaviour: returned intent state published from process_own_message");
                        }
                        IntentState::Error => {
                            tracing::error!("Intent [{}] moved to error status", intent_id);
                            db.set_group_intent_error(intent_id)?;
                        }
                        IntentState::Processed => {
                            tracing::debug!("Intent [{}] moved to Processed status", intent_id);
                            db.set_group_intent_processed(intent_id)?;
                        }
                    }
                    Ok(())
                })?;
                identifier.build()
            }
            // No matching intent found. The message did not originate here.
            None => {
                tracing::info!(
                    inbox_id = self.context.inbox_id(),
                    installation_id = %self.context.installation_id(),
                    group_id = hex::encode(&self.group_id),
                    cursor = %envelope.cursor,
                    "client [{}] is about to process external envelope [{}]",
                    self.context.inbox_id(),
                    envelope.cursor
                );
                let identifier = self
                    .validate_and_process_external_message(
                        mls_group,
                        envelope,
                        allow_cursor_increment,
                    )
                    .await?;
                Ok(identifier)
            }
        }
    }

    /// In case of metadataUpdate will extract the updated fields and store them to the db
    fn handle_metadata_update_from_intent(
        &self,
        intent: &StoredGroupIntent,
        storage: &impl XmtpMlsStorageProvider,
    ) -> Result<(), IntentError> {
        if intent.kind == MetadataUpdate {
            let data = UpdateMetadataIntentData::try_from(intent.data.clone())?;

            match data.field_name.as_str() {
                field_name if field_name == MetadataField::MessageDisappearFromNS.as_str() => {
                    storage.db().update_message_disappearing_from_ns(
                        self.group_id.clone(),
                        data.field_value.parse::<i64>().ok(),
                    )?
                }
                field_name if field_name == MetadataField::MessageDisappearInNS.as_str() => {
                    storage.db().update_message_disappearing_in_ns(
                        self.group_id.clone(),
                        data.field_value.parse::<i64>().ok(),
                    )?
                }
                _ => {} // handle other metadata updates
            }
        }

        Ok(())
    }

    fn handle_metadata_update_from_commit(
        &self,
        metadata_field_changes: &Vec<group_updated::MetadataFieldChange>,
        storage: &impl XmtpMlsStorageProvider,
    ) -> Result<(), StorageError> {
        for change in metadata_field_changes {
            match change.field_name.as_str() {
                field_name if field_name == MetadataField::MessageDisappearFromNS.as_str() => {
                    let parsed_value = change
                        .new_value
                        .as_deref()
                        .and_then(|v| v.parse::<i64>().ok());
                    storage
                        .db()
                        .update_message_disappearing_from_ns(self.group_id.clone(), parsed_value)?
                }
                field_name if field_name == MetadataField::MessageDisappearInNS.as_str() => {
                    let parsed_value = change
                        .new_value
                        .as_deref()
                        .and_then(|v| v.parse::<i64>().ok());
                    storage
                        .db()
                        .update_message_disappearing_in_ns(self.group_id.clone(), parsed_value)?
                }
                _ => {} // Handle other metadata updates if needed
            }
        }

        Ok(())
    }

    async fn post_process_message(
        &self,
        mls_group: &OpenMlsGroup,
        process_result: Result<MessageIdentifier, GroupMessageProcessingError>,
        envelope: &xmtp_proto::types::GroupMessage,
    ) -> Result<MessageIdentifier, GroupMessageProcessingError> {
        let message = match process_result {
            Ok(m) => {
                self.context.db().prune_icebox()?;
                tracing::info!(
                    "Transaction completed successfully: process for group [{}] envelope cursor[{}]",
                    &envelope.group_id,
                    envelope.cursor
                );
                Ok(m)
            }
            Err(GroupMessageProcessingError::CommitValidation(
                CommitValidationError::ProtocolVersionTooLow(min_version),
            )) => {
                // Instead of updating cursor, mark group as paused
                self.context
                    .db()
                    .set_group_paused(&self.group_id, &min_version)?;
                tracing::warn!(
                    "Group [{}] paused due to minimum protocol version requirement",
                    hex::encode(&self.group_id)
                );
                Err(GroupMessageProcessingError::GroupPaused)
            }
            Err(e) => {
                tracing::info!(
                    "Transaction failed: process for group [{}] envelope cursor [{}] error:[{}]",
                    &envelope.group_id,
                    envelope.cursor,
                    e
                );

                // Do not update the cursor if you have been removed from the group - you may be readded
                // later
                if !e.is_retryable() && mls_group.is_active()
                    && let Err(transaction_error) = self.context.mls_storage().transaction(|conn| {
                    let storage = conn.key_store();
                    let provider = XmtpOpenMlsProviderRef::new(&storage);
                    // TODO(rich): Add log_err! macro/trait for swallowing errors
                    if let Err(update_cursor_error) =
                        self.maybe_update_cursor(&storage.db(), envelope)
                    {
                        // We don't need to propagate the error if the cursor fails to update - the worst case is
                        // that the non-retriable error is processed again
                        tracing::error!("Error updating cursor for non-retriable error: {update_cursor_error:?}");
                    } else if envelope.is_commit()
                        && let Err(accounting_error) = mls_group.mark_failed_commit_logged(
                        &provider,
                        envelope.sequence_id(),
                        envelope.message.epoch(),
                        &e,
                    ) {
                        tracing::error!(
                                "Error inserting commit entry for failed commit: {}",
                                accounting_error
                        );
                    }
                    Ok::<(), GroupMessageProcessingError>(())
                }) {
                    tracing::error!("Error post-processing non-retryable error: {transaction_error:?}");
                };

                if let Err(accounting_error) = self
                    .process_group_message_error_for_fork_detection(
                        envelope.sequence_id(),
                        envelope.message.epoch(),
                        &e,
                        mls_group,
                    )
                    .await
                {
                    tracing::error!(
                        "Error trying to log fork detection errors: {}",
                        accounting_error
                    );
                }
                Err(e)
            }
        }?;
        Ok(message)
    }

    #[cfg_attr(
        any(test, feature = "test-utils"),
        tracing::instrument(level = "info", skip_all, fields(who = %self.context.inbox_id()))
    )]
    #[cfg_attr(
        not(any(test, feature = "test-utils")),
        tracing::instrument(level = "trace", skip_all)
    )]
    pub async fn process_messages(&self, messages: Vec<GroupMessage>) -> ProcessSummary {
        let mut summary = ProcessSummary::default();
        for message in messages {
            summary.add_id(message.cursor);

            let result = retry_async!(
                Retry::default(),
                (async { self.process_message(&message, true).await })
            );

            match result {
                Ok(m) => summary.add(m),
                Err(GroupMessageProcessingError::GroupPaused) => {
                    tracing::info!(
                        "Group [{}] is paused, skip syncing remaining messages",
                        hex::encode(&self.group_id),
                    );
                    return summary;
                }
                Err(e) => {
                    let is_retryable = e.is_retryable();
                    let error_message = e.to_string();
                    summary.errored(message.cursor, e);
                    // If the error is retryable we cannot move on to the next message
                    // otherwise you can get into a forked group state.
                    if is_retryable {
                        tracing::info!(
                            error = %error_message,
                            "Aborting message processing for retryable error: {}",
                            error_message
                        );
                        break;
                    }
                }
            }
        }
        summary
    }

    /// Receive messages from the last cursor network and try to process each message
    /// Return all the cursors of the messages we tried to process regardless
    /// if they were successful or not. It is important to return _all_
    /// cursor ids, so that streams do not unintentionally retry O(n^2) messages.
    #[tracing::instrument(skip_all, level = "trace")]
    pub async fn receive(&self) -> Result<ProcessSummary, GroupError> {
        let messages = MlsStore::new(self.context.clone())
            .query_group_messages(&self.group_id)
            .await?;

        let summary = self.process_messages(messages).await;
        Ok(summary)
    }

    #[tracing::instrument(skip_all, level = "trace")]
    fn maybe_update_cursor(
        &self,
        db: &impl DbQuery,
        message: &xmtp_proto::types::GroupMessage,
    ) -> Result<bool, StorageError> {
        let updated = db.update_cursor(&message.group_id, message.entity_kind(), message.cursor)?;
        if updated {
            log_event!(
                Event::GroupCursorUpdate,
                self.context.installation_id(),
                group_id = message.group_id.as_slice(),
                cursor = message.cursor.sequence_id,
                originator = message.cursor.originator_id
            );
        } else {
            tracing::debug!("no cursor update required");
        }
        Ok(updated)
    }

    fn save_transcript_message(
        &self,
        validated_commit: ValidatedCommit,
        timestamp_ns: u64,
        cursor: Cursor,
        storage: &impl XmtpMlsStorageProvider,
    ) -> Result<Option<(StoredGroupMessage, GroupUpdated)>, GroupMessageProcessingError> {
        if validated_commit.is_empty() {
            return Ok(None);
        }
        let sender_installation_id = validated_commit.actor_installation_id();
        let sender_inbox_id = validated_commit.actor_inbox_id();

        let pending_remove_users = &storage
            .db()
            .get_pending_remove_users(self.group_id.as_slice())?;
        let payload: GroupUpdated = validated_commit.into_with(pending_remove_users);
        tracing::info!("Storing transcript message");
        let encoded_payload = GroupUpdatedCodec::encode(payload.clone())?;
        let mut encoded_payload_bytes = Vec::new();
        encoded_payload.encode(&mut encoded_payload_bytes)?;

        let message_id = calculate_message_id(
            &self.group_id,
            encoded_payload_bytes.as_slice(),
            &timestamp_ns.to_string(),
        );
        let content_type = encoded_payload.r#type.unwrap_or_else(|| {
            tracing::warn!("Missing content type in encoded payload, using default values");
            // Default content type values
            xmtp_proto::xmtp::mls::message_contents::ContentTypeId {
                authority_id: "unknown".to_string(),
                type_id: "unknown".to_string(),
                version_major: 0,
                version_minor: 0,
            }
        });

        self.handle_metadata_update_from_commit(&payload.metadata_field_changes, storage)?;

        // When a DM is stitched, it can repeat group updates. We want to prevent saving those messages.
        if self.update_already_exists(&payload, storage)? {
            return Ok(None);
        }

        let msg = StoredGroupMessage {
            id: message_id,
            group_id: self.group_id.clone(),
            decrypted_message_bytes: encoded_payload_bytes,
            sent_at_ns: timestamp_ns as i64,
            kind: GroupMessageKind::MembershipChange,
            sender_installation_id,
            sender_inbox_id,
            delivery_status: DeliveryStatus::Published,
            content_type: content_type.type_id.into(),
            version_major: content_type.version_major as i32,
            version_minor: content_type.version_minor as i32,
            authority_id: content_type.authority_id.to_string(),
            reference_id: None,
            sequence_id: cursor.sequence_id as i64,
            originator_id: cursor.originator_id as i64,
            expire_at_ns: None,
            inserted_at_ns: 0, // Will be set by database
            should_push: true,
        };

        msg.store_or_ignore(&storage.db())?;
        Ok(Some((msg, payload)))
    }

    fn update_already_exists(
        &self,
        payload: &GroupUpdated,
        storage: &impl XmtpMlsStorageProvider,
    ) -> Result<bool, GroupMessageProcessingError> {
        if self.dm_id.is_none() || payload.added_inboxes.is_empty() {
            // Only dedupe for DMs.
            // Only dedupe for group adds.
            return Ok(false);
        }

        let mut deduper = GroupUpdateDeduper::default();
        let mut inserted_after_ns = None;
        let mut msgs;
        loop {
            // DMs are stitched, so we don't want to have the same
            // group updates from multiple DMs being saved to the database.
            msgs = self.find_messages_v2_with_conn(
                &MsgQueryArgs {
                    content_types: Some(vec![ContentType::GroupUpdated]),
                    inserted_after_ns,
                    limit: Some(100),
                    ..Default::default()
                },
                storage.db(),
            )?;

            let Some(msg) = msgs.last() else {
                break;
            };
            inserted_after_ns = Some(msg.metadata.inserted_at_ns);

            for msg in msgs {
                let MessageBody::GroupUpdated(update) = msg.content else {
                    continue;
                };

                deduper.consume(&update);
            }
        }

        Ok(deduper.is_dupe(payload))
    }

    async fn process_group_message_error_for_fork_detection(
        &self,
        message_cursor: u64,
        message_epoch: GroupEpoch,
        error: &GroupMessageProcessingError,
        mls_group: &OpenMlsGroup,
    ) -> Result<(), GroupMessageProcessingError> {
        if !matches!(
            error,
            OpenMlsProcessMessage(ProcessMessageError::ValidationError(
                ValidationError::WrongEpoch,
            ))
        ) {
            return Ok(());
        }

        let group_epoch = mls_group.epoch().as_u64();
        let epoch_validation_result = Self::validate_message_epoch(
            self.context.inbox_id(),
            0,
            GroupEpoch::from(group_epoch),
            message_epoch,
            MAX_PAST_EPOCHS,
        );

        if let Err(GroupMessageProcessingError::FutureEpoch(_, _)) = &epoch_validation_result {
            let fork_details = format!(
                "Message cursor [{}] epoch [{}] is greater than group epoch [{}], your group may be forked",
                message_cursor, message_epoch, group_epoch
            );
            tracing::error!(
                inbox_id = self.context.inbox_id(),
                installation_id = %self.context.installation_id(),
                group_id = hex::encode(&self.group_id),
                original_error = error.to_string(),
                fork_details
            );
            let _ = self
                .context
                .db()
                .mark_group_as_maybe_forked(&self.group_id, fork_details);
            return epoch_validation_result;
        }

        Ok(())
    }

    #[tracing::instrument]
    pub(super) async fn publish_intents(&self) -> Result<(), GroupError> {
        let db = self.context.db();
        self.load_mls_group_with_lock_async(async |mut mls_group| {
            let intents = db.find_group_intents(
                self.group_id.clone(),
                Some(vec![IntentState::ToPublish]),
                None,
            )?;

            for intent in intents {
                let result = retry_async!(
                    Retry::default(),
                    (async {
                        self.get_publish_intent_data(&mut mls_group, &intent)
                            .await
                    })
                );

                match result {
                    Err(err) => {
                        tracing::error!(error = %err, "error getting publish intent data {:?}", err);
                        if (intent.publish_attempts + 1) as usize >= MAX_INTENT_PUBLISH_ATTEMPTS {
                            tracing::error!(
                                intent.id,
                                intent.kind = %intent.kind,
                                inbox_id = self.context.inbox_id(),
                                installation_id = %self.context.installation_id(),group_id = hex::encode(&self.group_id),
                                "intent {} has reached max publish attempts", intent.id);
                            // TODO: Eventually clean up errored attempts
                            let id = utils::id::calculate_message_id_for_intent(&intent)?;
                            db.set_group_intent_error_and_fail_msg(&intent, id)?;
                        } else {
                            db.increment_intent_publish_attempt_count(intent.id)?;
                        }

                        return Err(err);
                    }
                    Ok(Some(PublishIntentData {
                                payloads_to_publish,
                                post_commit_action,
                                staged_commit,
                                should_send_push_notification,
                                group_epoch
                            })) => {
                        // For multiple payloads (proposals), hash them all concatenated
                        // For single payloads (commits/messages), this is the same as before
                        let all_bytes: Vec<u8> = payloads_to_publish.iter().flatten().copied().collect();
                        let has_staged_commit = staged_commit.is_some();
                        let intent_hash = sha256(&all_bytes);
                        // removing this transaction causes missed messages
                        self.context.mls_storage().transaction(|conn| {
                            let storage = conn.key_store();
                            let db = storage.db();
                            db.set_group_intent_published(
                                intent.id,
                                &intent_hash,
                                post_commit_action,
                                staged_commit,
                                group_epoch as i64,
                            )
                        })?;
                        tracing::debug!(
                            inbox_id = self.context.inbox_id(),
                            installation_id = %self.context.installation_id(),
                            intent.id,
                            intent.kind = %intent.kind,
                            group_id = hex::encode(&self.group_id),
                            "[{}] set stored intent [{}] with hash [{}] to state `published`",
                            self.context.inbox_id(),
                            intent.id,
                            hex::encode(&intent_hash)
                        );

                        // Prepare messages for all payloads
                        let payload_pairs: Vec<_> = payloads_to_publish
                            .iter()
                            .map(|p| (p.as_slice(), should_send_push_notification))
                            .collect();
                        let messages = self.prepare_group_messages(payload_pairs)?;
                        let result = self.context
                            .api()
                            .send_group_messages(messages)
                            .await;

                        match (intent.kind, result) {
                            (IntentKind::SendMessage, Ok(_)) => {
                                log_event!(
                                    Event::GroupSyncApplicationMessagePublishSuccess,
                                    self.context.installation_id(),
                                    group_id = intent.group_id,
                                    intent_id = intent.id
                                );
                            }
                            (kind, Err(err)) => {
                                log_event!(
                                    Event::GroupSyncPublishFailed,
                                    self.context.installation_id(),
                                    group_id = intent.group_id,
                                    intent_id = intent.id,
                                    intent_kind = ?kind,
                                    err = ?err
                                );
                                return Err(err)?;
                            }
                            (kind, Ok(_)) => {
                                log_event!(
                                    Event::GroupSyncCommitPublishSuccess,
                                    self.context.installation_id(),
                                    group_id = intent.group_id,
                                    intent_id = intent.id,
                                    intent_kind = ?kind,
                                    commit_hash = hex::encode(sha256(&all_bytes))
                                )
                            }
                        }

                        if has_staged_commit {
                            log_event!(Event::GroupSyncStagedCommitPresent, self.context.installation_id(), group_id = intent.group_id);
                            return Ok(());
                        }
                    }
                    Ok(None) => {
                        tracing::info!(
                            inbox_id = self.context.inbox_id(),
                            installation_id = %self.context.installation_id(),
                            "Skipping intent because no publish data returned"
                        );
                        db.set_group_intent_processed(intent.id)?
                    }
                }
            }

            Ok(())
        }).await
    }

    // Takes a StoredGroupIntent and returns the payload and post commit data as a tuple
    // A return value of [`Option::None`] means this intent would not change the group.
    #[allow(clippy::type_complexity)]
    #[tracing::instrument(level = "trace", skip_all)]
    async fn get_publish_intent_data(
        &self,
        openmls_group: &mut OpenMlsGroup,
        intent: &StoredGroupIntent,
    ) -> Result<Option<PublishIntentData>, GroupError> {
        let storage = self.context.mls_storage();
        match intent.kind {
            IntentKind::UpdateGroupMembership => {
                let intent_data =
                    UpdateGroupMembershipIntentData::try_from(intent.data.as_slice())?;
                let signer = &self.context.identity().installation_keys;
                apply_update_group_membership_intent(
                    &self.context,
                    openmls_group,
                    intent_data,
                    signer,
                )
                .await
            }
            IntentKind::SendMessage => {
                // We can safely assume all SendMessage intents have data
                let intent_data = SendMessageIntentData::from_bytes(intent.data.as_slice())?;
                // Pending proposals are handled at the API level (in send_message)
                // by committing them before creating the SendMessage intent
                let group_epoch = openmls_group.epoch().as_u64();
                let msg = openmls_group.create_message(
                    &self.context.mls_provider(),
                    &self.context.identity().installation_keys,
                    intent_data.message.as_slice(),
                )?;

                Ok(Some(PublishIntentData {
                    payloads_to_publish: vec![msg.tls_serialize_detached()?],
                    post_commit_action: None,
                    staged_commit: None,
                    should_send_push_notification: intent.should_push,
                    group_epoch,
                }))
            }
            IntentKind::KeyUpdate => {
                let keys = self.context.identity().installation_keys.clone();
                let (bundle, staged_commit, group_epoch) =
                    generate_commit_with_rollback(storage, openmls_group, |group, provider| {
                        group.self_update(provider, &keys, LeafNodeParameters::default())
                    })?;
                Ok(Some(PublishIntentData {
                    payloads_to_publish: vec![bundle.commit().tls_serialize_detached()?],
                    staged_commit,
                    post_commit_action: None,
                    should_send_push_notification: intent.should_push,
                    group_epoch,
                }))
            }
            IntentKind::MetadataUpdate => {
                let metadata_intent = UpdateMetadataIntentData::try_from(intent.data.clone())?;
                let mutable_metadata_extensions = build_extensions_for_metadata_update(
                    openmls_group,
                    metadata_intent.field_name,
                    metadata_intent.field_value,
                )?;

                let keys = self.context.identity().installation_keys.clone();
                let ((commit, _, _), staged_commit, group_epoch) =
                    generate_commit_with_rollback(storage, openmls_group, |group, provider| {
                        group.update_group_context_extensions(
                            provider,
                            mutable_metadata_extensions.clone(),
                            &keys,
                        )
                    })?;

                let commit_bytes = commit.tls_serialize_detached()?;

                Ok(Some(PublishIntentData {
                    payloads_to_publish: vec![commit_bytes],
                    staged_commit,
                    post_commit_action: None,
                    should_send_push_notification: intent.should_push,
                    group_epoch,
                }))
            }
            IntentKind::UpdateAdminList => {
                let admin_list_update_intent =
                    UpdateAdminListIntentData::try_from(intent.data.clone())?;
                let mutable_metadata_extensions = build_extensions_for_admin_lists_update(
                    openmls_group,
                    admin_list_update_intent,
                )?;

                let keys = self.context.identity().installation_keys.clone();
                let ((commit, _, _), staged_commit, group_epoch) =
                    generate_commit_with_rollback(storage, openmls_group, |group, provider| {
                        group.update_group_context_extensions(
                            provider,
                            mutable_metadata_extensions.clone(),
                            &keys,
                        )
                    })?;

                let commit_bytes = commit.tls_serialize_detached()?;

                Ok(Some(PublishIntentData {
                    payloads_to_publish: vec![commit_bytes],
                    staged_commit,
                    post_commit_action: None,
                    should_send_push_notification: intent.should_push,
                    group_epoch,
                }))
            }
            IntentKind::UpdatePermission => {
                let update_permissions_intent =
                    UpdatePermissionIntentData::try_from(intent.data.clone())?;
                let group_permissions_extensions = build_extensions_for_permissions_update(
                    openmls_group,
                    update_permissions_intent,
                )?;

                let keys = self.context.identity().installation_keys.clone();
                let ((commit, _, _), staged_commit, group_epoch) =
                    generate_commit_with_rollback(storage, openmls_group, |group, provider| {
                        group.update_group_context_extensions(
                            provider,
                            group_permissions_extensions.clone(),
                            &keys,
                        )
                    })?;

                let commit_bytes = commit.tls_serialize_detached()?;
                Ok(Some(PublishIntentData {
                    payloads_to_publish: vec![commit_bytes],
                    staged_commit,
                    post_commit_action: None,
                    should_send_push_notification: intent.should_push,
                    group_epoch,
                }))
            }
            IntentKind::ReaddInstallations => {
                let intent_data = ReaddInstallationsIntentData::try_from(intent.data.as_slice())?;
                let signer = &self.context.identity().installation_keys;
                apply_readd_installations_intent(&self.context, openmls_group, intent_data, signer)
                    .await
            }
            IntentKind::ProposeMemberUpdate => {
                let intent_data = ProposeMemberUpdateIntentData::try_from(intent.data.as_slice())?;
                let group_epoch = openmls_group.epoch().as_u64();
                let signer = &self.context.identity().installation_keys;
                let mut proposal_payloads = Vec::new();

                // Handle adds
                if !intent_data.add_inbox_ids.is_empty() {
                    // Get current group membership
                    let extensions: Extensions<GroupContext> = openmls_group.extensions().clone();
                    let old_group_membership = extract_group_membership(&extensions)?;

                    // Get latest sequence IDs for the inbox_ids to add
                    let inbox_ids_to_add: Vec<&str> = intent_data
                        .add_inbox_ids
                        .iter()
                        .map(|s| s.as_str())
                        .collect();

                    load_identity_updates(
                        self.context.api(),
                        &self.context.db(),
                        &inbox_ids_to_add,
                    )
                    .await?;

                    let latest_sequence_ids = self
                        .context
                        .db()
                        .get_latest_sequence_id(&inbox_ids_to_add)?;

                    // Build the new membership with the added inbox_ids
                    let mut new_membership = old_group_membership.clone();
                    for inbox_id in &intent_data.add_inbox_ids {
                        let sequence_id = latest_sequence_ids
                            .get(inbox_id.as_str())
                            .copied()
                            .ok_or(GroupError::MissingSequenceId)?;
                        new_membership.add(inbox_id.clone(), sequence_id as u64);
                    }

                    // Get key packages for the installations to add
                    let changes_with_kps = calculate_membership_changes_with_keypackages(
                        &self.context,
                        &self.group_id,
                        &new_membership,
                        &old_group_membership,
                    )
                    .await?;

                    // If we failed to fetch key packages for all installations, error
                    if !changes_with_kps.failed_installations.is_empty()
                        && changes_with_kps.new_key_packages.is_empty()
                    {
                        return Err(GroupError::FailedToVerifyInstallations);
                    }

                    // Generate add proposals for each key package
                    for key_package in &changes_with_kps.new_key_packages {
                        let (proposal_msg, _proposal_ref) = openmls_group
                            .propose_add_member(&self.context.mls_provider(), signer, key_package)
                            .map_err(GroupError::ProposeAddMember)?;
                        proposal_payloads.push(proposal_msg.tls_serialize_detached()?);
                    }
                }

                // Handle removes
                if !intent_data.remove_inbox_ids.is_empty() {
                    let inbox_ids_to_remove: HashSet<_> =
                        intent_data.remove_inbox_ids.iter().cloned().collect();
                    let mut members_to_remove = Vec::new();
                    for member in openmls_group.members() {
                        let credential = BasicCredential::try_from(member.credential.clone())?;
                        let member_inbox_id = parse_credential(credential.identity())?;
                        if inbox_ids_to_remove.contains(&member_inbox_id) {
                            members_to_remove.push(member.index);
                        }
                    }

                    // Generate remove proposals for collected members
                    for member_index in members_to_remove {
                        let (proposal_msg, _proposal_ref) = openmls_group
                            .propose_remove_member(
                                &self.context.mls_provider(),
                                signer,
                                member_index,
                            )
                            .map_err(GroupError::ProposeRemoveMember)?;
                        proposal_payloads.push(proposal_msg.tls_serialize_detached()?);
                    }
                }

                if proposal_payloads.is_empty() {
                    return Ok(None);
                }

                // Note: The GroupContextExtensions proposal to update membership is created
                // by CommitPendingProposals, not here. This avoids issues with tracking
                // multiple message hashes per intent.

                Ok(Some(PublishIntentData {
                    payloads_to_publish: proposal_payloads,
                    staged_commit: None,
                    post_commit_action: None,
                    should_send_push_notification: intent.should_push,
                    group_epoch,
                }))
            }
            IntentKind::ProposeGroupContextExtensions => {
                let intent_data =
                    ProposeGroupContextExtensionsIntentData::try_from(intent.data.as_slice())?;
                let group_epoch = openmls_group.epoch().as_u64();

                // Deserialize the extensions using tls_codec
                use openmls::prelude::tls_codec::Deserialize;
                let extensions =
                    Extensions::tls_deserialize(&mut intent_data.extensions_bytes.as_slice())?;

                let signer = &self.context.identity().installation_keys;
                let (proposal_msg, _proposal_ref) = openmls_group
                    .propose_group_context_extensions(
                        &self.context.mls_provider(),
                        extensions,
                        signer,
                    )
                    .map_err(GroupError::Proposal)?;

                Ok(Some(PublishIntentData {
                    payloads_to_publish: vec![proposal_msg.tls_serialize_detached()?],
                    staged_commit: None,
                    post_commit_action: None,
                    should_send_push_notification: intent.should_push,
                    group_epoch,
                }))
            }
            IntentKind::CommitPendingProposals => {
                use crate::verified_key_package_v2::VerifiedKeyPackageV2;

                let _intent_data =
                    CommitPendingProposalsIntentData::try_from(intent.data.as_slice())?;

                // Check if there are any pending proposals to commit
                if openmls_group.pending_proposals().next().is_none() {
                    tracing::debug!("No pending proposals to commit");
                    return Ok(None);
                }

                let signer = &self.context.identity().installation_keys;

                // Get current group membership
                let current_extensions: Extensions<GroupContext> =
                    openmls_group.extensions().clone();
                let current_membership = extract_group_membership(&current_extensions)?;

                // Analyze pending proposals to determine membership changes and collect installations
                let mut inbox_ids_to_add: Vec<String> = Vec::new();
                let mut inbox_ids_to_remove: Vec<String> = Vec::new();
                let mut installations_to_welcome: Vec<Installation> = Vec::new();
                let mut key_packages_to_add: Vec<openmls::key_packages::KeyPackage> = Vec::new();

                for proposal_ref in openmls_group.pending_proposals() {
                    match proposal_ref.proposal() {
                        Proposal::Add(add_proposal) => {
                            let key_package = add_proposal.key_package();
                            let credential = BasicCredential::try_from(
                                key_package.leaf_node().credential().clone(),
                            )?;
                            let inbox_id = parse_credential(credential.identity())?;
                            if !inbox_ids_to_add.contains(&inbox_id)
                                && current_membership.get(&inbox_id).is_none()
                            {
                                inbox_ids_to_add.push(inbox_id);

                                // Collect the key package for proposal support check
                                key_packages_to_add.push(key_package.clone());

                                // Extract installation info from the key package for welcome sending
                                if let Ok(verified_kp) =
                                    VerifiedKeyPackageV2::try_from(key_package.clone())
                                    && let Ok(installation) =
                                        Installation::from_verified_key_package(&verified_kp)
                                {
                                    installations_to_welcome.push(installation);
                                }
                            }
                        }
                        Proposal::Remove(remove_proposal) => {
                            if let Some(member) = openmls_group.member_at(remove_proposal.removed())
                            {
                                let credential = BasicCredential::try_from(member.credential)?;
                                let inbox_id = parse_credential(credential.identity())?;
                                if !inbox_ids_to_remove.contains(&inbox_id) {
                                    inbox_ids_to_remove.push(inbox_id);
                                }
                            }
                        }
                        _ => {}
                    }
                }

                // Build the updated membership
                let mut new_membership = current_membership.clone();

                // Add new members with their latest sequence IDs
                if !inbox_ids_to_add.is_empty() {
                    let inbox_ids_refs: Vec<&str> =
                        inbox_ids_to_add.iter().map(|s| s.as_str()).collect();
                    load_identity_updates(self.context.api(), &self.context.db(), &inbox_ids_refs)
                        .await?;
                    let latest_sequence_ids =
                        self.context.db().get_latest_sequence_id(&inbox_ids_refs)?;

                    for inbox_id in &inbox_ids_to_add {
                        let sequence_id = latest_sequence_ids
                            .get(inbox_id.as_str())
                            .copied()
                            .ok_or(GroupError::MissingSequenceId)?;
                        new_membership.add(inbox_id.clone(), sequence_id as u64);
                    }
                }

                // Remove members
                for inbox_id in &inbox_ids_to_remove {
                    new_membership.remove(inbox_id);
                }

                // Create a GCE proposal with the updated membership if there are membership changes
                let membership_changed =
                    !inbox_ids_to_add.is_empty() || !inbox_ids_to_remove.is_empty();
                if membership_changed {
                    let mut new_extensions =
                        build_extensions_for_membership_update(openmls_group, &new_membership)?;

                    // Check if proposals need to be disabled due to new members not supporting them
                    let proposals_currently_enabled = self.proposals_enabled(openmls_group);
                    if proposals_currently_enabled && !key_packages_to_add.is_empty() {
                        let new_members_support_proposals = self
                            .validate_key_packages_support_proposals(&key_packages_to_add)
                            .is_ok();

                        if !new_members_support_proposals {
                            tracing::info!(
                                "Disabling proposals: new members don't support proposal extension"
                            );
                            new_extensions
                                .remove(ExtensionType::Unknown(PROPOSAL_SUPPORT_EXTENSION_ID));
                        }
                    }

                    let (_gce_proposal_msg, _gce_proposal_ref) = openmls_group
                        .propose_group_context_extensions(
                            &self.context.mls_provider(),
                            new_extensions,
                            signer,
                        )
                        .map_err(GroupError::Proposal)?;
                    tracing::debug!(
                        inbox_ids_to_add = ?inbox_ids_to_add,
                        inbox_ids_to_remove = ?inbox_ids_to_remove,
                        "Created GCE proposal with updated membership for CommitPendingProposals"
                    );
                }

                // Use generate_commit_with_rollback to create the commit
                let ((commit, maybe_welcome, _), staged_commit, group_epoch) =
                    generate_commit_with_rollback(storage, openmls_group, |group, provider| {
                        group.commit_to_pending_proposals(provider, signer)
                    })?;

                let staged_commit =
                    staged_commit.ok_or_else(|| GroupError::MissingPendingCommit)?;

                // Build post commit action if there's a welcome message
                let post_commit_action = match maybe_welcome {
                    Some(welcome_message) => {
                        tracing::debug!(
                            num_installations = installations_to_welcome.len(),
                            "Creating post commit action with installations to welcome"
                        );
                        Some(PostCommitAction::from_welcome(
                            welcome_message,
                            installations_to_welcome,
                        )?)
                    }
                    None => None,
                };

                Ok(Some(PublishIntentData {
                    payloads_to_publish: vec![commit.tls_serialize_detached()?],
                    staged_commit: Some(staged_commit),
                    post_commit_action: post_commit_action.map(|action| action.to_bytes()),
                    should_send_push_notification: intent.should_push,
                    group_epoch,
                }))
            }
        }
    }

    #[tracing::instrument(skip_all)]
    pub(crate) async fn post_commit(&self) -> Result<(), GroupError> {
        let db = self.context.db();
        let intents = db.find_group_intents(
            self.group_id.clone(),
            Some(vec![IntentState::Committed]),
            None,
        )?;

        for intent in intents {
            if let Some(post_commit_data) = intent.post_commit_data {
                tracing::debug!(
                    inbox_id = self.context.inbox_id(),
                    installation_id = %self.context.installation_id(),
                    intent.id,
                    intent.kind = %intent.kind, "taking post commit action"
                );

                let post_commit_action = PostCommitAction::from_bytes(post_commit_data.as_slice())?;
                match post_commit_action {
                    PostCommitAction::SendWelcomes(action) => {
                        self.send_welcomes(action, intent.sequence_id).await?;
                    }
                }
            }
            db.set_group_intent_processed(intent.id)?
        }

        Ok(())
    }

    pub async fn maybe_update_installations(
        &self,
        update_interval_ns: Option<i64>,
    ) -> Result<(), GroupError> {
        let db = self.context.db();
        let Some(stored_group) = db.find_group(&self.group_id)? else {
            return Err(GroupError::NotFound(NotFound::GroupById(
                self.group_id.clone(),
            )));
        };
        if stored_group.conversation_type.is_virtual() {
            return Ok(());
        }

        // determine how long of an interval in time to use before updating list
        let interval_ns = update_interval_ns.unwrap_or(SYNC_UPDATE_INSTALLATIONS_INTERVAL_NS);

        let now_ns = xmtp_common::time::now_ns();
        let last_ns = db.get_installations_time_checked(self.group_id.clone())?;
        let elapsed_ns = now_ns - last_ns;
        if elapsed_ns > interval_ns && self.is_active()? {
            self.add_missing_installations().await?;
            db.update_installations_time_checked(self.group_id.clone())?;
        }

        Ok(())
    }

    /**
     * Checks each member of the group for `IdentityUpdates` after their current sequence_id. If updates
     * are found the method will construct an [`UpdateGroupMembershipIntentData`] and create a change
     * to the [`GroupMembership`] that will add any missing installations.
     *
     * This is designed to handle cases where existing members have added a new installation to their inbox or revoked an installation
     * and the group has not been updated to include it.
     */
    #[cfg_attr(any(test, feature = "test-utils"), tracing::instrument(level = "info", fields(who = %self.context.inbox_id()), skip_all))]
    #[cfg_attr(
        not(any(test, feature = "test-utils")),
        tracing::instrument(level = "trace", skip_all)
    )]
    pub(super) async fn add_missing_installations(&self) -> Result<(), GroupError> {
        let intent_data = self.get_membership_update_intent(&[], &[]).await?;

        // If there is nothing to do, stop here
        if intent_data.is_empty() {
            return Ok(());
        }

        debug!(
            inbox_id = self.context.inbox_id(),
            installation_id = %self.context.installation_id(),
            "Adding missing installations {:?}",
            intent_data
        );

        let intent = QueueIntent::update_group_membership()
            .data(intent_data)
            .queue(self)?;

        let _ = self.sync_until_intent_resolved(intent.id).await?;
        Ok(())
    }

    /**
     * get_membership_update_intent will query the network for any new [`IdentityUpdate`]s for any of the existing
     * group members
     *
     * Callers may also include a list of added or removed inboxes
     */
    #[tracing::instrument(level = "trace", skip_all)]
    pub(crate) async fn get_membership_update_intent(
        &self,
        inbox_ids_to_add: &[InboxIdRef<'_>],
        inbox_ids_to_remove: &[InboxIdRef<'_>],
    ) -> Result<UpdateGroupMembershipIntentData, GroupError> {
        self.load_mls_group_with_lock_async(async |mls_group| {
            let existing_group_membership = extract_group_membership(mls_group.extensions())?;
            // TODO:nm prevent querying for updates on members who are being removed
            let mut inbox_ids = existing_group_membership.inbox_ids();
            inbox_ids.extend_from_slice(inbox_ids_to_add);
            let conn = self.context.db();
            // Load any missing updates from the network
            load_identity_updates(self.context.sync_api(), &conn, &inbox_ids).await?;

            let latest_sequence_id_map = conn.get_latest_sequence_id(&inbox_ids as &[&str])?;

            // Get a list of all inbox IDs that have increased sequence_id for the group
            let changed_inbox_ids =
                inbox_ids
                    .iter()
                    .try_fold(HashMap::new(), |mut updates, inbox_id| {
                        match (
                            latest_sequence_id_map.get(inbox_id as &str),
                            existing_group_membership.get(inbox_id),
                        ) {
                            // This is an update. We have a new sequence ID and an existing one
                            (Some(latest_sequence_id), Some(current_sequence_id)) => {
                                let latest_sequence_id_u64 = *latest_sequence_id as u64;
                                if latest_sequence_id_u64.gt(current_sequence_id) {
                                    updates.insert(inbox_id.to_string(), latest_sequence_id_u64);
                                }
                            }
                            // This is for new additions to the group
                            (Some(latest_sequence_id), None) => {
                                // This is the case for net new members to the group
                                updates.insert(inbox_id.to_string(), *latest_sequence_id as u64);
                            }
                            (_, _) => {
                                tracing::warn!(
                                    "Could not find existing sequence ID for inbox {}",
                                    inbox_id
                                );
                                return Err(GroupError::MissingSequenceId);
                            }
                        }

                        Ok(updates)
                    })?;
            let extensions: Extensions<GroupContext> = mls_group.extensions().clone();
            let old_group_membership = extract_group_membership(&extensions)?;
            let mut new_membership = old_group_membership.clone();
            for (inbox_id, sequence_id) in changed_inbox_ids.iter() {
                new_membership.add(inbox_id.clone(), *sequence_id);
            }
            for inbox_id in inbox_ids_to_remove {
                new_membership.remove(inbox_id);
            }

            let changes_with_kps = calculate_membership_changes_with_keypackages(
                &self.context,
                &self.group_id,
                &new_membership,
                &old_group_membership,
            )
            .await?;

            // If we fail to fetch or verify all the added members' KeyPackage, return an error.
            // skip if the inbox ids is 0 from the beginning
            if !inbox_ids_to_add.is_empty()
                && !changes_with_kps.failed_installations.is_empty()
                && changes_with_kps.new_installations.is_empty()
            {
                return Err(GroupError::FailedToVerifyInstallations);
            }

            Ok(UpdateGroupMembershipIntentData::new(
                changed_inbox_ids,
                inbox_ids_to_remove
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<String>>(),
                changes_with_kps.failed_installations,
            ))
        })
        .await
    }

    /**
     * Sends welcome messages to the installations specified in the action
     *
     * Internally, this breaks the request into chunks to avoid exceeding the GRPC max message size limits
     */
    #[cfg_attr(any(test, feature = "test-utils"), tracing::instrument(level = "info", skip_all, fields(who = %self.context.inbox_id())))]
    #[cfg_attr(not(any(test, feature = "test-utils")), tracing::instrument(skip_all))]
    pub(super) async fn send_welcomes(
        &self,
        action: SendWelcomesAction,
        message_cursor: Option<i64>,
    ) -> Result<(), GroupError> {
        // Only encode welcome metadata once
        let welcome_metadata = WelcomeMetadata {
            message_cursor: message_cursor.unwrap_or(0) as u64,
        };
        let welcome_metadata_bytes = welcome_metadata.encode_to_vec();

        let wp_capable = action
            .installations
            .iter()
            .filter(|installation| {
                installation
                    .welcome_pointee_encryption_aead_types
                    .compatible()
            })
            .count();

        let (welcome_pointer_bytes, welcome_pointee) = if wp_capable
            > xmtp_configuration::INSTALLATION_THRESHOLD_FOR_WELCOME_POINTER_SENDING
        {
            let destination = xmtp_common::rand_array::<32>();
            tracing::debug!(
                wp_capable,
                destination = %hex::encode(destination),
                "Using welcome pointers"
            );
            let symmetric_key = Zeroizing::new(xmtp_common::rand_array::<32>());
            let data_nonce = Zeroizing::new(xmtp_common::rand_array::<12>());
            let mut welcome_metadata_nonce = Zeroizing::new(xmtp_common::rand_array::<12>());
            // ensure that the welcome pointer nonce is different from the data nonce
            while welcome_metadata_nonce == data_nonce {
                welcome_metadata_nonce = Zeroizing::new(xmtp_common::rand_array::<12>());
            }

            let aead_type = crate::groups::mls_ext::WelcomePointersExtension::preferred_type();
            let data = crate::groups::mls_ext::wrap_welcome_symmetric(
                &action.welcome_message,
                aead_type,
                symmetric_key.as_ref(),
                data_nonce.as_ref(),
            )?;
            let welcome_metadata = crate::groups::mls_ext::wrap_welcome_symmetric(
                &welcome_metadata_bytes,
                aead_type,
                symmetric_key.as_ref(),
                welcome_metadata_nonce.as_ref(),
            )?;

            let welcome_pointee = WelcomeMessageInput {
                version: Some(WelcomeMessageInputVersion::V1(WelcomeMessageInputV1 {
                    installation_key: destination.into(),
                    data,
                    hpke_public_key: vec![],
                    wrapper_algorithm: xmtp_proto::xmtp::mls::message_contents::WelcomeWrapperAlgorithm::SymmetricKey.into(),
                    welcome_metadata,
                })),
            };
            let welcome_pointer_bytes = Zeroizing::new(WelcomePointerProto {
                version: Some(
                    xmtp_proto::xmtp::mls::message_contents::welcome_pointer::Version::WelcomeV1Pointer(
                        xmtp_proto::xmtp::mls::message_contents::welcome_pointer::WelcomeV1Pointer {
                            destination: destination.into(),
                            aead_type: xmtp_proto::xmtp::mls::message_contents::WelcomePointeeEncryptionAeadType::Chacha20Poly1305.into(),
                            encryption_key: symmetric_key.as_ref().to_vec(),
                            data_nonce: data_nonce.as_ref().to_vec(),
                            welcome_metadata_nonce: welcome_metadata_nonce.as_ref().to_vec(),
                        },
                    ),
                ),
            }.encode_to_vec());

            (Some(welcome_pointer_bytes), Some(welcome_pointee))
        } else {
            (None, None)
        };

        let total_installations = action.installations.len();

        let welcomes_iter = action.installations.into_iter().map(
            |installation| -> Result<WelcomeMessageInput, WrapWelcomeError> {
                // Unconditionally use the wrapper algorithm for the welcome pointer because it will always be post quantum compatible.
                let algorithm = installation.welcome_wrapper_algorithm;
                let wp_cap = installation.welcome_pointee_encryption_aead_types;
                if let Some(welcome_pointer) = &welcome_pointer_bytes
                    && wp_cap.compatible()
                {
                    Ok(WelcomeMessageInput {
                        version: Some(WelcomeMessageInputVersion::WelcomePointer(
                            WelcomePointerInput {
                                installation_key: installation.installation_key,
                                welcome_pointer: wrap_welcome(
                                    welcome_pointer.as_ref(),
                                    &[],
                                    &installation.hpke_public_key,
                                    algorithm,
                                )?
                                .0,
                                hpke_public_key: installation.hpke_public_key,
                                wrapper_algorithm: algorithm.into(),
                            },
                        )),
                    })
                } else {
                    let installation_key = installation.installation_key;

                    let (data, welcome_metadata) = wrap_welcome(
                        &action.welcome_message,
                        &welcome_metadata_bytes,
                        &installation.hpke_public_key,
                        algorithm,
                    )?;
                    Ok(WelcomeMessageInput {
                        version: Some(WelcomeMessageInputVersion::V1(WelcomeMessageInputV1 {
                            installation_key,
                            data,
                            hpke_public_key: installation.hpke_public_key,
                            wrapper_algorithm: algorithm.into(),
                            welcome_metadata,
                        })),
                    })
                }
            },
        );

        let welcomes = welcome_pointee
            .into_iter()
            .map(Ok)
            .chain(welcomes_iter)
            .collect::<Result<Vec<WelcomeMessageInput>, WrapWelcomeError>>()?;

        assert_eq!(
            welcomes.len(),
            total_installations + usize::from(welcome_pointer_bytes.is_some())
        );

        let welcome = welcomes.first().ok_or(GroupError::NoWelcomesToSend)?;

        // Compute the estimated bytes for one welcome message.
        let welcome_calculated_payload_size = welcome
            .version
            .as_ref()
            .map(|w| match w {
                WelcomeMessageInputVersion::V1(w) => {
                    let size = w.installation_key.len()
                        + w.data.len()
                        + w.hpke_public_key.len()
                        + w.welcome_metadata.len();
                    tracing::debug!("total welcome message proto bytes={size}");
                    size
                }
                WelcomeMessageInputVersion::WelcomePointer(welcome_pointer) => {
                    let size = welcome_pointer.installation_key.len()
                        + welcome_pointer.welcome_pointer.len()
                        + welcome_pointer.hpke_public_key.len();
                    tracing::debug!("total welcome pointer proto bytes={size}");
                    size
                }
            })
            // Fallback if the version is missing
            .unwrap_or(GRPC_PAYLOAD_LIMIT / MAX_GROUP_SIZE);

        // Ensure the denominator is at least 1 to avoid div-by-zero.
        let per_welcome = welcome_calculated_payload_size.max(1);

        // Compute chunk_size and ensure it's at least 1 so chunks(n) won't panic.
        let chunk_size = (GRPC_PAYLOAD_LIMIT / per_welcome).clamp(1, 50);

        tracing::debug!("welcome chunk_size={chunk_size}");
        let api = self.context.api();
        let mut futures = vec![];
        for welcomes in welcomes.chunks(chunk_size) {
            futures.push(api.send_welcome_messages(welcomes));
        }
        try_join_all(futures).await?;
        Ok(())
    }

    /// Provides hmac keys for a range of epochs around current epoch
    /// `group.hmac_keys(-1..=1)`` will provide 3 keys consisting of last epoch, current epoch, and next epoch
    /// `group.hmac_keys(0..=0) will provide 1 key, consisting of only the current epoch
    #[tracing::instrument(level = "trace", skip_all)]
    pub fn hmac_keys(
        &self,
        epoch_delta_range: RangeInclusive<i64>,
    ) -> Result<Vec<HmacKey>, StorageError> {
        let conn = self.context.db();

        let preferences = StoredUserPreferences::load(&conn)?;
        let mut ikm = match preferences.hmac_key {
            Some(ikm) => ikm,
            None => {
                let key = HmacKey::random_key();
                StoredUserPreferences::store_hmac_key(&conn, &key, None)?;
                key
            }
        };
        ikm.extend(&self.group_id);
        let hkdf = Hkdf::<Sha256>::new(Some(HMAC_SALT), &ikm);

        let mut result = vec![];
        let current_epoch = hmac_epoch();
        for delta in epoch_delta_range {
            let epoch = current_epoch + delta;

            let mut info = self.group_id.clone();
            info.extend(&epoch.to_le_bytes());

            let mut key = [0; 42];
            hkdf.expand(&info, &mut key).expect("Length is correct");

            result.push(HmacKey { key, epoch });
        }

        Ok(result)
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub(super) fn prepare_group_messages(
        &self,
        payloads: Vec<(&[u8], bool)>,
    ) -> Result<Vec<GroupMessageInput>, GroupError> {
        let hmac_key = self
            .hmac_keys(0..=0)?
            .pop()
            .expect("Range of count 1 was provided.");
        let sender_hmac =
            Hmac::<Sha256>::new_from_slice(&hmac_key.key).expect("HMAC can take key of any size");

        let mut result = vec![];
        for (payload, should_push) in payloads {
            let mut sender_hmac = sender_hmac.clone();
            sender_hmac.update(payload);
            let sender_hmac = sender_hmac.finalize();

            result.push(GroupMessageInput {
                version: Some(GroupMessageInputVersion::V1(GroupMessageInputV1 {
                    data: payload.to_vec(),
                    sender_hmac: sender_hmac.into_bytes().to_vec(),
                    should_push,
                })),
            });
        }

        Ok(result)
    }
}

// Extracts the message sender, but does not do any validation to ensure that the
// installation_id is actually part of the inbox.
fn extract_message_sender(
    openmls_group: &mut OpenMlsGroup,
    decrypted_message: &ProcessedMessage,
    message_created_ns: u64,
) -> Result<(InboxId, Vec<u8>), GroupMessageProcessingError> {
    if let Sender::Member(leaf_node_index) = decrypted_message.sender()
        && let Some(member) = openmls_group.member_at(*leaf_node_index)
        && member.credential.eq(decrypted_message.credential())
    {
        let basic_credential = BasicCredential::try_from(member.credential)?;
        let sender_inbox_id = parse_credential(basic_credential.identity())?;
        return Ok((sender_inbox_id, member.signature_key));
    }

    let basic_credential = BasicCredential::try_from(decrypted_message.credential().clone())?;
    Err(GroupMessageProcessingError::InvalidSender {
        message_time_ns: message_created_ns,
        credential: basic_credential.identity().to_vec(),
    })
}

async fn calculate_membership_changes_with_keypackages<'a>(
    context: &impl XmtpSharedContext,
    group_id: &[u8],
    new_group_membership: &'a GroupMembership,
    old_group_membership: &'a GroupMembership,
) -> Result<MembershipDiffWithKeyPackages, GroupError> {
    let membership_diff = old_group_membership.diff(new_group_membership);

    let identity = IdentityUpdates::new(&context);
    let mut installation_diff = identity
        .get_installation_diff(
            &context.db(),
            group_id,
            old_group_membership,
            new_group_membership,
            &membership_diff,
        )
        .await?;

    let mut new_installations = Vec::new();
    let mut new_key_packages = Vec::new();
    let mut new_failed_installations = Vec::new();

    if !installation_diff.added_installations.is_empty() {
        get_keypackages_for_installation_ids(
            context,
            installation_diff.added_installations,
            &mut new_installations,
            &mut new_key_packages,
            &mut new_failed_installations,
        )
        .await?;
    }

    let mut failed_installations: HashSet<Vec<u8>> = old_group_membership
        .failed_installations
        .clone()
        .into_iter()
        .chain(new_failed_installations)
        .collect();

    let common: HashSet<_> = failed_installations
        .intersection(&installation_diff.removed_installations)
        .cloned()
        .collect();

    failed_installations.retain(|item| !common.contains(item));

    installation_diff
        .removed_installations
        .retain(|item| !common.contains(item));

    Ok(MembershipDiffWithKeyPackages::new(
        new_installations,
        new_key_packages,
        installation_diff.removed_installations,
        failed_installations.into_iter().collect(),
    ))
}

#[allow(dead_code)]
#[cfg(any(test, feature = "test-utils"))]
async fn inject_failed_installations_for_test(
    key_packages: &mut HashMap<
        Vec<u8>,
        Result<
            crate::verified_key_package_v2::VerifiedKeyPackageV2,
            crate::verified_key_package_v2::KeyPackageVerificationError,
        >,
    >,
    failed_installations: &mut Vec<Vec<u8>>,
) {
    use crate::utils::test_mocks_helpers::{
        get_test_mode_malformed_installations, is_test_mode_upload_malformed_keypackage,
    };
    if is_test_mode_upload_malformed_keypackage() {
        let malformed_installations = get_test_mode_malformed_installations();
        key_packages.retain(|id, _| !malformed_installations.contains(id));
        failed_installations.extend(malformed_installations);
    }
}

async fn get_keypackages_for_installation_ids(
    context: impl XmtpSharedContext,
    requested_installations: HashSet<Vec<u8>>,
    fetched_installations: &mut Vec<Installation>,
    fetched_key_packages: &mut Vec<KeyPackage>,
    failed_installations: &mut Vec<Vec<u8>>,
) -> Result<(), GroupError> {
    let my_installation_id = context.installation_id().to_vec();
    let store = MlsStore::new(context.clone());
    #[allow(unused_mut)]
    let mut key_packages = store
        .get_key_packages_for_installation_ids(
            requested_installations
                .iter()
                .filter(|installation| my_installation_id.ne(*installation))
                .cloned()
                .collect(),
        )
        .await?;

    #[cfg(any(test, feature = "test-utils"))]
    inject_failed_installations_for_test(&mut key_packages, failed_installations).await;

    for (installation_id, result) in key_packages {
        match result {
            Ok(verified_key_package) => {
                fetched_installations.push(Installation::from_verified_key_package(
                    &verified_key_package,
                )?);
                fetched_key_packages.push(verified_key_package.inner.clone());
            }
            Err(_) => failed_installations.push(installation_id.clone()),
        }
    }

    Ok(())
}

fn get_removed_leaf_nodes(
    openmls_group: &mut OpenMlsGroup,
    removed_installations: &HashSet<Vec<u8>>,
) -> Vec<LeafNodeIndex> {
    openmls_group
        .members()
        .filter(|member| removed_installations.contains(&member.signature_key))
        .map(|member| member.index)
        .collect()
}

/// Execute a commit-creating operation using a savepoint pattern.
///
/// This function:
/// 1. Runs the operation in a transaction savepoint
/// 2. Extracts the pending commit data
/// 3. Rolls back the transaction (avoiding the need for clear_pending_commit)
/// 4. Returns the operation result, the staged commit, and the group epoch the commit was created in
///
/// This is more reliable than using `clear_pending_commit` because it uses
/// SQLite's built-in savepoint rollback mechanism.
///
/// The epoch is captured from within the transaction before the operation,
/// ensuring it reflects the state used during the commit creation even if
/// the database is updated between the transaction and when the caller uses it.
pub(super) fn generate_commit_with_rollback<S, R, E, F>(
    storage: &S,
    openmls_group: &mut OpenMlsGroup,
    operation: F,
) -> Result<(R, Option<Vec<u8>>, u64), GroupError>
where
    S: XmtpMlsStorageProvider,
    E: Into<GroupError>,
    F: for<'a> FnOnce(
        &mut OpenMlsGroup,
        &XmtpOpenMlsProviderRef<<S::TxQuery as TransactionalKeyStore>::Store<'a>>,
    ) -> Result<R, E>,
{
    let mut result = None;
    let mut staged_commit = None;
    let mut group_epoch = None;

    let transaction_result = storage.transaction(|conn| {
        let key_store = conn.key_store();
        let provider = XmtpOpenMlsProviderRef::new(&key_store);

        // Capture the epoch before the operation to ensure we have the correct
        // epoch even if the database is updated after the transaction and before we save the intent locally.
        group_epoch = Some(openmls_group.epoch().as_u64());

        // Execute the operation (e.g., self_update, update_group_context_extensions, etc.)
        result = Some(operation(openmls_group, &provider));

        // Extract the staged commit data before rollback
        staged_commit = openmls_group
            .pending_commit()
            .as_ref()
            .map(xmtp_db::db_serialize)
            .transpose()
            .inspect_err(|error| tracing::error!(%error, "Error serializing staged commit"))
            .ok()
            .flatten();

        // Rollback the transaction to avoid persisting the commit
        Err::<(), StorageError>(StorageError::IntentionalRollback)
    });

    let Err(e) = transaction_result else {
        unreachable!("Transaction never returns ok");
    };

    // Check if the transaction was intentionally rolled back (expected)
    // or if there was a real error

    if !matches!(e, StorageError::IntentionalRollback) {
        return Err(e.into());
    }

    // Return early if group epoch is not set otherwise unwrap the group epoch
    let group_epoch = group_epoch.expect("Group epoch should have been captured in transaction");

    // This must go after error checking
    // Reload the group to clear its internal cache after rollback
    openmls_group.reload(storage)?;

    // Extract and handle the operation result
    let operation_result = result
        .expect("Operation should have been called")
        .map_err(|e| e.into())?;

    Ok((operation_result, staged_commit, group_epoch))
}

pub(crate) fn decode_staged_commit(
    data: &[u8],
) -> Result<StagedCommit, GroupMessageProcessingError> {
    Ok(xmtp_db::db_deserialize(data)?)
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use super::*;
    use crate::{builder::ClientBuilder, utils::TestMlsGroup};
    use std::sync::Arc;
    use xmtp_cryptography::utils::generate_local_wallet;

    /// This test is not reproducible in webassembly, b/c webassembly has only one thread.
    #[cfg_attr(
        not(target_arch = "wasm32"),
        tokio::test(flavor = "multi_thread", worker_threads = 10)
    )]
    #[cfg(not(target_family = "wasm"))]
    async fn publish_intents_worst_case_scenario() {
        use crate::tester;

        tester!(amal_a, triggers);
        let amal_group_a: Arc<MlsGroup<_>> =
            Arc::new(amal_a.create_group(None, Default::default()).unwrap());

        let db = amal_a.context.db();

        // create group intent
        amal_group_a.sync().await.unwrap();
        assert_eq!(db.intents_processed(), 1);

        for _ in 0..100 {
            use crate::groups::send_message_opts::SendMessageOpts;

            let s = xmtp_common::rand_string::<100>();
            amal_group_a
                .send_message_optimistic(s.as_bytes(), SendMessageOpts::default())
                .unwrap();
        }

        let mut set = tokio::task::JoinSet::new();
        for _ in 0..50 {
            let g = amal_group_a.clone();
            set.spawn(async move { g.publish_intents().await });
        }

        let res = set.join_all().await;
        let errs: Vec<&Result<_, _>> = res.iter().filter(|r| r.is_err()).collect();
        errs.iter().for_each(|e| {
            tracing::error!("{}", e.as_ref().unwrap_err());
        });

        let published = db.intents_published();
        assert_eq!(published, 101);
        let created = db.intents_created();
        assert_eq!(created, 101);
        if !errs.is_empty() {
            panic!("Errors during publish");
        }
    }

    #[xmtp_common::test]
    async fn hmac_keys_work_as_expected() {
        let wallet = generate_local_wallet();
        let amal = Arc::new(ClientBuilder::new_test_client(&wallet).await);
        let amal_group: Arc<TestMlsGroup> =
            Arc::new(amal.create_group(None, Default::default()).unwrap());

        let hmac_keys = amal_group.hmac_keys(-1..=1).unwrap();
        let current_hmac_key = amal_group.hmac_keys(0..=0).unwrap().pop().unwrap();
        assert_eq!(hmac_keys.len(), 3);
        assert_eq!(hmac_keys[1].key, current_hmac_key.key);
        assert_eq!(hmac_keys[1].epoch, current_hmac_key.epoch);

        // Make sure the keys are different
        assert_ne!(hmac_keys[0].key, hmac_keys[1].key);
        assert_ne!(hmac_keys[0].key, hmac_keys[2].key);
        assert_ne!(hmac_keys[1].key, hmac_keys[2].key);

        // Make sure the epochs align
        let current_epoch = hmac_epoch();
        assert_eq!(hmac_keys[0].epoch, current_epoch - 1);
        assert_eq!(hmac_keys[1].epoch, current_epoch);
        assert_eq!(hmac_keys[2].epoch, current_epoch + 1);
    }

    /// Test that process_delete_message handles completely malformed bytes gracefully
    ///
    /// This verifies sync resilience when receiving corrupted DeleteMessage protos.
    #[xmtp_common::test(unwrap_try = true)]
    async fn test_process_delete_message_malformed_encoded_content() {
        use crate::tester;
        use xmtp_db::group_message::{ContentType, DeliveryStatus, GroupMessageKind};

        tester!(alix);
        let alix_group = alix.create_group(None, None)?;

        // Create a message with completely invalid EncodedContent proto
        let malformed_message = xmtp_db::group_message::StoredGroupMessage {
            id: vec![1, 2, 3],
            group_id: alix_group.group_id.clone(),
            decrypted_message_bytes: vec![0xFF, 0xFE, 0xFD], // Invalid protobuf
            sent_at_ns: xmtp_common::time::now_ns(),
            kind: GroupMessageKind::Application,
            sender_installation_id: vec![1, 2, 3],
            sender_inbox_id: alix.inbox_id().to_string(),
            delivery_status: DeliveryStatus::Published,
            content_type: ContentType::DeleteMessage,
            version_major: 1,
            version_minor: 0,
            authority_id: "xmtp.org".to_string(),
            reference_id: None,
            expire_at_ns: None,
            sequence_id: 1,
            originator_id: 1,
            inserted_at_ns: 0,
            should_push: false,
        };

        // Use load_mls_group_with_lock to get access to the MLS group and call process_delete_message
        let storage = alix.context.mls_storage();
        let result: Result<(), crate::groups::GroupError> =
            alix_group.load_mls_group_with_lock(storage, |mls_group| {
                let inner_result =
                    alix_group.process_delete_message(&mls_group, storage, &malformed_message);
                match inner_result {
                    Ok(()) => Ok(()),
                    Err(_) => Err(crate::groups::GroupError::InvalidGroupMembership),
                }
            });

        assert!(
            result.is_ok(),
            "Malformed EncodedContent should not cause error"
        );
    }

    /// Test that process_delete_message handles valid EncodedContent with malformed inner proto
    #[xmtp_common::test(unwrap_try = true)]
    async fn test_process_delete_message_malformed_inner_proto() {
        use crate::tester;
        use prost::Message;
        use xmtp_db::group_message::{ContentType, DeliveryStatus, GroupMessageKind};
        use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

        tester!(alix);
        let alix_group = alix.create_group(None, None)?;

        // Create a valid EncodedContent wrapper but with invalid inner DeleteMessage content
        let encoded_content = EncodedContent {
            r#type: Some(xmtp_proto::xmtp::mls::message_contents::ContentTypeId {
                authority_id: "xmtp.org".to_string(),
                type_id: "deleteMessage".to_string(),
                version_major: 1,
                version_minor: 0,
            }),
            parameters: std::collections::HashMap::new(),
            fallback: None,
            compression: None,
            content: vec![0xFF, 0xFE, 0xFD], // Invalid DeleteMessage proto bytes
        };

        let mut encoded_bytes = Vec::new();
        encoded_content.encode(&mut encoded_bytes)?;

        let malformed_message = xmtp_db::group_message::StoredGroupMessage {
            id: vec![4, 5, 6],
            group_id: alix_group.group_id.clone(),
            decrypted_message_bytes: encoded_bytes,
            sent_at_ns: xmtp_common::time::now_ns(),
            kind: GroupMessageKind::Application,
            sender_installation_id: vec![1, 2, 3],
            sender_inbox_id: alix.inbox_id().to_string(),
            delivery_status: DeliveryStatus::Published,
            content_type: ContentType::DeleteMessage,
            version_major: 1,
            version_minor: 0,
            authority_id: "xmtp.org".to_string(),
            reference_id: None,
            expire_at_ns: None,
            sequence_id: 2,
            originator_id: 1,
            inserted_at_ns: 0,
            should_push: false,
        };

        let storage = alix.context.mls_storage();
        let result: Result<(), crate::groups::GroupError> =
            alix_group.load_mls_group_with_lock(storage, |mls_group| {
                let inner_result =
                    alix_group.process_delete_message(&mls_group, storage, &malformed_message);
                match inner_result {
                    Ok(()) => Ok(()),
                    Err(_) => Err(crate::groups::GroupError::InvalidGroupMembership),
                }
            });

        assert!(
            result.is_ok(),
            "Malformed inner DeleteMessage proto should not cause error"
        );
    }

    /// Test that process_delete_message handles invalid hex message_id gracefully
    #[xmtp_common::test(unwrap_try = true)]
    async fn test_process_delete_message_invalid_hex_message_id() {
        use crate::tester;
        use prost::Message;
        use xmtp_db::group_message::{ContentType, DeliveryStatus, GroupMessageKind};
        use xmtp_proto::xmtp::mls::message_contents::EncodedContent;
        use xmtp_proto::xmtp::mls::message_contents::content_types::DeleteMessage;

        tester!(alix);
        let alix_group = alix.create_group(None, None)?;

        // Create a valid DeleteMessage but with invalid hex in message_id
        let delete_msg = DeleteMessage {
            message_id: "not_valid_hex!!!".to_string(), // Invalid hex
        };

        let mut delete_bytes = Vec::new();
        delete_msg.encode(&mut delete_bytes)?;

        let encoded_content = EncodedContent {
            r#type: Some(xmtp_proto::xmtp::mls::message_contents::ContentTypeId {
                authority_id: "xmtp.org".to_string(),
                type_id: "deleteMessage".to_string(),
                version_major: 1,
                version_minor: 0,
            }),
            parameters: std::collections::HashMap::new(),
            fallback: None,
            compression: None,
            content: delete_bytes,
        };

        let mut encoded_bytes = Vec::new();
        encoded_content.encode(&mut encoded_bytes)?;

        let message_with_bad_hex = xmtp_db::group_message::StoredGroupMessage {
            id: vec![7, 8, 9],
            group_id: alix_group.group_id.clone(),
            decrypted_message_bytes: encoded_bytes,
            sent_at_ns: xmtp_common::time::now_ns(),
            kind: GroupMessageKind::Application,
            sender_installation_id: vec![1, 2, 3],
            sender_inbox_id: alix.inbox_id().to_string(),
            delivery_status: DeliveryStatus::Published,
            content_type: ContentType::DeleteMessage,
            version_major: 1,
            version_minor: 0,
            authority_id: "xmtp.org".to_string(),
            reference_id: None,
            expire_at_ns: None,
            sequence_id: 3,
            originator_id: 1,
            inserted_at_ns: 0,
            should_push: false,
        };

        let storage = alix.context.mls_storage();
        let result: Result<(), crate::groups::GroupError> =
            alix_group.load_mls_group_with_lock(storage, |mls_group| {
                let inner_result =
                    alix_group.process_delete_message(&mls_group, storage, &message_with_bad_hex);
                match inner_result {
                    Ok(()) => Ok(()),
                    Err(_) => Err(crate::groups::GroupError::InvalidGroupMembership),
                }
            });

        assert!(
            result.is_ok(),
            "Invalid hex message_id should not cause error"
        );
    }
}

/// Collects events that should be sent after database transactions complete
#[derive(Default)]
pub struct DeferredEvents {
    worker_events: VecDeque<SyncWorkerEvent>,
    local_events: VecDeque<LocalEvents>,
}

impl DeferredEvents {
    pub fn new() -> Self {
        Self {
            worker_events: VecDeque::new(),
            local_events: VecDeque::new(),
        }
    }

    pub fn add_worker_event(&mut self, event: SyncWorkerEvent) {
        self.worker_events.push_back(event);
    }

    pub fn add_local_event(&mut self, event: LocalEvents) {
        self.local_events.push_back(event);
    }

    /// Send all collected events to their respective channels
    pub fn send_all<Context: XmtpSharedContext>(&mut self, context: &Context) {
        while let Some(event) = self.worker_events.pop_front() {
            let _ = context.worker_events().send(event);
        }

        while let Some(event) = self.local_events.pop_front() {
            let _ = context.local_events().send(event);
        }
    }
}
