mod group_metadata;
mod group_permissions;
mod intents;
mod members;
mod subscriptions;
mod sync;
pub mod validated_commit;

use openmls::{
    extensions::{Extension, Extensions, Metadata},
    group::{MlsGroupCreateConfig, MlsGroupJoinConfig},
    prelude::{
        CredentialWithKey, CryptoConfig, GroupId, MlsGroup as OpenMlsGroup, Welcome as MlsWelcome,
        WireFormatPolicy,
    },
};
use openmls_traits::OpenMlsProvider;
use thiserror::Error;

use intents::SendMessageIntentData;

use crate::{
    client::{deserialize_welcome, ClientError, MessageProcessingError},
    configuration::CIPHERSUITE,
    hpke::{decrypt_welcome, HpkeError},
    identity::{Identity, IdentityError},
    retry::RetryableError,
    retryable,
    storage::{
        group::{GroupMembershipState, StoredGroup},
        group_intent::{IntentKind, NewGroupIntent},
        group_message::{GroupMessageKind, StoredGroupMessage},
        StorageError,
    },
    utils::{
        address::{sanitize_evm_addresses, AddressValidationError},
        time::now_ns,
    },
    xmtp_openmls_provider::XmtpOpenMlsProvider,
    Client, Store,
};

use xmtp_cryptography::signature::is_valid_ed25519_public_key;
use xmtp_proto::{
    api_client::XmtpMlsClient,
    xmtp::mls::api::v1::{
        group_message::{Version as GroupMessageVersion, V1 as GroupMessageV1},
        GroupMessage,
    },
};

pub use self::intents::{AddressesOrInstallationIds, IntentError};

