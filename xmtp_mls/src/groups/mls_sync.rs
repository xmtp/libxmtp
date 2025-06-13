use super::{
    build_extensions_for_admin_lists_update, build_extensions_for_metadata_update,
    build_extensions_for_permissions_update, build_group_membership_extension,
    intents::{
        Installation, IntentError, PostCommitAction, SendMessageIntentData, SendWelcomesAction,
        UpdateAdminListIntentData, UpdateGroupMembershipIntentData, UpdatePermissionIntentData,
    },
    summary::{MessageIdentifier, MessageIdentifierBuilder, ProcessSummary, SyncSummary},
    validated_commit::{extract_group_membership, CommitValidationError, LibXMTPVersion},
    GroupError, HmacKey, MlsGroup,
};
use crate::{
    client::ClientError, mls_store::MlsStore,
    subscriptions::stream_messages::extract_message_cursor,
};
use crate::{
    configuration::sync_update_installations_interval_ns, identity_updates::IdentityUpdates,
};
use crate::{
    configuration::{
        GRPC_DATA_LIMIT, HMAC_SALT, MAX_GROUP_SIZE, MAX_INTENT_PUBLISH_ATTEMPTS, MAX_PAST_EPOCHS,
    },
    context::XmtpMlsLocalContext,
    groups::{
        device_sync_legacy::DeviceSyncContent, intents::UpdateMetadataIntentData,
        validated_commit::ValidatedCommit,
    },
    hpke::{encrypt_welcome, HpkeError},
    identity::{parse_credential, IdentityError},
    identity_updates::load_identity_updates,
    intents::ProcessIntentError,
    subscriptions::LocalEvents,
    utils::{self, hash::sha256, id::calculate_message_id, time::hmac_epoch},
};
use crate::{
    context::XmtpContextProvider,
    groups::device_sync_legacy::preference_sync_legacy::process_incoming_preference_update,
};
use crate::{
    groups::group_membership::{GroupMembership, MembershipDiffWithKeyPackages},
    utils::id::calculate_message_id_for_intent,
};
use crate::{
    groups::mls_ext::MergeStagedCommitAndLog,
    verified_key_package_v2::{KeyPackageVerificationError, VerifiedKeyPackageV2},
};
use crate::{subscriptions::SyncWorkerEvent, track};
use xmtp_api::XmtpApi;
use xmtp_db::{
    group::{ConversationType, StoredGroup},
    group_intent::{IntentKind, IntentState, StoredGroupIntent, ID},
    group_message::{ContentType, DeliveryStatus, GroupMessageKind, StoredGroupMessage},
    local_commit_log::LocalCommitLog,
    refresh_state::EntityKind,
    remote_commit_log::CommitResult,
    sql_key_store::{self, SqlKeyStore},
    user_preferences::StoredUserPreferences,
    ConnectionExt, Fetch, MlsProviderExt, StorageError, Store, StoreOrIgnore, XmtpDb,
};
use xmtp_mls_common::group_mutable_metadata::MetadataField;

use crate::groups::mls_sync::GroupMessageProcessingError::OpenMlsProcessMessage;
use futures::future::try_join_all;
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use openmls::group::{ProcessMessageError, ValidationError};
use openmls::{
    credentials::BasicCredential,
    extensions::Extensions,
    framing::{ContentType as MlsContentType, ProtocolMessage},
    group::{GroupEpoch, StagedCommit},
    prelude::{
        tls_codec::{Deserialize, Error as TlsCodecError, Serialize},
        LeafNodeIndex, MlsGroup as OpenMlsGroup, MlsMessageBodyIn, MlsMessageIn, PrivateMessageIn,
        ProcessedMessage, ProcessedMessageContent, Sender,
    },
    treesync::LeafNodeParameters,
};
use openmls::{framing::WireFormat, prelude::BasicCredentialError};
use openmls_traits::{signatures::Signer, OpenMlsProvider};
use prost::bytes::Bytes;
use prost::Message;
use sha2::Sha256;
use std::sync::Arc;
use std::{
    collections::{HashMap, HashSet},
    mem::{discriminant, Discriminant},
    ops::RangeInclusive,
};
use thiserror::Error;
use tracing::debug;
use xmtp_common::{retry_async, Retry, RetryableError};
use xmtp_content_types::{group_updated::GroupUpdatedCodec, CodecError, ContentCodec};
use xmtp_db::{group_intent::IntentKind::MetadataUpdate, NotFound};
use xmtp_id::{InboxId, InboxIdRef};
use xmtp_proto::xmtp::mls::message_contents::{group_updated, WelcomeWrapperAlgorithm};
use xmtp_proto::xmtp::mls::{
    api::v1::{
        group_message::{Version as GroupMessageVersion, V1 as GroupMessageV1},
        group_message_input::{Version as GroupMessageInputVersion, V1 as GroupMessageInputV1},
        welcome_message_input::{
            Version as WelcomeMessageInputVersion, V1 as WelcomeMessageInputV1,
        },
        GroupMessage, GroupMessageInput, WelcomeMessageInput,
    },
    message_contents::{
        plaintext_envelope::{v2::MessageType, Content, V1, V2},
        GroupUpdated, PlaintextEnvelope,
    },
};

