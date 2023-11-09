mod intents;

use intents::SendMessageIntentData;
use openmls::{
    prelude::{
        CredentialWithKey, CryptoConfig, GroupId, LeafNodeIndex, MlsGroup as OpenMlsGroup,
        MlsGroupConfig, MlsMessageIn, MlsMessageInBody, ProcessedMessageContent, Sender,
        WireFormatPolicy,
    },
    prelude_test::KeyPackage,
};
use openmls_traits::OpenMlsProvider;
use thiserror::Error;
use tls_codec::{Deserialize, Serialize};
use xmtp_proto::api_client::{XmtpApiClient, XmtpMlsClient};

use self::intents::{AddMembersIntentData, IntentError, PostCommitAction, RemoveMembersIntentData};
use crate::{
    client::ClientError,
    configuration::CIPHERSUITE,
    identity::Identity,
    storage::{
        group::{GroupMembershipState, StoredGroup},
        group_intent::{IntentKind, IntentState, NewGroupIntent, StoredGroupIntent},
        group_message::{GroupMessageKind, StoredGroupMessage},
        DbConnection, StorageError,
    },
    utils::{hash::sha256, time::now_ns, topic::get_group_topic},
    xmtp_openmls_provider::XmtpOpenMlsProvider,
    Client, Store,
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
    #[error("client: {0}")]
    Client(#[from] ClientError),
    #[error("receive errors: {0:?}")]
    ReceiveError(Vec<GroupReceiveError>),
    #[error("generic: {0}")]
    Generic(String),
}

#[derive(Debug, Error)]
pub enum GroupReceiveError {
    #[error("[{message_time_ns:?}] invalid sender with credential: {credential:?}")]
    InvalidSender {
        message_time_ns: u64,
        credential: Vec<u8>,
    },
}

pub struct MlsGroup<'c, ApiClient> {
    pub group_id: Vec<u8>,
    client: &'c Client<ApiClient>,
}

