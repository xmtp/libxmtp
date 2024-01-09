mod group_permissions;
mod intents;
mod members;
mod subscriptions;
pub mod validated_commit;
use crate::codecs::ContentCodec;
use intents::SendMessageIntentData;
use log::debug;
use openmls::{
    framing::ProtocolMessage,
    group::MergePendingCommitError,
    prelude::{
        CredentialWithKey, CryptoConfig, GroupId, LeafNodeIndex, MlsGroup as OpenMlsGroup,
        MlsGroupConfig, MlsMessageIn, MlsMessageInBody, PrivateMessageIn, ProcessedMessage,
        ProcessedMessageContent, Sender, Welcome as MlsWelcome, WireFormatPolicy,
    },
    prelude_test::KeyPackage,
};
use openmls_traits::OpenMlsProvider;
use prost::Message;
use std::mem::discriminant;
use thiserror::Error;
use tls_codec::{Deserialize, Serialize};
use xmtp_cryptography::signature::{is_valid_ed25519_public_key, is_valid_ethereum_address};
use xmtp_proto::{
    api_client::{Envelope, XmtpApiClient, XmtpMlsClient},
    xmtp::mls::message_contents::GroupMembershipChanges,
};

pub use self::intents::{AddressesOrInstallationIds, IntentError};
use self::{
    intents::{AddMembersIntentData, PostCommitAction, RemoveMembersIntentData},
    validated_commit::CommitValidationError,
};
use crate::{
    api_client_wrapper::WelcomeMessage,
    client::{ClientError, MessageProcessingError},
    codecs::membership_change::GroupMembershipChangeCodec,
    configuration::CIPHERSUITE,
    groups::validated_commit::ValidatedCommit,
    identity::Identity,
    retry,
    retry::{Retry, RetryableError},
    retry_async, retryable,
    storage::{
        db_connection::DbConnection,
        group::{GroupMembershipState, StoredGroup},
        group_intent::{IntentKind, IntentState, NewGroupIntent, StoredGroupIntent},
        group_message::{GroupMessageKind, StoredGroupMessage},
        StorageError,
    },
    utils::{hash::sha256, id::get_message_id, time::now_ns, topic::get_group_topic},
    xmtp_openmls_provider::XmtpOpenMlsProvider,
    Client, Delete, Store,
};