use self::{
    group_metadata::{ConversationType, GroupMetadata, GroupMetadataError},
    group_permissions::{default_group_policy, PolicySet},
    intents::{AddMembersIntentData, RemoveMembersIntentData},
    validated_commit::CommitValidationError,
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
    #[error("Invalid extension {0}")]
    InvalidExtension(#[from] openmls::prelude::InvalidExtensionError),
    #[error("Invalid signature: {0}")]
    Signature(#[from] openmls::prelude::SignatureError),
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
    AddressValidation(#[from] AddressValidationError),
    #[error("Public Keys {0:?} are not valid ed25519 public keys")]
    InvalidPublicKeys(Vec<Vec<u8>>),
    #[error("Commit validation error {0}")]
    CommitValidation(#[from] CommitValidationError),
    #[error("Metadata error {0}")]
    GroupMetadata(#[from] GroupMetadataError),
    #[error("Errors occurred during sync {0:?}")]
    Sync(Vec<GroupError>),
    #[error("Hpke error: {0}")]
    Hpke(#[from] HpkeError),
    #[error("identity error: {0}")]
    Identity(#[from] IdentityError),
}

impl RetryableError for GroupError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Diesel(diesel) => retryable!(diesel),
            Self::Storage(storage) => retryable!(storage),
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
    ApiClient: XmtpMlsClient,
{
    // Creates a new group instance. Does not validate that the group exists in the DB
    pub fn new(client: &'c Client<ApiClient>, group_id: Vec<u8>, created_at_ns: i64) -> Self {
        Self {
            client,
            group_id,
            created_at_ns,
        }
    }

    // Load the stored MLS group from the OpenMLS provider's keystore
    fn load_mls_group(&self, provider: &XmtpOpenMlsProvider) -> Result<OpenMlsGroup, GroupError> {
        let mls_group =
            OpenMlsGroup::load(&GroupId::from_slice(&self.group_id), provider.key_store())
                .ok_or(GroupError::GroupNotFound)?;

        Ok(mls_group)
    }

    // Create a new group and save it to the DB
    pub fn create_and_insert(
        client: &'c Client<ApiClient>,
        membership_state: GroupMembershipState,
    ) -> Result<Self, GroupError> {
        let conn = client.store.conn()?;
        let provider = XmtpOpenMlsProvider::new(&conn);
        let protected_metadata =
            build_protected_metadata_extension(&client.identity, default_group_policy())?;
        let group_config = build_group_config(protected_metadata)?;

        let mut mls_group = OpenMlsGroup::new(
            &provider,
            &client.identity.installation_keys,
            &group_config,
            CredentialWithKey {
                credential: client.identity.credential()?,
                signature_key: client.identity.installation_keys.to_public_vec().into(),
            },
        )?;
        mls_group.save(provider.key_store())?;

        let group_id = mls_group.group_id().to_vec();
        let stored_group = StoredGroup::new(group_id.clone(), now_ns(), membership_state);
        stored_group.store(provider.conn())?;
        Ok(Self::new(client, group_id, stored_group.created_at_ns))
    }

    // Create a group from a decrypted and decoded welcome message
    // If the group already exists in the store, overwrite the MLS state and do not update the group entry
    fn create_from_welcome(
        client: &'c Client<ApiClient>,
        provider: &XmtpOpenMlsProvider,
        welcome: MlsWelcome,
    ) -> Result<Self, GroupError> {
        let mut mls_group =
            OpenMlsGroup::new_from_welcome(provider, &build_group_join_config(), welcome, None)?;
        mls_group.save(provider.key_store())?;
        let group_id = mls_group.group_id().to_vec();

        let to_store = StoredGroup::new(group_id, now_ns(), GroupMembershipState::Pending);
        let stored_group = provider.conn().insert_or_ignore_group(to_store)?;

        Ok(Self::new(
            client,
            stored_group.id,
            stored_group.created_at_ns,
        ))
    }

    // Decrypt a welcome message using HPKE and then create and save a group from the stored message
    pub fn create_from_encrypted_welcome(
        client: &'c Client<ApiClient>,
        provider: &XmtpOpenMlsProvider,
        hpke_public_key: &[u8],
        encrypted_welcome_bytes: Vec<u8>,
    ) -> Result<Self, GroupError> {
        let welcome_bytes = decrypt_welcome(provider, hpke_public_key, &encrypted_welcome_bytes)?;

        let welcome = deserialize_welcome(&welcome_bytes)?;

        Self::create_from_welcome(client, provider, welcome)
    }

    pub async fn send_message(&self, message: &[u8]) -> Result<(), GroupError> {
        let conn = &mut self.client.store.conn()?;

        self.maybe_update_installation_list(conn).await?;

        let intent_data: Vec<u8> = SendMessageIntentData::new(message.to_vec()).into();
        let intent =
            NewGroupIntent::new(IntentKind::SendMessage, self.group_id.clone(), intent_data);
        intent.store(conn)?;

        // Skipping a full sync here and instead just firing and forgetting
        if let Err(err) = self.publish_intents(conn).await {
            println!("error publishing intents: {:?}", err);
        }
        Ok(())
    }

    // Query the database for stored messages. Optionally filtered by time, kind, and limit
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

    pub async fn add_members(
        &self,
        account_addresses_to_add: Vec<String>,
    ) -> Result<(), GroupError> {
        let account_addresses = sanitize_evm_addresses(account_addresses_to_add)?;
        let conn = &mut self.client.store.conn()?;
        let intent_data: Vec<u8> =
            AddMembersIntentData::new(account_addresses.into()).try_into()?;
        let intent = conn.insert_group_intent(NewGroupIntent::new(
            IntentKind::AddMembers,
            self.group_id.clone(),
            intent_data,
        ))?;

        self.sync_until_intent_resolved(conn, intent.id).await
    }

    pub async fn add_members_by_installation_id(
        &self,
        installation_ids: Vec<Vec<u8>>,
    ) -> Result<(), GroupError> {
        validate_ed25519_keys(&installation_ids)?;
        let conn = &mut self.client.store.conn()?;
        let intent_data: Vec<u8> = AddMembersIntentData::new(installation_ids.into()).try_into()?;
        let intent = conn.insert_group_intent(NewGroupIntent::new(
            IntentKind::AddMembers,
            self.group_id.clone(),
            intent_data,
        ))?;

        self.sync_until_intent_resolved(conn, intent.id).await
    }

    pub async fn remove_members(
        &self,
        account_addresses_to_remove: Vec<String>,
    ) -> Result<(), GroupError> {
        let account_addresses = sanitize_evm_addresses(account_addresses_to_remove)?;
        let conn = &mut self.client.store.conn()?;
        let intent_data: Vec<u8> = RemoveMembersIntentData::new(account_addresses.into()).into();
        let intent = conn.insert_group_intent(NewGroupIntent::new(
            IntentKind::RemoveMembers,
            self.group_id.clone(),
            intent_data,
        ))?;

        self.sync_until_intent_resolved(conn, intent.id).await
    }

    #[allow(dead_code)]
    pub(crate) async fn remove_members_by_installation_id(
        &self,
        installation_ids: Vec<Vec<u8>>,
    ) -> Result<(), GroupError> {
        validate_ed25519_keys(&installation_ids)?;
        let conn = &mut self.client.store.conn()?;
        let intent_data: Vec<u8> = RemoveMembersIntentData::new(installation_ids.into()).into();
        let intent = conn.insert_group_intent(NewGroupIntent::new(
            IntentKind::RemoveMembers,
            self.group_id.clone(),
            intent_data,
        ))?;

        self.sync_until_intent_resolved(conn, intent.id).await
    }

    // Update this installation's leaf key in the group by creating a key update commit
    pub async fn key_update(&self) -> Result<(), GroupError> {
        let conn = &mut self.client.store.conn()?;
        let intent = NewGroupIntent::new(IntentKind::KeyUpdate, self.group_id.clone(), vec![]);
        intent.store(conn)?;

        self.sync_with_conn(conn).await
    }

    pub fn is_active(&self) -> Result<bool, GroupError> {
        let conn = &self.client.store.conn()?;
        let provider = XmtpOpenMlsProvider::new(conn);
        let mls_group = self.load_mls_group(&provider)?;

        Ok(mls_group.is_active())
    }
}

fn extract_message_v1(message: GroupMessage) -> Result<GroupMessageV1, MessageProcessingError> {
    match message.version {
        Some(GroupMessageVersion::V1(value)) => Ok(value),
        _ => Err(MessageProcessingError::InvalidPayload),
    }
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

fn build_protected_metadata_extension(
    identity: &Identity,
    policies: PolicySet,
) -> Result<Extension, GroupError> {
    let metadata = GroupMetadata::new(
        ConversationType::Group,
        identity.account_address.clone(),
        policies,
    );
    let protected_metadata = Metadata::new(metadata.try_into()?);

    Ok(Extension::ImmutableMetadata(protected_metadata))
}

fn build_group_config(
    protected_metadata_extension: Extension,
) -> Result<MlsGroupCreateConfig, GroupError> {
    let extensions = Extensions::single(protected_metadata_extension);

    Ok(MlsGroupCreateConfig::builder()
        .with_group_context_extensions(extensions)?
        .crypto_config(CryptoConfig::with_default_version(CIPHERSUITE))
        .wire_format_policy(WireFormatPolicy::default())
        .max_past_epochs(3) // Trying with 3 max past epochs for now
        .use_ratchet_tree_extension(true)
        .build())
}

fn build_group_join_config() -> MlsGroupJoinConfig {
    MlsGroupJoinConfig::builder()
        .wire_format_policy(WireFormatPolicy::default())
        .max_past_epochs(3) // Trying with 3 max past epochs for now
        .use_ratchet_tree_extension(true)
        .build()
}

#[cfg(test)]
mod tests {
    use openmls::prelude::Member;
    use prost::Message;
    use xmtp_api_grpc::grpc_api_helper::Client as GrpcClient;
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_proto::{api_client::XmtpMlsClient, xmtp::mls::message_contents::EncodedContent};

    use crate::{
        builder::ClientBuilder,
        codecs::{membership_change::GroupMembershipChangeCodec, ContentCodec},
        storage::{
            group_intent::IntentState,
            group_message::{GroupMessageKind, StoredGroupMessage},
        },
        Client, InboxOwner,
    };

    use super::MlsGroup;

    async fn receive_group_invite<ApiClient>(client: &Client<ApiClient>) -> MlsGroup<ApiClient>
    where
        ApiClient: XmtpMlsClient,
    {
        client.sync_welcomes().await.unwrap();
        let mut groups = client.find_groups(None, None, None, None).unwrap();

        groups.remove(0)
    }

    async fn get_latest_message<'c>(group: &MlsGroup<'c, GrpcClient>) -> StoredGroupMessage {
        group.sync().await.unwrap();
        let mut messages = group.find_messages(None, None, None, None).unwrap();

        messages.pop().unwrap()
    }

    #[tokio::test]
    async fn test_send_message() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;
        let group = client.create_group().expect("create group");
        group.send_message(b"hello").await.expect("send message");

        let messages = client
            .api_client
            .query_group_messages(group.group_id, None)
            .await
            .expect("read topic");

        assert_eq!(messages.len(), 1)
    }

    #[tokio::test]
    async fn test_receive_self_message() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;
        let group = client.create_group().expect("create group");
        let msg = b"hello";
        group.send_message(msg).await.expect("send message");

        group.receive(&client.store.conn().unwrap()).await.unwrap();
        // Check for messages
        // println!("HERE: {:#?}", messages);
        let messages = group.find_messages(None, None, None, None).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages.first().unwrap().decrypted_message_bytes, msg);
    }

    // Amal and Bola will both try and add Charlie from the same epoch.
    // The group should resolve to a consistent state
    #[tokio::test]
    async fn test_add_member_conflict() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let charlie = ClientBuilder::new_test_client(&generate_local_wallet()).await;

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
            .expect_err("expected err");

        amal_group
            .receive(&amal.store.conn().unwrap())
            .await
            .expect_err("expected error");

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
    async fn test_add_installation() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let client_2 = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let group = client.create_group().expect("create group");

        group
            .add_members_by_installation_id(vec![client_2.installation_public_key()])
            .await
            .unwrap();

        let group_id = group.group_id;

        let messages = client
            .api_client
            .query_group_messages(group_id, None)
            .await
            .unwrap();

        assert_eq!(messages.len(), 1);
    }

    #[tokio::test]
    async fn test_add_invalid_member() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let group = client.create_group().expect("create group");

        let result = group
            .add_members_by_installation_id(vec![b"1234".to_vec()])
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_add_unregistered_member() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let unconnected_wallet_address = generate_local_wallet().get_address();
        let group = amal.create_group().unwrap();
        let result = group.add_members(vec![unconnected_wallet_address]).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_remove_installation() {
        let client_1 = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        // Add another client onto the network
        let client_2 = ClientBuilder::new_test_client(&generate_local_wallet()).await;

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
        let group_id = group.group_id;
        let messages = client_1
            .api_client
            .query_group_messages(group_id, None)
            .await
            .expect("read topic");

        assert_eq!(messages.len(), 2);
    }

    #[tokio::test]
    async fn test_key_update() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola_client = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let group = client.create_group().expect("create group");
        group
            .add_members(vec![bola_client.account_address()])
            .await
            .unwrap();

        group.key_update().await.unwrap();

        let messages = client
            .api_client
            .query_group_messages(group.group_id.clone(), None)
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
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let client_2 = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let group = client.create_group().expect("create group");

        group
            .add_members_by_installation_id(vec![client_2.installation_public_key()])
            .await
            .unwrap();

        // Check if the welcome was actually sent
        let welcome_messages = client
            .api_client
            .query_welcome_messages(client_2.installation_public_key(), None)
            .await
            .unwrap();

        assert_eq!(welcome_messages.len(), 1);
    }

    #[tokio::test]
    async fn test_remove_by_account_address() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let charlie = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let group = amal.create_group().unwrap();
        group
            .add_members(vec![bola.account_address(), charlie.account_address()])
            .await
            .unwrap();
        assert_eq!(group.members().unwrap().len(), 3);
        let messages = group.find_messages(None, None, None, None).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].kind, GroupMessageKind::MembershipChange);
        let encoded_content =
            EncodedContent::decode(messages[0].decrypted_message_bytes.as_slice()).unwrap();
        let members_changed_codec = GroupMembershipChangeCodec::decode(encoded_content).unwrap();
        assert_eq!(members_changed_codec.members_added.len(), 2);
        assert_eq!(members_changed_codec.members_removed.len(), 0);
        assert_eq!(members_changed_codec.installations_added.len(), 0);
        assert_eq!(members_changed_codec.installations_removed.len(), 0);

        group
            .remove_members(vec![bola.account_address()])
            .await
            .unwrap();
        assert_eq!(group.members().unwrap().len(), 2);
        let messages = group.find_messages(None, None, None, None).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[1].kind, GroupMessageKind::MembershipChange);
        let encoded_content =
            EncodedContent::decode(messages[1].decrypted_message_bytes.as_slice()).unwrap();
        let members_changed_codec = GroupMembershipChangeCodec::decode(encoded_content).unwrap();
        assert_eq!(members_changed_codec.members_added.len(), 0);
        assert_eq!(members_changed_codec.members_removed.len(), 1);
        assert_eq!(members_changed_codec.installations_added.len(), 0);
        assert_eq!(members_changed_codec.installations_removed.len(), 0);

        let bola_group = receive_group_invite(&bola).await;
        bola_group.sync().await.unwrap();
        assert!(!bola_group.is_active().unwrap())
    }

    #[tokio::test]
    async fn test_get_missing_members() {
        // Setup for test
        let amal_wallet = generate_local_wallet();
        let amal = ClientBuilder::new_test_client(&amal_wallet).await;
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let group = amal.create_group().unwrap();
        group
            .add_members(vec![bola.account_address()])
            .await
            .unwrap();
        assert_eq!(group.members().unwrap().len(), 2);

        let conn = &amal.store.conn().unwrap();
        let provider = super::XmtpOpenMlsProvider::new(conn);
        // Finished with setup

        let (noone_to_add, _placeholder) = group.get_missing_members(&provider).await.unwrap();
        assert_eq!(noone_to_add.len(), 0);
        assert_eq!(_placeholder.len(), 0);

        // Add a second installation for amal using the same wallet
        let _amal_2nd = ClientBuilder::new_test_client(&amal_wallet).await;

        // Here we should find a new installation
        let (missing_members, _placeholder) = group.get_missing_members(&provider).await.unwrap();
        assert_eq!(missing_members.len(), 1);
        assert_eq!(_placeholder.len(), 0);

        let _result = group.add_members_by_installation_id(missing_members).await;

        // After we added the new installation the list should again be empty
        let (missing_members, _placeholder) = group.get_missing_members(&provider).await.unwrap();
        assert_eq!(missing_members.len(), 0);
        assert_eq!(_placeholder.len(), 0);
    }

    #[tokio::test]
    async fn test_add_missing_installations() {
        // Setup for test
        let amal_wallet = generate_local_wallet();
        let amal = ClientBuilder::new_test_client(&amal_wallet).await;
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let group = amal.create_group().unwrap();
        group
            .add_members(vec![bola.account_address()])
            .await
            .unwrap();
        assert_eq!(group.members().unwrap().len(), 2);

        let conn = &amal.store.conn().unwrap();
        let provider = super::XmtpOpenMlsProvider::new(conn);
        // Finished with setup

        // add a second installation for amal using the same wallet
        let _amal_2nd = ClientBuilder::new_test_client(&amal_wallet).await;

        // test that adding the new installation(s), worked
        let new_installations_were_added = group.add_missing_installations(provider).await;
        assert!(new_installations_were_added.is_ok());
    }

    #[tokio::test]
    async fn test_self_resolve_epoch_mismatch() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let charlie = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let dave = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let amal_group = amal.create_group().unwrap();
        // Add bola to the group
        amal_group
            .add_members(vec![bola.account_address()])
            .await
            .unwrap();

        let bola_group = receive_group_invite(&bola).await;
        bola_group.sync().await.unwrap();
        // Both Amal and Bola are up to date on the group state. Now each of them want to add someone else
        amal_group
            .add_members(vec![charlie.account_address()])
            .await
            .unwrap();

        bola_group
            .add_members(vec![dave.account_address()])
            .await
            .unwrap();

        // Send a message to the group, now that everyone is invited
        amal_group.sync().await.unwrap();
        amal_group.send_message(b"hello").await.unwrap();

        let charlie_group = receive_group_invite(&charlie).await;
        let dave_group = receive_group_invite(&dave).await;

        let (amal_latest_message, bola_latest_message, charlie_latest_message, dave_latest_message) = tokio::join!(
            get_latest_message(&amal_group),
            get_latest_message(&bola_group),
            get_latest_message(&charlie_group),
            get_latest_message(&dave_group)
        );

        let expected_latest_message = b"hello".to_vec();
        assert!(expected_latest_message.eq(&amal_latest_message.decrypted_message_bytes));
        assert!(expected_latest_message.eq(&bola_latest_message.decrypted_message_bytes));
        assert!(expected_latest_message.eq(&charlie_latest_message.decrypted_message_bytes));
        assert!(expected_latest_message.eq(&dave_latest_message.decrypted_message_bytes));
    }
}
