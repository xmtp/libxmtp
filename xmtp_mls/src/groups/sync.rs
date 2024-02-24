use std::{collections::HashMap, mem::discriminant};

use log::debug;
use openmls::{
    framing::ProtocolMessage,
    group::MergePendingCommitError,
    prelude::{
        LeafNodeIndex, MlsGroup as OpenMlsGroup, MlsMessageIn, MlsMessageInBody, PrivateMessageIn,
        ProcessedMessage, ProcessedMessageContent, Sender,
    },
    prelude_test::KeyPackage,
};
use openmls_traits::OpenMlsProvider;
use prost::Message;
use tls_codec::{Deserialize, Serialize};

use xmtp_proto::{
    api_client::XmtpMlsClient,
    xmtp::mls::api::v1::{
        group_message::{Version as GroupMessageVersion, V1 as GroupMessageV1},
        welcome_message_input::{
            Version as WelcomeMessageInputVersion, V1 as WelcomeMessageInputV1,
        },
        GroupMessage, WelcomeMessageInput,
    },
    xmtp::mls::message_contents::GroupMembershipChanges,
};

use super::{
    intents::{
        AddMembersIntentData, AddressesOrInstallationIds, Installation, PostCommitAction,
        RemoveMembersIntentData, SendMessageIntentData, SendWelcomesAction,
    },
    members::GroupMember,
    GroupError, MlsGroup,
};
use crate::{
    api_client_wrapper::IdentityUpdate,
    client::MessageProcessingError,
    codecs::{membership_change::GroupMembershipChangeCodec, ContentCodec},
    configuration::{MAX_INTENT_PUBLISH_ATTEMPTS, UPDATE_INSTALLATION_LIST_INTERVAL_NS},
    groups::validated_commit::ValidatedCommit,
    hpke::{encrypt_welcome, HpkeError},
    identity::Identity,
    retry,
    retry::Retry,
    retry_async,
    storage::{
        db_connection::DbConnection,
        group_intent::{IntentKind, IntentState, StoredGroupIntent, ID},
        group_message::{GroupMessageKind, StoredGroupMessage},
        refresh_state::EntityKind,
        StorageError,
    },
    utils::{hash::sha256, id::get_message_id},
    xmtp_openmls_provider::XmtpOpenMlsProvider,
    Delete, Fetch, Store,
};

