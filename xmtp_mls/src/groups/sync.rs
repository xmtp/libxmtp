use std::{
    collections::{HashMap, HashSet},
    mem::discriminant,
};

use super::{
    build_extensions_for_admin_lists_update, build_extensions_for_metadata_update,
    build_extensions_for_permissions_update, build_group_membership_extension,
    intents::{
        Installation, PostCommitAction, SendMessageIntentData, SendWelcomesAction,
        UpdateAdminListIntentData, UpdateGroupMembershipIntentData, UpdatePermissionIntentData,
    },
    validated_commit::extract_group_membership,
    GroupError, MlsGroup,
};
use crate::{
    client::MessageProcessingError,
    codecs::{group_updated::GroupUpdatedCodec, ContentCodec},
    configuration::{
        DELIMITER, GRPC_DATA_LIMIT, MAX_GROUP_SIZE, MAX_INTENT_PUBLISH_ATTEMPTS,
        UPDATE_INSTALLATIONS_INTERVAL_NS,
    },
    groups::{
        intents::UpdateMetadataIntentData,
        message_history::{decrypt_history_file, download_history_bundle},
        validated_commit::ValidatedCommit,
    },
    hpke::{encrypt_welcome, HpkeError},
    identity::parse_credential,
    identity_updates::load_identity_updates,
    retry::Retry,
    retry_async,
    storage::{
        db_connection::DbConnection,
        group_intent::{IntentKind, IntentState, NewGroupIntent, StoredGroupIntent, ID},
        group_message::{DeliveryStatus, GroupMessageKind, StoredGroupMessage},
        refresh_state::EntityKind,
    },
    utils::{hash::sha256, id::calculate_message_id},
    xmtp_openmls_provider::XmtpOpenMlsProvider,
    Client, Delete, Fetch, Store, XmtpApi,
};
use futures::future::try_join_all;
use log::debug;
use openmls::{
    credentials::BasicCredential,
    extensions::Extensions,
    framing::{MlsMessageOut, ProtocolMessage},
    group::GroupEpoch,
    prelude::{
        tls_codec::{Deserialize, Serialize},
        LeafNodeIndex, MlsGroup as OpenMlsGroup, MlsMessageBodyIn, MlsMessageIn, PrivateMessageIn,
        ProcessedMessage, ProcessedMessageContent, Sender,
    },
    prelude_test::KeyPackage,
};
use openmls_basic_credential::SignatureKeyPair;
use openmls_traits::OpenMlsProvider;
use prost::bytes::Bytes;
use prost::Message;
use xmtp_id::InboxId;
use xmtp_proto::xmtp::mls::{
    api::v1::{
        group_message::{Version as GroupMessageVersion, V1 as GroupMessageV1},
        welcome_message_input::{
            Version as WelcomeMessageInputVersion, V1 as WelcomeMessageInputV1,
        },
        GroupMessage, WelcomeMessageInput,
    },
    message_contents::{
        plaintext_envelope::{
            v2::MessageType::{Reply, Request},
            Content, V1, V2,
        },
        GroupUpdated, MessageHistoryReply, MessageHistoryRequest, PlaintextEnvelope,
    },
};

impl MlsGroup {
    pub async fn sync<ApiClient>(&self, client: &Client<ApiClient>) -> Result<(), GroupError>
    where
        ApiClient: XmtpApi,
    {
        let conn = self.context.store.conn()?;
        let mls_provider = client.mls_provider(conn.clone());

        log::info!("[{}] syncing group", client.inbox_id());
        log::info!(
            "current epoch for [{}] in sync() is Epoch: [{}]",
            client.inbox_id(),
            self.load_mls_group(mls_provider).unwrap().epoch()
        );
        self.maybe_update_installations(conn.clone(), None, client)
            .await?;

        self.sync_with_conn(conn, client).await
    }

    #[tracing::instrument(level = "trace", skip(client, self, conn))]
    pub(super) async fn sync_with_conn<ApiClient>(
        &self,
        conn: DbConnection,
        client: &Client<ApiClient>,
    ) -> Result<(), GroupError>
    where
        ApiClient: XmtpApi,
    {
        let mut errors: Vec<GroupError> = vec![];

        // Even if publish fails, continue to receiving
        if let Err(publish_error) = self.publish_intents(conn.clone(), client).await {
            log::error!("Sync: error publishing intents {:?}", publish_error);
            errors.push(publish_error);
        }

        // Even if receiving fails, continue to post_commit
        if let Err(receive_error) = self.receive(&conn, client).await {
            log::error!("receive error {:?}", receive_error);
            // We don't return an error if receive fails, because it's possible this is caused
            // by malicious data sent over the network, or messages from before the user was
            // added to the group
        }

        if let Err(post_commit_err) = self.post_commit(&conn, client).await {
            log::error!("post commit error {:?}", post_commit_err);
            errors.push(post_commit_err);
        }

        // Return a combination of publish and post_commit errors
        if !errors.is_empty() {
            return Err(GroupError::Sync(errors));
        }

        Ok(())
    }