#[derive(Debug, Error)]
pub enum GroupError {
    #[error("group not found")]
    GroupNotFound,
    #[error("api error: {0}")]
    Api(#[from] xmtp_proto::api_client::Error),
    #[error("storage error: {0}")]
    Storage(#[from] crate::storage::StorageError),
    #[error("intent error: {0}")]
    Intent(#[from] IntentError),
    #[error("create message: {0}")]
    CreateMessage(#[from] openmls::prelude::CreateMessageError),
    #[error("tls serialization: {0}")]
    TlsSerialization(#[from] tls_codec::Error),
    #[error("add members: {0}")]
    AddMembers(#[from] openmls::prelude::AddMembersError<StorageError>),
    #[error("remove members: {0}")]
    RemoveMembers(#[from] openmls::prelude::RemoveMembersError<StorageError>),
    #[error("group create: {0}")]
    GroupCreate(#[from] openmls::prelude::NewGroupError<StorageError>),
    #[error("self update: {0}")]
    SelfUpdate(#[from] openmls::group::SelfUpdateError<StorageError>),
    #[error("welcome error: {0}")]
    WelcomeError(#[from] openmls::prelude::WelcomeError<StorageError>),
    #[error("client: {0}")]
    Client(#[from] ClientError),
    #[error("receive error: {0}")]
    ReceiveError(#[from] MessageProcessingError),
    #[error("Receive errors: {0:?}")]
    ReceiveErrors(Vec<MessageProcessingError>),
    #[error("generic: {0}")]
    Generic(String),
    #[error("diesel error {0}")]
    Diesel(#[from] diesel::result::Error),
    #[error("The address {0:?} is not a valid ethereum address")]
    InvalidAddresses(Vec<String>),
    #[error("Public Keys {0:?} are not valid ed25519 public keys")]
    InvalidPublicKeys(Vec<Vec<u8>>),
    #[error("Commit validation error {0}")]
    CommitValidation(#[from] CommitValidationError),
}

impl RetryableError for GroupError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::ReceiveError(msg) => retryable!(msg),
            Self::AddMembers(members) => retryable!(members),
            Self::RemoveMembers(members) => retryable!(members),
            Self::GroupCreate(group) => retryable!(group),
            Self::SelfUpdate(update) => retryable!(update),
            Self::WelcomeError(welcome) => retryable!(welcome),
            _ => false,
        }
    }
}

pub struct MlsGroup<'c, ApiClient> {
    pub group_id: Vec<u8>,
    pub created_at_ns: i64,
    client: &'c Client<ApiClient>,
}

impl<'c, ApiClient> MlsGroup<'c, ApiClient>
where
    ApiClient: XmtpApiClient + XmtpMlsClient,
{
    // Creates a new group instance. Does not validate that the group exists in the DB
    pub fn new(client: &'c Client<ApiClient>, group_id: Vec<u8>, created_at_ns: i64) -> Self {
        Self {
            client,
            group_id,
            created_at_ns,
        }
    }

    pub fn load_mls_group(
        &self,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<OpenMlsGroup, GroupError> {
        let mls_group =
            OpenMlsGroup::load(&GroupId::from_slice(&self.group_id), provider.key_store())
                .ok_or(GroupError::GroupNotFound)?;

        Ok(mls_group)
    }

    pub fn create_and_insert(
        client: &'c Client<ApiClient>,
        membership_state: GroupMembershipState,
    ) -> Result<Self, GroupError> {
        let conn = client.store.conn()?;
        let provider = XmtpOpenMlsProvider::new(&conn);
        let mut mls_group = OpenMlsGroup::new(
            &provider,
            &client.identity.installation_keys,
            &build_group_config(),
            CredentialWithKey {
                credential: client.identity.credential.clone(),
                signature_key: client.identity.installation_keys.to_public_vec().into(),
            },
        )?;

        mls_group.save(provider.key_store())?;
        let group_id = mls_group.group_id().to_vec();
        let stored_group = StoredGroup::new(group_id.clone(), now_ns(), membership_state);
        stored_group.store(provider.conn())?;
        Ok(Self::new(client, group_id, stored_group.created_at_ns))
    }

    pub fn create_from_welcome(
        client: &'c Client<ApiClient>,
        provider: &XmtpOpenMlsProvider,
        welcome: MlsWelcome,
    ) -> Result<Self, GroupError> {
        let mut mls_group =
            OpenMlsGroup::new_from_welcome(provider, &build_group_config(), welcome, None)?;
        mls_group.save(provider.key_store())?;

        let group_id = mls_group.group_id().to_vec();
        let stored_group =
            StoredGroup::new(group_id.clone(), now_ns(), GroupMembershipState::Pending);
        stored_group.store(provider.conn())?;

        Ok(Self::new(client, group_id, stored_group.created_at_ns))
    }

    pub fn find_messages(
        &self,
        kind: Option<GroupMessageKind>,
        sent_before_ns: Option<i64>,
        sent_after_ns: Option<i64>,
        limit: Option<i64>,
    ) -> Result<Vec<StoredGroupMessage>, GroupError> {
        let conn = self.client.store.conn()?;
        let messages =
            conn.get_group_messages(&self.group_id, sent_after_ns, sent_before_ns, kind, limit)?;

        Ok(messages)
    }

    fn validate_message_sender(
        &self,
        openmls_group: &mut OpenMlsGroup,
        decrypted_message: &ProcessedMessage,
        envelope_timestamp_ns: u64,
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
                message_time_ns: envelope_timestamp_ns,
                credential: decrypted_message.credential().identity().to_vec(),
            });
        }
        Ok((
            sender_account_address.unwrap(),
            sender_installation_id.unwrap(),
        ))
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

                    return Err(MessageProcessingError::NoPendingCommit {
                        message_epoch,
                        group_epoch,
                    });
                }
                let maybe_validated_commit = ValidatedCommit::from_staged_commit(
                    maybe_pending_commit.expect("already checked"),
                    openmls_group,
                )?;

                debug!("[{}] merging pending commit", self.client.account_address());
                if let Err(MergePendingCommitError::MlsGroupStateError(err)) =
                    openmls_group.merge_pending_commit(provider)
                {
                    debug!("error merging commit: {}", err);
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

    fn process_private_message(
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
            self.validate_message_sender(openmls_group, &decrypted_message, envelope_timestamp_ns)?;

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

    fn process_message(
        &self,
        openmls_group: &mut OpenMlsGroup,
        provider: &XmtpOpenMlsProvider,
        envelope: &Envelope,
        allow_epoch_increment: bool,
    ) -> Result<(), MessageProcessingError> {
        let mls_message_in = MlsMessageIn::tls_deserialize_exact(&envelope.message)?;

        let message = match mls_message_in.extract() {
            MlsMessageInBody::PrivateMessage(message) => Ok(message),
            other => Err(MessageProcessingError::UnsupportedMessageType(
                discriminant(&other),
            )),
        }?;

        let intent = provider
            .conn()
            .find_group_intent_by_payload_hash(sha256(envelope.message.as_slice()));
        match intent {
            // Intent with the payload hash matches
            Ok(Some(intent)) => self.process_own_message(
                intent,
                openmls_group,
                provider,
                message.into(),
                envelope.timestamp_ns,
                allow_epoch_increment,
            ),
            Err(err) => Err(MessageProcessingError::Storage(err)),
            // No matching intent found
            Ok(None) => self.process_private_message(
                openmls_group,
                provider,
                message,
                envelope.timestamp_ns,
                allow_epoch_increment,
            ),
        }
    }

    fn consume_message(
        &self,
        envelope: &Envelope,
        openmls_group: &mut OpenMlsGroup,
    ) -> Result<(), MessageProcessingError> {
        self.client.process_for_topic(
            &self.topic(),
            envelope.timestamp_ns,
            |provider| -> Result<(), MessageProcessingError> {
                self.process_message(openmls_group, &provider, envelope, true)?;
                openmls_group.save(provider.key_store())?;
                Ok(())
            },
        )?;
        Ok(())
    }

    pub fn process_messages(&self, envelopes: Vec<Envelope>) -> Result<(), GroupError> {
        let conn = &self.client.store.conn()?;
        let provider = self.client.mls_provider(conn);
        let mut openmls_group = self.load_mls_group(&provider)?;

        let receive_errors: Vec<MessageProcessingError> = envelopes
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

    pub async fn receive(&self) -> Result<(), GroupError> {
        let envelopes = self.client.pull_from_topic(&self.topic()).await?;

        self.process_messages(envelopes)?;

        Ok(())
    }

    pub async fn send_message(&self, message: &[u8]) -> Result<(), GroupError> {
        let conn = &mut self.client.store.conn()?;
        let intent_data: Vec<u8> = SendMessageIntentData::new(message.to_vec()).into();
        let intent =
            NewGroupIntent::new(IntentKind::SendMessage, self.group_id.clone(), intent_data);
        intent.store(conn)?;

        // Skipping a full sync here and instead just firing and forgetting
        self.publish_intents(conn).await?;
        Ok(())
    }

    fn validate_evm_addresses(account_addresses: &[String]) -> Result<(), GroupError> {
        let mut invalid = account_addresses
            .iter()
            .filter(|a| !is_valid_ethereum_address(a))
            .peekable();

        if invalid.peek().is_some() {
            return Err(GroupError::InvalidAddresses(
                invalid.map(ToString::to_string).collect::<Vec<_>>(),
            ));
        }

        Ok(())
    }

    fn validate_ed25519_keys(keys: &[Vec<u8>]) -> Result<(), GroupError> {
        let mut invalid = keys
            .iter()
            .filter(|a| !is_valid_ed25519_public_key(a))
            .peekable();

        if invalid.peek().is_some() {
            return Err(GroupError::InvalidPublicKeys(
                invalid.map(Clone::clone).collect::<Vec<_>>(),
            ));
        }

        Ok(())
    }

    pub async fn add_members(&self, account_addresses: Vec<String>) -> Result<(), GroupError> {
        Self::validate_evm_addresses(&account_addresses)?;
        let conn = &mut self.client.store.conn()?;
        let intent_data: Vec<u8> =
            AddMembersIntentData::new(account_addresses.into()).try_into()?;
        let intent =
            NewGroupIntent::new(IntentKind::AddMembers, self.group_id.clone(), intent_data);
        intent.store(conn)?;

        self.sync_with_conn(conn).await
    }

    pub async fn add_members_by_installation_id(
        &self,
        installation_ids: Vec<Vec<u8>>,
    ) -> Result<(), GroupError> {
        Self::validate_ed25519_keys(&installation_ids)?;
        let conn = &mut self.client.store.conn()?;
        let intent_data: Vec<u8> = AddMembersIntentData::new(installation_ids.into()).try_into()?;
        let intent =
            NewGroupIntent::new(IntentKind::AddMembers, self.group_id.clone(), intent_data);
        intent.store(conn)?;

        self.sync_with_conn(conn).await
    }

    pub async fn remove_members(&self, account_addresses: Vec<String>) -> Result<(), GroupError> {
        Self::validate_evm_addresses(&account_addresses)?;
        let conn = &mut self.client.store.conn()?;
        let intent_data: Vec<u8> = RemoveMembersIntentData::new(account_addresses.into()).into();
        let intent = NewGroupIntent::new(
            IntentKind::RemoveMembers,
            self.group_id.clone(),
            intent_data,
        );
        intent.store(conn)?;

        self.sync_with_conn(conn).await
    }

    #[allow(dead_code)]
    pub(crate) async fn remove_members_by_installation_id(
        &self,
        installation_ids: Vec<Vec<u8>>,
    ) -> Result<(), GroupError> {
        Self::validate_ed25519_keys(&installation_ids)?;
        let conn = &mut self.client.store.conn()?;
        let intent_data: Vec<u8> = RemoveMembersIntentData::new(installation_ids.into()).into();
        let intent = NewGroupIntent::new(
            IntentKind::RemoveMembers,
            self.group_id.clone(),
            intent_data,
        );
        intent.store(conn)?;

        self.sync_with_conn(conn).await
    }

    pub async fn key_update(&self) -> Result<(), GroupError> {
        let conn = &mut self.client.store.conn()?;
        let intent = NewGroupIntent::new(IntentKind::KeyUpdate, self.group_id.clone(), vec![]);
        intent.store(conn)?;

        self.sync_with_conn(conn).await
    }

    pub async fn sync(&self) -> Result<(), GroupError> {
        let conn = &mut self.client.store.conn()?;
        self.sync_with_conn(conn).await
    }

    async fn sync_with_conn<'a>(&self, conn: &'a DbConnection<'a>) -> Result<(), GroupError> {
        self.publish_intents(conn).await?;
        if let Err(e) = self.receive().await {
            log::warn!("receive error {:?}", e);
        }
        self.post_commit(conn).await?;

        Ok(())
    }

    pub(crate) async fn publish_intents<'a>(
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

        for intent in intents {
            let result = retry_async!(
                Retry::default(),
                (async {
                    self.get_publish_intent_data(&provider, &mut openmls_group, &intent)
                        .await
                })
            );
            if let Err(e) = result {
                log::error!("error getting publish intent data {:?}", e);
                // TODO: Figure out which types of errors we should abort completely on and which
                // ones are safe to continue with
                continue;
            }

            let (payload, post_commit_data) = result.expect("result already checked");
            let payload_slice = payload.as_slice();

            self.client
                .api_client
                .publish_to_group(vec![payload_slice])
                .await?;

            provider.conn().set_group_intent_published(
                intent.id,
                sha256(payload_slice),
                post_commit_data,
            )?;
        }
        openmls_group.save(provider.key_store())?;

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

                // If somehow another installation has made it into the commit, this will be missing
                // their installation ID
                let installation_ids: Vec<Vec<u8>> =
                    key_packages.iter().map(|kp| kp.installation_id()).collect();

                let post_commit_data =
                    Some(PostCommitAction::from_welcome(welcome, installation_ids)?.to_bytes());

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
                        let welcomes: Vec<WelcomeMessage> = action
                            .installation_ids
                            .into_iter()
                            .map(|installation_id| WelcomeMessage {
                                installation_id,
                                ciphertext: action.welcome_message.clone(),
                            })
                            .collect();
                        debug!("Sending {} welcomes", welcomes.len());
                        self.client.api_client.publish_welcomes(welcomes).await?;
                    }
                }
            }
            let deleter: &dyn Delete<StoredGroupIntent, Key = i32> = conn;
            deleter.delete(intent.id)?;
        }

        Ok(())
    }

    fn topic(&self) -> String {
        get_group_topic(&self.group_id)
    }
}

fn build_group_config() -> MlsGroupConfig {
    MlsGroupConfig::builder()
        .crypto_config(CryptoConfig::with_default_version(CIPHERSUITE))
        .wire_format_policy(WireFormatPolicy::default())
        .max_past_epochs(3) // Trying with 3 max past epochs for now
        .use_ratchet_tree_extension(true)
        .build()
}

#[cfg(test)]
mod tests {
    use openmls::prelude::Member;
    use xmtp_cryptography::utils::generate_local_wallet;

    use crate::{
        builder::ClientBuilder, storage::group_intent::IntentState, utils::topic::get_welcome_topic,
    };

    #[tokio::test]
    async fn test_send_message() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(wallet.into()).await;
        let group = client.create_group().expect("create group");
        group.send_message(b"hello").await.expect("send message");

        let topic = group.topic();

        let messages = client
            .api_client
            .read_topic(topic.as_str(), 0)
            .await
            .expect("read topic");

        assert_eq!(messages.len(), 1)
    }

    #[tokio::test]
    async fn test_receive_self_message() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(wallet.into()).await;
        let group = client.create_group().expect("create group");
        let msg = b"hello";
        group.send_message(msg).await.expect("send message");

        group.receive().await.unwrap();
        // Check for messages
        let messages = group.find_messages(None, None, None, None).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages.first().unwrap().decrypted_message_bytes, msg);
    }

    // Amal and Bola will both try and add Charlie from the same epoch.
    // The group should resolve to a consistent state
    #[tokio::test]
    async fn test_add_member_conflict() {
        let amal = ClientBuilder::new_test_client(generate_local_wallet().into()).await;
        let bola = ClientBuilder::new_test_client(generate_local_wallet().into()).await;
        let charlie = ClientBuilder::new_test_client(generate_local_wallet().into()).await;
        futures::future::join_all(vec![
            amal.register_identity(),
            bola.register_identity(),
            charlie.register_identity(),
        ])
        .await;

        let amal_group = amal.create_group().unwrap();
        // Add bola
        amal_group
            .add_members_by_installation_id(vec![bola.installation_public_key()])
            .await
            .unwrap();

        // Get bola's version of the same group
        let bola_groups = bola.sync_welcomes().await.unwrap();
        let bola_group = bola_groups.first().unwrap();

        // Have amal and bola both invite charlie.
        amal_group
            .add_members_by_installation_id(vec![charlie.installation_public_key()])
            .await
            .expect("failed to add charlie");
        bola_group
            .add_members_by_installation_id(vec![charlie.installation_public_key()])
            .await
            .unwrap();

        amal_group.receive().await.expect_err("expected error");
        bola_group.receive().await.expect_err("expected error");

        // Check Amal's MLS group state.
        let amal_db = amal.store.conn().unwrap();
        let amal_mls_group = amal_group
            .load_mls_group(&amal.mls_provider(&amal_db))
            .unwrap();
        let amal_members: Vec<Member> = amal_mls_group.members().collect();
        assert_eq!(amal_members.len(), 3);

        // Check Bola's MLS group state.
        let bola_db = bola.store.conn().unwrap();
        let bola_mls_group = bola_group
            .load_mls_group(&bola.mls_provider(&bola_db))
            .unwrap();
        let bola_members: Vec<Member> = bola_mls_group.members().collect();
        assert_eq!(bola_members.len(), 3);

        let amal_uncommitted_intents = amal_db
            .find_group_intents(
                amal_group.group_id.clone(),
                Some(vec![IntentState::ToPublish, IntentState::Published]),
                None,
            )
            .unwrap();
        assert_eq!(amal_uncommitted_intents.len(), 0);

        let bola_uncommitted_intents = bola_db
            .find_group_intents(
                bola_group.group_id.clone(),
                Some(vec![IntentState::ToPublish, IntentState::Published]),
                None,
            )
            .unwrap();
        // Bola should have one uncommitted intent for the failed attempt at adding Charlie, who is already in the group
        assert_eq!(bola_uncommitted_intents.len(), 1);
    }

    #[tokio::test]
    async fn test_add_members() {
        let client = ClientBuilder::new_test_client(generate_local_wallet().into()).await;
        let client_2 = ClientBuilder::new_test_client(generate_local_wallet().into()).await;
        client_2.register_identity().await.unwrap();
        let group = client.create_group().expect("create group");

        group
            .add_members_by_installation_id(vec![client_2.installation_public_key()])
            .await
            .unwrap();

        let topic = group.topic();

        let messages = client
            .api_client
            .read_topic(topic.as_str(), 0)
            .await
            .unwrap();

        assert_eq!(messages.len(), 1);
    }

    #[tokio::test]
    async fn test_add_invalid_member() {
        let client = ClientBuilder::new_test_client(generate_local_wallet().into()).await;
        let group = client.create_group().expect("create group");

        let result = group
            .add_members_by_installation_id(vec![b"1234".to_vec()])
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_remove_member() {
        let client_1 = ClientBuilder::new_test_client(generate_local_wallet().into()).await;
        // Add another client onto the network
        let client_2 = ClientBuilder::new_test_client(generate_local_wallet().into()).await;
        client_2.register_identity().await.unwrap();

        let group = client_1.create_group().expect("create group");
        group
            .add_members_by_installation_id(vec![client_2.installation_public_key()])
            .await
            .expect("group create failure");

        let messages_with_add = group.find_messages(None, None, None, None).unwrap();
        assert_eq!(messages_with_add.len(), 1);

        // Try and add another member without merging the pending commit
        group
            .remove_members_by_installation_id(vec![client_2.installation_public_key()])
            .await
            .expect("group create failure");

        let messages_with_remove = group.find_messages(None, None, None, None).unwrap();
        assert_eq!(messages_with_remove.len(), 2);

        // We are expecting 1 message on the group topic, not 2, because the second one should have
        // failed
        let topic = group.topic();
        let messages = client_1
            .api_client
            .read_topic(topic.as_str(), 0)
            .await
            .expect("read topic");

        assert_eq!(messages.len(), 2);
    }

    #[tokio::test]
    async fn test_key_update() {
        let client = ClientBuilder::new_test_client(generate_local_wallet().into()).await;
        let bola_client = ClientBuilder::new_test_client(generate_local_wallet().into()).await;
        bola_client.register_identity().await.unwrap();

        let group = client.create_group().expect("create group");
        group
            .add_members(vec![bola_client.account_address()])
            .await
            .unwrap();

        group.key_update().await.unwrap();

        let messages = client
            .api_client
            .read_topic(group.topic().as_str(), 0)
            .await
            .unwrap();
        assert_eq!(messages.len(), 2);

        let conn = &client.store.conn().unwrap();
        let provider = super::XmtpOpenMlsProvider::new(conn);
        let mls_group = group.load_mls_group(&provider).unwrap();
        let pending_commit = mls_group.pending_commit();
        assert!(pending_commit.is_none());

        group.send_message(b"hello").await.expect("send message");

        bola_client.sync_welcomes().await.unwrap();
        let bola_groups = bola_client.find_groups(None, None, None, None).unwrap();
        let bola_group = bola_groups.first().unwrap();
        bola_group.sync().await.unwrap();
        let bola_messages = bola_group.find_messages(None, None, None, None).unwrap();
        assert_eq!(bola_messages.len(), 1);
    }

    #[tokio::test]
    async fn test_post_commit() {
        let client = ClientBuilder::new_test_client(generate_local_wallet().into()).await;
        let client_2 = ClientBuilder::new_test_client(generate_local_wallet().into()).await;
        client_2.register_identity().await.unwrap();
        let group = client.create_group().expect("create group");

        group
            .add_members_by_installation_id(vec![client_2.installation_public_key()])
            .await
            .unwrap();

        // Check if the welcome was actually sent
        let welcome_topic = get_welcome_topic(&client_2.identity.installation_keys.to_public_vec());
        let welcome_messages = client
            .api_client
            .read_topic(welcome_topic.as_str(), 0)
            .await
            .unwrap();

        assert_eq!(welcome_messages.len(), 1);
    }

    #[tokio::test]
    async fn test_remove_by_account_address() {
        let amal = ClientBuilder::new_test_client(generate_local_wallet().into()).await;
        amal.register_identity().await.unwrap();
        let bola = ClientBuilder::new_test_client(generate_local_wallet().into()).await;
        bola.register_identity().await.unwrap();
        let charlie = ClientBuilder::new_test_client(generate_local_wallet().into()).await;
        charlie.register_identity().await.unwrap();

        let group = amal.create_group().unwrap();
        group
            .add_members(vec![bola.account_address(), charlie.account_address()])
            .await
            .unwrap();
        assert_eq!(group.members().unwrap().len(), 3);

        group
            .remove_members(vec![bola.account_address()])
            .await
            .unwrap();
        assert_eq!(group.members().unwrap().len(), 2);
    }
}
