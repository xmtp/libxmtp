use super::{
    build_extensions_for_admin_lists_update, build_extensions_for_metadata_update,
    build_extensions_for_permissions_update, build_group_membership_extension,
    intents::{
        Installation, IntentError, PostCommitAction, SendMessageIntentData, SendWelcomesAction,
        UpdateAdminListIntentData, UpdateGroupMembershipIntentData, UpdatePermissionIntentData,
    },
    validated_commit::{extract_group_membership, CommitValidationError},
    GroupError, HmacKey, MlsGroup, ScopedGroupClient,
};
use crate::configuration::sync_update_installations_interval_ns;
use crate::groups::group_membership::{GroupMembership, MembershipDiffWithKeyPackages};
use crate::storage::{group_intent::IntentKind::MetadataUpdate, NotFound};
use crate::verified_key_package_v2::{KeyPackageVerificationError, VerifiedKeyPackageV2};
use crate::{client::ClientError, groups::group_mutable_metadata::MetadataField};
use crate::{
    configuration::{
        GRPC_DATA_LIMIT, HMAC_SALT, MAX_GROUP_SIZE, MAX_INTENT_PUBLISH_ATTEMPTS, MAX_PAST_EPOCHS,
    },
    groups::{
        device_sync::{preference_sync::UserPreferenceUpdate, DeviceSyncContent},
        intents::UpdateMetadataIntentData,
        validated_commit::ValidatedCommit,
    },
    hpke::{encrypt_welcome, HpkeError},
    identity::{parse_credential, IdentityError},
    identity_updates::load_identity_updates,
    intents::ProcessIntentError,
    storage::xmtp_openmls_provider::XmtpOpenMlsProvider,
    storage::{
        db_connection::DbConnection,
        group_intent::{IntentKind, IntentState, StoredGroupIntent, ID},
        group_message::{ContentType, DeliveryStatus, GroupMessageKind, StoredGroupMessage},
        refresh_state::EntityKind,
        serialization::{db_deserialize, db_serialize},
        sql_key_store,
        user_preferences::StoredUserPreferences,
        ProviderTransactions, StorageError,
    },
    subscriptions::{LocalEvents, SyncMessage},
    utils::{hash::sha256, id::calculate_message_id, time::hmac_epoch},
    Delete, Fetch, StoreOrIgnore,
};
use futures::future::try_join_all;
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
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
use std::{
    collections::{HashMap, HashSet},
    mem::{discriminant, Discriminant},
    ops::RangeInclusive,
};
use thiserror::Error;
use tracing::debug;
use xmtp_common::{retry_async, Retry, RetryableError};
use xmtp_content_types::{group_updated::GroupUpdatedCodec, CodecError, ContentCodec};
use xmtp_id::{InboxId, InboxIdRef};
use xmtp_proto::xmtp::mls::message_contents::group_updated;
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
    #[error("[{0}] already processed")]
    AlreadyProcessed(u64),
    #[error("diesel error: {0}")]
    Diesel(#[from] diesel::result::Error),
    #[error("[{message_time_ns:?}] invalid sender with credential: {credential:?}")]
    InvalidSender {
        message_time_ns: u64,
        credential: Vec<u8>,
    },
    #[error("invalid payload")]
    InvalidPayload,
    #[error("storage error: {0}")]
    Storage(#[from] crate::storage::StorageError),
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
}

impl RetryableError for GroupMessageProcessingError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Diesel(err) => err.is_retryable(),
            Self::Storage(err) => err.is_retryable(),
            Self::Identity(err) => err.is_retryable(),
            Self::OpenMlsProcessMessage(err) => err.is_retryable(),
            Self::MergeStagedCommit(err) => err.is_retryable(),
            Self::ProcessIntent(err) => err.is_retryable(),
            Self::CommitValidation(err) => err.is_retryable(),
            Self::ClearPendingCommit(err) => err.is_retryable(),
            Self::Client(err) => err.is_retryable(),
            Self::WrongCredentialType(_)
            | Self::Codec(_)
            | Self::AlreadyProcessed(_)
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
            | Self::UnsupportedMessageType(_) => false,
        }
    }
}

#[derive(Debug)]
struct PublishIntentData {
    staged_commit: Option<Vec<u8>>,
    post_commit_action: Option<Vec<u8>>,
    payload_to_publish: Vec<u8>,
}