    pub(super) async fn sync_until_last_intent_resolved<ApiClient>(
        &self,
        conn: DbConnection,
        client: &Client<ApiClient>,
    ) -> Result<(), GroupError>
    where
        ApiClient: XmtpApi,
    {
        let intents = conn.find_group_intents(
            self.group_id.clone(),
            Some(vec![IntentState::ToPublish, IntentState::Published]),
            None,
        )?;

        if intents.is_empty() {
            return Ok(());
        }

        self.sync_until_intent_resolved(conn, intents[intents.len() - 1].id, client)
            .await
    }

    /**
     * Sync the group and wait for the intent to be deleted
     * Group syncing may involve picking up messages unrelated to the intent, so simply checking for errors
     * does not give a clear signal as to whether the intent was successfully completed or not.
     *
     * This method will retry up to `crate::configuration::MAX_GROUP_SYNC_RETRIES` times.
     */
    #[tracing::instrument(level = "trace", skip(client, self, conn))]
    pub(super) async fn sync_until_intent_resolved<ApiClient>(
        &self,
        conn: DbConnection,
        intent_id: ID,
        client: &Client<ApiClient>,
    ) -> Result<(), GroupError>
    where
        ApiClient: XmtpApi,
    {
        let mut num_attempts = 0;
        // Return the last error to the caller if we fail to sync
        let mut last_err: Option<GroupError> = None;
        while num_attempts < crate::configuration::MAX_GROUP_SYNC_RETRIES {
            if let Err(err) = self.sync_with_conn(conn.clone(), client).await {
                log::error!("error syncing group {:?}", err);
                last_err = Some(err);
            }

            match Fetch::<StoredGroupIntent>::fetch(&conn, &intent_id) {
                Ok(None) => {
                    // This is expected. The intent gets deleted on success
                    return Ok(());
                }
                Ok(Some(StoredGroupIntent {
                    id,
                    state: IntentState::Error,
                    ..
                })) => {
                    log::warn!("not retrying intent ID {id}. since it is in state Error",);
                    return Err(last_err.unwrap_or(GroupError::Generic(
                        "Group intent could not be committed".to_string(),
                    )));
                }
                Ok(Some(StoredGroupIntent { id, state, .. })) => {
                    log::warn!("retrying intent ID {id}. intent currently in state {state:?}");
                }
                Err(err) => {
                    log::error!("database error fetching intent {:?}", err);
                    last_err = Some(GroupError::Storage(err));
                }
            };
            num_attempts += 1;
        }

        Err(last_err.unwrap_or(GroupError::Generic("failed to wait for intent".to_string())))
    }