#[derive(Debug, Error)]
pub enum GroupMessageProcessingError {
    #[error("message with cursor [{}] for group [{}] already processed", _0.cursor, xmtp_common::fmt::debug_hex(&_0.group_id))]
    MessageAlreadyProcessed(MessageIdentifier),
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
    OpenMlsProcessMessage(#[from] openmls::prelude::ProcessMessageError),
    #[error("merge staged commit: {0}")]
    MergeStagedCommit(#[from] openmls::group::MergeCommitError<sql_key_store::SqlKeyStoreError>),
    #[error("TLS Codec error: {0}")]
    TlsError(#[from] TlsCodecError),
    #[error("unsupported message type: {0:?}")]
    UnsupportedMessageType(Discriminant<MlsMessageBodyIn>),
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
}

impl RetryableError for GroupMessageProcessingError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Storage(err) => err.is_retryable(),
            Self::Identity(err) => err.is_retryable(),
            Self::OpenMlsProcessMessage(err) => err.is_retryable(),
            Self::MergeStagedCommit(err) => err.is_retryable(),
            Self::ProcessIntent(err) => err.is_retryable(),
            Self::CommitValidation(err) => err.is_retryable(),
            Self::ClearPendingCommit(err) => err.is_retryable(),
            Self::Client(err) => err.is_retryable(),
            Self::Db(e) => e.is_retryable(),
            Self::WrongCredentialType(_)
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

#[derive(Debug)]
struct PublishIntentData {
    staged_commit: Option<Vec<u8>>,
    post_commit_action: Option<Vec<u8>>,
    payload_to_publish: Vec<u8>,
    should_send_push_notification: bool,
}

impl<ApiClient, Db> MlsGroup<ApiClient, Db>
where
    ApiClient: XmtpApi,
    Db: XmtpDb,
{
    #[tracing::instrument]
    pub async fn sync(&self) -> Result<SyncSummary, GroupError> {
        let conn = self.context.db();

        let epoch = self.epoch().await?;
        tracing::info!(
            inbox_id = self.context.inbox_id(),
            installation_id = %self.context.installation_id(),
            group_id = hex::encode(&self.group_id),
            epoch = epoch,
            "[{}] syncing group, epoch = {}",
            self.context.inbox_id(),
            epoch
        );

        // Also sync the "stitched DMs", if any...
        for other_dm in conn.other_dms(&self.group_id)? {
            let other_dm = Self::new_from_arc(
                self.context.clone(),
                other_dm.id,
                other_dm.dm_id.clone(),
                other_dm.created_at_ns,
            );
            other_dm.maybe_update_installations(None).await?;
            other_dm.sync_with_conn().await?;
        }

        self.maybe_update_installations(None).await?;
        self.sync_with_conn().await.map_err(GroupError::from)
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
    #[tracing::instrument(skip_all)]
    pub async fn sync_with_conn(&self) -> Result<SyncSummary, SyncSummary> {
        let _mutex = self.mutex.lock().await;
        let mut summary = SyncSummary::default();

        if let Err(e) = self.handle_group_paused() {
            if matches!(e, GroupError::GroupPausedUntilUpdate(_)) {
                // nothing synced
                return Ok(summary);
            } else {
                return Err(SyncSummary::other(e));
            }
        }

        // Even if publish fails, continue to receiving
        if let Err(e) = self.publish_intents().await {
            tracing::error!("Sync: error publishing intents {e:?}",);
            summary.add_publish_err(e);
        }

        // Even if receiving fails, we continue to post_commit
        // Errors are collected in the summary.
        match self.receive().await {
            Ok(s) => summary.add_process(s),
            Err(e) => {
                summary.add_other(e);
                // We don't return an error if receive fails, because it's possible this is caused
                // by malicious data sent over the network, or messages from before the user was
                // added to the group
            }
        }

        if let Err(e) = self.post_commit().await {
            tracing::error!("post commit error {e:?}",);
            summary.add_post_commit_err(e);
        }

        if summary.is_errored() {
            Err(summary)
        } else {
            Ok(summary)
        }
    }

    #[tracing::instrument(skip_all)]
    pub(super) async fn sync_until_last_intent_resolved(&self) -> Result<SyncSummary, GroupError> {
        let intents = self.mls_provider().db().find_group_intents(
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
     * This method will retry up to `crate::configuration::MAX_GROUP_SYNC_RETRIES` times.
     */
    #[tracing::instrument(level = "debug", skip_all)]
    pub(super) async fn sync_until_intent_resolved(
        &self,
        intent_id: ID,
    ) -> Result<SyncSummary, GroupError> {
        let provider = self.mls_provider();
        let mut num_attempts = 0;
        let mut summary = SyncSummary::default();
        // Return the last error to the caller if we fail to sync
        while num_attempts < crate::configuration::MAX_GROUP_SYNC_RETRIES {
            match self.sync_with_conn().await {
                Ok(s) => summary.extend(s),
                Err(s) => {
                    tracing::error!("error syncing group {}", s);
                    summary.extend(s);
                }
            }
            match Fetch::<StoredGroupIntent>::fetch(provider.db(), &intent_id) {
                Ok(None) => {
                    // This is expected. The intent gets deleted on success
                    return Ok(summary);
                }
                Ok(Some(StoredGroupIntent {
                    id,
                    state: IntentState::Error,
                    ..
                })) => {
                    tracing::warn!(
                        "not retrying intent ID {id}. since it is in state Error. {:?}",
                        summary
                    );
                    return Err(GroupError::from(summary));
                }
                Ok(Some(StoredGroupIntent {
                    id,
                    state: IntentState::Processed,
                    ..
                })) => {
                    tracing::debug!(
                        "not retrying intent ID {id}. since it is in state processed. {}",
                        summary
                    );
                    return Ok(summary);
                }
                Ok(Some(StoredGroupIntent { id, state, .. })) => {
                    tracing::warn!("retrying intent ID {id}. intent currently in state {state:?}");
                }
                Err(err) => {
                    tracing::error!("database error fetching intent {:?}", err);
                    summary.add_other(GroupError::Storage(err));
                }
            };
            num_attempts += 1;
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
        utils::maybe_mock_future_epoch_for_tests()?;

        if message_epoch.as_u64() + max_past_epochs as u64 <= group_epoch.as_u64() {
            tracing::warn!(
                inbox_id,
                message_epoch = message_epoch.as_u64(),
                group_epoch = group_epoch.as_u64(),
                intent_id,
                "[{}] own message epoch {} is {} or more less than the group epoch {} for intent {}. Retrying message",
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
                "[{}] own message epoch {} is greater than group epoch {} for intent {}. Retrying message",
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
        message: &ProtocolMessage,
        envelope: &GroupMessageV1,
    ) -> Result<
        Option<(StagedCommit, ValidatedCommit)>,
        Result<IntentState, GroupMessageProcessingError>,
    > {
        let GroupMessageV1 {
            created_ns: envelope_timestamp_ns,
            id: ref cursor,
            ..
        } = *envelope;

        let group_epoch = mls_group.epoch();
        let message_epoch = message.epoch();

        match intent.kind {
            IntentKind::KeyUpdate
            | IntentKind::UpdateGroupMembership
            | IntentKind::UpdateAdminList
            | IntentKind::MetadataUpdate
            | IntentKind::UpdatePermission => {
                if let Some(published_in_epoch) = intent.published_in_epoch {
                    let group_epoch = group_epoch.as_u64() as i64;

                    if published_in_epoch != group_epoch {
                        tracing::warn!(
                            inbox_id = self.context.inbox_id(),
                            installation_id = %self.context.installation_id(),
                            group_id = hex::encode(&self.group_id),
                            cursor,
                            intent.id,
                            intent.kind = %intent.kind,
                            "Intent for msg = [{cursor}] was published in epoch {} but group is currently in epoch {}",
                            published_in_epoch,
                            group_epoch
                        );

                        return Err(Ok(IntentState::ToPublish));
                    }

                    let staged_commit = if let Some(staged_commit) = &intent.staged_commit {
                        match decode_staged_commit(staged_commit) {
                            Err(err) => return Err(Err(err)),
                            Ok(staged_commit) => staged_commit,
                        }
                    } else {
                        return Err(Err(GroupMessageProcessingError::IntentMissingStagedCommit));
                    };

                    tracing::info!(
                        "[{}] Validating commit for intent {}. Message timestamp: {envelope_timestamp_ns}",
                        self.context().inbox_id(),
                        intent.id
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
                                "Error validating commit for own message. Intent ID [{}]: {err:?}",
                                intent.id,
                            );
                            // Return before merging commit since it does not pass validation
                            // Return OK so that the group intent update is still written to the DB
                            return Err(Ok(IntentState::Error));
                        }
                        Ok(validated_commit) => validated_commit,
                    };

                    return Ok(Some((staged_commit, validated_commit)));
                }
            }

            IntentKind::SendMessage => {
                Self::validate_message_epoch(
                    self.context().inbox_id(),
                    intent.id,
                    group_epoch,
                    message_epoch,
                    MAX_PAST_EPOCHS,
                )
                .map_err(|_| Ok(IntentState::ToPublish))?;
            }
        }

        Ok(None)
    }

    #[allow(clippy::too_many_arguments)]
    #[tracing::instrument(level = "debug", skip(mls_group, commit, intent, message, envelope))]
    fn process_own_message(
        &self,
        mls_group: &mut OpenMlsGroup,
        commit: Option<(StagedCommit, ValidatedCommit)>,
        intent: &StoredGroupIntent,
        message: &ProtocolMessage,
        envelope: &GroupMessageV1,
    ) -> Result<(IntentState, Option<Vec<u8>>), GroupMessageProcessingError> {
        if intent.state == IntentState::Committed {
            return Ok((IntentState::Committed, None));
        }

        let provider = self.mls_provider();
        let conn = provider.db();
        let message_epoch = message.epoch();
        let GroupMessageV1 {
            created_ns: envelope_timestamp_ns,
            id: ref cursor,
            ..
        } = *envelope;

        tracing::debug!(
            inbox_id = self.context.inbox_id(),
            installation_id = %self.context.installation_id(),
            group_id = hex::encode(&self.group_id),
            cursor,
            intent.id,
            intent.kind = %intent.kind,
            "[{}]-[{}] processing own message for intent {} / {}, message_epoch: {}",
            self.context().inbox_id(),
            hex::encode(self.group_id.clone()),
            intent.id,
            intent.kind,
            message_epoch.clone()
        );

        if let Some((staged_commit, validated_commit)) = commit {
            tracing::info!(
                "[{}] merging pending commit for intent {}",
                self.context().inbox_id(),
                intent.id
            );

            if mls_group
                .merge_staged_commit_and_log(&provider, staged_commit, &validated_commit)
                .is_err()
            {
                return Ok((IntentState::ToPublish, None));
            }

            track!(
                "Epoch Change",
                {
                    "cursor": cursor,
                    "prev_epoch": message_epoch.as_u64(),
                    "new_epoch": mls_group.epoch().as_u64(),
                    "validated_commit": Some(&validated_commit)
                        .and_then(|c| serde_json::to_string_pretty(c).ok())
                },
                group: &envelope.group_id
            );

            // If no error committing the change, write a transcript message
            let msg =
                self.save_transcript_message(validated_commit, envelope_timestamp_ns, *cursor)?;
            return Ok((IntentState::Committed, msg.map(|m| m.id)));
        } else if let Some(id) = calculate_message_id_for_intent(intent)? {
            tracing::debug!("setting message @cursor=[{}] to published", envelope.id);
            conn.set_delivery_status_to_published(&id, envelope_timestamp_ns, envelope.id as i64)?;
            return Ok((IntentState::Processed, Some(id)));
        }
        // TODO: Should clarify when this would happen
        Ok((IntentState::Committed, None))
    }

    #[tracing::instrument(level = "debug", skip(mls_group, message, envelope))]
    async fn validate_and_process_external_message(
        &self,
        mls_group: &mut OpenMlsGroup,
        message: PrivateMessageIn,
        envelope: &GroupMessageV1,
        allow_cursor_increment: bool,
    ) -> Result<MessageIdentifier, GroupMessageProcessingError> {
        #[cfg(any(test, feature = "test-utils"))]
        {
            use crate::utils::maybe_mock_wrong_epoch_for_tests;
            maybe_mock_wrong_epoch_for_tests()?;
        }

        let GroupMessageV1 {
            created_ns: envelope_timestamp_ns,
            id: ref cursor,
            ..
        } = *envelope;
        let mut identifier = MessageIdentifierBuilder::from(envelope);

        let provider = self.mls_provider();
        // We need to process the message twice to avoid an async transaction.
        // We'll process for the first time, get the processed message,
        // and roll the transaction back, so we can fetch updates from the server before
        // being ready to process the message for a second time.
        let mut processed_message = None;
        let result = provider.transaction(|provider| {
            processed_message = Some(mls_group.process_message(&provider, message.clone()));
            // Rollback the transaction. We want to synchronize with the server before committing.
            Err::<(), StorageError>(StorageError::IntentionalRollback)
        });
        if !matches!(result, Err(StorageError::IntentionalRollback)) {
            result.inspect_err(|e| tracing::debug!("immutable process message failed {}", e))?;
        }
        let processed_message = processed_message.expect("Was just set to Some")?;

        // Reload the mlsgroup to clear the it's internal cache
        *mls_group = OpenMlsGroup::load(provider.storage(), mls_group.group_id())?.ok_or(
            GroupMessageProcessingError::Storage(StorageError::NotFound(NotFound::MlsGroup)),
        )?;

        let (sender_inbox_id, sender_installation_id) =
            extract_message_sender(mls_group, &processed_message, envelope_timestamp_ns)?;

        tracing::info!(
            inbox_id = self.context.inbox_id(),
            installation_id = %self.context.installation_id(),sender_inbox_id = sender_inbox_id,
            sender_installation_id = hex::encode(&sender_installation_id),
            group_id = hex::encode(&self.group_id),
            current_epoch = mls_group.epoch().as_u64(),
            msg_epoch = processed_message.epoch().as_u64(),
            msg_group_id = hex::encode(processed_message.group_id().as_slice()),
            cursor,
            "[{}] extracted sender inbox id: {}",
            self.context.inbox_id(),
            sender_inbox_id
        );

        let validated_commit = match &processed_message.content() {
            ProcessedMessageContent::StagedCommitMessage(staged_commit) => {
                let validated_commit =
                    ValidatedCommit::from_staged_commit(&self.context, staged_commit, mls_group)
                        .await?;
                identifier.group_context(staged_commit.group_context().clone());
                Some(validated_commit)
            }
            _ => None,
        };

        let identifier = self.mls_provider().transaction(|provider| {
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
                tracing::info!(
                    "calling update cursor for group {}, with cursor {}, allow_cursor_increment is true",
                    hex::encode(envelope.group_id.as_slice()),
                    *cursor
                );
                provider.db().update_cursor(
                    &envelope.group_id,
                    EntityKind::Group,
                    *cursor as i64,
                )?
            } else {
                tracing::info!(
                    "will not call update cursor for group {}, with cursor {}, allow_cursor_increment is false",
                    hex::encode(envelope.group_id.as_slice()),
                    *cursor
                );
                let current_cursor = provider
                    .db()
                    .get_last_cursor_for_id(&envelope.group_id, EntityKind::Group)?;
                current_cursor < *cursor as i64
            };
            if !requires_processing {
                // early return if the message is already procesed
                // _NOTE_: Not early returning and re-processing a message that
                // has already been processed, has the potential to result in forks.
                tracing::debug!("message @cursor=[{}] for group=[{}] created_at=[{}] no longer require processing, should be available in database",
                    envelope.id,
                    xmtp_common::fmt::debug_hex(&envelope.group_id),
                    envelope.created_ns
                 );
                identifier.previously_processed(true);
                return identifier.build();
            }
            // once the checks for processing pass, actually process the message
            let processed_message = mls_group.process_message(&provider, message)?;
            let previous_epoch = mls_group.epoch().as_u64();
            let identifier = self.process_external_message(
                mls_group,
                processed_message,
                envelope,
                validated_commit.clone(),
            )?;
            let new_epoch = mls_group.epoch().as_u64();
            if new_epoch > previous_epoch {
                track!(
                    "Epoch Change",
                    {
                        "cursor": *cursor as i64,
                        "prev_epoch": previous_epoch as i64,
                        "new_epoch": new_epoch as i64,
                        "validated_commit": validated_commit.as_ref().and_then(|c| serde_json::to_string_pretty(c).ok())
                    },
                    group: &envelope.group_id
                );
                tracing::info!(
                    "[{}] externally processed message [{}] advanced epoch from [{}] to [{}]",
                    self.context.inbox_id(),
                    cursor,
                    previous_epoch,
                    new_epoch
                );
            }
            Ok::<_, GroupMessageProcessingError>(identifier)
        })?;

        Ok(identifier)
    }

    /// Process an external message
    /// returns a MessageIdentifier, identifiying the message processed if any.
    #[tracing::instrument(
        level = "debug",
        skip(mls_group, processed_message, envelope, validated_commit)
    )]
    fn process_external_message(
        &self,
        mls_group: &mut OpenMlsGroup,
        processed_message: ProcessedMessage,
        envelope: &GroupMessageV1,
        validated_commit: Option<ValidatedCommit>,
    ) -> Result<MessageIdentifier, GroupMessageProcessingError> {
        let GroupMessageV1 {
            created_ns: envelope_timestamp_ns,
            id: ref cursor,
            ..
        } = *envelope;
        let provider = self.mls_provider();
        let msg_epoch = processed_message.epoch().as_u64();
        let msg_group_id = hex::encode(processed_message.group_id().as_slice());
        let (sender_inbox_id, sender_installation_id) =
            extract_message_sender(mls_group, &processed_message, envelope_timestamp_ns)?;

        let mut identifier = MessageIdentifierBuilder::from(envelope);
        match processed_message.into_content() {
            ProcessedMessageContent::ApplicationMessage(application_message) => {
                tracing::info!(
                    inbox_id = self.context.inbox_id(),
                    sender_inbox_id = sender_inbox_id,
                    sender_installation_id = hex::encode(&sender_installation_id),
                    installation_id = %self.context.installation_id(),group_id = hex::encode(&self.group_id),
                    current_epoch = mls_group.epoch().as_u64(),
                    msg_epoch,
                    msg_group_id,
                    cursor,
                    "[{}] decoding application message",
                    self.context().inbox_id()
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
                            sent_at_ns: envelope_timestamp_ns as i64,
                            kind: GroupMessageKind::Application,
                            sender_installation_id,
                            sender_inbox_id: sender_inbox_id.clone(),
                            delivery_status: DeliveryStatus::Published,
                            content_type: queryable_content_fields.content_type,
                            version_major: queryable_content_fields.version_major,
                            version_minor: queryable_content_fields.version_minor,
                            authority_id: queryable_content_fields.authority_id,
                            reference_id: queryable_content_fields.reference_id,
                            sequence_id: Some(*cursor as i64),
                            originator_id: None,
                        };
                        message.store_or_ignore(provider.db())?;
                        // make sure internal id is on return type after its stored successfully
                        identifier.internal_id(message_id);

                        // If this message was sent by us on another installation, check if it
                        // belongs to a sync group, and if it is - notify the worker.
                        if sender_inbox_id == self.context.inbox_id() {
                            if let Some(StoredGroup {
                                conversation_type: ConversationType::Sync,
                                ..
                            }) = provider.db().find_group(&self.group_id)?
                            {
                                let _ = self
                                    .context
                                    .worker_events()
                                    .send(SyncWorkerEvent::NewSyncGroupMsg);
                            }
                        }
                        Ok::<_, GroupMessageProcessingError>(())
                    }
                    Some(Content::V2(V2 {
                        idempotency_key,
                        message_type,
                    })) => {
                        match message_type {
                            Some(MessageType::DeviceSyncRequest(history_request)) => {
                                let content = DeviceSyncContent::Request(history_request);
                                let content_bytes = serde_json::to_vec(&content)?;
                                let message_id = calculate_message_id(
                                    &self.group_id,
                                    &content_bytes,
                                    &idempotency_key,
                                );

                                // store the request message
                                let message = StoredGroupMessage {
                                    id: message_id.clone(),
                                    group_id: self.group_id.clone(),
                                    decrypted_message_bytes: content_bytes,
                                    sent_at_ns: envelope_timestamp_ns as i64,
                                    kind: GroupMessageKind::Application,
                                    sender_installation_id,
                                    sender_inbox_id: sender_inbox_id.clone(),
                                    delivery_status: DeliveryStatus::Published,
                                    content_type: ContentType::Unknown,
                                    version_major: 0,
                                    version_minor: 0,
                                    authority_id: "unknown".to_string(),
                                    reference_id: None,
                                    sequence_id: Some(*cursor as i64),
                                    originator_id: None,
                                };
                                message.store_or_ignore(provider.db())?;
                                identifier.internal_id(message_id.clone());

                                tracing::info!("Received a history request.");
                                let _ = self
                                    .context
                                    .worker_events()
                                    .send(SyncWorkerEvent::Request { message_id });
                                Ok(())
                            }
                            Some(MessageType::DeviceSyncReply(history_reply)) => {
                                let content = DeviceSyncContent::Reply(history_reply);
                                let content_bytes = serde_json::to_vec(&content)?;
                                let message_id = calculate_message_id(
                                    &self.group_id,
                                    &content_bytes,
                                    &idempotency_key,
                                );

                                // store the reply message
                                let message = StoredGroupMessage {
                                    id: message_id.clone(),
                                    group_id: self.group_id.clone(),
                                    decrypted_message_bytes: content_bytes,
                                    sent_at_ns: envelope_timestamp_ns as i64,
                                    kind: GroupMessageKind::Application,
                                    sender_installation_id,
                                    sender_inbox_id,
                                    delivery_status: DeliveryStatus::Published,
                                    content_type: ContentType::Unknown,
                                    version_major: 0,
                                    version_minor: 0,
                                    authority_id: "unknown".to_string(),
                                    reference_id: None,
                                    sequence_id: Some(*cursor as i64),
                                    originator_id: None,
                                };
                                message.store_or_ignore(provider.db())?;
                                identifier.internal_id(message_id.clone());

                                tracing::info!("Received a history reply.");
                                let _ = self
                                    .context
                                    .worker_events()
                                    .send(SyncWorkerEvent::Reply { message_id });
                                Ok(())
                            }
                            Some(MessageType::UserPreferenceUpdate(update)) => {
                                // This function inserts the updates appropriately,
                                // and returns a copy of what was inserted
                                let updates =
                                    process_incoming_preference_update(update, &self.context)?;

                                // Broadcast those updates for integrators to be notified of changes
                                let _ = self
                                    .context
                                    .local_events()
                                    .send(LocalEvents::PreferencesChanged(updates));
                                Ok(())
                            }
                            _ => {
                                return Err(GroupMessageProcessingError::InvalidPayload);
                            }
                        }
                    }
                    None => {
                        return Err(GroupMessageProcessingError::InvalidPayload);
                    }
                }
            }
            ProcessedMessageContent::ProposalMessage(_proposal_ptr) => {
                Ok(())
                // intentionally left blank.
            }
            ProcessedMessageContent::ExternalJoinProposalMessage(_external_proposal_ptr) => {
                Ok(())
                // intentionally left blank.
            }
            ProcessedMessageContent::StagedCommitMessage(staged_commit) => {
                let staged_commit = *staged_commit;
                let validated_commit =
                    validated_commit.expect("Needs to be present when this is a staged commit");

                tracing::info!(
                    inbox_id = self.context.inbox_id(),
                    sender_inbox_id = sender_inbox_id,
                    installation_id = %self.context.installation_id(),sender_installation_id = hex::encode(&sender_installation_id),
                    group_id = hex::encode(&self.group_id),
                    current_epoch = mls_group.epoch().as_u64(),
                    msg_epoch,
                    msg_group_id,
                    cursor,
                    "[{}] received staged commit. Merging and clearing any pending commits",
                    self.context().inbox_id()
                );

                tracing::info!(
                    inbox_id = self.context.inbox_id(),
                    sender_inbox_id = sender_inbox_id,
                    installation_id = %self.context.installation_id(),sender_installation_id = hex::encode(&sender_installation_id),
                    group_id = hex::encode(&self.group_id),
                    current_epoch = mls_group.epoch().as_u64(),
                    msg_epoch,
                    msg_group_id,
                    cursor,
                    "[{}] staged commit is valid, will attempt to merge",
                    self.context().inbox_id()
                );
                identifier.group_context(staged_commit.group_context().clone());

                mls_group.merge_staged_commit_and_log(
                    &provider,
                    staged_commit,
                    &validated_commit,
                )?;

                let msg =
                    self.save_transcript_message(validated_commit, envelope_timestamp_ns, *cursor)?;
                identifier.internal_id(msg.as_ref().map(|m| m.id.clone()));
                Ok(())
            }
        }?;
        identifier.build()
    }

    /// This function is idempotent. No need to wrap in a transaction.
    ///
    /// # Parameters
    /// * `envelope` - The message envelope to process
    /// * `trust_message_order` - Controls whether to allow epoch increments from commits and msg cursor increments.
    ///   Set to `true` when processing messages from trusted ordered sources (queries), and `false` when
    ///   processing from potentially out-of-order sources like streams.
    #[tracing::instrument(skip(envelope), level = "debug")]
    pub(crate) async fn process_message(
        &self,
        envelope: &GroupMessageV1,
        trust_message_order: bool,
    ) -> Result<MessageIdentifier, GroupMessageProcessingError> {
        let provider = self.mls_provider();
        let allow_epoch_increment = trust_message_order;
        let allow_cursor_increment = trust_message_order;
        let cursor = envelope.id;
        let mls_message_in = MlsMessageIn::tls_deserialize_exact(&envelope.data)?;

        let message = match mls_message_in.extract() {
            MlsMessageBodyIn::PrivateMessage(message) => Ok(message),
            other => Err(GroupMessageProcessingError::UnsupportedMessageType(
                discriminant(&other),
            )),
        }?;
        if !allow_epoch_increment && message.content_type() == MlsContentType::Commit {
            return Err(GroupMessageProcessingError::EpochIncrementNotAllowed);
        }

        let intent = provider
            .db()
            .find_group_intent_by_payload_hash(&sha256(envelope.data.as_slice()))
            .map_err(GroupMessageProcessingError::Storage)?;
        tracing::info!(
            inbox_id = self.context.inbox_id(),
            installation_id = %self.context.installation_id(),
            group_id = hex::encode(&self.group_id),
            cursor = envelope.id,
            "Processing envelope with hash {}, sequence_id = {}, is_own_intent={}",
            hex::encode(sha256(envelope.data.as_slice())),
            envelope.id,
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
                    cursor = envelope.id,
                    intent_id,
                    intent.kind = %intent.kind,
                    "client [{}] is about to process own envelope [{}] for intent [{}] [{}]",
                    self.context.inbox_id(),
                    envelope.id,
                    intent_id,
                    intent.kind
                );

                let identifier = self.load_mls_group_with_lock_async(|mut mls_group| async move {
                    let message = message.into();
                    // TODO: the return type of stage_and_validated_commit should be fixed
                    // We should never be nesting Result types within the error return type, it is
                    // super confusing. Instead of failing one way, this function can now fail
                    // in four ways with ambiguous meaning:
                    //  - Be OK but produce a null pointer (None) -- what does this mean?
                    //  - Be Ok but produce a staged commit and validated commit
                    //  - be an Error but produce a valid intent state?
                    //  - be an error and produce a GroupMessage Processing Error
                    let maybe_validated_commit = self.stage_and_validate_intent(&mls_group, &intent, &message, envelope).await;
                    provider.transaction(|provider| {
                        let requires_processing = if allow_cursor_increment {
                            tracing::info!(
                                "calling update cursor for group {}, with cursor {}, allow_cursor_increment is true",
                                hex::encode(envelope.group_id.as_slice()),
                                cursor
                            );
                            let updated = provider.db().update_cursor(
                                &envelope.group_id,
                                EntityKind::Group,
                                cursor as i64,
                            )?;
                            if updated {
                                tracing::debug!("cursor updated to [{}]", cursor as i64);
                            } else { tracing::debug!("no cursor update required"); }
                            updated
                        } else {
                            tracing::info!(
                                "will not call update cursor for group {}, with cursor {}, allow_cursor_increment is false",
                                hex::encode(envelope.group_id.as_slice()),
                                cursor
                            );
                            let current_cursor = provider
                                .db()
                                .get_last_cursor_for_id(&envelope.group_id, EntityKind::Group)?;
                            current_cursor < cursor as i64
                        };
                        if !requires_processing {
                            tracing::debug!("message @cursor=[{}] for group=[{}] created_at=[{}] no longer require processing, should be available in database",
                                envelope.id,
                                xmtp_common::fmt::debug_hex(&envelope.group_id),
                                envelope.created_ns
                            );

                            // early return if the message is already procesed
                            // _NOTE_: Not early returning and re-processing a message that
                            // has already been processed, has the potential to result in forks.
                            // In some cases, we may want to roll back the cursor if we updated the
                            // cursor, but actually cannot process the message.
                            identifier.previously_processed(true);
                            return Ok(());
                        }
                        let (intent_state, internal_message_id) = match maybe_validated_commit {
                            // the error is also a Result
                            Err(err) => match err {
                                Ok(intent_state) => (intent_state, None),
                                Err(e) => return Err(e)
                            },
                            Ok(commit) => {
                                self
                                .process_own_message(&mut mls_group, commit, &intent, &message, envelope)?
                            }
                        };
                        identifier.internal_id(internal_message_id.clone());

                        match intent_state {
                            IntentState::ToPublish => {
                                provider.db().set_group_intent_to_publish(intent_id)?;
                            }
                            IntentState::Committed => {
                                self.handle_metadata_update_from_intent(&intent)?;
                                provider.db().set_group_intent_committed(intent_id)?;
                            }
                            IntentState::Published => {
                                tracing::error!("Unexpected behaviour: returned intent state published from process_own_message");
                            }
                            IntentState::Error => {
                                tracing::error!("Intent [{}] moved to error status", intent_id);
                                provider.db().set_group_intent_error(intent_id)?;
                            }
                            IntentState::Processed => {
                                tracing::debug!("Intent [{}] moved to Processed status", intent_id);
                                provider.db().set_group_intent_processed(intent_id)?;
                            }
                        }
                        Ok(())
                    })?;
                    Ok::<_, GroupMessageProcessingError>(identifier)
                }).await?;
                identifier.build()
            }
            // No matching intent found. The message did not originate here.
            None => {
                tracing::info!(
                    inbox_id = self.context.inbox_id(),
                    installation_id = %self.context.installation_id(),
                    group_id = hex::encode(&self.group_id),
                    cursor = envelope.id,
                    "client [{}] is about to process external envelope [{}]",
                    self.context.inbox_id(),
                    envelope.id
                );

                let identifier = self
                    .load_mls_group_with_lock_async(|mut mls_group| async move {
                        self.validate_and_process_external_message(
                            &mut mls_group,
                            message,
                            envelope,
                            allow_cursor_increment,
                        )
                        .await
                    })
                    .await?;

                Ok(identifier)
            }
        }
    }