impl<'c, ApiClient> MlsGroup<'c, ApiClient>
where
    ApiClient: XmtpApiClient + XmtpMlsClient,
{
    // Creates a new group instance. Does not validate that the group exists in the DB
    pub fn new(group_id: Vec<u8>, client: &'c Client<ApiClient>) -> Self {
        Self { client, group_id }
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
        let provider = client.mls_provider();
        let mut conn = client.store.conn()?;
        let mut mls_group = OpenMlsGroup::new(
            &provider,
            &client.identity.installation_keys,
            &build_group_config(),
            // TODO: Confirm I should be using the installation keys here
            CredentialWithKey {
                credential: client.identity.credential.clone(),
                signature_key: client.identity.installation_keys.to_public_vec().into(),
            },
        )?;

        mls_group.save(provider.key_store())?;
        let group_id = mls_group.group_id().to_vec();
        let stored_group = StoredGroup::new(group_id.clone(), now_ns() as i64, membership_state);
        stored_group.store(&mut conn)?;

        Ok(Self::new(group_id, client))
    }

    pub async fn receive(&self) -> Result<(), GroupError> {
        let topic = get_group_topic(&self.group_id);
        let envelopes = self
            .client
            .api_client
            .read_topic(
                &topic, 0, // TODO: query from last query point
            )
            .await?;

        let provider = self.client.mls_provider();
        let mut openmls_group = self.load_mls_group(&provider)?;
        let mut receive_errors = Vec::<GroupReceiveError>::new();
        for envelope in envelopes {
            let mls_message_in = MlsMessageIn::tls_deserialize_exact(&envelope.message)
                .expect("Could not deserialize message.");

            match mls_message_in.extract() {
                MlsMessageInBody::PrivateMessage(message) => {
                    let decrypted_message = openmls_group
                        .process_message(&provider, message)
                        .expect("Could not parse message.");
                    let mut sender_account_address = None;
                    let mut sender_installation_id = None;
                    if let Sender::Member(leaf_node_index) = decrypted_message.sender() {
                        if let Some(member) = openmls_group.member_at(*leaf_node_index) {
                            if member.credential == *decrypted_message.credential() {
                                sender_account_address = Identity::get_validated_account_address(
                                    &member.signature_key,
                                    member.credential.identity(),
                                )
                                .ok();
                                sender_installation_id = Some(member.signature_key);
                            }
                        }
                    }

                    if sender_account_address.is_none() {
                        receive_errors.push(GroupReceiveError::InvalidSender {
                            message_time_ns: envelope.timestamp_ns,
                            credential: decrypted_message.credential().identity().to_vec(),
                        });
                        continue;
                    }

                    match decrypted_message.into_content() {
                        ProcessedMessageContent::ApplicationMessage(application_message) => {
                            let message_bytes = application_message.into_bytes();
                            let mut id_vec = Vec::new();
                            id_vec.extend_from_slice(&self.group_id);
                            id_vec.extend_from_slice(&envelope.timestamp_ns.to_be_bytes());
                            id_vec.extend_from_slice(&message_bytes);
                            let message_id = sha256(&id_vec);
                            let message = StoredGroupMessage {
                                id: message_id,
                                group_id: self.group_id.clone(),
                                decrypted_message_bytes: message_bytes,
                                sent_at_ns: envelope.timestamp_ns as i64, // TODO validate that this timestamp is controlled by server
                                kind: GroupMessageKind::Application,
                                sender_installation_id: sender_installation_id.unwrap(),
                                sender_wallet_address: sender_account_address.unwrap(),
                            };
                            message.store(&mut self.client.store.conn()?)?;
                        }
                        ProcessedMessageContent::ProposalMessage(_proposal_ptr) => {
                            // intentionally left blank.
                        }
                        ProcessedMessageContent::ExternalJoinProposalMessage(
                            _external_proposal_ptr,
                        ) => {
                            // intentionally left blank.
                        }
                        ProcessedMessageContent::StagedCommitMessage(_commit_ptr) => {
                            // intentionally left blank.
                        }
                    }
                }
                _ => panic!("Unsupported message type"),
            }
        }

        if receive_errors.is_empty() {
            Ok(())
        } else {
            Err(GroupError::ReceiveError(receive_errors))
        }
    }

    pub async fn send_message(&self, message: &[u8]) -> Result<(), GroupError> {
        let mut conn = self.client.store.conn()?;
        let intent_data: Vec<u8> = SendMessageIntentData::new(message.to_vec()).into();
        let intent =
            NewGroupIntent::new(IntentKind::SendMessage, self.group_id.clone(), intent_data);
        intent.store(&mut conn)?;

        self.publish_intents(&mut conn).await?;
        Ok(())
    }

    pub async fn add_members_by_installation_id(
        &self,
        installation_ids: Vec<Vec<u8>>,
    ) -> Result<(), GroupError> {
        let mut conn = self.client.store.conn()?;
        let key_packages = self
            .client
            .get_key_packages_for_installation_ids(installation_ids)
            .await?;
        let intent_data: Vec<u8> = AddMembersIntentData::new(key_packages).try_into()?;
        let intent =
            NewGroupIntent::new(IntentKind::AddMembers, self.group_id.clone(), intent_data);
        intent.store(&mut conn)?;

        self.publish_intents(&mut conn).await?;

        Ok(())
    }

    pub async fn remove_members_by_installation_id(
        &self,
        installation_ids: Vec<Vec<u8>>,
    ) -> Result<(), GroupError> {
        let mut conn = self.client.store.conn()?;
        let intent_data: Vec<u8> = RemoveMembersIntentData::new(installation_ids).into();
        let intent = NewGroupIntent::new(
            IntentKind::RemoveMembers,
            self.group_id.clone(),
            intent_data,
        );
        intent.store(&mut conn)?;

        self.publish_intents(&mut conn).await?;

        Ok(())
    }

    pub async fn key_update(&self) -> Result<(), GroupError> {
        let mut conn = self.client.store.conn()?;
        let intent = NewGroupIntent::new(IntentKind::KeyUpdate, self.group_id.clone(), vec![]);
        intent.store(&mut conn)?;

        self.publish_intents(&mut conn).await?;

        Ok(())
    }

    pub(crate) async fn publish_intents(&self, conn: &mut DbConnection) -> Result<(), GroupError> {
        let provider = self.client.mls_provider();
        let mut openmls_group = self.load_mls_group(&provider)?;

        let intents = self.client.store.find_group_intents(
            conn,
            self.group_id.clone(),
            Some(vec![IntentState::ToPublish]),
            None,
        )?;

        for intent in intents {
            // TODO: Wrap in a transaction once we can synchronize with the MLS Keystore
            let result = self.get_publish_intent_data(&provider, &mut openmls_group, &intent);
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

            self.client.store.set_group_intent_published(
                conn,
                intent.id,
                sha256(payload_slice),
                post_commit_data,
            )?;
        }

        openmls_group.save(provider.key_store())?;

        Ok(())
    }

    // Takes a StoredGroupIntent and returns the payload and post commit data as a tuple
    fn get_publish_intent_data(
        &self,
        provider: &XmtpOpenMlsProvider,
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
                let intent_data =
                    AddMembersIntentData::from_bytes(intent.data.as_slice(), provider)?;

                let key_packages: Vec<KeyPackage> = intent_data
                    .key_packages
                    .iter()
                    .map(|kp| kp.inner.clone())
                    .collect();

                let (commit, welcome, _group_info) = openmls_group.add_members(
                    provider,
                    &self.client.identity.installation_keys,
                    key_packages.as_slice(),
                )?;

                let commit_bytes = commit.tls_serialize_detached()?;

                // If somehow another installation has made it into the commit, this will be missing
                // their installation ID
                let installation_ids: Vec<Vec<u8>> = intent_data
                    .key_packages
                    .iter()
                    .map(|kp| kp.installation_id())
                    .collect();

                let post_commit_data =
                    Some(PostCommitAction::from_welcome(welcome, installation_ids)?.to_bytes());

                Ok((commit_bytes, post_commit_data))
            }
            IntentKind::RemoveMembers => {
                let intent_data = RemoveMembersIntentData::from_bytes(intent.data.as_slice())?;
                let leaf_nodes: Vec<LeafNodeIndex> = openmls_group
                    .members()
                    .filter(|member| intent_data.installation_ids.contains(&member.signature_key))
                    .map(|member| member.index)
                    .collect();

                let num_leaf_nodes = leaf_nodes.len();

                if num_leaf_nodes != intent_data.installation_ids.len() {
                    return Err(GroupError::Generic(format!(
                        "expected {} leaf nodes, found {}",
                        intent_data.installation_ids.len(),
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

    pub fn topic(&self) -> String {
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
    use openmls_traits::OpenMlsProvider;
    use xmtp_cryptography::utils::generate_local_wallet;

    use crate::builder::ClientBuilder;

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
    async fn test_add_members() {
        let client = ClientBuilder::new_test_client(generate_local_wallet().into()).await;
        let client_2 = ClientBuilder::new_test_client(generate_local_wallet().into()).await;
        client_2.register_identity().await.unwrap();
        let group = client.create_group().expect("create group");

        group
            .add_members_by_installation_id(vec![client_2
                .identity
                .installation_keys
                .to_public_vec()])
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

        let provider = client_1.mls_provider();
        let group = client_1.create_group().expect("create group");
        group
            .add_members_by_installation_id(vec![client_2
                .identity
                .installation_keys
                .to_public_vec()])
            .await
            .expect("group create failure");

        // Try and add another member without merging the pending commit
        group
            .remove_members_by_installation_id(vec![client_2
                .identity
                .installation_keys
                .to_public_vec()])
            .await
            .expect("group create failure");

        // We are expecting 1 message on the group topic, not 2, because the second one should have
        // failed
        let topic = group.topic();
        let messages = client_1
            .api_client
            .read_topic(topic.as_str(), 0)
            .await
            .expect("read topic");

        assert_eq!(messages.len(), 1);
        // Now merge the commit and try again
        let mut mls_group = group.load_mls_group(&provider).unwrap();
        mls_group.merge_pending_commit(&provider).unwrap();
        mls_group.save(provider.key_store()).unwrap();

        group
            .publish_intents(&mut client_1.store.conn().unwrap())
            .await
            .unwrap();

        let messages_after_second_try = client_1
            .api_client
            .read_topic(topic.as_str(), 0)
            .await
            .expect("read topic");

        assert_eq!(messages_after_second_try.len(), 2)
    }

    #[tokio::test]
    async fn test_key_update() {
        let client = ClientBuilder::new_test_client(generate_local_wallet().into()).await;
        let group = client.create_group().expect("create group");

        group.key_update().await.unwrap();

        let messages = client
            .api_client
            .read_topic(group.topic().as_str(), 0)
            .await
            .unwrap();
        assert_eq!(messages.len(), 1);

        let mls_group = group.load_mls_group(&client.mls_provider()).unwrap();
        let pending_commit = mls_group.pending_commit();
        assert!(pending_commit.is_some());
    }
}