    fn is_valid_epoch(
        inbox_id: InboxId,
        intent_id: i32,
        group_epoch: GroupEpoch,
        message_epoch: GroupEpoch,
        max_past_epochs: usize,
    ) -> bool {
        if message_epoch.as_u64() + max_past_epochs as u64 <= group_epoch.as_u64() {
            log::warn!(
                "[{}] own message epoch {} is {} or more less than group epoch {} for intent {}. Retrying message",
                inbox_id,
                message_epoch,
                max_past_epochs,
                group_epoch,
                intent_id
            );
            return false;
        } else if message_epoch.as_u64() > group_epoch.as_u64() {
            // Should not happen, logging proactively
            log::error!(
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

    #[allow(clippy::too_many_arguments)]
    #[tracing::instrument(level = "trace", skip_all)]
    async fn process_own_message<ApiClient: XmtpApi>(
        &self,
        client: &Client<ApiClient>,
        intent: StoredGroupIntent,
        openmls_group: &mut OpenMlsGroup,
        provider: &XmtpOpenMlsProvider,
        message: ProtocolMessage,
        envelope_timestamp_ns: u64,
        allow_epoch_increment: bool,
    ) -> Result<(), MessageProcessingError> {
        if intent.state == IntentState::Committed {
            return Ok(());
        }
        let message_epoch = message.epoch();
        let group_epoch = openmls_group.epoch();
        debug!(
            "[{}] processing own message for intent {} / {:?}, group epoch: {}, message_epoch: {}",
            self.context.inbox_id(),
            intent.id,
            intent.kind,
            group_epoch,
            message_epoch
        );

        let conn = provider.conn();
        match intent.kind {
            IntentKind::KeyUpdate
            | IntentKind::UpdateGroupMembership
            | IntentKind::UpdateAdminList
            | IntentKind::MetadataUpdate
            | IntentKind::UpdatePermission => {
                if !allow_epoch_increment {
                    return Err(MessageProcessingError::EpochIncrementNotAllowed);
                }
                let maybe_pending_commit = openmls_group.pending_commit();
                // We don't get errors with merge_pending_commit when there are no commits to merge
                if maybe_pending_commit.is_none() {
                    debug!(
                        "no pending commit to merge. Group epoch: {}. Message epoch: {}",
                        group_epoch, message_epoch
                    );
                    conn.set_group_intent_to_publish(intent.id)?;

                    // Return OK here, because an error will roll back the transaction
                    return Ok(());
                }
                log::info!(
                    "[{}] Validating commit for intent {}. Message timestamp: {}",
                    self.context.inbox_id(),
                    intent.id,
                    envelope_timestamp_ns
                );
                let maybe_validated_commit = ValidatedCommit::from_staged_commit(
                    client,
                    &conn,
                    maybe_pending_commit.expect("already checked"),
                    openmls_group,
                )
                .await;

                if maybe_validated_commit.is_err() {
                    log::warn!("error validating commit: {:?}", maybe_validated_commit);
                    match openmls_group.clear_pending_commit(provider.storage()) {
                        Ok(_) => (),
                        Err(err) => return Err(MessageProcessingError::ClearPendingCommit(err)),
                    }
                    conn.set_group_intent_error(intent.id)?;
                    // Return before merging commit since it does not pass validation
                    // An error will roll back clearing pending commit, so we return Ok here
                    return Ok(());
                }

                let validated_commit = maybe_validated_commit.unwrap();

                log::info!(
                    "[{}] merging pending commit for intent {}",
                    self.context.inbox_id(),
                    intent.id
                );
                if let Err(err) = openmls_group.merge_pending_commit(&provider) {
                    log::error!("error merging commit: {}", err);
                    match openmls_group.clear_pending_commit(provider.storage()) {
                        Ok(_) => (),
                        Err(err) => return Err(MessageProcessingError::ClearPendingCommit(err)),
                    }

                    conn.set_group_intent_to_publish(intent.id)?;
                } else {
                    // If no error committing the change, write a transcript message
                    self.save_transcript_message(&conn, validated_commit, envelope_timestamp_ns)?;
                }
            }
            IntentKind::SendMessage => {
                if !Self::is_valid_epoch(
                    self.context.inbox_id(),
                    intent.id,
                    group_epoch,
                    message_epoch,
                    3, // max_past_epochs, TODO: expose from OpenMLS MlsGroup
                ) {
                    conn.set_group_intent_to_publish(intent.id)?;
                    return Ok(());
                }
                if let Some(id) = intent.message_id()? {
                    conn.set_delivery_status_to_published(&id, envelope_timestamp_ns)?;
                }
            }
        };

        conn.set_group_intent_committed(intent.id)?;

        Ok(())
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn process_external_message<ApiClient: XmtpApi>(
        &self,
        client: &Client<ApiClient>,
        openmls_group: &mut OpenMlsGroup,
        provider: &XmtpOpenMlsProvider,
        message: PrivateMessageIn,
        envelope_timestamp_ns: u64,
        allow_epoch_increment: bool,
    ) -> Result<(), MessageProcessingError> {
        let decrypted_message = openmls_group.process_message(provider, message)?;
        let (sender_inbox_id, sender_installation_id) =
            extract_message_sender(openmls_group, &decrypted_message, envelope_timestamp_ns)?;
        log::info!(
            "[{}] extracted sender inbox id: {}",
            self.context.inbox_id(),
            sender_inbox_id
        );
        match decrypted_message.into_content() {
            ProcessedMessageContent::ApplicationMessage(application_message) => {
                log::info!("[{}] decoding application message", self.context.inbox_id());
                let message_bytes = application_message.into_bytes();

                let mut bytes = Bytes::from(message_bytes.clone());
                let envelope = PlaintextEnvelope::decode(&mut bytes)
                    .map_err(MessageProcessingError::DecodeError)?;

                match envelope.content {
                    Some(Content::V1(V1 {
                        idempotency_key,
                        content,
                    })) => {
                        let message_id =
                            calculate_message_id(&self.group_id, &content, &idempotency_key);
                        StoredGroupMessage {
                            id: message_id,
                            group_id: self.group_id.clone(),
                            decrypted_message_bytes: content,
                            sent_at_ns: envelope_timestamp_ns as i64,
                            kind: GroupMessageKind::Application,
                            sender_installation_id,
                            sender_inbox_id,
                            delivery_status: DeliveryStatus::Published,
                        }
                        .store(provider.conn_ref())?
                    }
                    Some(Content::V2(V2 {
                        idempotency_key,
                        message_type,
                    })) => match message_type {
                        Some(Request(MessageHistoryRequest {
                            request_id,
                            pin_code,
                        })) => {
                            // store the request message
                            let contents =
                                format!("{request_id}{DELIMITER}{pin_code}").into_bytes();
                            let message_id =
                                calculate_message_id(&self.group_id, &contents, &idempotency_key);
                            let message = StoredGroupMessage {
                                id: message_id,
                                group_id: self.group_id.clone(),
                                decrypted_message_bytes: contents,
                                sent_at_ns: envelope_timestamp_ns as i64,
                                kind: GroupMessageKind::Application,
                                sender_installation_id,
                                sender_inbox_id: sender_inbox_id.clone(),
                                delivery_status: DeliveryStatus::Published,
                            };
                            message.store(provider.conn_ref())?;

                            // ensure the requester is a member of all the groups
                            let _ = client
                                .ensure_member_of_all_groups(sender_inbox_id)
                                .await
                                .map_err(|e| MessageProcessingError::Group(Box::new(e)));

                            // prepare and send the reply
                            let Some(ref url) = client.history_sync_url else {
                                return Err(MessageProcessingError::Generic(
                                    "history sync url not set".to_string(),
                                ));
                            };
                            match client.prepare_history_reply(&request_id, url).await {
                                Ok(history_reply) => client
                                    .send_history_reply(history_reply.into())
                                    .await
                                    .map_err(|e| {
                                        MessageProcessingError::Generic(format!(
                                            "could not send history reply: {e}"
                                        ))
                                    })?,
                                Err(e) => {
                                    return Err(MessageProcessingError::Generic(format!(
                                        "error preparing history reply: {e}"
                                    )));
                                }
                            }
                        }
                        Some(Reply(MessageHistoryReply {
                            request_id: _,
                            url,
                            encryption_key,
                            signing_key,
                            bundle_hash,
                        })) => {
                            let Some(sign_key) = signing_key else {
                                return Err(MessageProcessingError::InvalidPayload);
                            };

                            let Some(enc_key) = encryption_key else {
                                return Err(MessageProcessingError::InvalidPayload);
                            };

                            // store the reply message
                            let contents = format!(
                                "{url}{DELIMITER}{:?}{DELIMITER}{:?}{DELIMITER}{:?}",
                                enc_key, sign_key, bundle_hash
                            )
                            .into_bytes();
                            let message_id =
                                calculate_message_id(&self.group_id, &contents, &idempotency_key);
                            StoredGroupMessage {
                                id: message_id,
                                group_id: self.group_id.clone(),
                                decrypted_message_bytes: contents,
                                sent_at_ns: envelope_timestamp_ns as i64,
                                kind: GroupMessageKind::Application,
                                sender_installation_id,
                                sender_inbox_id,
                                delivery_status: DeliveryStatus::Published,
                            }
                            .store(provider.conn_ref())?;

                            // handle the reply and fetch the history
                            let enc_file_path =
                                download_history_bundle(&url, bundle_hash, sign_key)
                                    .await
                                    .map_err(|e| MessageProcessingError::Generic(format!("{e}")))?;

                            let messages_path = std::env::temp_dir().join("messages.jsonl");

                            decrypt_history_file(&enc_file_path, &messages_path, enc_key)
                                .map_err(|e| MessageProcessingError::Generic(format!("{e}")))?;

                            client
                                .insert_history_bundle(&messages_path)
                                .map_err(|e| MessageProcessingError::Generic(format!("{e}")))?;

                            client
                                .sync_welcomes()
                                .await
                                .map_err(|e| MessageProcessingError::Generic(e.to_string()))?;

                            let conn = provider.conn_ref();
                            let groups = conn.find_groups(None, None, None, None)?;
                            for crate::storage::group::StoredGroup { id, .. } in groups.into_iter()
                            {
                                let group = client
                                    .group(id)
                                    .map_err(|e| MessageProcessingError::Generic(e.to_string()))?;
                                Box::pin(group.sync(client))
                                    .await
                                    .map_err(|e| MessageProcessingError::Generic(e.to_string()))?;
                            }
                        }
                        _ => {
                            return Err(MessageProcessingError::InvalidPayload);
                        }
                    },
                    None => return Err(MessageProcessingError::InvalidPayload),
                }
            }
            ProcessedMessageContent::ProposalMessage(_proposal_ptr) => {
                // intentionally left blank.
            }
            ProcessedMessageContent::ExternalJoinProposalMessage(_external_proposal_ptr) => {
                // intentionally left blank.
            }
            ProcessedMessageContent::StagedCommitMessage(staged_commit) => {
                if !allow_epoch_increment {
                    return Err(MessageProcessingError::EpochIncrementNotAllowed);
                }
                log::info!(
                    "[{}] received staged commit. Merging and clearing any pending commits",
                    self.context.inbox_id()
                );

                let sc = *staged_commit;

                // Validate the commit
                let validated_commit = ValidatedCommit::from_staged_commit(
                    client,
                    provider.conn_ref(),
                    &sc,
                    openmls_group,
                )
                .await?;
                log::info!(
                    "[{}] staged commit is valid, will attempt to merge",
                    self.context.inbox_id()
                );
                openmls_group.merge_staged_commit(provider, sc)?;
                self.save_transcript_message(
                    provider.conn_ref(),
                    validated_commit,
                    envelope_timestamp_ns,
                )?;
            }
        };

        Ok(())
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub(super) async fn process_message<ApiClient: XmtpApi>(
        &self,
        client: &Client<ApiClient>,
        openmls_group: &mut OpenMlsGroup,
        provider: &XmtpOpenMlsProvider,
        envelope: &GroupMessageV1,
        allow_epoch_increment: bool,
    ) -> Result<(), MessageProcessingError> {
        let mls_message_in = MlsMessageIn::tls_deserialize_exact(&envelope.data)?;

        let message = match mls_message_in.extract() {
            MlsMessageBodyIn::PrivateMessage(message) => Ok(message),
            other => Err(MessageProcessingError::UnsupportedMessageType(
                discriminant(&other),
            )),
        }?;

        let intent = provider
            .conn()
            .find_group_intent_by_payload_hash(sha256(envelope.data.as_slice()));

        match intent {
            // Intent with the payload hash matches
            Ok(Some(intent)) => {
                log::info!(
                    "client [{}] is about to process own envelope [{}]",
                    client.inbox_id(),
                    envelope.id
                );
                log::info!(
                    "envelope [{}] is equal to intent [{}]",
                    envelope.id,
                    intent.id
                );
                self.process_own_message(
                    client,
                    intent,
                    openmls_group,
                    provider,
                    message.into(),
                    envelope.created_ns,
                    allow_epoch_increment,
                )
                .await
            }
            // No matching intent found
            Ok(None) => {
                log::info!(
                    "client [{}] is about to process external envelope [{}]",
                    client.inbox_id(),
                    envelope.id
                );
                self.process_external_message(
                    client,
                    openmls_group,
                    provider,
                    message,
                    envelope.created_ns,
                    allow_epoch_increment,
                )
                .await
            }
            Err(err) => Err(MessageProcessingError::Storage(err)),
        }
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn consume_message<ApiClient>(
        &self,
        envelope: &GroupMessage,
        openmls_group: &mut OpenMlsGroup,
        client: &Client<ApiClient>,
    ) -> Result<(), MessageProcessingError>
    where
        ApiClient: XmtpApi,
    {
        let msgv1 = match &envelope.version {
            Some(GroupMessageVersion::V1(value)) => value,
            _ => return Err(MessageProcessingError::InvalidPayload),
        };

        client
            .process_for_id(
                &msgv1.group_id,
                EntityKind::Group,
                msgv1.id,
                |provider| async move {
                    self.process_message(client, openmls_group, &provider, msgv1, true)
                        .await?;
                    Ok(())
                },
            )
            .await?;
        Ok(())
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn process_messages<ApiClient>(
        &self,
        messages: Vec<GroupMessage>,
        conn: DbConnection,
        client: &Client<ApiClient>,
    ) -> Result<(), GroupError>
    where
        ApiClient: XmtpApi,
    {
        let provider = self.context.mls_provider(conn);
        let mut openmls_group = self.load_mls_group(&provider)?;

        let mut receive_errors = vec![];
        for message in messages.into_iter() {
            let result = retry_async!(
                Retry::default(),
                (async {
                    self.consume_message(&message, &mut openmls_group, client)
                        .await
                })
            );
            if let Err(e) = result {
                receive_errors.push(e);
            }
        }

        if receive_errors.is_empty() {
            Ok(())
        } else {
            log::error!("Message processing errors: {:?}", receive_errors);
            Err(GroupError::ReceiveErrors(receive_errors))
        }
    }

    #[tracing::instrument(level = "trace", skip(conn, client, self))]
    pub(super) async fn receive<ApiClient>(
        &self,
        conn: &DbConnection,
        client: &Client<ApiClient>,
    ) -> Result<(), GroupError>
    where
        ApiClient: XmtpApi,
    {
        let messages = client.query_group_messages(&self.group_id, conn).await?;
        self.process_messages(messages, conn.clone(), client)
            .await?;
        Ok(())
    }

    fn save_transcript_message(
        &self,
        conn: &DbConnection,
        validated_commit: ValidatedCommit,
        timestamp_ns: u64,
    ) -> Result<Option<StoredGroupMessage>, MessageProcessingError> {
        if validated_commit.is_empty() {
            return Ok(None);
        }

        log::info!(
            "{}: Storing a transcript message with {} members added and {} members removed and {} metadata changes",
            self.context.inbox_id(),
            validated_commit.added_inboxes.len(),
            validated_commit.removed_inboxes.len(),
            validated_commit.metadata_changes.metadata_field_changes.len(),
        );
        let sender_installation_id = validated_commit.actor_installation_id();
        let sender_inbox_id = validated_commit.actor_inbox_id();
        // TODO:nm replace with new membership change codec
        let payload: GroupUpdated = validated_commit.into();
        let encoded_payload = GroupUpdatedCodec::encode(payload)?;
        let mut encoded_payload_bytes = Vec::new();
        encoded_payload.encode(&mut encoded_payload_bytes)?;

        let group_id = self.group_id.as_slice();
        let message_id = calculate_message_id(
            group_id,
            encoded_payload_bytes.as_slice(),
            &timestamp_ns.to_string(),
        );

        let msg = StoredGroupMessage {
            id: message_id,
            group_id: group_id.to_vec(),
            decrypted_message_bytes: encoded_payload_bytes.to_vec(),
            sent_at_ns: timestamp_ns as i64,
            kind: GroupMessageKind::MembershipChange,
            sender_installation_id,
            sender_inbox_id,
            delivery_status: DeliveryStatus::Published,
        };

        msg.store(conn)?;
        Ok(Some(msg))
    }

    #[tracing::instrument(level = "trace", skip(conn, self, client))]
    pub(super) async fn publish_intents<ApiClient>(
        &self,
        conn: DbConnection,
        client: &Client<ApiClient>,
    ) -> Result<(), GroupError>
    where
        ApiClient: XmtpApi,
    {
        let provider = self.context.mls_provider(conn);
        let mut openmls_group = self.load_mls_group(&provider)?;

        let intents = provider.conn().find_group_intents(
            self.group_id.clone(),
            Some(vec![IntentState::ToPublish]),
            None,
        )?;

        for intent in intents {
            let result = retry_async!(
                Retry::default(),
                (async {
                    self.get_publish_intent_data(&provider, client, &mut openmls_group, &intent)
                        .await
                })
            );

            if let Err(err) = result {
                log::error!("error getting publish intent data {:?}", err);
                if (intent.publish_attempts + 1) as usize >= MAX_INTENT_PUBLISH_ATTEMPTS {
                    log::error!("intent {} has reached max publish attempts", intent.id);
                    // TODO: Eventually clean up errored attempts
                    provider
                        .conn()
                        .set_group_intent_error_and_fail_msg(&intent)?;
                } else {
                    provider
                        .conn()
                        .increment_intent_publish_attempt_count(intent.id)?;
                }

                return Err(err);
            }

            if let Some((payload, post_commit_data)) = result.expect("checked") {
                let payload_slice = payload.as_slice();

                client
                    .api_client
                    .send_group_messages(vec![payload_slice])
                    .await?;
                log::info!(
                    "[{}] published intent [{}] of type [{}]",
                    client.inbox_id(),
                    intent.id,
                    intent.kind
                );
                provider.conn().set_group_intent_published(
                    intent.id,
                    sha256(payload_slice),
                    post_commit_data,
                )?;
                log::debug!(
                    "client [{}] set stored intent [{}] to state `published`",
                    client.inbox_id(),
                    intent.id
                );
            } else {
                provider
                    .conn()
                    .set_group_intent_error_and_fail_msg(&intent)?;
            }
        }

        Ok(())
    }

    // Takes a StoredGroupIntent and returns the payload and post commit data as a tuple
    // A return value of [`Option::None`] means this intent would not change the group.
    #[allow(clippy::type_complexity)]
    #[tracing::instrument(level = "trace", skip_all)]
    async fn get_publish_intent_data<ApiClient>(
        &self,
        provider: &XmtpOpenMlsProvider,
        client: &Client<ApiClient>,
        openmls_group: &mut OpenMlsGroup,
        intent: &StoredGroupIntent,
    ) -> Result<Option<(Vec<u8>, Option<Vec<u8>>)>, GroupError>
    where
        ApiClient: XmtpApi,
    {
        match intent.kind {
            IntentKind::UpdateGroupMembership => {
                let intent_data = UpdateGroupMembershipIntentData::try_from(&intent.data)?;
                let signer = &self.context.identity.installation_keys;
                if let Some((commit, post_commit_action)) = apply_update_group_membership_intent(
                    client,
                    provider,
                    openmls_group,
                    intent_data,
                    signer,
                )
                .await?
                {
                    Ok(Some((
                        commit.tls_serialize_detached()?,
                        post_commit_action.map(|action| action.to_bytes()),
                    )))
                } else {
                    Ok(None)
                }
            }
            IntentKind::SendMessage => {
                // We can safely assume all SendMessage intents have data
                let intent_data = SendMessageIntentData::from_bytes(intent.data.as_slice())?;
                // TODO: Handle pending_proposal errors and UseAfterEviction errors
                let msg = openmls_group.create_message(
                    &provider,
                    &self.context.identity.installation_keys,
                    intent_data.message.as_slice(),
                )?;

                let msg_bytes = msg.tls_serialize_detached()?;
                Ok(Some((msg_bytes, None)))
            }
            IntentKind::KeyUpdate => {
                let (commit, _, _) = openmls_group
                    .self_update(&provider, &self.context.identity.installation_keys)?;

                Ok(Some((commit.tls_serialize_detached()?, None)))
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
                    &self.context.identity.installation_keys,
                )?;

                let commit_bytes = commit.tls_serialize_detached()?;

                Ok(Some((commit_bytes, None)))
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
                    &self.context.identity.installation_keys,
                )?;
                let commit_bytes = commit.tls_serialize_detached()?;
                Ok(Some((commit_bytes, None)))
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
                    &self.context.identity.installation_keys,
                )?;
                let commit_bytes = commit.tls_serialize_detached()?;
                Ok(Some((commit_bytes, None)))
            }
        }
    }

    #[tracing::instrument(level = "trace", skip(conn, client))]
    pub(crate) async fn post_commit<ApiClient>(
        &self,
        conn: &DbConnection,
        client: &Client<ApiClient>,
    ) -> Result<(), GroupError>
    where
        ApiClient: XmtpApi,
    {
        let intents = conn.find_group_intents(
            self.group_id.clone(),
            Some(vec![IntentState::Committed]),
            None,
        )?;

        for intent in intents {
            if intent.post_commit_data.is_some() {
                let post_commit_data = intent.post_commit_data.unwrap();
                let post_commit_action = PostCommitAction::from_bytes(post_commit_data.as_slice())?;
                match post_commit_action {
                    PostCommitAction::SendWelcomes(action) => {
                        self.send_welcomes(action, client).await?;
                    }
                }
            }
            let deleter: &dyn Delete<StoredGroupIntent, Key = i32> = conn;
            deleter.delete(intent.id)?;
        }

        Ok(())
    }

    pub(super) async fn maybe_update_installations<ApiClient>(
        &self,
        conn: DbConnection,
        update_interval: Option<i64>,
        client: &Client<ApiClient>,
    ) -> Result<(), GroupError>
    where
        ApiClient: XmtpApi,
    {
        // determine how long of an interval in time to use before updating list
        let interval = match update_interval {
            Some(val) => val,
            None => UPDATE_INSTALLATIONS_INTERVAL_NS,
        };

        let now = crate::utils::time::now_ns();
        let last = conn.get_installations_time_checked(self.group_id.clone())?;
        let elapsed = now - last;
        if elapsed > interval {
            let provider = self.context.mls_provider(conn.clone());
            self.add_missing_installations(&provider, client).await?;
            conn.update_installations_time_checked(self.group_id.clone())?;
        }

        Ok(())
    }

    /**
     * Checks each member of the group for `IdentityUpdates` after their current sequence_id. If updates
     * are found the method will construct an [`UpdateGroupMembershipIntentData`] and publish a change
     * to the [`GroupMembership`] that will add any missing installations.
     *
     * This is designed to handle cases where existing members have added a new installation to their inbox
     * and the group has not been updated to include it.
     */
    pub(super) async fn add_missing_installations<ApiClient>(
        &self,
        provider: &XmtpOpenMlsProvider,
        client: &Client<ApiClient>,
    ) -> Result<(), GroupError>
    where
        ApiClient: XmtpApi,
    {
        let intent_data = self
            .get_membership_update_intent(client, provider, vec![], vec![])
            .await?;

        // If there is nothing to do, stop here
        if intent_data.is_empty() {
            return Ok(());
        }

        debug!("Adding missing installations {:?}", intent_data);

        let conn = provider.conn();
        let intent = conn.insert_group_intent(NewGroupIntent::new(
            IntentKind::UpdateGroupMembership,
            self.group_id.clone(),
            intent_data.into(),
        ))?;

        self.sync_until_intent_resolved(conn, intent.id, client)
            .await
    }

    /**
     * get_membership_update_intent will query the network for any new [`IdentityUpdate`]s for any of the existing
     * group members
     *
     * Callers may also include a list of added or removed inboxes
     */
    #[tracing::instrument(level = "trace", skip_all)]
    pub(super) async fn get_membership_update_intent<ApiClient: XmtpApi>(
        &self,
        client: &Client<ApiClient>,
        provider: &XmtpOpenMlsProvider,
        inbox_ids_to_add: Vec<InboxId>,
        inbox_ids_to_remove: Vec<InboxId>,
    ) -> Result<UpdateGroupMembershipIntentData, GroupError> {
        let mls_group = self.load_mls_group(provider)?;
        let existing_group_membership = extract_group_membership(mls_group.extensions())?;

        // TODO:nm prevent querying for updates on members who are being removed
        let mut inbox_ids = existing_group_membership.inbox_ids();
        inbox_ids.extend(inbox_ids_to_add);
        let conn = provider.conn_ref();
        // Load any missing updates from the network
        load_identity_updates(&client.api_client, conn, inbox_ids.clone()).await?;

        let latest_sequence_id_map = conn.get_latest_sequence_id(&inbox_ids)?;

        // Get a list of all inbox IDs that have increased sequence_id for the group
        let changed_inbox_ids =
            inbox_ids
                .iter()
                .try_fold(HashMap::new(), |mut updates, inbox_id| {
                    match (
                        latest_sequence_id_map.get(inbox_id),
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
                        (Some(latest_sequence_id), _) => {
                            // This is the case for net new members to the group
                            updates.insert(inbox_id.to_string(), *latest_sequence_id as u64);
                        }
                        (_, _) => {
                            log::warn!(
                                "Could not find existing sequence ID for inbox {}",
                                inbox_id
                            );
                            return Err(GroupError::MissingSequenceId);
                        }
                    }

                    Ok(updates)
                })?;

        Ok(UpdateGroupMembershipIntentData::new(
            changed_inbox_ids,
            inbox_ids_to_remove,
        ))
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub(super) async fn send_welcomes<ApiClient>(
        &self,
        action: SendWelcomesAction,
        client: &Client<ApiClient>,
    ) -> Result<(), GroupError>
    where
        ApiClient: XmtpApi,
    {
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
                        log::debug!("total welcome message proto bytes={w}");
                        w
                    }
                })
                .unwrap_or(GRPC_DATA_LIMIT / usize::from(MAX_GROUP_SIZE));

        log::debug!("welcome chunk_size={chunk_size}");
        let mut futures = vec![];
        for welcomes in welcomes.chunks(chunk_size) {
            futures.push(client.api_client.send_welcome_messages(welcomes));
        }
        try_join_all(futures).await?;
        Ok(())
    }
}

// Extracts the message sender, but does not do any validation to ensure that the
// installation_id is actually part of the inbox.
fn extract_message_sender(
    openmls_group: &mut OpenMlsGroup,
    decrypted_message: &ProcessedMessage,
    message_created_ns: u64,
) -> Result<(InboxId, Vec<u8>), MessageProcessingError> {
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
    return Err(MessageProcessingError::InvalidSender {
        message_time_ns: message_created_ns,
        credential: basic_credential.identity().to_vec(),
    });
}

// Takes UpdateGroupMembershipIntentData and applies it to the openmls group
// returning the commit and post_commit_action
#[tracing::instrument(level = "trace", skip_all)]
async fn apply_update_group_membership_intent<ApiClient: XmtpApi>(
    client: &Client<ApiClient>,
    provider: &XmtpOpenMlsProvider,
    openmls_group: &mut OpenMlsGroup,
    intent_data: UpdateGroupMembershipIntentData,
    signer: &SignatureKeyPair,
) -> Result<Option<(MlsMessageOut, Option<PostCommitAction>)>, GroupError> {
    let extensions: Extensions = openmls_group.extensions().clone();

    let old_group_membership = extract_group_membership(&extensions)?;
    let new_group_membership = intent_data.apply_to_group_membership(&old_group_membership);

    // Diff the two membership hashmaps getting a list of inboxes that have been added, removed, or updated
    let membership_diff = old_group_membership.diff(&new_group_membership);

    // Construct a diff of the installations that have been added or removed.
    // This function goes to the network and fills in any missing Identity Updates
    let installation_diff = client
        .get_installation_diff(
            &provider.conn(),
            &old_group_membership,
            &new_group_membership,
            &membership_diff,
        )
        .await?;

    let mut new_installations: Vec<Installation> = vec![];
    let mut new_key_packages: Vec<KeyPackage> = vec![];

    if !installation_diff.added_installations.is_empty() {
        let my_installation_id = &client.installation_public_key();
        // Go to the network and load the key packages for any new installation
        let key_packages = client
            .get_key_packages_for_installation_ids(
                installation_diff
                    .added_installations
                    .into_iter()
                    .filter(|installation| my_installation_id.ne(installation))
                    .collect(),
            )
            .await?;

        for key_package in key_packages {
            // Add a proposal to add the member to the local proposal queue
            new_installations.push(Installation::from_verified_key_package(&key_package));
            new_key_packages.push(key_package.inner);
        }
    }

    let leaf_nodes_to_remove: Vec<LeafNodeIndex> =
        get_removed_leaf_nodes(openmls_group, &installation_diff.removed_installations);

    if leaf_nodes_to_remove.is_empty()
        && new_key_packages.is_empty()
        && membership_diff.updated_inboxes.is_empty()
    {
        return Ok(None);
    }

    // Update the extensions to have the new GroupMembership
    let mut new_extensions = extensions.clone();
    new_extensions.add_or_replace(build_group_membership_extension(&new_group_membership));

    // Commit to the pending proposals, which will clear the proposal queue
    let (commit, maybe_welcome_message, _) = openmls_group.update_group_membership(
        provider,
        signer,
        &new_key_packages,
        &leaf_nodes_to_remove,
        new_extensions,
    )?;

    let post_commit_action = match maybe_welcome_message {
        Some(welcome_message) => Some(PostCommitAction::from_welcome(
            welcome_message,
            new_installations,
        )?),
        None => None,
    };

    Ok(Some((commit, post_commit_action)))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::ClientBuilder;
    use futures::future;
    use std::sync::Arc;
    use xmtp_cryptography::utils::generate_local_wallet;

    #[tokio::test(flavor = "multi_thread")]
    async fn publish_intents_worst_case_scenario() {
        let wallet = generate_local_wallet();
        let amal = Arc::new(ClientBuilder::new_test_client(&wallet).await);
        let amal_group: Arc<MlsGroup> =
            Arc::new(amal.create_group(None, Default::default()).unwrap());

        amal_group.send_message_optimistic(b"1").unwrap();
        amal_group.send_message_optimistic(b"2").unwrap();
        amal_group.send_message_optimistic(b"3").unwrap();
        amal_group.send_message_optimistic(b"4").unwrap();
        amal_group.send_message_optimistic(b"5").unwrap();
        amal_group.send_message_optimistic(b"6").unwrap();

        let mut futures = vec![];
        let conn = amal.context().store.conn().unwrap();

        for _ in 0..10 {
            let client = amal.clone();
            let conn = conn.clone();
            let group = amal_group.clone();

            futures.push(async move {
                group.publish_intents(conn, &client).await.unwrap();
            });
        }
        future::join_all(futures).await;
    }
}