    /// In case of metadataUpdate will extract the updated fields and store them to the db
    fn handle_metadata_update_from_intent(
        &self,
        intent: &StoredGroupIntent,
    ) -> Result<(), IntentError> {
        let provider = self.mls_provider();
        if intent.kind == MetadataUpdate {
            let data = UpdateMetadataIntentData::try_from(intent.data.clone())?;

            match data.field_name.as_str() {
                field_name if field_name == MetadataField::MessageDisappearFromNS.as_str() => {
                    provider.db().update_message_disappearing_from_ns(
                        self.group_id.clone(),
                        data.field_value.parse::<i64>().ok(),
                    )?
                }
                field_name if field_name == MetadataField::MessageDisappearInNS.as_str() => {
                    provider.db().update_message_disappearing_in_ns(
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
        metadata_field_changes: Vec<group_updated::MetadataFieldChange>,
    ) -> Result<(), StorageError> {
        let provider = self.mls_provider();
        let conn = provider.db();
        for change in metadata_field_changes {
            match change.field_name.as_str() {
                field_name if field_name == MetadataField::MessageDisappearFromNS.as_str() => {
                    let parsed_value = change
                        .new_value
                        .as_deref()
                        .and_then(|v| v.parse::<i64>().ok());
                    conn.update_message_disappearing_from_ns(self.group_id.clone(), parsed_value)?
                }
                field_name if field_name == MetadataField::MessageDisappearInNS.as_str() => {
                    let parsed_value = change
                        .new_value
                        .as_deref()
                        .and_then(|v| v.parse::<i64>().ok());
                    conn.update_message_disappearing_in_ns(self.group_id.clone(), parsed_value)?
                }
                _ => {} // Handle other metadata updates if needed
            }
        }

        Ok(())
    }

    /// Consume a message, decrypting its contents and processing it
    /// Applies the message if it is a commit.
    /// Returns a 'MessageIdentifier' for this message
    #[tracing::instrument(level = "debug", skip(envelope))]
    async fn consume_message(
        &self,
        envelope: &GroupMessage,
    ) -> Result<MessageIdentifier, GroupMessageProcessingError> {
        let provider = self.mls_provider();
        let msgv1 = match &envelope.version {
            Some(GroupMessageVersion::V1(value)) => value,
            _ => return Err(GroupMessageProcessingError::InvalidPayload),
        };

        let mls_message_in = MlsMessageIn::tls_deserialize_exact(&msgv1.data)?;
        let message_entity_kind = match mls_message_in.wire_format() {
            WireFormat::Welcome => EntityKind::Welcome,
            _ => EntityKind::Group,
        };

        let last_cursor = provider
            .db()
            .get_last_cursor_for_id(&self.group_id, message_entity_kind)?;
        let should_skip_message = last_cursor > msgv1.id as i64;
        if should_skip_message {
            tracing::info!(
                inbox_id = self.context.inbox_id(),
                installation_id = %self.context.installation_id(),
                group_id = hex::encode(&self.group_id),
                "Message already processed: skipped cursor:[{}] entity kind:[{:?}] last cursor in db: [{}]",
                msgv1.id,
                message_entity_kind,
                last_cursor
            );
            // early return if the message is already procesed
            // _NOTE_: Not early returning and re-processing a message that
            // has already been processed, has the potential to result in forks.
            return MessageIdentifierBuilder::from(msgv1).build();
        }

        // Download all unread welcome messages and convert to groups.Run `man nix.conf` for more information on the `substituters` configuration option.
        // In a database transaction, increment the cursor for a given entity and
        // apply the update after the provided `ProcessingFn` has completed successfully.
        let message = match self.process_message(msgv1, true).await {
            Ok(m) => {
                tracing::info!(
                    "Transaction completed successfully: process for group [{}] envelope cursor[{}]",
                    hex::encode(&msgv1.group_id),
                    msgv1.id
                );
                Ok(m)
            }
            Err(GroupMessageProcessingError::CommitValidation(
                CommitValidationError::MinimumSupportedProtocolVersionExceedsCurrentVersion(
                    min_version,
                ),
            )) => {
                // Instead of updating cursor, mark group as paused
                provider
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
                    hex::encode(&msgv1.group_id),
                    msgv1.id,
                    e
                );
                self.process_group_message_error_for_fork_detection(msgv1, &e)
                    .await?;
                Err(e)
            }
        }?;
        Ok(message)
    }

    #[tracing::instrument(level = "debug", skip(messages))]
    pub async fn process_messages(&self, messages: Vec<GroupMessage>) -> ProcessSummary {
        let mut summary = ProcessSummary::default();
        for message in messages.into_iter() {
            let message_cursor = match extract_message_cursor(&message) {
                // None means unsupported message version
                // skip the message
                None => continue,
                Some(c) => c,
            };
            summary.add_id(message_cursor);
            let result = retry_async!(
                Retry::default(),
                (async { self.consume_message(&message).await })
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
                    summary.errored(message_cursor, e);
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
    /// if they were succesfull or not. It is important to return _all_
    /// cursor ids, so that streams do not unintentially retry O(n^2) messages.
    #[tracing::instrument(skip_all, level = "debug")]
    pub(super) async fn receive(&self) -> Result<ProcessSummary, GroupError> {
        let provider = self.mls_provider();
        let messages = MlsStore::new(self.context.clone())
            .query_group_messages(&self.group_id, provider.db())
            .await?;
        let summary = self.process_messages(messages).await;
        Ok(summary)
    }

    fn save_transcript_message(
        &self,
        validated_commit: ValidatedCommit,
        timestamp_ns: u64,
        cursor: u64,
    ) -> Result<Option<StoredGroupMessage>, GroupMessageProcessingError> {
        let provider = self.mls_provider();
        let conn = provider.db();
        if validated_commit.is_empty() {
            return Ok(None);
        }

        tracing::info!(
            "[{}]: Storing a transcript message with {} members added and {} members removed and {} metadata changes",
            self.context().inbox_id(),
            validated_commit.added_inboxes.len(),
            validated_commit.removed_inboxes.len(),
            validated_commit.metadata_validation_info.metadata_field_changes.len(),
        );
        let sender_installation_id = validated_commit.actor_installation_id();
        let sender_inbox_id = validated_commit.actor_inbox_id();

        let payload: GroupUpdated = validated_commit.into();
        let encoded_payload = GroupUpdatedCodec::encode(payload.clone())?;
        let mut encoded_payload_bytes = Vec::new();
        encoded_payload.encode(&mut encoded_payload_bytes)?;

        let group_id = self.group_id.as_slice();
        let message_id = calculate_message_id(
            group_id,
            encoded_payload_bytes.as_slice(),
            &timestamp_ns.to_string(),
        );
        let content_type = match encoded_payload.r#type {
            Some(ct) => ct,
            None => {
                tracing::warn!("Missing content type in encoded payload, using default values");
                // Default content type values
                xmtp_proto::xmtp::mls::message_contents::ContentTypeId {
                    authority_id: "unknown".to_string(),
                    type_id: "unknown".to_string(),
                    version_major: 0,
                    version_minor: 0,
                }
            }
        };
        self.handle_metadata_update_from_commit(payload.metadata_field_changes)?;
        let msg = StoredGroupMessage {
            id: message_id,
            group_id: group_id.to_vec(),
            decrypted_message_bytes: encoded_payload_bytes.to_vec(),
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
            sequence_id: Some(cursor as i64),
            originator_id: None,
        };
        msg.store_or_ignore(conn)?;
        Ok(Some(msg))
    }

    async fn process_group_message_error_for_fork_detection(
        &self,
        message: &GroupMessageV1,
        error: &GroupMessageProcessingError,
    ) -> Result<(), GroupMessageProcessingError> {
        let group_id = message.group_id.clone();
        if let OpenMlsProcessMessage(ProcessMessageError::ValidationError(
            ValidationError::WrongEpoch,
        )) = error
        {
            let group_epoch = match self.epoch().await {
                Ok(epoch) => epoch,
                Err(error) => {
                    tracing::info!(
                        "WrongEpoch encountered but group_epoch could not be calculated, error:{}",
                        error
                    );
                    return Ok(());
                }
            };

            let mls_message_in = match MlsMessageIn::tls_deserialize_exact(&message.data) {
                Ok(msg) => msg,
                Err(error) => {
                    tracing::info!(
                        "WrongEpoch encountered but failed to deserialize the message, error:{}",
                        error
                    );
                    return Ok(());
                }
            };

            let protocol_message = match mls_message_in.extract() {
                MlsMessageBodyIn::PrivateMessage(msg) => msg,
                _ => {
                    tracing::info!("WrongEpoch encountered but failed to extract PrivateMessage");
                    return Ok(());
                }
            };

            let message_epoch = protocol_message.epoch();
            let epoch_validation_result = Self::validate_message_epoch(
                self.context().inbox_id(),
                0,
                GroupEpoch::from(group_epoch),
                message_epoch,
                MAX_PAST_EPOCHS,
            );

            if let Err(GroupMessageProcessingError::FutureEpoch(_, _)) = &epoch_validation_result {
                let fork_details = format!(
                    "Message cursor [{}] epoch [{}] is greater than group epoch [{}], your group may be forked",
                    message.id, message_epoch, group_epoch
                );
                tracing::error!(fork_details);
                let _ = self
                    .context()
                    .db()
                    .mark_group_as_maybe_forked(&group_id, fork_details);
                return epoch_validation_result;
            }

            return Ok(());
        }

        Ok(())
    }

    #[tracing::instrument]
    pub(super) async fn publish_intents(&self) -> Result<(), GroupError> {
        let provider = self.mls_provider();
        self.load_mls_group_with_lock_async(|mut mls_group| async move {
            let intents = provider.db().find_group_intents(
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
                            provider
                                .db()
                                .set_group_intent_error_and_fail_msg(&intent, id)?;
                        } else {
                            provider
                                .db()
                                .increment_intent_publish_attempt_count(intent.id)?;
                        }

                        return Err(err);
                    }
                    Ok(Some(PublishIntentData {
                                payload_to_publish,
                                post_commit_action,
                                staged_commit,
                                should_send_push_notification
                            })) => {
                        let payload_slice = payload_to_publish.as_slice();
                        let has_staged_commit = staged_commit.is_some();
                        let intent_hash = sha256(payload_slice);
                        // removing this transaction causes missed messages
                        provider.transaction(|provider| {
                            provider.db().set_group_intent_published(
                                intent.id,
                                &intent_hash,
                                post_commit_action,
                                staged_commit,
                                mls_group.epoch().as_u64() as i64,
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

                        let messages = self.prepare_group_messages(vec![(payload_slice, should_send_push_notification)])?;
                        self.context
                            .api()
                            .send_group_messages(messages)
                            .await?;

                        tracing::info!(
                            intent.id,
                            intent.kind = %intent.kind,
                            inbox_id = self.context.inbox_id(),
                            installation_id = %self.context.installation_id(),
                            group_id = hex::encode(&self.group_id),
                            "[{}] published intent [{}] of type [{}] with hash [{}]",
                            self.context.inbox_id(),
                            intent.id,
                            intent.kind,
                            hex::encode(sha256(payload_slice))
                        );
                        if has_staged_commit {
                            tracing::info!("Commit sent. Stopping further publishes for this round");
                            return Ok(());
                        }
                    }
                    Ok(None) => {
                        tracing::info!(
                            inbox_id = self.context.inbox_id(),
                            installation_id = %self.context.installation_id(),
                            "Skipping intent because no publish data returned"
                        );
                        provider.db().set_group_intent_processed(intent.id)?
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
        let provider = self.mls_provider();
        match intent.kind {
            IntentKind::UpdateGroupMembership => {
                let intent_data =
                    UpdateGroupMembershipIntentData::try_from(intent.data.as_slice())?;
                let signer = &self.context().identity.installation_keys;
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
                // TODO: Handle pending_proposal errors and UseAfterEviction errors
                let msg = openmls_group.create_message(
                    &provider,
                    &self.context().identity.installation_keys,
                    intent_data.message.as_slice(),
                )?;

                Ok(Some(PublishIntentData {
                    payload_to_publish: msg.tls_serialize_detached()?,
                    post_commit_action: None,
                    staged_commit: None,
                    should_send_push_notification: intent.should_push,
                }))
            }
            IntentKind::KeyUpdate => {
                let provider = self.mls_provider();
                let (bundle, staged_commit) = provider.transaction(|provider| {
                    let bundle = openmls_group.self_update(
                        &provider,
                        &self.context().identity.installation_keys,
                        LeafNodeParameters::default(),
                    )?;
                    let staged_commit = get_and_clear_pending_commit(openmls_group, provider)?;

                    Ok::<_, GroupError>((bundle, staged_commit))
                })?;

                Ok(Some(PublishIntentData {
                    payload_to_publish: bundle.commit().tls_serialize_detached()?,
                    staged_commit,
                    post_commit_action: None,
                    should_send_push_notification: intent.should_push,
                }))
            }
            IntentKind::MetadataUpdate => {
                let metadata_intent = UpdateMetadataIntentData::try_from(intent.data.clone())?;
                let mutable_metadata_extensions = build_extensions_for_metadata_update(
                    openmls_group,
                    metadata_intent.field_name,
                    metadata_intent.field_value,
                )?;

                let provider = self.mls_provider();
                let (commit, staged_commit) = provider.transaction(|provider| {
                    let (commit, _, _) = openmls_group.update_group_context_extensions(
                        &provider,
                        mutable_metadata_extensions,
                        &self.context().identity.installation_keys,
                    )?;
                    let staged_commit = get_and_clear_pending_commit(openmls_group, provider)?;

                    Ok::<_, GroupError>((commit, staged_commit))
                })?;

                let commit_bytes = commit.tls_serialize_detached()?;

                Ok(Some(PublishIntentData {
                    payload_to_publish: commit_bytes,
                    staged_commit,
                    post_commit_action: None,
                    should_send_push_notification: intent.should_push,
                }))
            }
            IntentKind::UpdateAdminList => {
                let admin_list_update_intent =
                    UpdateAdminListIntentData::try_from(intent.data.clone())?;
                let mutable_metadata_extensions = build_extensions_for_admin_lists_update(
                    openmls_group,
                    admin_list_update_intent,
                )?;

                let provider = self.mls_provider();
                let (commit, staged_commit) = provider.transaction(|provider| {
                    let (commit, _, _) = openmls_group.update_group_context_extensions(
                        &provider,
                        mutable_metadata_extensions,
                        &self.context().identity.installation_keys,
                    )?;
                    let staged_commit = get_and_clear_pending_commit(openmls_group, provider)?;

                    Ok::<_, GroupError>((commit, staged_commit))
                })?;

                let commit_bytes = commit.tls_serialize_detached()?;

                Ok(Some(PublishIntentData {
                    payload_to_publish: commit_bytes,
                    staged_commit,
                    post_commit_action: None,
                    should_send_push_notification: intent.should_push,
                }))
            }
            IntentKind::UpdatePermission => {
                let update_permissions_intent =
                    UpdatePermissionIntentData::try_from(intent.data.clone())?;
                let group_permissions_extensions = build_extensions_for_permissions_update(
                    openmls_group,
                    update_permissions_intent,
                )?;

                let provider = self.mls_provider();
                let (commit, staged_commit) = provider.transaction(|provider| {
                    let (commit, _, _) = openmls_group.update_group_context_extensions(
                        &provider,
                        group_permissions_extensions,
                        &self.context().identity.installation_keys,
                    )?;
                    let staged_commit = get_and_clear_pending_commit(openmls_group, provider)?;

                    Ok::<_, GroupError>((commit, staged_commit))
                })?;

                let commit_bytes = commit.tls_serialize_detached()?;
                Ok(Some(PublishIntentData {
                    payload_to_publish: commit_bytes,
                    staged_commit,
                    post_commit_action: None,
                    should_send_push_notification: intent.should_push,
                }))
            }
        }
    }

    #[tracing::instrument(skip_all)]
    pub(crate) async fn post_commit(&self) -> Result<(), GroupError> {
        let provider = self.mls_provider();
        let conn = provider.db();
        let intents = conn.find_group_intents(
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
                        self.send_welcomes(action).await?;
                    }
                }
            }
            conn.set_group_intent_processed(intent.id)?
        }

        Ok(())
    }

    pub async fn maybe_update_installations(
        &self,
        update_interval_ns: Option<i64>,
    ) -> Result<(), GroupError> {
        let provider = self.mls_provider();
        let Some(stored_group) = provider.db().find_group(&self.group_id)? else {
            return Err(GroupError::NotFound(NotFound::GroupById(
                self.group_id.clone(),
            )));
        };
        if stored_group.conversation_type == ConversationType::Sync {
            // Sync groups should not add new installations, new installations will create their own.
            return Ok(());
        }

        // determine how long of an interval in time to use before updating list
        let interval_ns = update_interval_ns.unwrap_or(sync_update_installations_interval_ns());

        let now_ns = xmtp_common::time::now_ns();
        let last_ns = provider
            .db()
            .get_installations_time_checked(self.group_id.clone())?;
        let elapsed_ns = now_ns - last_ns;
        if elapsed_ns > interval_ns && self.is_active()? {
            self.add_missing_installations().await?;
            provider
                .db()
                .update_installations_time_checked(self.group_id.clone())?;
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

        let intent =
            self.queue_intent(IntentKind::UpdateGroupMembership, intent_data.into(), false)?;

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
    pub(super) async fn get_membership_update_intent(
        &self,
        inbox_ids_to_add: &[InboxIdRef<'_>],
        inbox_ids_to_remove: &[InboxIdRef<'_>],
    ) -> Result<UpdateGroupMembershipIntentData, GroupError> {
        let provider = self.mls_provider();
        self.load_mls_group_with_lock_async(|mls_group| async move {
            let existing_group_membership = extract_group_membership(mls_group.extensions())?;
            // TODO:nm prevent querying for updates on members who are being removed
            let mut inbox_ids = existing_group_membership.inbox_ids();
            inbox_ids.extend_from_slice(inbox_ids_to_add);
            let conn = provider.db();
            // Load any missing updates from the network
            load_identity_updates(self.context.api(), conn, &inbox_ids).await?;

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
            let extensions: Extensions = mls_group.extensions().clone();
            let old_group_membership = extract_group_membership(&extensions)?;
            let mut new_membership = old_group_membership.clone();
            for (inbox_id, sequence_id) in changed_inbox_ids.iter() {
                new_membership.add(inbox_id.clone(), *sequence_id);
            }
            for inbox_id in inbox_ids_to_remove {
                new_membership.remove(inbox_id);
            }

            let changes_with_kps = calculate_membership_changes_with_keypackages(
                self.context.clone(),
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
    #[tracing::instrument(level = "trace", skip_all)]
    pub(super) async fn send_welcomes(&self, action: SendWelcomesAction) -> Result<(), GroupError> {
        let welcomes = action
            .installations
            .into_iter()
            .map(|installation| -> Result<WelcomeMessageInput, HpkeError> {
                let installation_key = installation.installation_key;
                let encrypted = encrypt_welcome(
                    action.welcome_message.as_slice(),
                    installation.hpke_public_key.as_slice(),
                )?;
                Ok(WelcomeMessageInput {
                    version: Some(WelcomeMessageInputVersion::V1(WelcomeMessageInputV1 {
                        installation_key,
                        data: encrypted,
                        hpke_public_key: installation.hpke_public_key,
                        wrapper_algorithm: WelcomeWrapperAlgorithm::Curve25519.into(),
                    })),
                })
            })
            .collect::<Result<Vec<WelcomeMessageInput>, HpkeError>>()?;

        let welcome = welcomes.first().ok_or(GroupError::NoWelcomesToSend)?;

        let chunk_size = GRPC_DATA_LIMIT
            / welcome
                .version
                .as_ref()
                .map(|w| match w {
                    WelcomeMessageInputVersion::V1(w) => {
                        let w = w.installation_key.len() + w.data.len() + w.hpke_public_key.len();
                        tracing::debug!("total welcome message proto bytes={w}");
                        w
                    }
                })
                .unwrap_or(GRPC_DATA_LIMIT / MAX_GROUP_SIZE);

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
    if let Sender::Member(leaf_node_index) = decrypted_message.sender() {
        if let Some(member) = openmls_group.member_at(*leaf_node_index) {
            if member.credential.eq(decrypted_message.credential()) {
                let basic_credential = BasicCredential::try_from(member.credential)?;
                let sender_inbox_id = parse_credential(basic_credential.identity())?;
                return Ok((sender_inbox_id, member.signature_key));
            }
        }
    }

    let basic_credential = BasicCredential::try_from(decrypted_message.credential().clone())?;
    Err(GroupMessageProcessingError::InvalidSender {
        message_time_ns: message_created_ns,
        credential: basic_credential.identity().to_vec(),
    })
}

async fn calculate_membership_changes_with_keypackages<'a, ApiClient, Db>(
    context: Arc<XmtpMlsLocalContext<ApiClient, Db>>,
    new_group_membership: &'a GroupMembership,
    old_group_membership: &'a GroupMembership,
) -> Result<MembershipDiffWithKeyPackages, GroupError>
where
    ApiClient: XmtpApi,
    Db: XmtpDb,
{
    let provider = context.mls_provider();
    let membership_diff = old_group_membership.diff(new_group_membership);

    let identity = IdentityUpdates::new(context.clone());
    let mut installation_diff = identity
        .get_installation_diff(
            provider.db(),
            old_group_membership,
            new_group_membership,
            &membership_diff,
        )
        .await?;

    let mut new_installations = Vec::new();
    let mut new_key_packages = Vec::new();
    let mut new_failed_installations = Vec::new();

    if !installation_diff.added_installations.is_empty() {
        let key_packages = get_keypackages_for_installation_ids(
            context,
            installation_diff.added_installations,
            &mut new_failed_installations,
        )
        .await?;
        for (installation_id, result) in key_packages {
            match result {
                Ok(verified_key_package) => {
                    new_installations.push(Installation::from_verified_key_package(
                        &verified_key_package,
                    ));
                    new_key_packages.push(verified_key_package.inner.clone());
                }
                Err(_) => new_failed_installations.push(installation_id.clone()),
            }
        }
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
async fn get_keypackages_for_installation_ids<ApiClient, Db>(
    context: Arc<XmtpMlsLocalContext<ApiClient, Db>>,
    added_installations: HashSet<Vec<u8>>,
    failed_installations: &mut Vec<Vec<u8>>,
) -> Result<HashMap<Vec<u8>, Result<VerifiedKeyPackageV2, KeyPackageVerificationError>>, ClientError>
where
    ApiClient: XmtpApi,
    Db: XmtpDb,
{
    use crate::utils::{
        get_test_mode_malformed_installations, is_test_mode_upload_malformed_keypackage,
    };

    let my_installation_id = context.installation_id().to_vec();
    let store = MlsStore::new(context.clone());
    let mut key_packages = store
        .get_key_packages_for_installation_ids(
            added_installations
                .iter()
                .filter(|installation| my_installation_id.ne(*installation))
                .cloned()
                .collect(),
        )
        .await?;

    tracing::info!("trying to validate keypackages");

    if is_test_mode_upload_malformed_keypackage() {
        let malformed_installations = get_test_mode_malformed_installations();
        key_packages.retain(|id, _| !malformed_installations.contains(id));
        failed_installations.extend(malformed_installations);
    }

    Ok(key_packages)
}
#[allow(unused_variables, dead_code)]
#[cfg(not(any(test, feature = "test-utils")))]
async fn get_keypackages_for_installation_ids<ApiClient, Db>(
    context: Arc<XmtpMlsLocalContext<ApiClient, Db>>,
    added_installations: HashSet<Vec<u8>>,
    failed_installations: &mut [Vec<u8>],
) -> Result<HashMap<Vec<u8>, Result<VerifiedKeyPackageV2, KeyPackageVerificationError>>, ClientError>
where
    ApiClient: XmtpApi,
    Db: XmtpDb,
{
    let my_installation_id = context.installation_public_key().to_vec();
    let store = MlsStore::new(context.clone());
    store
        .get_key_packages_for_installation_ids(
            added_installations
                .iter()
                .filter(|installation| my_installation_id.ne(*installation))
                .cloned()
                .collect(),
        )
        .await
        .map_err(Into::into)
}

// Takes UpdateGroupMembershipIntentData and applies it to the openmls group
// returning the commit and post_commit_action
#[tracing::instrument(level = "trace", skip_all)]
async fn apply_update_group_membership_intent<ApiClient, Db>(
    context: &Arc<XmtpMlsLocalContext<ApiClient, Db>>,
    openmls_group: &mut OpenMlsGroup,
    intent_data: UpdateGroupMembershipIntentData,
    signer: impl Signer,
) -> Result<Option<PublishIntentData>, GroupError>
where
    ApiClient: XmtpApi,
    Db: XmtpDb,
{
    let provider = context.mls_provider();
    let extensions: Extensions = openmls_group.extensions().clone();
    let old_group_membership = extract_group_membership(&extensions)?;
    let new_group_membership = intent_data.apply_to_group_membership(&old_group_membership);
    let membership_diff = old_group_membership.diff(&new_group_membership);

    let changes_with_kps = calculate_membership_changes_with_keypackages(
        context.clone(),
        &new_group_membership,
        &old_group_membership,
    )
    .await?;
    let leaf_nodes_to_remove: Vec<LeafNodeIndex> =
        get_removed_leaf_nodes(openmls_group, &changes_with_kps.removed_installations);

    if leaf_nodes_to_remove.contains(&openmls_group.own_leaf_index()) {
        tracing::info!("Cannot remove own leaf node");
        return Ok(None);
    }

    if leaf_nodes_to_remove.is_empty()
        && changes_with_kps.new_key_packages.is_empty()
        && membership_diff.updated_inboxes.is_empty()
    {
        return Ok(None);
    }

    // Update the extensions to have the new GroupMembership
    let mut new_extensions = extensions.clone();

    new_extensions.add_or_replace(build_group_membership_extension(&new_group_membership));

    let (commit, post_commit_action, staged_commit) = provider.transaction(|provider| {
        // Create the commit
        let (commit, maybe_welcome_message, _) = openmls_group.update_group_membership(
            &provider,
            &signer,
            &changes_with_kps.new_key_packages,
            &leaf_nodes_to_remove,
            new_extensions,
        )?;

        let post_commit_action = match maybe_welcome_message {
            Some(welcome_message) => Some(PostCommitAction::from_welcome(
                welcome_message,
                changes_with_kps.new_installations,
            )?),
            None => None,
        };

        let staged_commit = get_and_clear_pending_commit(openmls_group, provider)?
            .ok_or_else(|| GroupError::MissingPendingCommit)?;

        Ok::<_, GroupError>((commit, post_commit_action, staged_commit))
    })?;

    Ok(Some(PublishIntentData {
        payload_to_publish: commit.tls_serialize_detached()?,
        post_commit_action: post_commit_action.map(|action| action.to_bytes()),
        staged_commit: Some(staged_commit),
        should_send_push_notification: false,
    }))
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

fn get_and_clear_pending_commit<C: ConnectionExt>(
    openmls_group: &mut OpenMlsGroup,
    provider: impl OpenMlsProvider<StorageProvider = SqlKeyStore<C>>,
) -> Result<Option<Vec<u8>>, GroupError> {
    let commit = openmls_group
        .pending_commit()
        .as_ref()
        .map(xmtp_db::db_serialize)
        .transpose()?;
    openmls_group.clear_pending_commit(provider.storage())?;
    Ok(commit)
}

fn decode_staged_commit(data: &[u8]) -> Result<StagedCommit, GroupMessageProcessingError> {
    Ok(xmtp_db::db_deserialize(data)?)
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use super::*;
    use crate::{builder::ClientBuilder, utils::ConcreteMlsGroup};
    use std::sync::Arc;
    use xmtp_cryptography::utils::generate_local_wallet;

    /// This test is not reproducible in webassembly, b/c webassembly has only one thread.
    #[cfg_attr(
        not(target_arch = "wasm32"),
        tokio::test(flavor = "multi_thread", worker_threads = 10)
    )]
    #[cfg(not(target_family = "wasm"))]
    async fn publish_intents_worst_case_scenario() {
        use crate::utils::Tester;

        let amal_a = Tester::new().await;
        let amal_group_a: Arc<MlsGroup<_, _>> =
            Arc::new(amal_a.create_group(None, Default::default()).unwrap());

        let conn = amal_a.context().mls_provider();
        let provider = Arc::new(conn);

        // create group intent
        amal_group_a.sync().await.unwrap();
        assert_eq!(provider.db().intents_processed(), 1);

        for _ in 0..100 {
            let s = xmtp_common::rand_string::<100>();
            amal_group_a.send_message_optimistic(s.as_bytes()).unwrap();
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

        let published = provider.db().intents_published();
        assert_eq!(published, 101);
        let created = provider.db().intents_created();
        assert_eq!(created, 101);
        if !errs.is_empty() {
            panic!("Errors during publish");
        }
    }

    #[xmtp_common::test]
    async fn hmac_keys_work_as_expected() {
        let wallet = generate_local_wallet();
        let amal = Arc::new(ClientBuilder::new_test_client(&wallet).await);
        let amal_group: Arc<ConcreteMlsGroup> =
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
}