impl<ScopedClient> MlsGroup<ScopedClient>
where
    ScopedClient: ScopedGroupClient,
{
    #[tracing::instrument(skip_all)]
    pub async fn sync(&self) -> Result<(), GroupError> {
        let conn = self.context().store().conn()?;
        let mls_provider = XmtpOpenMlsProvider::from(conn);
        let epoch = self.epoch(&mls_provider).await?;
        tracing::info!(
            inbox_id = self.client.inbox_id(),
            installation_id = %self.client.installation_id(),
            group_id = hex::encode(&self.group_id),
            epoch = epoch,
            "[{}] syncing group, epoch = {}",
            self.client.inbox_id(),
            epoch
        );
        self.maybe_update_installations(&mls_provider, None).await?;

        self.sync_with_conn(&mls_provider).await
    }

    // TODO: Should probably be renamed to `sync_with_provider`
    #[tracing::instrument(skip_all)]
    pub async fn sync_with_conn(&self, provider: &XmtpOpenMlsProvider) -> Result<(), GroupError> {
        let start = std::time::Instant::now();
        tracing::info!(
            inbox_id = self.client.inbox_id(),
            group_id = hex::encode(&self.group_id),
            "Starting sync_with_conn"
        );

        let lock_start = std::time::Instant::now();
        let _mutex = self.mutex.lock().await;
        tracing::info!("Acquiring mutex lock took: {:?}", lock_start.elapsed());

        let mut errors: Vec<GroupError> = vec![];
        let conn = provider.conn_ref();

        // Publish intents
        let publish_start = std::time::Instant::now();
        if let Err(publish_error) = self.publish_intents(provider).await {
            tracing::error!(
                error = %publish_error,
                duration = ?publish_start.elapsed(),
                "Sync: error publishing intents {:?}",
                publish_error
            );
            errors.push(publish_error);
        } else {
            tracing::info!(
                "publish_intents succeeded, took: {:?}",
                publish_start.elapsed()
            );
        }

        // Receive messages
        let receive_start = std::time::Instant::now();
        if let Err(receive_error) = self.receive(provider).await {
            tracing::error!(
                error = %receive_error,
                duration = ?receive_start.elapsed(),
                "receive error {:?}",
                receive_error
            );
            // We don't return an error if receive fails, because it's possible this is caused
            // by malicious data sent over the network, or messages from before the user was
            // added to the group
        } else {
            tracing::info!("receive succeeded, took: {:?}", receive_start.elapsed());
        }

        // Post commit
        let post_commit_start = std::time::Instant::now();
        if let Err(post_commit_err) = self.post_commit(conn).await {
            tracing::error!(
                error = %post_commit_err,
                duration = ?post_commit_start.elapsed(),
                "post commit error {:?}",
                post_commit_err
            );
            errors.push(post_commit_err);
        } else {
            tracing::info!(
                "post_commit succeeded, took: {:?}",
                post_commit_start.elapsed()
            );
        }

        let total_duration = start.elapsed();
        tracing::info!(
            inbox_id = self.client.inbox_id(),
            group_id = hex::encode(&self.group_id),
            "sync_with_conn completed in {:?}",
            total_duration
        );

        // Return a combination of publish and post_commit errors
        if !errors.is_empty() {
            return Err(GroupError::Sync(errors));
        }
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub(super) async fn sync_until_last_intent_resolved(
        &self,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<(), GroupError> {
        let intents = provider.conn_ref().find_group_intents(
            self.group_id.clone(),
            Some(vec![IntentState::ToPublish, IntentState::Published]),
            None,
        )?;

        if intents.is_empty() {
            return Ok(());
        }

        self.sync_until_intent_resolved(provider, intents[intents.len() - 1].id)
            .await
    }

    /**
     * Sync the group and wait for the intent to be deleted
     * Group syncing may involve picking up messages unrelated to the intent, so simply checking for errors
     * does not give a clear signal as to whether the intent was successfully completed or not.
     *
     * This method will retry up to `crate::configuration::MAX_GROUP_SYNC_RETRIES` times.
     */
    #[tracing::instrument(skip_all)]
    pub(super) async fn sync_until_intent_resolved(
        &self,
        provider: &XmtpOpenMlsProvider,
        intent_id: ID,
    ) -> Result<(), GroupError> {
        let mut num_attempts = 0;
        // Return the last error to the caller if we fail to sync
        let mut last_err: Option<GroupError> = None;
        while num_attempts < crate::configuration::MAX_GROUP_SYNC_RETRIES {
            if let Err(err) = self.sync_with_conn(provider).await {
                tracing::error!("error syncing group {:?}", err);
                last_err = Some(err);
            }

            match Fetch::<StoredGroupIntent>::fetch(provider.conn_ref(), &intent_id) {
                Ok(None) => {
                    // This is expected. The intent gets deleted on success
                    return Ok(());
                }
                Ok(Some(StoredGroupIntent {
                    id,
                    state: IntentState::Error,
                    ..
                })) => {
                    tracing::warn!(
                        "not retrying intent ID {id}. since it is in state Error. {:?}",
                        last_err
                    );
                    return Err(last_err.unwrap_or(GroupError::IntentNotCommitted));
                }
                Ok(Some(StoredGroupIntent { id, state, .. })) => {
                    tracing::warn!("retrying intent ID {id}. intent currently in state {state:?}");
                }
                Err(err) => {
                    tracing::error!("database error fetching intent {:?}", err);
                    last_err = Some(GroupError::Storage(err));
                }
            };
            num_attempts += 1;
        }

        Err(last_err.unwrap_or(GroupError::SyncFailedToWait))
    }

    fn is_valid_epoch(
        inbox_id: InboxIdRef<'_>,
        intent_id: i32,
        group_epoch: GroupEpoch,
        message_epoch: GroupEpoch,
        max_past_epochs: usize,
    ) -> bool {
        if message_epoch.as_u64() + max_past_epochs as u64 <= group_epoch.as_u64() {
            tracing::warn!(
                inbox_id,
                message_epoch = message_epoch.as_u64(),
                group_epoch = group_epoch.as_u64(),
                intent_id,
                "[{}] own message epoch {} is {} or more less than group epoch {} for intent {}. Retrying message",
                inbox_id,
                message_epoch,
                max_past_epochs,
                group_epoch.as_u64(),
                intent_id
            );
            return false;
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
            return false;
        }
        true
    }

    // This function is intended to isolate the async validation code to
    // validate the message and prepare it for database insertion synchronously.
    async fn stage_and_validate_intent(
        &self,
        mls_group: &openmls::group::MlsGroup,
        intent: &StoredGroupIntent,
        provider: &XmtpOpenMlsProvider,
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
                            inbox_id = self.client.inbox_id(),
                            installation_id = %self.client.installation_id(),
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
                        &self.client,
                        provider.conn_ref(),
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
                if !Self::is_valid_epoch(
                    self.context().inbox_id(),
                    intent.id,
                    group_epoch,
                    message_epoch,
                    MAX_PAST_EPOCHS,
                ) {
                    return Err(Ok(IntentState::ToPublish));
                }
            }
        }

        Ok(None)
    }

    #[allow(clippy::too_many_arguments)]
    #[tracing::instrument(level = "trace", skip_all)]
    fn process_own_message(
        &self,
        mls_group: &mut OpenMlsGroup,
        commit: Option<(StagedCommit, ValidatedCommit)>,
        intent: &StoredGroupIntent,
        provider: &XmtpOpenMlsProvider,
        message: &ProtocolMessage,
        envelope: &GroupMessageV1,
    ) -> Result<IntentState, GroupMessageProcessingError> {
        if intent.state == IntentState::Committed {
            return Ok(IntentState::Committed);
        }

        let conn = provider.conn_ref();
        let message_epoch = message.epoch();
        let GroupMessageV1 {
            created_ns: envelope_timestamp_ns,
            id: ref cursor,
            ..
        } = *envelope;

        tracing::debug!(
            inbox_id = self.client.inbox_id(),
            installation_id = %self.client.installation_id(),
            group_id = hex::encode(&self.group_id),
            cursor,
            intent.id,
            intent.kind = %intent.kind,
            "[{}]-[{}] processing own message for intent {} / {}, message_epoch: {}",
            self.context().inbox_id(),
            hex::encode(self.group_id.clone()),
            intent.id,
            intent.kind,
            message_epoch
        );

        if let Some((staged_commit, validated_commit)) = commit {
            tracing::info!(
                "[{}] merging pending commit for intent {}",
                self.context().inbox_id(),
                intent.id
            );
            if let Err(err) = mls_group.merge_staged_commit(&provider, staged_commit) {
                tracing::error!("error merging commit: {err}");
                return Ok(IntentState::ToPublish);
            } else {
                // If no error committing the change, write a transcript message
                self.save_transcript_message(conn, validated_commit, envelope_timestamp_ns)?;
            }
        } else if let Some(id) = intent.message_id()? {
            conn.set_delivery_status_to_published(&id, envelope_timestamp_ns)?;
        }

        Ok(IntentState::Committed)
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn validate_and_process_external_message(
        &self,
        provider: &XmtpOpenMlsProvider,
        mls_group: &mut OpenMlsGroup,
        message: PrivateMessageIn,
        envelope: &GroupMessageV1,
    ) -> Result<(), GroupMessageProcessingError> {
        let GroupMessageV1 {
            created_ns: envelope_timestamp_ns,
            id: ref cursor,
            ..
        } = *envelope;

        // We need to process the message twice to avoid an async transaction.
        // We'll process for the first time, get the processed message,
        // and roll the transaction back, so we can fetch updates from the server before
        // being ready to process the message for a second time.
        let mut processed_message = None;
        let result = provider.transaction(|provider| {
            processed_message = Some(mls_group.process_message(provider, message.clone()));
            // Rollback the transaction. We want to synchronize with the server before committing.
            Err::<(), StorageError>(StorageError::IntentionalRollback)
        });
        if !matches!(result, Err(StorageError::IntentionalRollback)) {
            result?;
        }
        let processed_message = processed_message.expect("Was just set to Some")?;

        // Reload the mlsgroup to clear the it's internal cache
        *mls_group = OpenMlsGroup::load(provider.storage(), mls_group.group_id())?.ok_or(
            GroupMessageProcessingError::Storage(StorageError::NotFound(NotFound::MlsGroup)),
        )?;

        let (sender_inbox_id, sender_installation_id) =
            extract_message_sender(mls_group, &processed_message, envelope_timestamp_ns)?;

        tracing::info!(
            inbox_id = self.client.inbox_id(),
            installation_id = %self.client.installation_id(),sender_inbox_id = sender_inbox_id,
            sender_installation_id = hex::encode(&sender_installation_id),
            group_id = hex::encode(&self.group_id),
            current_epoch = mls_group.epoch().as_u64(),
            msg_epoch = processed_message.epoch().as_u64(),
            msg_group_id = hex::encode(processed_message.group_id().as_slice()),
            cursor,
            "[{}] extracted sender inbox id: {}",
            self.client.inbox_id(),
            sender_inbox_id
        );

        let validated_commit = match &processed_message.content() {
            ProcessedMessageContent::StagedCommitMessage(staged_commit) => {
                let validated_commit = ValidatedCommit::from_staged_commit(
                    &self.client,
                    provider.conn_ref(),
                    staged_commit,
                    mls_group,
                )
                .await?;

                Some(validated_commit)
            }
            _ => None,
        };

        provider.transaction(|provider| {
            tracing::debug!(
                inbox_id = self.client.inbox_id(),
                installation_id = %self.client.installation_id(),
                group_id = hex::encode(&self.group_id),
                current_epoch = mls_group.epoch().as_u64(),
                msg_epoch = processed_message.epoch().as_u64(),
                cursor = ?cursor,
                "[{}] processing message in transaction epoch = {}, cursor = {:?}",
                self.client.inbox_id(),
                mls_group.epoch().as_u64(),
                cursor
            );
            let processed_message = mls_group.process_message(provider, message)?;

            let is_updated = provider.conn_ref().update_cursor(
                &envelope.group_id,
                EntityKind::Group,
                *cursor as i64,
            )?;
            if !is_updated {
                return Err(ProcessIntentError::AlreadyProcessed(*cursor).into());
            }
            let previous_epoch = mls_group.epoch().as_u64();

            self.process_external_message(
                provider,
                mls_group,
                processed_message,
                envelope,
                validated_commit,
            )?;
            let new_epoch = mls_group.epoch().as_u64();
            if new_epoch > previous_epoch {
                tracing::info!(
                    "[{}] externally processed message [{}] advanced epoch from [{}] to [{}]",
                    self.client.inbox_id(),
                    cursor,
                    previous_epoch,
                    new_epoch
                );
            }
            Ok::<_, GroupMessageProcessingError>(())
        })?;

        Ok(())
    }

    #[tracing::instrument(level = "trace", skip_all)]
    fn process_external_message(
        &self,
        provider: &XmtpOpenMlsProvider,
        mls_group: &mut OpenMlsGroup,
        processed_message: ProcessedMessage,
        envelope: &GroupMessageV1,
        validated_commit: Option<ValidatedCommit>,
    ) -> Result<(), GroupMessageProcessingError> {
        let GroupMessageV1 {
            created_ns: envelope_timestamp_ns,
            id: ref cursor,
            ..
        } = *envelope;

        let msg_epoch = processed_message.epoch().as_u64();
        let msg_group_id = hex::encode(processed_message.group_id().as_slice());
        let (sender_inbox_id, sender_installation_id) =
            extract_message_sender(mls_group, &processed_message, envelope_timestamp_ns)?;

        match processed_message.into_content() {
            ProcessedMessageContent::ApplicationMessage(application_message) => {
                tracing::info!(
                    inbox_id = self.client.inbox_id(),
                    sender_inbox_id = sender_inbox_id,
                    sender_installation_id = hex::encode(&sender_installation_id),
                    installation_id = %self.client.installation_id(),group_id = hex::encode(&self.group_id),
                    current_epoch = mls_group.epoch().as_u64(),
                    msg_epoch,
                    msg_group_id,
                    cursor,
                    "[{}] decoding application message",
                    self.context().inbox_id()
                );
                let message_bytes = application_message.into_bytes();

                let mut bytes = Bytes::from(message_bytes.clone());
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
                        StoredGroupMessage {
                            id: message_id,
                            group_id: self.group_id.clone(),
                            decrypted_message_bytes: content,
                            sent_at_ns: envelope_timestamp_ns as i64,
                            kind: GroupMessageKind::Application,
                            sender_installation_id,
                            sender_inbox_id,
                            delivery_status: DeliveryStatus::Published,
                            content_type: queryable_content_fields.content_type,
                            version_major: queryable_content_fields.version_major,
                            version_minor: queryable_content_fields.version_minor,
                            authority_id: queryable_content_fields.authority_id,
                            reference_id: queryable_content_fields.reference_id,
                        }
                        .store_or_ignore(provider.conn_ref())?
                    }
                    Some(Content::V2(V2 {
                        idempotency_key,
                        message_type,
                    })) => {
                        match message_type {
                            Some(MessageType::DeviceSyncRequest(history_request)) => {
                                let content: DeviceSyncContent =
                                    DeviceSyncContent::Request(history_request);
                                let content_bytes = serde_json::to_vec(&content)?;
                                let message_id = calculate_message_id(
                                    &self.group_id,
                                    &content_bytes,
                                    &idempotency_key,
                                );

                                // store the request message
                                StoredGroupMessage {
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
                                }
                                .store_or_ignore(provider.conn_ref())?;

                                tracing::info!("Received a history request.");
                                let _ = self.client.local_events().send(LocalEvents::SyncMessage(
                                    SyncMessage::Request { message_id },
                                ));
                            }

                            Some(MessageType::DeviceSyncReply(history_reply)) => {
                                let content: DeviceSyncContent =
                                    DeviceSyncContent::Reply(history_reply);
                                let content_bytes = serde_json::to_vec(&content)?;
                                let message_id = calculate_message_id(
                                    &self.group_id,
                                    &content_bytes,
                                    &idempotency_key,
                                );

                                // store the reply message
                                StoredGroupMessage {
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
                                }
                                .store_or_ignore(provider.conn_ref())?;

                                tracing::info!("Received a history reply.");
                                let _ = self.client.local_events().send(LocalEvents::SyncMessage(
                                    SyncMessage::Reply { message_id },
                                ));
                            }
                            Some(MessageType::UserPreferenceUpdate(update)) => {
                                // This function inserts the updates appropriately,
                                // and returns a copy of what was inserted
                                let updates =
                                    UserPreferenceUpdate::process_incoming_preference_update(
                                        update, provider,
                                    )?;

                                // Broadcast those updates for integrators to be notified of changes
                                let _ = self
                                    .client
                                    .local_events()
                                    .send(LocalEvents::IncomingPreferenceUpdate(updates));
                            }
                            _ => {
                                return Err(GroupMessageProcessingError::InvalidPayload);
                            }
                        }
                    }
                    None => return Err(GroupMessageProcessingError::InvalidPayload),
                }
            }
            ProcessedMessageContent::ProposalMessage(_proposal_ptr) => {
                // intentionally left blank.
            }
            ProcessedMessageContent::ExternalJoinProposalMessage(_external_proposal_ptr) => {
                // intentionally left blank.
            }
            ProcessedMessageContent::StagedCommitMessage(staged_commit) => {
                let staged_commit = *staged_commit;
                let validated_commit =
                    validated_commit.expect("Needs to be present when this is a staged commit");

                tracing::info!(
                    inbox_id = self.client.inbox_id(),
                    sender_inbox_id = sender_inbox_id,
                    installation_id = %self.client.installation_id(),sender_installation_id = hex::encode(&sender_installation_id),
                    group_id = hex::encode(&self.group_id),
                    current_epoch = mls_group.epoch().as_u64(),
                    msg_epoch,
                    msg_group_id,
                    cursor,
                    "[{}] received staged commit. Merging and clearing any pending commits",
                    self.context().inbox_id()
                );

                tracing::info!(
                    inbox_id = self.client.inbox_id(),
                    sender_inbox_id = sender_inbox_id,
                    installation_id = %self.client.installation_id(),sender_installation_id = hex::encode(&sender_installation_id),
                    group_id = hex::encode(&self.group_id),
                    current_epoch = mls_group.epoch().as_u64(),
                    msg_epoch,
                    msg_group_id,
                    cursor,
                    "[{}] staged commit is valid, will attempt to merge",
                    self.context().inbox_id()
                );

                mls_group.merge_staged_commit(provider, staged_commit)?;
                self.save_transcript_message(
                    provider.conn_ref(),
                    validated_commit,
                    envelope_timestamp_ns,
                )?;
            }
        };

        Ok(())
    }

    /// This function is idempotent. No need to wrap in a transaction.
    #[tracing::instrument(skip(self, provider, envelope), level = "debug")]
    pub(crate) async fn process_message(
        &self,
        provider: &XmtpOpenMlsProvider,
        envelope: &GroupMessageV1,
        allow_epoch_increment: bool,
    ) -> Result<(), GroupMessageProcessingError> {
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
            .conn_ref()
            .find_group_intent_by_payload_hash(sha256(envelope.data.as_slice()));
        tracing::info!(
            inbox_id = self.client.inbox_id(),
            installation_id = %self.client.installation_id(),
            group_id = hex::encode(&self.group_id),
            cursor = envelope.id,
            "Processing envelope with hash {}, id = {}",
            hex::encode(sha256(envelope.data.as_slice())),
            envelope.id
        );

        match intent {
            // Intent with the payload hash matches
            Ok(Some(intent)) => {
                let intent_id = intent.id;
                tracing::info!(
                    inbox_id = self.client.inbox_id(),
                    installation_id = %self.client.installation_id(),
                    group_id = hex::encode(&self.group_id),
                    cursor = envelope.id,
                    intent_id,
                    intent.kind = %intent.kind,
                    "client [{}] is about to process own envelope [{}] for intent [{}]",
                    self.client.inbox_id(),
                    envelope.id,
                    intent_id
                );

                self.load_mls_group_with_lock_async(provider, |mut mls_group| async move  {
                    let message = message.into();
                    let maybe_validated_commit = self.stage_and_validate_intent(&mls_group, &intent, provider, &message, envelope).await;

                    provider.transaction(|provider| {
                        let is_updated =
                            provider
                                .conn_ref()
                                .update_cursor(&envelope.group_id, EntityKind::Group, cursor as i64)?;
                        if !is_updated {
                            return Err(ProcessIntentError::AlreadyProcessed(cursor).into());
                        }

                        let intent_state = match maybe_validated_commit {
                            Err(err) => err?,
                            Ok(commit) => {
                                self
                                .process_own_message(&mut mls_group, commit, &intent, provider, &message, envelope)?
                            }
                        };
                        match intent_state {
                            IntentState::ToPublish => {
                                Ok::<_, GroupMessageProcessingError>(provider.conn_ref().set_group_intent_to_publish(intent_id)?)
                            }
                            IntentState::Committed => {
                                self.handle_metadata_update_from_intent(provider, &intent)?;
                                Ok(provider.conn_ref().set_group_intent_committed(intent_id)?)
                            }
                            IntentState::Published => {
                                tracing::error!("Unexpected behaviour: returned intent state published from process_own_message");
                                Ok(())
                            }
                            IntentState::Error => {
                                tracing::warn!("Intent [{}] moved to error status", intent_id);
                                Ok(provider.conn_ref().set_group_intent_error(intent_id)?)
                            }
                        }
                    })
                }).await?;

                Ok(())
            }
            // No matching intent found. The message did not originate here.
            Ok(None) => {
                tracing::info!(
                    inbox_id = self.client.inbox_id(),
                    installation_id = %self.client.installation_id(),
                    group_id = hex::encode(&self.group_id),
                    cursor = envelope.id,
                    "client [{}] is about to process external envelope [{}]",
                    self.client.inbox_id(),
                    envelope.id
                );

                self.load_mls_group_with_lock_async(provider, |mut mls_group| async move {
                    self.validate_and_process_external_message(
                        provider,
                        &mut mls_group,
                        message,
                        envelope,
                    )
                    .await
                })
                .await?;

                Ok(())
            }
            Err(err) => Err(GroupMessageProcessingError::Storage(err)),
        }
    }

    /// In case of metadataUpdate will extract the updated fields and store them to the db
    fn handle_metadata_update_from_intent(
        &self,
        provider: &XmtpOpenMlsProvider,
        intent: &StoredGroupIntent,
    ) -> Result<(), StorageError> {
        if intent.kind == MetadataUpdate {
            let data = UpdateMetadataIntentData::try_from(intent.data.clone())?;

            match data.field_name.as_str() {
                field_name if field_name == MetadataField::MessageDisappearFromNS.as_str() => {
                    provider.conn_ref().update_message_disappearing_from_ns(
                        self.group_id.clone(),
                        data.field_value.parse::<i64>().ok(),
                    )?
                }
                field_name if field_name == MetadataField::MessageDisappearInNS.as_str() => {
                    provider.conn_ref().update_message_disappearing_in_ns(
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
        conn: &DbConnection,
        metadata_field_changes: Vec<group_updated::MetadataFieldChange>,
    ) -> Result<(), StorageError> {
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

    #[tracing::instrument(level = "trace", skip_all)]
    async fn consume_message(
        &self,
        provider: &XmtpOpenMlsProvider,
        envelope: &GroupMessage,
    ) -> Result<(), GroupMessageProcessingError> {
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
            .conn_ref()
            .get_last_cursor_for_id(&self.group_id, message_entity_kind)?;
        let should_skip_message = last_cursor > msgv1.id as i64;
        if should_skip_message {
            tracing::info!(
                inbox_id = self.client.inbox_id(),
                installation_id = %self.client.installation_id(),
                group_id = hex::encode(&self.group_id),
                "Message already processed: skipped cursor:[{}] entity kind:[{:?}] last cursor in db: [{}]",
                msgv1.id,
                message_entity_kind,
                last_cursor
            );
            Err(GroupMessageProcessingError::AlreadyProcessed(msgv1.id))
        } else {
            // Download all unread welcome messages and convert to groups.
            // In a database transaction, increment the cursor for a given entity and
            // apply the update after the provided `ProcessingFn` has completed successfully.
            self.process_message(provider, msgv1, true).await
            .inspect(|_| {
                tracing::info!(
                    "Transaction completed successfully: process for group [{}] envelope cursor[{}]",
                    hex::encode(&msgv1.group_id),
                    msgv1.id
                );
            })
            .inspect_err(|err| {
                tracing::info!(
                    "Transaction failed: process for group [{}] envelope cursor [{}] error:[{}]",
                    hex::encode(&msgv1.group_id),
                    msgv1.id,
                    err
                );
            })?;
            Ok(())
        }
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn process_messages(
        &self,
        messages: Vec<GroupMessage>,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<(), GroupError> {
        let mut receive_errors: Vec<GroupMessageProcessingError> = vec![];
        for message in messages.into_iter() {
            let result = retry_async!(
                Retry::default(),
                (async { self.consume_message(provider, &message).await })
            );
            if let Err(e) = result {
                let is_retryable = e.is_retryable();
                let error_message = e.to_string();
                receive_errors.push(e);
                // If the error is retryable we cannot move on to the next message
                // otherwise you can get into a forked group state.
                if is_retryable {
                    tracing::error!(
                        error = %error_message,
                        "Aborting message processing for retryable error: {}",
                        error_message
                    );
                    break;
                }
            }
        }

        if receive_errors.is_empty() {
            Ok(())
        } else {
            tracing::error!(
                group_id = hex::encode(&self.group_id),
                inbox_id = self.client.inbox_id(),
                installation_id = hex::encode(self.client.installation_id()),
                "Message processing errors: {:?}",
                receive_errors
            );
            Err(GroupError::ReceiveErrors(receive_errors))
        }
    }

    #[tracing::instrument(skip_all, level = "debug")]
    pub(super) async fn receive(&self, provider: &XmtpOpenMlsProvider) -> Result<(), GroupError> {
        let messages = self
            .client
            .query_group_messages(&self.group_id, provider.conn_ref())
            .await?;
        self.process_messages(messages, provider).await?;
        Ok(())
    }

    fn save_transcript_message(
        &self,
        conn: &DbConnection,
        validated_commit: ValidatedCommit,
        timestamp_ns: u64,
    ) -> Result<Option<StoredGroupMessage>, GroupMessageProcessingError> {
        if validated_commit.is_empty() {
            return Ok(None);
        }

        tracing::info!(
            "[{}]: Storing a transcript message with {} members added and {} members removed and {} metadata changes",
            self.context().inbox_id(),
            validated_commit.added_inboxes.len(),
            validated_commit.removed_inboxes.len(),
            validated_commit.metadata_changes.metadata_field_changes.len(),
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
        self.handle_metadata_update_from_commit(conn, payload.metadata_field_changes)?;
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
        };
        msg.store_or_ignore(conn)?;
        Ok(Some(msg))
    }

    #[tracing::instrument(skip_all)]
    pub(super) async fn publish_intents(
        &self,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<(), GroupError> {
        let start = std::time::Instant::now();
        tracing::info!(
            inbox_id = self.client.inbox_id(),
            group_id = hex::encode(&self.group_id),
            "Starting publish_intents"
        );

        self.load_mls_group_with_lock_async(provider, |mut mls_group| async move {
        let find_intents_start = std::time::Instant::now();
        let intents = provider.conn_ref().find_group_intents(
            self.group_id.clone(),
            Some(vec![IntentState::ToPublish]),
            None,
        )?;
        tracing::info!(
            "Found {} intents to publish in {:?}",
            intents.len(),
            find_intents_start.elapsed()
        );

        // Count intents by type
        let intent_types: std::collections::HashMap<_, _> = intents
            .iter()
            .fold(std::collections::HashMap::new(), |mut map, intent| {
                *map.entry(intent.kind.clone()).or_insert(0) += 1;
                map
            });
        tracing::info!("Intent types to publish: {:?}", intent_types);

        let mut published_count = 0;
        let mut skipped_count = 0;
        let mut error_count = 0;

        for intent in intents {
            let intent_start = std::time::Instant::now();
            let get_data_start = std::time::Instant::now();
            let result = retry_async!(
                Retry::default(),
                (async {
                    self.get_publish_intent_data(provider, &mut mls_group, &intent)
                        .await
                })
            );
            let get_data_duration = get_data_start.elapsed();

            match result {
                Err(err) => {
                    error_count += 1;
                    let duration = intent_start.elapsed();
                    tracing::error!(
                        error = %err,
                        intent_type = %intent.kind,
                        duration = ?duration,
                        get_data_duration = ?get_data_duration,
                        "Error getting publish intent data (get_data: {:?}, total: {:?})",
                        get_data_duration,
                        duration
                    );
                    if (intent.publish_attempts + 1) as usize >= MAX_INTENT_PUBLISH_ATTEMPTS {
                        tracing::error!(
                            intent.id,
                            intent.kind = %intent.kind,
                            inbox_id = self.client.inbox_id(),
                            installation_id = %self.client.installation_id(),
                            group_id = hex::encode(&self.group_id),
                            duration = ?duration,
                            "Intent {} has reached max publish attempts (took {:?})",
                            intent.id,
                            duration
                        );
                        provider
                            .conn_ref()
                            .set_group_intent_error_and_fail_msg(&intent)?;
                    } else {
                        provider
                            .conn_ref()
                            .increment_intent_publish_attempt_count(intent.id)?;
                    }
                    return Err(err);
                }
                Ok(Some(PublishIntentData {
                    payload_to_publish,
                    post_commit_action,
                    staged_commit,
                })) => {
                    published_count += 1;
                    let payload_slice = payload_to_publish.as_slice();
                    let has_staged_commit = staged_commit.is_some();

                    let set_published_start = std::time::Instant::now();
                    provider.conn_ref().set_group_intent_published(
                        intent.id,
                        sha256(payload_slice),
                        post_commit_action,
                        staged_commit,
                        mls_group.epoch().as_u64() as i64,
                    )?;
                    let set_published_duration = set_published_start.elapsed();

                    let prepare_msgs_start = std::time::Instant::now();
                    let messages = self.prepare_group_messages(vec![payload_slice])?;
                    let prepare_msgs_duration = prepare_msgs_start.elapsed();

                    let send_msgs_start = std::time::Instant::now();
                    self.client
                        .api()
                        .send_group_messages(messages)
                        .await?;
                    let send_msgs_duration = send_msgs_start.elapsed();

                    let total_duration = intent_start.elapsed();

                    tracing::info!(
                        intent.id,
                        intent.kind = %intent.kind,
                        inbox_id = self.client.inbox_id(),
                        installation_id = %self.client.installation_id(),
                        group_id = hex::encode(&self.group_id),
                        get_data_duration = ?get_data_duration,
                        set_published_duration = ?set_published_duration,
                        prepare_msgs_duration = ?prepare_msgs_duration,
                        send_msgs_duration = ?send_msgs_duration,
                        total_duration = ?total_duration,
                        "Published intent successfully (get_data: {:?}, set_published: {:?}, prepare_msgs: {:?}, send_msgs: {:?}, total: {:?})",
                        get_data_duration,
                        set_published_duration,
                        prepare_msgs_duration,
                        send_msgs_duration,
                        total_duration
                    );

                    if has_staged_commit {
                        tracing::info!(
                            "Commit sent. Stopping further publishes for this round. Published {} intents, skipped {}, errors {} (get_data: {:?}, set_published: {:?}, prepare_msgs: {:?}, send_msgs: {:?}, total: {:?})",
                            published_count,
                            skipped_count,
                            error_count,
                            get_data_duration,
                            set_published_duration,
                            prepare_msgs_duration,
                            send_msgs_duration,
                            total_duration
                        );
                        return Ok(());
                    }
                }
                Ok(None) => {
                    skipped_count += 1;
                    let duration = intent_start.elapsed();
                    tracing::info!(
                        inbox_id = self.client.inbox_id(),
                        installation_id = %self.client.installation_id(),
                        duration = ?duration,
                        "Skipping intent because no publish data returned (total: {:?})",
                        duration
                    );
                    let deleter: &dyn Delete<StoredGroupIntent, Key = i32> = provider.conn_ref();
                    deleter.delete(intent.id)?;
                }
            }
        }

        let total_duration = start.elapsed();
        tracing::info!(
            "Completed publish_intents in {:?}. Published: {}, Skipped: {}, Errors: {}",
            total_duration,
            published_count,
            skipped_count,
            error_count
        );

        Ok(())
    })
    .await
    }

    // Takes a StoredGroupIntent and returns the payload and post commit data as a tuple
    // A return value of [`Option::None`] means this intent would not change the group.
    #[allow(clippy::type_complexity)]
    #[tracing::instrument(level = "trace", skip_all)]
    async fn get_publish_intent_data(
        &self,
        provider: &XmtpOpenMlsProvider,
        openmls_group: &mut OpenMlsGroup,
        intent: &StoredGroupIntent,
    ) -> Result<Option<PublishIntentData>, GroupError> {
        match intent.kind {
            IntentKind::UpdateGroupMembership => {
                let intent_data = UpdateGroupMembershipIntentData::try_from(&intent.data)?;
                let signer = &self.context().identity.installation_keys;
                apply_update_group_membership_intent(
                    self.client.as_ref(),
                    provider,
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
                }))
            }
            IntentKind::KeyUpdate => {
                let (commit, _, _) = openmls_group.self_update(
                    &provider,
                    &self.context().identity.installation_keys,
                    LeafNodeParameters::default(),
                )?;

                Ok(Some(PublishIntentData {
                    payload_to_publish: commit.tls_serialize_detached()?,
                    staged_commit: get_and_clear_pending_commit(openmls_group, provider)?,
                    post_commit_action: None,
                }))
            }
            IntentKind::MetadataUpdate => {
                let metadata_intent = UpdateMetadataIntentData::try_from(intent.data.clone())?;
                let mutable_metadata_extensions = build_extensions_for_metadata_update(
                    openmls_group,
                    metadata_intent.field_name,
                    metadata_intent.field_value,
                )?;

                let (commit, _, _) = openmls_group.update_group_context_extensions(
                    &provider,
                    mutable_metadata_extensions,
                    &self.context().identity.installation_keys,
                )?;

                let commit_bytes = commit.tls_serialize_detached()?;

                Ok(Some(PublishIntentData {
                    payload_to_publish: commit_bytes,
                    staged_commit: get_and_clear_pending_commit(openmls_group, provider)?,
                    post_commit_action: None,
                }))
            }
            IntentKind::UpdateAdminList => {
                let admin_list_update_intent =
                    UpdateAdminListIntentData::try_from(intent.data.clone())?;
                let mutable_metadata_extensions = build_extensions_for_admin_lists_update(
                    openmls_group,
                    admin_list_update_intent,
                )?;

                let (commit, _, _) = openmls_group.update_group_context_extensions(
                    provider,
                    mutable_metadata_extensions,
                    &self.context().identity.installation_keys,
                )?;
                let commit_bytes = commit.tls_serialize_detached()?;

                Ok(Some(PublishIntentData {
                    payload_to_publish: commit_bytes,
                    staged_commit: get_and_clear_pending_commit(openmls_group, provider)?,
                    post_commit_action: None,
                }))
            }
            IntentKind::UpdatePermission => {
                let update_permissions_intent =
                    UpdatePermissionIntentData::try_from(intent.data.clone())?;
                let group_permissions_extensions = build_extensions_for_permissions_update(
                    openmls_group,
                    update_permissions_intent,
                )?;
                let (commit, _, _) = openmls_group.update_group_context_extensions(
                    provider,
                    group_permissions_extensions,
                    &self.context().identity.installation_keys,
                )?;
                let commit_bytes = commit.tls_serialize_detached()?;
                Ok(Some(PublishIntentData {
                    payload_to_publish: commit_bytes,
                    staged_commit: get_and_clear_pending_commit(openmls_group, provider)?,
                    post_commit_action: None,
                }))
            }
        }
    }

    #[tracing::instrument(skip_all)]
    pub(crate) async fn post_commit(&self, conn: &DbConnection) -> Result<(), GroupError> {
        let intents = conn.find_group_intents(
            self.group_id.clone(),
            Some(vec![IntentState::Committed]),
            None,
        )?;

        for intent in intents {
            if let Some(post_commit_data) = intent.post_commit_data {
                tracing::debug!(
                    inbox_id = self.client.inbox_id(),
                    installation_id = %self.client.installation_id(),
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
            let deleter: &dyn Delete<StoredGroupIntent, Key = i32> = conn;
            deleter.delete(intent.id)?;
        }

        Ok(())
    }

    pub async fn maybe_update_installations(
        &self,
        provider: &XmtpOpenMlsProvider,
        update_interval_ns: Option<i64>,
    ) -> Result<(), GroupError> {
        // determine how long of an interval in time to use before updating list
        let interval_ns = update_interval_ns.unwrap_or(sync_update_installations_interval_ns());

        let now_ns = xmtp_common::time::now_ns();
        let last_ns = provider
            .conn_ref()
            .get_installations_time_checked(self.group_id.clone())?;
        let elapsed_ns = now_ns - last_ns;
        if elapsed_ns > interval_ns {
            self.add_missing_installations(provider).await?;
            provider
                .conn_ref()
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
    pub(super) async fn add_missing_installations(
        &self,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<(), GroupError> {
        let intent_data = self
            .get_membership_update_intent(provider, &[], &[])
            .await?;

        // If there is nothing to do, stop here
        if intent_data.is_empty() {
            return Ok(());
        }

        debug!(
            inbox_id = self.client.inbox_id(),
            installation_id = %self.client.installation_id(),
            "Adding missing installations {:?}",
            intent_data
        );

        let intent = self.queue_intent(
            provider,
            IntentKind::UpdateGroupMembership,
            intent_data.into(),
        )?;

        self.sync_until_intent_resolved(provider, intent.id).await
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
        provider: &XmtpOpenMlsProvider,
        inbox_ids_to_add: &[InboxIdRef<'_>],
        inbox_ids_to_remove: &[InboxIdRef<'_>],
    ) -> Result<UpdateGroupMembershipIntentData, GroupError> {
        let start = std::time::Instant::now();

        self.load_mls_group_with_lock_async(provider, |mls_group| async move {
            let extract_membership_start = std::time::Instant::now();
            let existing_group_membership = extract_group_membership(mls_group.extensions())?;
            let mut inbox_ids = existing_group_membership.inbox_ids();
            inbox_ids.extend_from_slice(inbox_ids_to_add);
            let extract_membership_duration = extract_membership_start.elapsed();

            tracing::debug!(
                inbox_id = self.client.inbox_id(),
                group_id = hex::encode(&self.group_id),
                extract_membership_duration = ?extract_membership_duration,
                "Extracted group membership in {:?}",
                extract_membership_duration
            );

            let conn = provider.conn_ref();

            // Load any missing updates from the network
            let load_updates_start = std::time::Instant::now();
            load_identity_updates(self.client.api(), conn, &inbox_ids).await?;
            let load_updates_duration = load_updates_start.elapsed();

            tracing::debug!(
                inbox_id = self.client.inbox_id(),
                group_id = hex::encode(&self.group_id),
                load_updates_duration = ?load_updates_duration,
                "Loaded identity updates in {:?}",
                load_updates_duration
            );

            let sequence_id_start = std::time::Instant::now();
            let latest_sequence_id_map = conn.get_latest_sequence_id(&inbox_ids as &[&str])?;
            let sequence_id_duration = sequence_id_start.elapsed();

            // Get a list of all inbox IDs that have increased sequence_id for the group
            let changed_inbox_ids =
                inbox_ids
                    .iter()
                    .try_fold(HashMap::new(), |mut updates, inbox_id| {
                        match (
                            latest_sequence_id_map.get(inbox_id as &str),
                            existing_group_membership.get(inbox_id),
                        ) {
                            (Some(latest_sequence_id), Some(current_sequence_id)) => {
                                let latest_sequence_id_u64 = *latest_sequence_id as u64;
                                if latest_sequence_id_u64.gt(current_sequence_id) {
                                    updates.insert(inbox_id.to_string(), latest_sequence_id_u64);
                                }
                            }
                            (Some(latest_sequence_id), None) => {
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

            let calc_changes_setup_start = std::time::Instant::now();
            let extensions: Extensions = mls_group.extensions().clone();
            let old_group_membership = extract_group_membership(&extensions)?;
            let mut new_membership = old_group_membership.clone();
            for (inbox_id, sequence_id) in changed_inbox_ids.iter() {
                new_membership.add(inbox_id.clone(), *sequence_id);
            }
            let calc_changes_setup_duration = calc_changes_setup_start.elapsed();

            let calc_changes_start = std::time::Instant::now();
            let changes_with_kps = calculate_membership_changes_with_keypackages(
                &self.client,
                provider,
                &new_membership,
                &old_group_membership,
            )
            .await?;
            let calc_changes_duration = calc_changes_start.elapsed();

            tracing::info!(
                inbox_id = self.client.inbox_id(),
                group_id = hex::encode(&self.group_id),
                extract_membership_duration = ?extract_membership_duration,
                load_updates_duration = ?load_updates_duration,
                sequence_id_duration = ?sequence_id_duration,
                calc_changes_setup_duration = ?calc_changes_setup_duration,
                calc_changes_duration = ?calc_changes_duration,
                total_duration = ?start.elapsed(),
                "Membership update intent processing times: extract={:?}, load_updates={:?}, sequence_id={:?}, calc_changes_setup={:?}, calc_changes={:?}, total={:?}",
                extract_membership_duration,
                load_updates_duration,
                sequence_id_duration,
                calc_changes_setup_duration,
                calc_changes_duration,
                start.elapsed()
            );

            // If we fail to fetch or verify all the added members' KeyPackage, return an error.
            if !inbox_ids_to_add.is_empty()
                && !changes_with_kps.failed_installations.is_empty()
                && changes_with_kps.new_installations.is_empty()
            {
                return Err(GroupError::Generic(
                    "Failed to verify all installations".to_string(),
                ));
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
                    })),
                })
            })
            .collect::<Result<Vec<WelcomeMessageInput>, HpkeError>>()?;

        let welcome = welcomes
            .first()
            .ok_or(GroupError::Generic("No welcomes to send".to_string()))?;

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
        let api = self.client.api();
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
        let conn = self.client.store().conn()?;

        let preferences = StoredUserPreferences::load(&conn)?;
        let mut ikm = match preferences.hmac_key {
            Some(ikm) => ikm,
            None => {
                let local_events = self.client.local_events();
                StoredUserPreferences::new_hmac_key(&conn, local_events)?
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
        payloads: Vec<&[u8]>,
    ) -> Result<Vec<GroupMessageInput>, GroupError> {
        let hmac_key = self
            .hmac_keys(0..=0)?
            .pop()
            .expect("Range of count 1 was provided.");
        let sender_hmac =
            Hmac::<Sha256>::new_from_slice(&hmac_key.key).expect("HMAC can take key of any size");

        let mut result = vec![];
        for payload in payloads {
            let mut sender_hmac = sender_hmac.clone();
            sender_hmac.update(payload);
            let sender_hmac = sender_hmac.finalize();

            result.push(GroupMessageInput {
                version: Some(GroupMessageInputVersion::V1(GroupMessageInputV1 {
                    data: payload.to_vec(),
                    sender_hmac: sender_hmac.into_bytes().to_vec(),
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

async fn calculate_membership_changes_with_keypackages<'a>(
    client: impl ScopedGroupClient,
    provider: &'a XmtpOpenMlsProvider,
    new_group_membership: &'a GroupMembership,
    old_group_membership: &'a GroupMembership,
) -> Result<MembershipDiffWithKeyPackages, GroupError> {
    let membership_diff = old_group_membership.diff(new_group_membership);

    let installation_diff = client
        .get_installation_diff(
            provider.conn_ref(),
            old_group_membership,
            new_group_membership,
            &membership_diff,
        )
        .await?;

    let mut new_installations = Vec::new();
    let mut new_key_packages = Vec::new();
    let mut failed_installations = Vec::new();

    if !installation_diff.added_installations.is_empty() {
        let key_packages = get_keypackages_for_installation_ids(
            client,
            installation_diff.added_installations,
            &mut failed_installations,
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
                Err(_) => failed_installations.push(installation_id.clone()),
            }
        }
    }

    Ok(MembershipDiffWithKeyPackages::new(
        new_installations,
        new_key_packages,
        installation_diff.removed_installations,
        failed_installations,
    ))
}
#[allow(dead_code)]
#[cfg(any(test, feature = "test-utils"))]
async fn get_keypackages_for_installation_ids(
    client: impl ScopedGroupClient,
    added_installations: HashSet<Vec<u8>>,
    failed_installations: &mut Vec<Vec<u8>>,
) -> Result<HashMap<Vec<u8>, Result<VerifiedKeyPackageV2, KeyPackageVerificationError>>, ClientError>
{
    use crate::utils::{
        get_test_mode_malformed_installations, is_test_mode_upload_malformed_keypackage,
    };

    let my_installation_id = client.context().installation_public_key().to_vec();
    let key_packages = client
        .get_key_packages_for_installation_ids(
            added_installations
                .iter()
                .filter(|installation| my_installation_id.ne(*installation))
                .cloned()
                .collect(),
        )
        .await?;

    tracing::info!("trying to validate keypackages");

    Ok(if is_test_mode_upload_malformed_keypackage() {
        let malformed_installations = get_test_mode_malformed_installations();
        failed_installations.extend(malformed_installations.clone());

        // Return only valid key packages (excluding malformed)
        key_packages
            .into_iter()
            .filter(|(id, _)| !malformed_installations.contains(id))
            .collect::<HashMap<_, _>>()
    } else {
        key_packages
    })
}
#[allow(unused_variables, dead_code)]
#[cfg(not(any(test, feature = "test-utils")))]
async fn get_keypackages_for_installation_ids(
    client: impl ScopedGroupClient,
    added_installations: HashSet<Vec<u8>>,
    failed_installations: &mut Vec<Vec<u8>>,
) -> Result<HashMap<Vec<u8>, Result<VerifiedKeyPackageV2, KeyPackageVerificationError>>, ClientError>
{
    let my_installation_id = client.context().installation_public_key().to_vec();
    client
        .get_key_packages_for_installation_ids(
            added_installations
                .iter()
                .filter(|installation| my_installation_id.ne(*installation))
                .cloned()
                .collect(),
        )
        .await
}

// Takes UpdateGroupMembershipIntentData and applies it to the openmls group
// returning the commit and post_commit_action
#[tracing::instrument(level = "trace", skip_all)]
async fn apply_update_group_membership_intent(
    client: impl ScopedGroupClient,
    provider: &XmtpOpenMlsProvider,
    openmls_group: &mut OpenMlsGroup,
    intent_data: UpdateGroupMembershipIntentData,
    signer: impl Signer,
) -> Result<Option<PublishIntentData>, GroupError> {
    let extensions: Extensions = openmls_group.extensions().clone();
    let old_group_membership = extract_group_membership(&extensions)?;
    let mut new_group_membership = intent_data.apply_to_group_membership(&old_group_membership);
    let membership_diff = old_group_membership.diff(&new_group_membership);

    let changes_with_kps = calculate_membership_changes_with_keypackages(
        client,
        provider,
        &new_group_membership,
        &old_group_membership,
    )
    .await?;
    let leaf_nodes_to_remove: Vec<LeafNodeIndex> =
        get_removed_leaf_nodes(openmls_group, &changes_with_kps.removed_installations);

    if leaf_nodes_to_remove.is_empty()
        && changes_with_kps.new_key_packages.is_empty()
        && membership_diff.updated_inboxes.is_empty()
    {
        return Ok(None);
    }

    // Update the extensions to have the new GroupMembership
    let mut new_extensions = extensions.clone();
    new_group_membership.failed_installations = changes_with_kps.failed_installations;
    new_extensions.add_or_replace(build_group_membership_extension(&new_group_membership));

    // Create the commit
    let (commit, maybe_welcome_message, _) = openmls_group.update_group_membership(
        provider,
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

    Ok(Some(PublishIntentData {
        payload_to_publish: commit.tls_serialize_detached()?,
        post_commit_action: post_commit_action.map(|action| action.to_bytes()),
        staged_commit: Some(staged_commit),
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

fn get_and_clear_pending_commit(
    openmls_group: &mut OpenMlsGroup,
    provider: &XmtpOpenMlsProvider,
) -> Result<Option<Vec<u8>>, GroupError> {
    let commit = openmls_group
        .pending_commit()
        .as_ref()
        .map(db_serialize)
        .transpose()?;
    openmls_group.clear_pending_commit(provider.storage())?;
    Ok(commit)
}

fn decode_staged_commit(data: &[u8]) -> Result<StagedCommit, GroupMessageProcessingError> {
    Ok(db_deserialize(data)?)
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use super::*;
    use crate::builder::ClientBuilder;
    use std::sync::Arc;
    use xmtp_cryptography::utils::generate_local_wallet;

    /// This test is not reproducible in webassembly, b/c webassembly has only one thread.
    #[cfg_attr(
        not(target_arch = "wasm32"),
        tokio::test(flavor = "multi_thread", worker_threads = 10)
    )]
    #[cfg(not(target_family = "wasm"))]
    async fn publish_intents_worst_case_scenario() {
        let wallet = generate_local_wallet();
        let amal_a = Arc::new(ClientBuilder::new_test_client(&wallet).await);
        let amal_group_a: Arc<MlsGroup<_>> =
            Arc::new(amal_a.create_group(None, Default::default()).unwrap());

        let conn = amal_a.context().store().conn().unwrap();
        let provider: Arc<XmtpOpenMlsProvider> = Arc::new(conn.into());

        // create group intent
        amal_group_a.sync().await.unwrap();
        assert_eq!(provider.conn_ref().intents_deleted(), 1);

        for _ in 0..100 {
            let s = xmtp_common::rand_string::<100>();
            amal_group_a.send_message_optimistic(s.as_bytes()).unwrap();
        }

        let mut set = tokio::task::JoinSet::new();
        for _ in 0..50 {
            let g = amal_group_a.clone();
            let p = provider.clone();
            set.spawn(async move { g.publish_intents(&p).await });
        }

        let res = set.join_all().await;
        let errs: Vec<&Result<_, _>> = res.iter().filter(|r| r.is_err()).collect();
        errs.iter().for_each(|e| {
            tracing::error!("{}", e.as_ref().unwrap_err());
        });

        let published = provider.conn_ref().intents_published();
        assert_eq!(published, 101);
        let created = provider.conn_ref().intents_created();
        assert_eq!(created, 101);
        if !errs.is_empty() {
            panic!("Errors during publish");
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test(flavor = "multi_thread"))]
    async fn hmac_keys_work_as_expected() {
        let wallet = generate_local_wallet();
        let amal = Arc::new(ClientBuilder::new_test_client(&wallet).await);
        let amal_group: Arc<MlsGroup<_>> =
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