impl<'c, ApiClient> MlsGroup<'c, ApiClient>
where
    ApiClient: XmtpMlsClient,
{
    pub async fn sync(&self) -> Result<(), GroupError> {
        let conn = &mut self.client.store.conn()?;

        self.maybe_update_installation_list(conn, None).await?;

        self.sync_with_conn(conn).await
    }

    pub(super) async fn sync_with_conn<'a>(
        &self,
        conn: &'a DbConnection<'a>,
    ) -> Result<(), GroupError> {
        let mut errors: Vec<GroupError> = vec![];
        log::info!("Sync1");

        // Even if publish fails, continue to receiving
        if let Err(publish_error) = self.publish_intents(conn).await {
            log::error!("error publishing intents {:?}", publish_error);
            errors.push(publish_error);
        }

        log::info!("Sync2");

        // Even if receiving fails, continue to post_commit
        if let Err(receive_error) = self.receive(conn).await {
            log::error!("receive error {:?}", receive_error);
            // We don't return an error if receive fails, because it's possible this is caused
            // by malicious data sent over the network, or messages from before the user was
            // added to the group
        }

        log::info!("Sync3");

        if let Err(post_commit_err) = self.post_commit(conn).await {
            log::error!("post commit error {:?}", post_commit_err);
            errors.push(post_commit_err);
        }

        log::info!("Sync4");

        // Return a combination of publish and post_commit errors
        if !errors.is_empty() {
            return Err(GroupError::Sync(errors));
        }

        log::info!("Sync5");

        Ok(())
    }

    /**
     * Sync the group and wait for the intent to be deleted
     * Group syncing may involve picking up messages unrelated to the intent, so simply checking for errors
     * does not give a clear signal as to whether the intent was successfully completed or not.
     *
     * This method will retry up to `crate::configuration::MAX_GROUP_SYNC_RETRIES` times.
     */
    pub(super) async fn sync_until_intent_resolved<'a>(
        &self,
        conn: &'a DbConnection<'a>,
        intent_id: ID,
    ) -> Result<(), GroupError> {
        let mut num_attempts = 0;
        // Return the last error to the caller if we fail to sync
        let mut last_err: Option<GroupError> = None;
        while num_attempts < crate::configuration::MAX_GROUP_SYNC_RETRIES {
            if let Err(err) = self.sync_with_conn(conn).await {
                log::error!("error syncing group {:?}", err);
                last_err = Some(err);
            }

            // This will return early if the fetch fails
            let intent: Result<Option<StoredGroupIntent>, StorageError> = conn.fetch(&intent_id);
            match intent {
                Ok(None) => {
                    // This is expected. The intent gets deleted on success
                    return Ok(());
                }
                Ok(Some(intent)) => {
                    log::warn!(
                        "retrying intent ID {}. intent currently in state {:?}",
                        intent.id,
                        intent.state
                    );
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

    fn process_own_message(
        &self,
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
        debug!(
            "[{}] processing own message for intent {} / {:?}",
            self.client.account_address(),
            intent.id,
            intent.kind
        );

        let conn = provider.conn();
        match intent.kind {
            IntentKind::AddMembers | IntentKind::RemoveMembers | IntentKind::KeyUpdate => {
                if !allow_epoch_increment {
                    return Err(MessageProcessingError::EpochIncrementNotAllowed);
                }
                let maybe_pending_commit = openmls_group.pending_commit();
                // We don't get errors with merge_pending_commit when there are no commits to merge
                if maybe_pending_commit.is_none() {
                    let message_epoch = message.epoch();
                    let group_epoch = openmls_group.epoch();
                    debug!(
                        "no pending commit to merge. Group epoch: {}. Message epoch: {}",
                        group_epoch, message_epoch
                    );
                    conn.set_group_intent_to_publish(intent.id)?;

                    // Return OK here, because an error will roll back the transaction
                    return Ok(());
                }
                let maybe_validated_commit = ValidatedCommit::from_staged_commit(
                    maybe_pending_commit.expect("already checked"),
                    openmls_group,
                )?;

                debug!("[{}] merging pending commit", self.client.account_address());
                if let Err(MergePendingCommitError::MlsGroupStateError(err)) =
                    openmls_group.merge_pending_commit(provider)
                {
                    log::error!("error merging commit: {}", err);
                    openmls_group.clear_pending_commit();
                    conn.set_group_intent_to_publish(intent.id)?;
                } else {
                    // If no error committing the change, write a transcript message
                    self.save_transcript_message(
                        conn,
                        maybe_validated_commit,
                        envelope_timestamp_ns,
                    )?;
                }
                // TOOD: Handle writing transcript messages for adding/removing members
            }
            IntentKind::SendMessage => {
                let intent_data = SendMessageIntentData::from_bytes(intent.data.as_slice())?;
                let group_id = openmls_group.group_id().as_slice();
                let decrypted_message_data = intent_data.message.as_slice();
                StoredGroupMessage {
                    id: get_message_id(decrypted_message_data, group_id, envelope_timestamp_ns),
                    group_id: group_id.to_vec(),
                    decrypted_message_bytes: intent_data.message,
                    sent_at_ns: envelope_timestamp_ns as i64,
                    kind: GroupMessageKind::Application,
                    sender_installation_id: self.client.installation_public_key(),
                    sender_account_address: self.client.account_address(),
                }
                .store(conn)?;
            }
        };

        conn.set_group_intent_committed(intent.id)?;

        Ok(())
    }

    fn process_external_message(
        &self,
        openmls_group: &mut OpenMlsGroup,
        provider: &XmtpOpenMlsProvider,
        message: PrivateMessageIn,
        envelope_timestamp_ns: u64,
        allow_epoch_increment: bool,
    ) -> Result<(), MessageProcessingError> {
        debug!(
            "[{}] processing private message",
            self.client.account_address()
        );
        let decrypted_message = openmls_group.process_message(provider, message)?;
        let (sender_account_address, sender_installation_id) =
            validate_message_sender(openmls_group, &decrypted_message, envelope_timestamp_ns)?;

        match decrypted_message.into_content() {
            ProcessedMessageContent::ApplicationMessage(application_message) => {
                let message_bytes = application_message.into_bytes();
                let message_id =
                    get_message_id(&message_bytes, &self.group_id, envelope_timestamp_ns);
                StoredGroupMessage {
                    id: message_id,
                    group_id: self.group_id.clone(),
                    decrypted_message_bytes: message_bytes,
                    sent_at_ns: envelope_timestamp_ns as i64,
                    kind: GroupMessageKind::Application,
                    sender_installation_id,
                    sender_account_address,
                }
                .store(provider.conn())?;
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
                debug!(
                    "[{}] received staged commit. Merging and clearing any pending commits",
                    self.client.account_address()
                );

                let sc = *staged_commit;
                // Validate the commit
                let validated_commit = ValidatedCommit::from_staged_commit(&sc, openmls_group)?;
                openmls_group.merge_staged_commit(provider, sc)?;
                self.save_transcript_message(
                    provider.conn(),
                    validated_commit,
                    envelope_timestamp_ns,
                )?;
            }
        };

        Ok(())
    }

    pub(super) fn process_message(
        &self,
        openmls_group: &mut OpenMlsGroup,
        provider: &XmtpOpenMlsProvider,
        envelope: &GroupMessageV1,
        allow_epoch_increment: bool,
    ) -> Result<(), MessageProcessingError> {
        let mls_message_in = MlsMessageIn::tls_deserialize_exact(&envelope.data)?;

        let message = match mls_message_in.extract() {
            MlsMessageInBody::PrivateMessage(message) => Ok(message),
            other => Err(MessageProcessingError::UnsupportedMessageType(
                discriminant(&other),
            )),
        }?;

        let intent = provider
            .conn()
            .find_group_intent_by_payload_hash(sha256(envelope.data.as_slice()));
        match intent {
            // Intent with the payload hash matches
            Ok(Some(intent)) => self.process_own_message(
                intent,
                openmls_group,
                provider,
                message.into(),
                envelope.created_ns,
                allow_epoch_increment,
            ),
            Err(err) => Err(MessageProcessingError::Storage(err)),
            // No matching intent found
            Ok(None) => self.process_external_message(
                openmls_group,
                provider,
                message,
                envelope.created_ns,
                allow_epoch_increment,
            ),
        }
    }

    fn consume_message(
        &self,
        envelope: &GroupMessage,
        openmls_group: &mut OpenMlsGroup,
    ) -> Result<(), MessageProcessingError> {
        let msgv1 = match &envelope.version {
            Some(GroupMessageVersion::V1(value)) => value,
            _ => return Err(MessageProcessingError::InvalidPayload),
        };

        self.client.process_for_id(
            &msgv1.group_id,
            EntityKind::Group,
            msgv1.id,
            |provider| -> Result<(), MessageProcessingError> {
                self.process_message(openmls_group, &provider, msgv1, true)?;
                openmls_group.save(provider.key_store())?;
                Ok(())
            },
        )?;
        Ok(())
    }

    pub fn process_messages<'a>(
        &self,
        messages: Vec<GroupMessage>,
        conn: &'a DbConnection<'a>,
    ) -> Result<(), GroupError> {
        let provider = self.client.mls_provider(conn);
        let mut openmls_group = self.load_mls_group(&provider)?;

        let receive_errors: Vec<MessageProcessingError> = messages
            .into_iter()
            .map(|envelope| -> Result<(), MessageProcessingError> {
                retry!(
                    Retry::default(),
                    (|| self.consume_message(&envelope, &mut openmls_group))
                )
            })
            .filter_map(Result::err)
            .collect();

        if receive_errors.is_empty() {
            Ok(())
        } else {
            debug!("Message processing errors: {:?}", receive_errors);
            Err(GroupError::ReceiveErrors(receive_errors))
        }
    }

    pub(super) async fn receive<'a>(&self, conn: &'a DbConnection<'a>) -> Result<(), GroupError> {
        let messages = self
            .client
            .query_group_messages(&self.group_id, conn)
            .await?;

        self.process_messages(messages, conn)?;

        Ok(())
    }

    fn save_transcript_message(
        &self,
        conn: &DbConnection,
        maybe_validated_commit: Option<ValidatedCommit>,
        timestamp_ns: u64,
    ) -> Result<Option<StoredGroupMessage>, MessageProcessingError> {
        let mut transcript_message = None;
        if let Some(validated_commit) = maybe_validated_commit {
            // If there are no members added or removed, don't write a transcript message
            if validated_commit.members_added.is_empty()
                && validated_commit.members_removed.is_empty()
            {
                return Ok(None);
            }
            log::info!(
                "Storing a transcript message with {} members added and {} members removed for address {}",
                validated_commit.members_added.len(),
                validated_commit.members_removed.len(),
                self.client.account_address()
            );
            let sender_installation_id = validated_commit.actor_installation_id();
            let sender_account_address = validated_commit.actor_account_address();
            let payload: GroupMembershipChanges = validated_commit.into();
            let encoded_payload = GroupMembershipChangeCodec::encode(payload)?;
            let mut encoded_payload_bytes = Vec::new();
            encoded_payload.encode(&mut encoded_payload_bytes)?;
            let group_id = self.group_id.as_slice();
            let message_id =
                get_message_id(encoded_payload_bytes.as_slice(), group_id, timestamp_ns);
            let msg = StoredGroupMessage {
                id: message_id,
                group_id: group_id.to_vec(),
                decrypted_message_bytes: encoded_payload_bytes.to_vec(),
                sent_at_ns: timestamp_ns as i64,
                kind: GroupMessageKind::MembershipChange,
                sender_installation_id,
                sender_account_address,
            };

            msg.store(conn)?;
            transcript_message = Some(msg);
        }

        Ok(transcript_message)
    }

    pub(super) async fn publish_intents<'a>(
        &self,
        conn: &'a DbConnection<'a>,
    ) -> Result<(), GroupError> {
        let provider = self.client.mls_provider(conn);
        let mut openmls_group = self.load_mls_group(&provider)?;

        let intents = provider.conn().find_group_intents(
            self.group_id.clone(),
            Some(vec![IntentState::ToPublish]),
            None,
        )?;
        let num_intents = intents.len();

        for intent in intents {
            let result = retry_async!(
                Retry::default(),
                (async {
                    self.get_publish_intent_data(&provider, &mut openmls_group, &intent)
                        .await
                })
            );

            if let Err(err) = result {
                log::error!("error getting publish intent data {:?}", err);
                if (intent.publish_attempts + 1) as usize >= MAX_INTENT_PUBLISH_ATTEMPTS {
                    log::error!("intent {} has reached max publish attempts", intent.id);
                    // TODO: Eventually clean up errored attempts
                    conn.set_group_intent_error(intent.id)?;
                } else {
                    conn.increment_intent_publish_attempt_count(intent.id)?;
                }

                return Err(err);
            }

            let (payload, post_commit_data) = result.expect("already checked");
            let payload_slice = payload.as_slice();

            self.client
                .api_client
                .send_group_messages(vec![payload_slice])
                .await?;

            provider.conn().set_group_intent_published(
                intent.id,
                sha256(payload_slice),
                post_commit_data,
            )?;
        }

        if num_intents > 0 {
            openmls_group.save(provider.key_store())?;
        }

        Ok(())
    }

    // Takes a StoredGroupIntent and returns the payload and post commit data as a tuple
    async fn get_publish_intent_data(
        &self,
        provider: &XmtpOpenMlsProvider<'_>,
        openmls_group: &mut OpenMlsGroup,
        intent: &StoredGroupIntent,
    ) -> Result<(Vec<u8>, Option<Vec<u8>>), GroupError> {
        match intent.kind {
            IntentKind::SendMessage => {
                // We can safely assume all SendMessage intents have data
                let intent_data = SendMessageIntentData::from_bytes(intent.data.as_slice())?;
                // TODO: Handle pending_proposal errors and UseAfterEviction errors
                let msg = openmls_group.create_message(
                    provider,
                    &self.client.identity.installation_keys,
                    intent_data.message.as_slice(),
                )?;

                let msg_bytes = msg.tls_serialize_detached()?;
                Ok((msg_bytes, None))
            }
            IntentKind::AddMembers => {
                let intent_data = AddMembersIntentData::from_bytes(intent.data.as_slice())?;

                let key_packages = self
                    .client
                    .get_key_packages(intent_data.address_or_id)
                    .await?;

                let mls_key_packages: Vec<KeyPackage> =
                    key_packages.iter().map(|kp| kp.inner.clone()).collect();

                let (commit, welcome, _group_info) = openmls_group.add_members(
                    provider,
                    &self.client.identity.installation_keys,
                    mls_key_packages.as_slice(),
                )?;

                if let Some(staged_commit) = openmls_group.pending_commit() {
                    // Validate the commit, even if it's from yourself
                    ValidatedCommit::from_staged_commit(staged_commit, openmls_group)?;
                }

                let commit_bytes = commit.tls_serialize_detached()?;

                let installations = key_packages
                    .iter()
                    .map(Installation::from_verified_key_package)
                    .collect();

                let post_commit_data =
                    Some(PostCommitAction::from_welcome(welcome, installations)?.to_bytes());

                Ok((commit_bytes, post_commit_data))
            }
            IntentKind::RemoveMembers => {
                let intent_data = RemoveMembersIntentData::from_bytes(intent.data.as_slice())?;

                let installation_ids = {
                    match intent_data.address_or_id {
                        AddressesOrInstallationIds::AccountAddresses(addrs) => {
                            self.client.get_all_active_installation_ids(addrs).await?
                        }
                        AddressesOrInstallationIds::InstallationIds(ids) => ids,
                    }
                };

                let leaf_nodes: Vec<LeafNodeIndex> = openmls_group
                    .members()
                    .filter(|member| installation_ids.contains(&member.signature_key))
                    .map(|member| member.index)
                    .collect();

                let num_leaf_nodes = leaf_nodes.len();

                if num_leaf_nodes != installation_ids.len() {
                    return Err(GroupError::Generic(format!(
                        "expected {} leaf nodes, found {}",
                        installation_ids.len(),
                        num_leaf_nodes
                    )));
                }

                // The second return value is a Welcome, which is only possible if there
                // are pending proposals. Ignoring for now
                let (commit, _, _) = openmls_group.remove_members(
                    provider,
                    &self.client.identity.installation_keys,
                    leaf_nodes.as_slice(),
                )?;

                if let Some(staged_commit) = openmls_group.pending_commit() {
                    // Validate the commit, even if it's from yourself
                    ValidatedCommit::from_staged_commit(staged_commit, openmls_group)?;
                }

                let commit_bytes = commit.tls_serialize_detached()?;

                Ok((commit_bytes, None))
            }
            IntentKind::KeyUpdate => {
                let (commit, _, _) =
                    openmls_group.self_update(provider, &self.client.identity.installation_keys)?;

                Ok((commit.tls_serialize_detached()?, None))
            }
        }
    }

    pub(crate) async fn post_commit(&self, conn: &DbConnection<'_>) -> Result<(), GroupError> {
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
                        self.send_welcomes(action).await?;
                    }
                }
            }
            let deleter: &dyn Delete<StoredGroupIntent, Key = i32> = conn;
            deleter.delete(intent.id)?;
        }

        Ok(())
    }

    pub(super) async fn maybe_update_installation_list<'a>(
        &self,
        conn: &'a DbConnection<'a>,
        update_interval: Option<i64>,
    ) -> Result<(), GroupError> {
        // determine how long of an interval in time to use before updating list
        let interval = match update_interval {
            Some(val) => val,
            None => UPDATE_INSTALLATION_LIST_INTERVAL_NS,
        };

        let now = crate::utils::time::now_ns();
        let last = conn.get_installation_list_time_checked(self.group_id.clone())?;
        let elapsed = now - last;
        if elapsed > interval {
            let provider = self.client.mls_provider(conn);
            self.add_missing_installations(provider).await?;
            conn.update_installation_list_time_checked(self.group_id.clone())?;
        }

        Ok(())
    }

    pub(super) async fn get_missing_members(
        &self,
        provider: &XmtpOpenMlsProvider<'_>,
    ) -> Result<(Vec<Vec<u8>>, Vec<Vec<u8>>), GroupError> {
        let current_members = self.members_with_provider(provider)?;
        let account_addresses = current_members
            .iter()
            .map(|m| m.account_address.clone())
            .collect();

        let current_member_map: HashMap<String, GroupMember> = current_members
            .into_iter()
            .map(|m| (m.account_address.clone(), m))
            .collect();

        let change_list = self
            .client
            .api_client
            // TODO: Get a real start time from the database
            .get_identity_updates(0, account_addresses)
            .await?;

        let to_add: Vec<Vec<u8>> = change_list
            .into_iter()
            .filter_map(|(account_address, updates)| {
                let member_changes: Vec<Vec<u8>> = updates
                    .into_iter()
                    .filter_map(|change| match change {
                        IdentityUpdate::NewInstallation(new_member) => {
                            let current_member = current_member_map.get(&account_address);
                            current_member?;
                            if current_member
                                .expect("already checked")
                                .installation_ids
                                .contains(&new_member.installation_key)
                            {
                                return None;
                            }

                            Some(new_member.installation_key)
                        }
                        IdentityUpdate::RevokeInstallation(_) => {
                            log::warn!("Revocation found. Not handled");

                            None
                        }
                        IdentityUpdate::Invalid => {
                            log::warn!("Invalid identity update found");

                            None
                        }
                    })
                    .collect();

                if !member_changes.is_empty() {
                    return Some(member_changes);
                }
                None
            })
            .flatten()
            .collect();

        Ok((to_add, vec![]))
    }

    pub(super) async fn add_missing_installations(
        &self,
        provider: XmtpOpenMlsProvider<'_>,
    ) -> Result<(), GroupError> {
        let (missing_members, _) = self.get_missing_members(&provider).await?;
        if missing_members.is_empty() {
            return Ok(());
        }
        self.add_members_by_installation_id(missing_members).await?;

        Ok(())
    }

    async fn send_welcomes(&self, action: SendWelcomesAction) -> Result<(), GroupError> {
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

        self.client
            .api_client
            .send_welcome_messages(welcomes)
            .await?;

        Ok(())
    }
}

fn validate_message_sender(
    openmls_group: &mut OpenMlsGroup,
    decrypted_message: &ProcessedMessage,
    message_created_ns: u64,
) -> Result<(String, Vec<u8>), MessageProcessingError> {
    let mut sender_account_address = None;
    let mut sender_installation_id = None;
    if let Sender::Member(leaf_node_index) = decrypted_message.sender() {
        if let Some(member) = openmls_group.member_at(*leaf_node_index) {
            if member.credential.eq(decrypted_message.credential()) {
                sender_account_address = Identity::get_validated_account_address(
                    member.credential.identity(),
                    &member.signature_key,
                )
                .ok();
                sender_installation_id = Some(member.signature_key);
            }
        }
    }

    if sender_account_address.is_none() {
        return Err(MessageProcessingError::InvalidSender {
            message_time_ns: message_created_ns,
            credential: decrypted_message.credential().identity().to_vec(),
        });
    }
    Ok((
        sender_account_address.unwrap(),
        sender_installation_id.unwrap(),
    ))
}
