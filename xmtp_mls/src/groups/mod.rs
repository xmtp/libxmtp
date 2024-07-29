pub mod group_membership;
pub mod group_metadata;
pub mod group_mutable_metadata;
pub mod group_permissions;
pub mod intents;
pub mod members;
#[allow(dead_code)]
pub(crate) mod message_history;
mod subscriptions;
mod sync;
pub mod validated_commit;

use intents::SendMessageIntentData;
use openmls::{
    credentials::{BasicCredential, CredentialType},
    error::LibraryError,
    extensions::{
        Extension, ExtensionType, Extensions, Metadata, RequiredCapabilitiesExtension,
        UnknownExtension,
    },
    group::{
        CreateGroupContextExtProposalError, MlsGroupCreateConfig, MlsGroupJoinConfig,
        ProcessedWelcome,
    },
    messages::proposals::ProposalType,
    prelude::{
        BasicCredentialError, Capabilities, CredentialWithKey, Error as TlsCodecError, GroupId,
        MlsGroup as OpenMlsGroup, StagedWelcome, Welcome as MlsWelcome, WireFormatPolicy,
    },
};
use openmls_traits::OpenMlsProvider;
use prost::Message;
use thiserror::Error;

pub use self::group_permissions::PreconfiguredPolicies;
pub use self::intents::{AddressesOrInstallationIds, IntentError};
use self::{
    group_membership::GroupMembership,
    group_metadata::extract_group_metadata,
    group_mutable_metadata::{GroupMutableMetadata, GroupMutableMetadataError, MetadataField},
    group_permissions::{
        extract_group_permissions, GroupMutablePermissions, GroupMutablePermissionsError,
    },
    intents::{
        AdminListActionType, PermissionPolicyOption, PermissionUpdateType,
        UpdateAdminListIntentData, UpdateMetadataIntentData, UpdatePermissionIntentData,
    },
    validated_commit::extract_group_membership,
};
use self::{
    group_metadata::{ConversationType, GroupMetadata, GroupMetadataError},
    group_permissions::PolicySet,
    message_history::MessageHistoryError,
    validated_commit::CommitValidationError,
};
use std::{collections::HashSet, sync::Arc};
use xmtp_cryptography::signature::{sanitize_evm_addresses, AddressValidationError};
use xmtp_id::InboxId;
use xmtp_proto::xmtp::mls::{
    api::v1::{
        group_message::{Version as GroupMessageVersion, V1 as GroupMessageV1},
        GroupMessage,
    },
    message_contents::{
        plaintext_envelope::{Content, V1},
        PlaintextEnvelope,
    },
};

use crate::{
    api::WrappedApiError,
    client::{deserialize_welcome, ClientError, MessageProcessingError, XmtpMlsLocalContext},
    configuration::{
        CIPHERSUITE, GROUP_MEMBERSHIP_EXTENSION_ID, GROUP_PERMISSIONS_EXTENSION_ID, MAX_GROUP_SIZE,
        MUTABLE_METADATA_EXTENSION_ID,
    },
    hpke::{decrypt_welcome, HpkeError},
    identity::{parse_credential, Identity, IdentityError},
    identity_updates::{load_identity_updates, InstallationDiffError},
    retry::RetryableError,
    storage::{
        db_connection::DbConnection,
        group::{GroupMembershipState, Purpose, StoredGroup},
        group_intent::{IntentKind, NewGroupIntent},
        group_message::{DeliveryStatus, GroupMessageKind, StoredGroupMessage},
        sql_key_store,
    },
    utils::{id::calculate_message_id, time::now_ns},
    xmtp_openmls_provider::XmtpOpenMlsProvider,
    Client, Store, XmtpApi,
};

#[derive(Debug, Error)]
pub enum GroupError {
    #[error("group not found")]
    GroupNotFound,
    #[error("Max user limit exceeded.")]
    UserLimitExceeded,
    #[error("api error: {0}")]
    Api(#[from] xmtp_proto::api_client::Error),
    #[error("api error: {0}")]
    WrappedApi(#[from] WrappedApiError),
    #[error("invalid group membership")]
    InvalidGroupMembership,
    #[error("storage error: {0}")]
    Storage(#[from] crate::storage::StorageError),
    #[error("intent error: {0}")]
    Intent(#[from] IntentError),
    #[error("create message: {0}")]
    CreateMessage(#[from] openmls::prelude::CreateMessageError<sql_key_store::SqlKeyStoreError>),
    #[error("TLS Codec error: {0}")]
    TlsError(#[from] TlsCodecError),
    #[error("SequenceId not found in local db")]
    MissingSequenceId,
    #[error("Addresses not found {0:?}")]
    AddressNotFound(Vec<String>),
    #[error("add members: {0}")]
    UpdateGroupMembership(
        #[from] openmls::prelude::UpdateGroupMembershipError<sql_key_store::SqlKeyStoreError>,
    ),
    #[error("group create: {0}")]
    GroupCreate(#[from] openmls::group::NewGroupError<sql_key_store::SqlKeyStoreError>),
    #[error("self update: {0}")]
    SelfUpdate(#[from] openmls::group::SelfUpdateError<sql_key_store::SqlKeyStoreError>),
    #[error("welcome error: {0}")]
    WelcomeError(#[from] openmls::prelude::WelcomeError<sql_key_store::SqlKeyStoreError>),
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
    #[error(transparent)]
    AddressValidation(#[from] AddressValidationError),
    #[error("Public Keys {0:?} are not valid ed25519 public keys")]
    InvalidPublicKeys(Vec<Vec<u8>>),
    #[error("Commit validation error {0}")]
    CommitValidation(#[from] CommitValidationError),
    #[error("Metadata error {0}")]
    GroupMetadata(#[from] GroupMetadataError),
    #[error("Mutable Metadata error {0}")]
    GroupMutableMetadata(#[from] GroupMutableMetadataError),
    #[error("Mutable Permissions error {0}")]
    GroupMutablePermissions(#[from] GroupMutablePermissionsError),
    #[error("Errors occurred during sync {0:?}")]
    Sync(Vec<GroupError>),
    #[error("Hpke error: {0}")]
    Hpke(#[from] HpkeError),
    #[error("identity error: {0}")]
    Identity(#[from] IdentityError),
    #[error("serialization error: {0}")]
    EncodeError(#[from] prost::EncodeError),
    #[error("create group context proposal error: {0}")]
    CreateGroupContextExtProposalError(
        #[from] CreateGroupContextExtProposalError<sql_key_store::SqlKeyStoreError>,
    ),
    #[error("Credential error")]
    CredentialError(#[from] BasicCredentialError),
    #[error("LeafNode error")]
    LeafNodeError(#[from] LibraryError),
    #[error("Message History error: {0}")]
    MessageHistory(#[from] Box<MessageHistoryError>),
    #[error("Installation diff error: {0}")]
    InstallationDiff(#[from] InstallationDiffError),
    #[error("PSKs are not support")]
    NoPSKSupport,
    #[error("Metadata update must specify a metadata field")]
    InvalidPermissionUpdate,
    #[error("The intent publishing task was cancelled")]
    PublishCancelled,
    #[error("the publish failed to complete due to panic")]
    PublishPanicked,
}

impl RetryableError for GroupError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Api(api_error) => api_error.is_retryable(),
            Self::Client(client_error) => client_error.is_retryable(),
            Self::Diesel(diesel) => diesel.is_retryable(),
            Self::Storage(storage) => storage.is_retryable(),
            Self::ReceiveError(msg) => msg.is_retryable(),
            Self::Hpke(hpke) => hpke.is_retryable(),
            Self::Identity(identity) => identity.is_retryable(),
            Self::UpdateGroupMembership(update) => update.is_retryable(),
            Self::GroupCreate(group) => group.is_retryable(),
            Self::SelfUpdate(update) => update.is_retryable(),
            Self::WelcomeError(welcome) => welcome.is_retryable(),
            Self::InstallationDiff(diff) => diff.is_retryable(),
            Self::CreateGroupContextExtProposalError(create) => create.is_retryable(),
            _ => false,
        }
    }
}

#[derive(Debug)]
pub struct MlsGroup {
    pub group_id: Vec<u8>,
    pub created_at_ns: i64,
    context: Arc<XmtpMlsLocalContext>,
}

#[derive(Default)]
pub struct GroupMetadataOptions {
    pub name: Option<String>,
    pub image_url_square: Option<String>,
    pub description: Option<String>,
    pub pinned_frame_url: Option<String>,
}

impl Clone for MlsGroup {
    fn clone(&self) -> Self {
        Self {
            context: self.context.clone(),
            group_id: self.group_id.clone(),
            created_at_ns: self.created_at_ns,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum UpdateAdminListType {
    Add,
    Remove,
    AddSuper,
    RemoveSuper,
}

impl MlsGroup {
    // Creates a new group instance. Does not validate that the group exists in the DB
    pub fn new(context: Arc<XmtpMlsLocalContext>, group_id: Vec<u8>, created_at_ns: i64) -> Self {
        Self {
            context,
            group_id,
            created_at_ns,
        }
    }

    // Load the stored MLS group from the OpenMLS provider's keystore
    #[tracing::instrument(level = "trace", skip_all)]
    fn load_mls_group(&self, provider: impl OpenMlsProvider) -> Result<OpenMlsGroup, GroupError> {
        let mls_group =
            OpenMlsGroup::load(provider.storage(), &GroupId::from_slice(&self.group_id))
                .map_err(|_| GroupError::GroupNotFound)?
                .ok_or(GroupError::GroupNotFound)?;

        Ok(mls_group)
    }

    // Create a new group and save it to the DB
    pub fn create_and_insert(
        context: Arc<XmtpMlsLocalContext>,
        membership_state: GroupMembershipState,
        permissions_policy_set: PolicySet,
        opts: GroupMetadataOptions,
    ) -> Result<Self, GroupError> {
        let conn = context.store.conn()?;
        let provider = XmtpOpenMlsProvider::new(conn);
        let protected_metadata =
            build_protected_metadata_extension(&context.identity, Purpose::Conversation)?;
        let mutable_metadata = build_mutable_metadata_extension_default(&context.identity, opts)?;
        let group_membership = build_starting_group_membership_extension(context.inbox_id(), 0);
        let mutable_permissions = build_mutable_permissions_extension(permissions_policy_set)?;
        let group_config = build_group_config(
            protected_metadata,
            mutable_metadata,
            group_membership,
            mutable_permissions,
        )?;

        let mls_group = OpenMlsGroup::new(
            &provider,
            &context.identity.installation_keys,
            &group_config,
            CredentialWithKey {
                credential: context.identity.credential(),
                signature_key: context.identity.installation_keys.to_public_vec().into(),
            },
        )?;

        let group_id = mls_group.group_id().to_vec();
        let stored_group = StoredGroup::new(
            group_id.clone(),
            now_ns(),
            membership_state,
            context.inbox_id(),
        );

        stored_group.store(provider.conn_ref())?;
        Ok(Self::new(
            context.clone(),
            group_id,
            stored_group.created_at_ns,
        ))
    }

    // Create a group from a decrypted and decoded welcome message
    // If the group already exists in the store, overwrite the MLS state and do not update the group entry
    async fn create_from_welcome<ApiClient: XmtpApi>(
        client: &Client<ApiClient>,
        provider: &XmtpOpenMlsProvider,
        welcome: MlsWelcome,
        added_by_inbox: String,
        welcome_id: i64,
    ) -> Result<Self, GroupError> {
        let mls_welcome =
            StagedWelcome::new_from_welcome(provider, &build_group_join_config(), welcome, None)?;

        let mls_group = mls_welcome.into_group(provider)?;
        let group_id = mls_group.group_id().to_vec();
        let metadata = extract_group_metadata(&mls_group)?;
        let group_type = metadata.conversation_type;

        let to_store = match group_type {
            ConversationType::Group | ConversationType::Dm => StoredGroup::new_from_welcome(
                group_id.clone(),
                now_ns(),
                GroupMembershipState::Pending,
                added_by_inbox,
                welcome_id,
                Purpose::Conversation,
            ),
            ConversationType::Sync => StoredGroup::new_from_welcome(
                group_id.clone(),
                now_ns(),
                GroupMembershipState::Allowed,
                added_by_inbox,
                welcome_id,
                Purpose::Sync,
            ),
        };

        validate_initial_group_membership(client, provider.conn_ref(), &mls_group).await?;

        let stored_group = provider.conn().insert_or_replace_group(to_store)?;

        Ok(Self::new(
            client.context.clone(),
            stored_group.id,
            stored_group.created_at_ns,
        ))
    }

    // Decrypt a welcome message using HPKE and then create and save a group from the stored message
    pub async fn create_from_encrypted_welcome<ApiClient: XmtpApi>(
        client: &Client<ApiClient>,
        provider: &XmtpOpenMlsProvider,
        hpke_public_key: &[u8],
        encrypted_welcome_bytes: Vec<u8>,
        welcome_id: i64,
    ) -> Result<Self, GroupError> {
        let welcome_bytes = decrypt_welcome(provider, hpke_public_key, &encrypted_welcome_bytes)?;

        let welcome = deserialize_welcome(&welcome_bytes)?;

        let join_config = build_group_join_config();

        let processed_welcome =
            ProcessedWelcome::new_from_welcome(provider, &join_config, welcome.clone())?;
        let psks = processed_welcome.psks();
        if !psks.is_empty() {
            return Err(GroupError::NoPSKSupport);
        }
        let staged_welcome = processed_welcome.into_staged_welcome(provider, None)?;

        let added_by_node = staged_welcome.welcome_sender()?;

        let added_by_credential = BasicCredential::try_from(added_by_node.credential().clone())?;
        let inbox_id = parse_credential(added_by_credential.identity())?;

        Self::create_from_welcome(client, provider, welcome, inbox_id, welcome_id).await
    }

    pub(crate) fn create_and_insert_sync_group(
        context: Arc<XmtpMlsLocalContext>,
    ) -> Result<MlsGroup, GroupError> {
        let conn = context.store.conn()?;
        // let my_sequence_id = context.inbox_sequence_id(&conn)?;
        let provider = XmtpOpenMlsProvider::new(conn);
        let protected_metadata =
            build_protected_metadata_extension(&context.identity, Purpose::Sync)?;
        let mutable_metadata = build_mutable_metadata_extension_default(
            &context.identity,
            GroupMetadataOptions::default(),
        )?;
        let group_membership = build_starting_group_membership_extension(context.inbox_id(), 0);
        let mutable_permissions =
            build_mutable_permissions_extension(PreconfiguredPolicies::default().to_policy_set())?;
        let group_config = build_group_config(
            protected_metadata,
            mutable_metadata,
            group_membership,
            mutable_permissions,
        )?;
        let mls_group = OpenMlsGroup::new(
            &provider,
            &context.identity.installation_keys,
            &group_config,
            CredentialWithKey {
                credential: context.identity.credential(),
                signature_key: context.identity.installation_keys.to_public_vec().into(),
            },
        )?;

        let group_id = mls_group.group_id().to_vec();
        let stored_group =
            StoredGroup::new_sync_group(group_id.clone(), now_ns(), GroupMembershipState::Allowed);

        stored_group.store(provider.conn_ref())?;

        Ok(Self::new(
            context.clone(),
            stored_group.id,
            stored_group.created_at_ns,
        ))
    }

    /// Send a message on this users XMTP [`Client`].
    pub async fn send_message<ApiClient>(
        &self,
        message: &[u8],
        client: &Client<ApiClient>,
    ) -> Result<Vec<u8>, GroupError>
    where
        ApiClient: XmtpApi,
    {
        let update_interval = Some(5_000_000); // 5 seconds in nanoseconds
        let conn = self.context.store.conn()?;
        self.maybe_update_installations(conn.clone(), update_interval, client)
            .await?;

        let message_id = self.prepare_message(message, &conn);

        // Skipping a full sync here and instead just firing and forgetting
        if let Err(err) = self.publish_intents(conn.clone(), client).await {
            log::error!("Send: error publishing intents: {:?}", err);
        }

        self.sync_until_last_intent_resolved(conn, client).await?;

        message_id
    }

    /// Publish all unpublished messages
    pub async fn publish_messages<ApiClient>(
        &self,
        client: &Client<ApiClient>,
    ) -> Result<(), GroupError>
    where
        ApiClient: XmtpApi,
    {
        let conn = self.context.store.conn()?;
        let update_interval = Some(5_000_000);
        self.maybe_update_installations(conn.clone(), update_interval, client)
            .await?;
        self.publish_intents(conn.clone(), client).await?;
        self.sync_until_last_intent_resolved(conn, client).await?;
        Ok(())
    }

    /// Send a message, optimistically returning the ID of the message before the result of a message publish.
    pub fn send_message_optimistic(&self, message: &[u8]) -> Result<Vec<u8>, GroupError> {
        let conn = self.context.store.conn()?;
        let message_id = self.prepare_message(message, &conn)?;

        Ok(message_id)
    }

    /// Prepare a message (intent & id) on this users XMTP [`Client`].
    fn prepare_message(&self, message: &[u8], conn: &DbConnection) -> Result<Vec<u8>, GroupError> {
        let now = now_ns();
        let plain_envelope = Self::into_envelope(message, &now.to_string());
        let mut encoded_envelope = vec![];
        plain_envelope
            .encode(&mut encoded_envelope)
            .map_err(GroupError::EncodeError)?;

        let intent_data: Vec<u8> = SendMessageIntentData::new(encoded_envelope).into();
        let intent =
            NewGroupIntent::new(IntentKind::SendMessage, self.group_id.clone(), intent_data);
        intent.store(conn)?;

        // store this unpublished message locally before sending
        let message_id = calculate_message_id(&self.group_id, message, &now.to_string());
        let group_message = StoredGroupMessage {
            id: message_id.clone(),
            group_id: self.group_id.clone(),
            decrypted_message_bytes: message.to_vec(),
            sent_at_ns: now,
            kind: GroupMessageKind::Application,
            sender_installation_id: self.context.installation_public_key(),
            sender_inbox_id: self.context.inbox_id(),
            delivery_status: DeliveryStatus::Unpublished,
        };
        group_message.store(conn)?;

        Ok(message_id)
    }

    fn into_envelope(encoded_msg: &[u8], idempotency_key: &str) -> PlaintextEnvelope {
        PlaintextEnvelope {
            content: Some(Content::V1(V1 {
                content: encoded_msg.to_vec(),
                idempotency_key: idempotency_key.into(),
            })),
        }
    }

    // Query the database for stored messages. Optionally filtered by time, kind, delivery_status
    // and limit
    pub fn find_messages(
        &self,
        kind: Option<GroupMessageKind>,
        sent_before_ns: Option<i64>,
        sent_after_ns: Option<i64>,
        delivery_status: Option<DeliveryStatus>,
        limit: Option<i64>,
    ) -> Result<Vec<StoredGroupMessage>, GroupError> {
        let conn = self.context.store.conn()?;
        let messages = conn.get_group_messages(
            &self.group_id,
            sent_after_ns,
            sent_before_ns,
            kind,
            delivery_status,
            limit,
        )?;

        Ok(messages)
    }

    /**
     * Add members to the group by account address
     *
     * If any existing members have new installations that have not been added, the missing installations
     * will be added as part of this process as well.
     */
    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn add_members<ApiClient>(
        &self,
        client: &Client<ApiClient>,
        account_addresses_to_add: Vec<String>,
    ) -> Result<(), GroupError>
    where
        ApiClient: XmtpApi,
    {
        let account_addresses = sanitize_evm_addresses(account_addresses_to_add)?;
        let inbox_id_map = client
            .api_client
            .get_inbox_ids(account_addresses.clone())
            .await?;
        // get current number of users in group
        let member_count = self.members()?.len();
        if member_count + inbox_id_map.len() > MAX_GROUP_SIZE as usize {
            return Err(GroupError::UserLimitExceeded);
        }

        if inbox_id_map.len() != account_addresses.len() {
            let found_addresses: HashSet<&String> = inbox_id_map.keys().collect();
            let to_add_hashset = HashSet::from_iter(account_addresses.iter());
            let missing_addresses = found_addresses.difference(&to_add_hashset);
            return Err(GroupError::AddressNotFound(
                missing_addresses.into_iter().cloned().cloned().collect(),
            ));
        }

        self.add_members_by_inbox_id(client, inbox_id_map.into_values().collect())
            .await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn add_members_by_inbox_id<ApiClient: XmtpApi>(
        &self,
        client: &Client<ApiClient>,
        inbox_ids: Vec<String>,
    ) -> Result<(), GroupError> {
        let conn = client.store().conn()?;
        let provider = client.mls_provider(conn);
        let intent_data = self
            .get_membership_update_intent(client, &provider, inbox_ids, vec![])
            .await?;

        // TODO:nm this isn't the best test for whether the request is valid
        // If some existing group member has an update, this will return an intent with changes
        // when we really should return an error
        if intent_data.is_empty() {
            log::warn!("Member already added");
            return Ok(());
        }

        let intent = provider.conn().insert_group_intent(NewGroupIntent::new(
            IntentKind::UpdateGroupMembership,
            self.group_id.clone(),
            intent_data.into(),
        ))?;

        self.sync_until_intent_resolved(provider.conn(), intent.id, client)
            .await
    }

    pub async fn remove_members<ApiClient: XmtpApi>(
        &self,
        client: &Client<ApiClient>,
        account_addresses_to_remove: Vec<InboxId>,
    ) -> Result<(), GroupError> {
        let account_addresses = sanitize_evm_addresses(account_addresses_to_remove)?;
        let inbox_id_map = client.api_client.get_inbox_ids(account_addresses).await?;

        self.remove_members_by_inbox_id(client, inbox_id_map.into_values().collect())
            .await
    }

    pub async fn remove_members_by_inbox_id<ApiClient: XmtpApi>(
        &self,
        client: &Client<ApiClient>,
        inbox_ids: Vec<InboxId>,
    ) -> Result<(), GroupError> {
        let conn = client.store().conn()?;
        let provider = client.mls_provider(conn);
        let intent_data = self
            .get_membership_update_intent(client, &provider, vec![], inbox_ids)
            .await?;

        let intent = provider
            .conn_ref()
            .insert_group_intent(NewGroupIntent::new(
                IntentKind::UpdateGroupMembership,
                self.group_id.clone(),
                intent_data.into(),
            ))?;

        self.sync_until_intent_resolved(provider.conn(), intent.id, client)
            .await
    }

    pub async fn update_group_name<ApiClient>(
        &self,
        client: &Client<ApiClient>,
        group_name: String,
    ) -> Result<(), GroupError>
    where
        ApiClient: XmtpApi,
    {
        let conn = self.context.store.conn()?;
        let intent_data: Vec<u8> =
            UpdateMetadataIntentData::new_update_group_name(group_name).into();
        let intent = conn.insert_group_intent(NewGroupIntent::new(
            IntentKind::MetadataUpdate,
            self.group_id.clone(),
            intent_data,
        ))?;

        self.sync_until_intent_resolved(conn, intent.id, client)
            .await
    }

    pub async fn update_permission_policy<ApiClient: XmtpApi>(
        &self,
        client: &Client<ApiClient>,
        permission_update_type: PermissionUpdateType,
        permission_policy: PermissionPolicyOption,
        metadata_field: Option<MetadataField>,
    ) -> Result<(), GroupError> {
        let conn = client.store().conn()?;

        if permission_update_type == PermissionUpdateType::UpdateMetadata
            && metadata_field.is_none()
        {
            return Err(GroupError::InvalidPermissionUpdate);
        }

        let intent_data: Vec<u8> = UpdatePermissionIntentData::new(
            permission_update_type,
            permission_policy,
            metadata_field.as_ref().map(|field| field.to_string()),
        )
        .into();

        let intent = conn.insert_group_intent(NewGroupIntent::new(
            IntentKind::UpdatePermission,
            self.group_id.clone(),
            intent_data,
        ))?;

        self.sync_until_intent_resolved(conn, intent.id, client)
            .await
    }

    pub fn group_name(&self) -> Result<String, GroupError> {
        let mutable_metadata = self.mutable_metadata()?;
        match mutable_metadata
            .attributes
            .get(&MetadataField::GroupName.to_string())
        {
            Some(group_name) => Ok(group_name.clone()),
            None => Err(GroupError::GroupMutableMetadata(
                GroupMutableMetadataError::MissingExtension,
            )),
        }
    }

    pub async fn update_group_description<ApiClient>(
        &self,
        client: &Client<ApiClient>,
        group_description: String,
    ) -> Result<(), GroupError>
    where
        ApiClient: XmtpApi,
    {
        let conn = self.context.store.conn()?;
        let intent_data: Vec<u8> =
            UpdateMetadataIntentData::new_update_group_description(group_description).into();
        let intent = conn.insert_group_intent(NewGroupIntent::new(
            IntentKind::MetadataUpdate,
            self.group_id.clone(),
            intent_data,
        ))?;

        self.sync_until_intent_resolved(conn, intent.id, client)
            .await
    }

    pub fn group_description(&self) -> Result<String, GroupError> {
        let mutable_metadata = self.mutable_metadata()?;
        match mutable_metadata
            .attributes
            .get(&MetadataField::Description.to_string())
        {
            Some(group_description) => Ok(group_description.clone()),
            None => Err(GroupError::GroupMutableMetadata(
                GroupMutableMetadataError::MissingExtension,
            )),
        }
    }

    pub async fn update_group_image_url_square<ApiClient>(
        &self,
        client: &Client<ApiClient>,
        group_image_url_square: String,
    ) -> Result<(), GroupError>
    where
        ApiClient: XmtpApi,
    {
        let conn = self.context.store.conn()?;
        let intent_data: Vec<u8> =
            UpdateMetadataIntentData::new_update_group_image_url_square(group_image_url_square)
                .into();
        let intent = conn.insert_group_intent(NewGroupIntent::new(
            IntentKind::MetadataUpdate,
            self.group_id.clone(),
            intent_data,
        ))?;

        self.sync_until_intent_resolved(conn, intent.id, client)
            .await
    }

    pub fn group_image_url_square(&self) -> Result<String, GroupError> {
        let mutable_metadata = self.mutable_metadata()?;
        match mutable_metadata
            .attributes
            .get(&MetadataField::GroupImageUrlSquare.to_string())
        {
            Some(group_image_url_square) => Ok(group_image_url_square.clone()),
            None => Err(GroupError::GroupMutableMetadata(
                GroupMutableMetadataError::MissingExtension,
            )),
        }
    }

    pub async fn update_group_pinned_frame_url<ApiClient>(
        &self,
        client: &Client<ApiClient>,
        pinned_frame_url: String,
    ) -> Result<(), GroupError>
    where
        ApiClient: XmtpApi,
    {
        let conn = self.context.store.conn()?;
        let intent_data: Vec<u8> =
            UpdateMetadataIntentData::new_update_group_pinned_frame_url(pinned_frame_url).into();
        let intent = conn.insert_group_intent(NewGroupIntent::new(
            IntentKind::MetadataUpdate,
            self.group_id.clone(),
            intent_data,
        ))?;

        self.sync_until_intent_resolved(conn, intent.id, client)
            .await
    }

    pub fn group_pinned_frame_url(&self) -> Result<String, GroupError> {
        let mutable_metadata = self.mutable_metadata()?;
        match mutable_metadata
            .attributes
            .get(&MetadataField::GroupPinnedFrameUrl.to_string())
        {
            Some(pinned_frame_url) => Ok(pinned_frame_url.clone()),
            None => Err(GroupError::GroupMutableMetadata(
                GroupMutableMetadataError::MissingExtension,
            )),
        }
    }

    pub fn admin_list(&self) -> Result<Vec<String>, GroupError> {
        let mutable_metadata = self.mutable_metadata()?;
        Ok(mutable_metadata.admin_list)
    }

    pub fn super_admin_list(&self) -> Result<Vec<String>, GroupError> {
        let mutable_metadata = self.mutable_metadata()?;
        Ok(mutable_metadata.super_admin_list)
    }

    pub fn is_admin(&self, inbox_id: String) -> Result<bool, GroupError> {
        let mutable_metadata = self.mutable_metadata()?;
        Ok(mutable_metadata.admin_list.contains(&inbox_id))
    }

    pub fn is_super_admin(&self, inbox_id: String) -> Result<bool, GroupError> {
        let mutable_metadata = self.mutable_metadata()?;
        Ok(mutable_metadata.super_admin_list.contains(&inbox_id))
    }

    pub async fn update_admin_list<ApiClient>(
        &self,
        client: &Client<ApiClient>,
        action_type: UpdateAdminListType,
        inbox_id: String,
    ) -> Result<(), GroupError>
    where
        ApiClient: XmtpApi,
    {
        let conn = self.context.store.conn()?;
        let intent_action_type = match action_type {
            UpdateAdminListType::Add => AdminListActionType::Add,
            UpdateAdminListType::Remove => AdminListActionType::Remove,
            UpdateAdminListType::AddSuper => AdminListActionType::AddSuper,
            UpdateAdminListType::RemoveSuper => AdminListActionType::RemoveSuper,
        };
        let intent_data: Vec<u8> =
            UpdateAdminListIntentData::new(intent_action_type, inbox_id).into();
        let intent = conn.insert_group_intent(NewGroupIntent::new(
            IntentKind::UpdateAdminList,
            self.group_id.clone(),
            intent_data,
        ))?;

        self.sync_until_intent_resolved(conn, intent.id, client)
            .await
    }

    /// Find the `inbox_id` of the group member who added the member to the group
    pub fn added_by_inbox_id(&self) -> Result<String, GroupError> {
        let conn = self.context.store.conn()?;
        conn.find_group(self.group_id.clone())
            .map_err(GroupError::from)
            .and_then(|fetch_result| {
                fetch_result
                    .map(|group| group.added_by_inbox_id.clone())
                    .ok_or_else(|| GroupError::GroupNotFound)
            })
    }

    // Update this installation's leaf key in the group by creating a key update commit
    pub async fn key_update<ApiClient>(&self, client: &Client<ApiClient>) -> Result<(), GroupError>
    where
        ApiClient: XmtpApi,
    {
        let conn = self.context.store.conn()?;
        let intent = NewGroupIntent::new(IntentKind::KeyUpdate, self.group_id.clone(), vec![]);
        intent.store(&conn)?;

        self.sync_with_conn(conn, client).await
    }

    pub fn is_active(&self) -> Result<bool, GroupError> {
        let conn = self.context.store.conn()?;
        let provider = XmtpOpenMlsProvider::new(conn);
        let mls_group = self.load_mls_group(&provider)?;

        Ok(mls_group.is_active())
    }

    pub fn metadata(&self) -> Result<GroupMetadata, GroupError> {
        let conn = self.context.store.conn()?;
        let provider = XmtpOpenMlsProvider::new(conn);
        let mls_group = self.load_mls_group(&provider)?;

        Ok(extract_group_metadata(&mls_group)?)
    }

    pub fn mutable_metadata(&self) -> Result<GroupMutableMetadata, GroupError> {
        let conn = self.context.store.conn()?;
        let provider = XmtpOpenMlsProvider::new(conn);
        let mls_group = &self.load_mls_group(&provider)?;

        Ok(mls_group.try_into()?)
    }

    pub fn permissions(&self) -> Result<GroupMutablePermissions, GroupError> {
        let conn = self.context.store.conn()?;
        let provider = XmtpOpenMlsProvider::new(conn);
        let mls_group = self.load_mls_group(&provider)?;

        Ok(extract_group_permissions(&mls_group)?)
    }
}

fn extract_message_v1(message: GroupMessage) -> Result<GroupMessageV1, MessageProcessingError> {
    match message.version {
        Some(GroupMessageVersion::V1(value)) => Ok(value),
        _ => Err(MessageProcessingError::InvalidPayload),
    }
}

pub fn extract_group_id(message: &GroupMessage) -> Result<Vec<u8>, MessageProcessingError> {
    match &message.version {
        Some(GroupMessageVersion::V1(value)) => Ok(value.group_id.clone()),
        _ => Err(MessageProcessingError::InvalidPayload),
    }
}

fn build_protected_metadata_extension(
    identity: &Identity,
    group_purpose: Purpose,
) -> Result<Extension, GroupError> {
    let group_type = match group_purpose {
        Purpose::Conversation => ConversationType::Group,
        Purpose::Sync => ConversationType::Sync,
    };
    let metadata = GroupMetadata::new(group_type, identity.inbox_id().clone());
    let protected_metadata = Metadata::new(metadata.try_into()?);

    Ok(Extension::ImmutableMetadata(protected_metadata))
}

fn build_mutable_permissions_extension(policies: PolicySet) -> Result<Extension, GroupError> {
    let permissions: Vec<u8> = GroupMutablePermissions::new(policies).try_into()?;
    let unknown_gc_extension = UnknownExtension(permissions);

    Ok(Extension::Unknown(
        GROUP_PERMISSIONS_EXTENSION_ID,
        unknown_gc_extension,
    ))
}

pub fn build_mutable_metadata_extension_default(
    identity: &Identity,
    opts: GroupMetadataOptions,
) -> Result<Extension, GroupError> {
    let mutable_metadata: Vec<u8> =
        GroupMutableMetadata::new_default(identity.inbox_id.clone(), opts).try_into()?;
    let unknown_gc_extension = UnknownExtension(mutable_metadata);

    Ok(Extension::Unknown(
        MUTABLE_METADATA_EXTENSION_ID,
        unknown_gc_extension,
    ))
}

#[tracing::instrument(level = "trace", skip_all)]
pub fn build_extensions_for_metadata_update(
    group: &OpenMlsGroup,
    field_name: String,
    field_value: String,
) -> Result<Extensions, GroupError> {
    let existing_metadata: GroupMutableMetadata = group.try_into()?;
    let mut attributes = existing_metadata.attributes.clone();
    attributes.insert(field_name, field_value);
    let new_mutable_metadata: Vec<u8> = GroupMutableMetadata::new(
        attributes,
        existing_metadata.admin_list,
        existing_metadata.super_admin_list,
    )
    .try_into()?;
    let unknown_gc_extension = UnknownExtension(new_mutable_metadata);
    let extension = Extension::Unknown(MUTABLE_METADATA_EXTENSION_ID, unknown_gc_extension);
    let mut extensions = group.extensions().clone();
    extensions.add_or_replace(extension);
    Ok(extensions)
}

#[tracing::instrument(level = "trace", skip_all)]
pub fn build_extensions_for_permissions_update(
    group: &OpenMlsGroup,
    update_permissions_intent: UpdatePermissionIntentData,
) -> Result<Extensions, GroupError> {
    let existing_permissions: GroupMutablePermissions = group.try_into()?;
    let existing_policy_set = existing_permissions.policies.clone();
    let new_policy_set = match update_permissions_intent.update_type {
        PermissionUpdateType::AddMember => PolicySet::new(
            update_permissions_intent.policy_option.into(),
            existing_policy_set.remove_member_policy,
            existing_policy_set.update_metadata_policy,
            existing_policy_set.add_admin_policy,
            existing_policy_set.remove_admin_policy,
            existing_policy_set.update_permissions_policy,
        ),
        PermissionUpdateType::RemoveMember => PolicySet::new(
            existing_policy_set.add_member_policy,
            update_permissions_intent.policy_option.into(),
            existing_policy_set.update_metadata_policy,
            existing_policy_set.add_admin_policy,
            existing_policy_set.remove_admin_policy,
            existing_policy_set.update_permissions_policy,
        ),
        PermissionUpdateType::AddAdmin => PolicySet::new(
            existing_policy_set.add_member_policy,
            existing_policy_set.remove_member_policy,
            existing_policy_set.update_metadata_policy,
            update_permissions_intent.policy_option.into(),
            existing_policy_set.remove_admin_policy,
            existing_policy_set.update_permissions_policy,
        ),
        PermissionUpdateType::RemoveAdmin => PolicySet::new(
            existing_policy_set.add_member_policy,
            existing_policy_set.remove_member_policy,
            existing_policy_set.update_metadata_policy,
            existing_policy_set.add_admin_policy,
            update_permissions_intent.policy_option.into(),
            existing_policy_set.update_permissions_policy,
        ),
        PermissionUpdateType::UpdateMetadata => {
            let mut metadata_policy = existing_policy_set.update_metadata_policy.clone();
            metadata_policy.insert(
                update_permissions_intent.metadata_field_name.unwrap(),
                update_permissions_intent.policy_option.into(),
            );
            PolicySet::new(
                existing_policy_set.add_member_policy,
                existing_policy_set.remove_member_policy,
                metadata_policy,
                existing_policy_set.add_admin_policy,
                existing_policy_set.remove_admin_policy,
                existing_policy_set.update_permissions_policy,
            )
        }
    };
    let new_group_permissions: Vec<u8> = GroupMutablePermissions::new(new_policy_set).try_into()?;
    let unknown_gc_extension = UnknownExtension(new_group_permissions);
    let extension = Extension::Unknown(GROUP_PERMISSIONS_EXTENSION_ID, unknown_gc_extension);
    let mut extensions = group.extensions().clone();
    extensions.add_or_replace(extension);
    Ok(extensions)
}

#[tracing::instrument(level = "trace", skip_all)]
pub fn build_extensions_for_admin_lists_update(
    group: &OpenMlsGroup,
    admin_lists_update: UpdateAdminListIntentData,
) -> Result<Extensions, GroupError> {
    let existing_metadata: GroupMutableMetadata = group.try_into()?;
    let attributes = existing_metadata.attributes.clone();
    let mut admin_list = existing_metadata.admin_list;
    let mut super_admin_list = existing_metadata.super_admin_list;
    match admin_lists_update.action_type {
        AdminListActionType::Add => {
            if !admin_list.contains(&admin_lists_update.inbox_id) {
                admin_list.push(admin_lists_update.inbox_id);
            }
        }
        AdminListActionType::Remove => admin_list.retain(|x| x != &admin_lists_update.inbox_id),
        AdminListActionType::AddSuper => {
            if !super_admin_list.contains(&admin_lists_update.inbox_id) {
                super_admin_list.push(admin_lists_update.inbox_id);
            }
        }
        AdminListActionType::RemoveSuper => {
            super_admin_list.retain(|x| x != &admin_lists_update.inbox_id)
        }
    }
    let new_mutable_metadata: Vec<u8> =
        GroupMutableMetadata::new(attributes, admin_list, super_admin_list).try_into()?;
    let unknown_gc_extension = UnknownExtension(new_mutable_metadata);
    let extension = Extension::Unknown(MUTABLE_METADATA_EXTENSION_ID, unknown_gc_extension);
    let mut extensions = group.extensions().clone();
    extensions.add_or_replace(extension);
    Ok(extensions)
}

pub fn build_starting_group_membership_extension(inbox_id: String, sequence_id: u64) -> Extension {
    let mut group_membership = GroupMembership::new();
    group_membership.add(inbox_id, sequence_id);
    build_group_membership_extension(&group_membership)
}

pub fn build_group_membership_extension(group_membership: &GroupMembership) -> Extension {
    let unknown_gc_extension = UnknownExtension(group_membership.into());

    Extension::Unknown(GROUP_MEMBERSHIP_EXTENSION_ID, unknown_gc_extension)
}

fn build_group_config(
    protected_metadata_extension: Extension,
    mutable_metadata_extension: Extension,
    group_membership_extension: Extension,
    mutable_permission_extension: Extension,
) -> Result<MlsGroupCreateConfig, GroupError> {
    let required_extension_types = &[
        ExtensionType::Unknown(GROUP_MEMBERSHIP_EXTENSION_ID),
        ExtensionType::Unknown(MUTABLE_METADATA_EXTENSION_ID),
        ExtensionType::Unknown(GROUP_PERMISSIONS_EXTENSION_ID),
        ExtensionType::ImmutableMetadata,
        ExtensionType::LastResort,
        ExtensionType::ApplicationId,
    ];

    let required_proposal_types = &[ProposalType::GroupContextExtensions];

    let capabilities = Capabilities::new(
        None,
        None,
        Some(required_extension_types),
        Some(required_proposal_types),
        None,
    );
    let credentials = &[CredentialType::Basic];

    let required_capabilities =
        Extension::RequiredCapabilities(RequiredCapabilitiesExtension::new(
            required_extension_types,
            required_proposal_types,
            credentials,
        ));

    let extensions = Extensions::from_vec(vec![
        protected_metadata_extension,
        mutable_metadata_extension,
        group_membership_extension,
        mutable_permission_extension,
        required_capabilities,
    ])?;

    Ok(MlsGroupCreateConfig::builder()
        .with_group_context_extensions(extensions)?
        .capabilities(capabilities)
        .ciphersuite(CIPHERSUITE)
        .wire_format_policy(WireFormatPolicy::default())
        .max_past_epochs(3) // Trying with 3 max past epochs for now
        .use_ratchet_tree_extension(true)
        .build())
}

async fn validate_initial_group_membership<ApiClient: XmtpApi>(
    client: &Client<ApiClient>,
    conn: &DbConnection,
    mls_group: &OpenMlsGroup,
) -> Result<(), GroupError> {
    let membership = extract_group_membership(mls_group.extensions())?;
    let needs_update = client.filter_inbox_ids_needing_updates(conn, membership.to_filters())?;
    if !needs_update.is_empty() {
        load_identity_updates(&client.api_client, conn, needs_update).await?;
    }

    let mut expected_installation_ids = HashSet::<Vec<u8>>::new();

    let futures: Vec<_> = membership
        .members
        .into_iter()
        .map(|(inbox_id, sequence_id)| {
            client.get_association_state(conn, inbox_id, Some(sequence_id as i64))
        })
        .collect();

    let results = futures::future::try_join_all(futures).await?;

    for association_state in results {
        expected_installation_ids.extend(association_state.installation_ids());
    }

    let actual_installation_ids: HashSet<Vec<u8>> = mls_group
        .members()
        .map(|member| member.signature_key)
        .collect();

    if expected_installation_ids != actual_installation_ids {
        return Err(GroupError::InvalidGroupMembership);
    }

    Ok(())
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
    use openmls::prelude::{tls_codec::Serialize, Member, MlsGroup as OpenMlsGroup};
    use prost::Message;
    use std::sync::Arc;
    use tracing_test::traced_test;
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

    use crate::{
        assert_logged,
        builder::ClientBuilder,
        codecs::{group_updated::GroupUpdatedCodec, ContentCodec},
        groups::{
            build_group_membership_extension,
            group_membership::GroupMembership,
            group_metadata::{ConversationType, GroupMetadata},
            group_mutable_metadata::MetadataField,
            intents::{PermissionPolicyOption, PermissionUpdateType},
            members::{GroupMember, PermissionLevel},
            DeliveryStatus, GroupMetadataOptions, PreconfiguredPolicies, UpdateAdminListType,
        },
        storage::{
            group_intent::IntentState,
            group_message::{GroupMessageKind, StoredGroupMessage},
        },
        xmtp_openmls_provider::XmtpOpenMlsProvider,
        Client, InboxOwner, XmtpApi,
    };

    use super::{
        intents::{Installation, SendWelcomesAction},
        MlsGroup,
    };

    async fn receive_group_invite<ApiClient>(client: &Client<ApiClient>) -> MlsGroup
    where
        ApiClient: XmtpApi,
    {
        client.sync_welcomes().await.unwrap();
        let mut groups = client.find_groups(None, None, None, None).unwrap();

        groups.remove(0)
    }

    async fn get_latest_message<ApiClient>(
        group: &MlsGroup,
        client: &Client<ApiClient>,
    ) -> StoredGroupMessage
    where
        ApiClient: XmtpApi,
    {
        group.sync(client).await.unwrap();
        let mut messages = group.find_messages(None, None, None, None, None).unwrap();
        messages.pop().unwrap()
    }

    // Adds a member to the group without the usual validations on group membership
    // Used for testing adversarial scenarios
    async fn force_add_member<ApiClient: XmtpApi>(
        sender_client: &Client<ApiClient>,
        new_member_client: &Client<ApiClient>,
        sender_group: &MlsGroup,
        sender_mls_group: &mut OpenMlsGroup,
        sender_provider: &XmtpOpenMlsProvider,
    ) {
        let new_member_provider =
            new_member_client.mls_provider(new_member_client.store().conn().unwrap());

        let key_package = new_member_client
            .identity()
            .new_key_package(&new_member_provider)
            .unwrap();
        let hpke_init_key = key_package.hpke_init_key().as_slice().to_vec();
        let (commit, welcome, _) = sender_mls_group
            .add_members(
                sender_provider,
                &sender_client.identity().installation_keys,
                &[key_package],
            )
            .unwrap();
        let serialized_commit = commit.tls_serialize_detached().unwrap();
        let serialized_welcome = welcome.tls_serialize_detached().unwrap();
        let send_welcomes_action = SendWelcomesAction::new(
            vec![Installation {
                installation_key: new_member_client.installation_public_key(),
                hpke_public_key: hpke_init_key,
            }],
            serialized_welcome,
        );
        sender_client
            .api_client
            .send_group_messages(vec![serialized_commit.as_slice()])
            .await
            .unwrap();
        sender_group
            .send_welcomes(send_welcomes_action, sender_client)
            .await
            .unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_send_message() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;
        let group = client
            .create_group(None, GroupMetadataOptions::default())
            .expect("create group");
        group
            .send_message(b"hello", &client)
            .await
            .expect("send message");

        let messages = client
            .api_client
            .query_group_messages(group.group_id, None)
            .await
            .expect("read topic");
        assert_eq!(messages.len(), 2);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_receive_self_message() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;
        let group = client
            .create_group(None, GroupMetadataOptions::default())
            .expect("create group");
        let msg = b"hello";
        group
            .send_message(msg, &client)
            .await
            .expect("send message");

        group
            .receive(&client.store().conn().unwrap(), &client)
            .await
            .unwrap();
        // Check for messages
        let messages = group.find_messages(None, None, None, None, None).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages.first().unwrap().decrypted_message_bytes, msg);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_receive_message_from_other() {
        let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let alix_group = alix
            .create_group(None, GroupMetadataOptions::default())
            .expect("create group");
        alix_group
            .add_members_by_inbox_id(&alix, vec![bo.inbox_id()])
            .await
            .unwrap();
        let alix_message = b"hello from alix";
        alix_group
            .send_message(alix_message, &alix)
            .await
            .expect("send message");

        let bo_group = receive_group_invite(&bo).await;
        let message = get_latest_message(&bo_group, &bo).await;
        assert_eq!(message.decrypted_message_bytes, alix_message);

        let bo_message = b"hello from bo";
        bo_group
            .send_message(bo_message, &bo)
            .await
            .expect("send message");

        let message = get_latest_message(&alix_group, &alix).await;
        assert_eq!(message.decrypted_message_bytes, bo_message);
    }

    // Test members function from non group creator
    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_members_func_from_non_creator() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let amal_group = amal
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        amal_group
            .add_members_by_inbox_id(&amal, vec![bola.inbox_id()])
            .await
            .unwrap();

        // Get bola's version of the same group
        let bola_groups = bola.sync_welcomes().await.unwrap();
        let bola_group = bola_groups.first().unwrap();

        // Call sync for both
        amal_group.sync(&amal).await.unwrap();
        bola_group.sync(&bola).await.unwrap();

        // Verify bola can see the group name
        let bola_group_name = bola_group.group_name().unwrap();
        assert_eq!(bola_group_name, "");

        // Check if both clients can see the members correctly
        let amal_members: Vec<GroupMember> = amal_group.members().unwrap();
        let bola_members: Vec<GroupMember> = bola_group.members().unwrap();

        assert_eq!(amal_members.len(), 2);
        assert_eq!(bola_members.len(), 2);

        for member in &amal_members {
            if member.inbox_id == amal.inbox_id() {
                assert_eq!(
                    member.permission_level,
                    PermissionLevel::SuperAdmin,
                    "Amal should be a super admin"
                );
            } else if member.inbox_id == bola.inbox_id() {
                assert_eq!(
                    member.permission_level,
                    PermissionLevel::Member,
                    "Bola should be a member"
                );
            }
        }
    }

    // Amal and Bola will both try and add Charlie from the same epoch.
    // The group should resolve to a consistent state
    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_add_member_conflict() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let charlie = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let amal_group = amal
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        // Add bola
        amal_group
            .add_members_by_inbox_id(&amal, vec![bola.inbox_id()])
            .await
            .unwrap();

        // Get bola's version of the same group
        let bola_groups = bola.sync_welcomes().await.unwrap();
        let bola_group = bola_groups.first().unwrap();
        bola_group.sync(&bola).await.unwrap();

        log::info!("Adding charlie from amal");
        // Have amal and bola both invite charlie.
        amal_group
            .add_members_by_inbox_id(&amal, vec![charlie.inbox_id()])
            .await
            .expect("failed to add charlie");
        log::info!("Adding charlie from bola");
        bola_group
            .add_members_by_inbox_id(&bola, vec![charlie.inbox_id()])
            .await
            .expect_err("expected error");

        amal_group
            .receive(&amal.store().conn().unwrap(), &amal)
            .await
            .expect_err("expected error");

        // Check Amal's MLS group state.
        let amal_db = amal.context.store.conn().unwrap();
        let amal_mls_group = amal_group
            .load_mls_group(amal.mls_provider(amal_db.clone()))
            .unwrap();
        let amal_members: Vec<Member> = amal_mls_group.members().collect();
        assert_eq!(amal_members.len(), 3);

        // Check Bola's MLS group state.
        let bola_db = bola.context.store.conn().unwrap();
        let bola_mls_group = bola_group
            .load_mls_group(bola.mls_provider(bola_db.clone()))
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

        let bola_failed_intents = bola_db
            .find_group_intents(
                bola_group.group_id.clone(),
                Some(vec![IntentState::Error]),
                None,
            )
            .unwrap();
        // Bola should have one uncommitted intent in `Error::Failed` state for the failed attempt at adding Charlie, who is already in the group
        assert_eq!(bola_failed_intents.len(), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    #[traced_test]
    async fn test_create_from_welcome_validation() {
        let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let alix_group: MlsGroup = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        let provider = alix.mls_provider(alix.store().conn().unwrap());
        // Doctor the group membership
        let mut mls_group = alix_group.load_mls_group(&provider).unwrap();
        let mut existing_extensions = mls_group.extensions().clone();
        let mut group_membership = GroupMembership::new();
        group_membership.add("foo".to_string(), 1);
        existing_extensions.add_or_replace(build_group_membership_extension(&group_membership));
        mls_group
            .update_group_context_extensions(
                &provider,
                existing_extensions.clone(),
                &alix.identity().installation_keys,
            )
            .unwrap();
        mls_group.merge_pending_commit(&provider).unwrap();

        // Now add bo to the group
        force_add_member(&alix, &bo, &alix_group, &mut mls_group, &provider).await;

        // Bo should not be able to actually read this group
        bo.sync_welcomes().await.unwrap();
        let groups = bo.find_groups(None, None, None, None).unwrap();
        assert_eq!(groups.len(), 0);
        assert_logged!("failed to create group from welcome", 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_add_inbox() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let client_2 = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let group = client
            .create_group(None, GroupMetadataOptions::default())
            .expect("create group");

        group
            .add_members_by_inbox_id(&client, vec![client_2.inbox_id()])
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

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_add_invalid_member() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let group = client
            .create_group(None, GroupMetadataOptions::default())
            .expect("create group");

        let result = group
            .add_members_by_inbox_id(&client, vec!["1234".to_string()])
            .await;

        assert!(result.is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_add_unregistered_member() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let unconnected_wallet_address = generate_local_wallet().get_address();
        let group = amal
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        let result = group
            .add_members(&amal, vec![unconnected_wallet_address])
            .await;

        assert!(result.is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_remove_inbox() {
        let client_1 = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        // Add another client onto the network
        let client_2 = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let group = client_1
            .create_group(None, GroupMetadataOptions::default())
            .expect("create group");
        group
            .add_members_by_inbox_id(&client_1, vec![client_2.inbox_id()])
            .await
            .expect("group create failure");

        let messages_with_add = group.find_messages(None, None, None, None, None).unwrap();
        assert_eq!(messages_with_add.len(), 1);

        // Try and add another member without merging the pending commit
        group
            .remove_members_by_inbox_id(&client_1, vec![client_2.inbox_id()])
            .await
            .expect("group remove members failure");

        let messages_with_remove = group.find_messages(None, None, None, None, None).unwrap();
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

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_key_update() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola_client = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let group = client
            .create_group(None, GroupMetadataOptions::default())
            .expect("create group");
        group
            .add_members_by_inbox_id(&client, vec![bola_client.inbox_id()])
            .await
            .unwrap();

        group.key_update(&client).await.unwrap();

        let messages = client
            .api_client
            .query_group_messages(group.group_id.clone(), None)
            .await
            .unwrap();
        assert_eq!(messages.len(), 2);

        let conn = &client.context.store.conn().unwrap();
        let provider = super::XmtpOpenMlsProvider::new(conn.clone());
        let mls_group = group.load_mls_group(&provider).unwrap();
        let pending_commit = mls_group.pending_commit();
        assert!(pending_commit.is_none());

        group
            .send_message(b"hello", &client)
            .await
            .expect("send message");

        bola_client.sync_welcomes().await.unwrap();
        let bola_groups = bola_client.find_groups(None, None, None, None).unwrap();
        let bola_group = bola_groups.first().unwrap();
        bola_group.sync(&bola_client).await.unwrap();
        let bola_messages = bola_group
            .find_messages(None, None, None, None, None)
            .unwrap();
        assert_eq!(bola_messages.len(), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_post_commit() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let client_2 = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let group = client
            .create_group(None, GroupMetadataOptions::default())
            .expect("create group");

        group
            .add_members_by_inbox_id(&client, vec![client_2.inbox_id()])
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

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_remove_by_account_address() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola_wallet = &generate_local_wallet();
        let bola = ClientBuilder::new_test_client(bola_wallet).await;
        let charlie_wallet = &generate_local_wallet();
        let _charlie = ClientBuilder::new_test_client(charlie_wallet).await;

        let group = amal
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        group
            .add_members(
                &amal,
                vec![bola_wallet.get_address(), charlie_wallet.get_address()],
            )
            .await
            .unwrap();
        log::info!("created the group with 2 additional members");
        assert_eq!(group.members().unwrap().len(), 3);
        let messages = group.find_messages(None, None, None, None, None).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].kind, GroupMessageKind::MembershipChange);
        let encoded_content =
            EncodedContent::decode(messages[0].decrypted_message_bytes.as_slice()).unwrap();
        let group_update = GroupUpdatedCodec::decode(encoded_content).unwrap();
        assert_eq!(group_update.added_inboxes.len(), 2);
        assert_eq!(group_update.removed_inboxes.len(), 0);

        group
            .remove_members(&amal, vec![bola_wallet.get_address()])
            .await
            .unwrap();
        assert_eq!(group.members().unwrap().len(), 2);
        log::info!("removed bola");
        let messages = group.find_messages(None, None, None, None, None).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[1].kind, GroupMessageKind::MembershipChange);
        let encoded_content =
            EncodedContent::decode(messages[1].decrypted_message_bytes.as_slice()).unwrap();
        let group_update = GroupUpdatedCodec::decode(encoded_content).unwrap();
        assert_eq!(group_update.added_inboxes.len(), 0);
        assert_eq!(group_update.removed_inboxes.len(), 1);

        let bola_group = receive_group_invite(&bola).await;
        bola_group.sync(&bola).await.unwrap();
        assert!(!bola_group.is_active().unwrap())
    }

    // TODO:nm add more tests for filling in missing installations

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_add_missing_installations() {
        // Setup for test
        let amal_wallet = generate_local_wallet();
        let amal = ClientBuilder::new_test_client(&amal_wallet).await;
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let group = amal
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        group
            .add_members_by_inbox_id(&amal, vec![bola.inbox_id()])
            .await
            .unwrap();

        assert_eq!(group.members().unwrap().len(), 2);

        let conn = &amal.context.store.conn().unwrap();
        let provider = super::XmtpOpenMlsProvider::new(conn.clone());
        // Finished with setup

        // add a second installation for amal using the same wallet
        let _amal_2nd = ClientBuilder::new_test_client(&amal_wallet).await;

        // test if adding the new installation(s) worked
        let new_installations_were_added = group.add_missing_installations(&provider, &amal).await;
        assert!(new_installations_were_added.is_ok());

        group.sync(&amal).await.unwrap();
        let mls_group = group.load_mls_group(&provider).unwrap();
        let num_members = mls_group.members().collect::<Vec<_>>().len();
        assert_eq!(num_members, 3);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 10)]
    async fn test_self_resolve_epoch_mismatch() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let charlie = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let dave_wallet = generate_local_wallet();
        let dave = ClientBuilder::new_test_client(&dave_wallet).await;
        let amal_group = amal
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        // Add bola to the group
        amal_group
            .add_members_by_inbox_id(&amal, vec![bola.inbox_id()])
            .await
            .unwrap();

        let bola_group = receive_group_invite(&bola).await;
        bola_group.sync(&bola).await.unwrap();
        // Both Amal and Bola are up to date on the group state. Now each of them want to add someone else
        amal_group
            .add_members_by_inbox_id(&amal, vec![charlie.inbox_id()])
            .await
            .unwrap();

        bola_group
            .add_members_by_inbox_id(&bola, vec![dave.inbox_id()])
            .await
            .unwrap();

        // Send a message to the group, now that everyone is invited
        amal_group.sync(&amal).await.unwrap();
        amal_group.send_message(b"hello", &amal).await.unwrap();

        let charlie_group = receive_group_invite(&charlie).await;
        let dave_group = receive_group_invite(&dave).await;

        let (amal_latest_message, bola_latest_message, charlie_latest_message, dave_latest_message) = tokio::join!(
            get_latest_message(&amal_group, &amal),
            get_latest_message(&bola_group, &bola),
            get_latest_message(&charlie_group, &charlie),
            get_latest_message(&dave_group, &dave)
        );

        let expected_latest_message = b"hello".to_vec();
        assert!(expected_latest_message.eq(&amal_latest_message.decrypted_message_bytes));
        assert!(expected_latest_message.eq(&bola_latest_message.decrypted_message_bytes));
        assert!(expected_latest_message.eq(&charlie_latest_message.decrypted_message_bytes));
        assert!(expected_latest_message.eq(&dave_latest_message.decrypted_message_bytes));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_group_permissions() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let charlie = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let amal_group = amal
            .create_group(
                Some(PreconfiguredPolicies::AdminsOnly.to_policy_set()),
                GroupMetadataOptions::default(),
            )
            .unwrap();
        // Add bola to the group
        amal_group
            .add_members_by_inbox_id(&amal, vec![bola.inbox_id()])
            .await
            .unwrap();

        let bola_group = receive_group_invite(&bola).await;
        bola_group.sync(&bola).await.unwrap();
        assert!(bola_group
            .add_members_by_inbox_id(&bola, vec![charlie.inbox_id()])
            .await
            .is_err(),);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_group_options() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let amal_group = amal
            .create_group(
                None,
                GroupMetadataOptions {
                    name: Some("Group Name".to_string()),
                    image_url_square: Some("url".to_string()),
                    description: Some("group description".to_string()),
                    pinned_frame_url: Some("pinned frame".to_string()),
                },
            )
            .unwrap();

        let binding = amal_group.mutable_metadata().expect("msg");
        let amal_group_name: &String = binding
            .attributes
            .get(&MetadataField::GroupName.to_string())
            .unwrap();
        let amal_group_image_url: &String = binding
            .attributes
            .get(&MetadataField::GroupImageUrlSquare.to_string())
            .unwrap();
        let amal_group_description: &String = binding
            .attributes
            .get(&MetadataField::Description.to_string())
            .unwrap();
        let amal_group_pinned_frame_url: &String = binding
            .attributes
            .get(&MetadataField::GroupPinnedFrameUrl.to_string())
            .unwrap();

        assert_eq!(amal_group_name, "Group Name");
        assert_eq!(amal_group_image_url, "url");
        assert_eq!(amal_group_description, "group description");
        assert_eq!(amal_group_pinned_frame_url, "pinned frame");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    // TODO: Need to enforce limits on max wallets on `add_members_by_inbox_id` and break up
    // requests into multiple transactions
    #[ignore]
    async fn test_max_limit_add() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let amal_group = amal
            .create_group(
                Some(PreconfiguredPolicies::AdminsOnly.to_policy_set()),
                GroupMetadataOptions::default(),
            )
            .unwrap();
        let mut clients = Vec::new();
        for _ in 0..249 {
            let wallet = generate_local_wallet();
            ClientBuilder::new_test_client(&wallet).await;
            clients.push(wallet.get_address());
        }
        amal_group.add_members(&amal, clients).await.unwrap();
        let bola_wallet = generate_local_wallet();
        ClientBuilder::new_test_client(&bola_wallet).await;
        assert!(amal_group
            .add_members_by_inbox_id(&amal, vec![bola_wallet.get_address()])
            .await
            .is_err(),);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_group_mutable_data() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        // Create a group and verify it has the default group name
        let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
        let amal_group: MlsGroup = amal
            .create_group(policy_set, GroupMetadataOptions::default())
            .unwrap();
        amal_group.sync(&amal).await.unwrap();

        let group_mutable_metadata = amal_group.mutable_metadata().unwrap();
        assert!(group_mutable_metadata.attributes.len().eq(&4));
        assert!(group_mutable_metadata
            .attributes
            .get(&MetadataField::GroupName.to_string())
            .unwrap()
            .eq(""));

        // Add bola to the group
        amal_group
            .add_members_by_inbox_id(&amal, vec![bola.inbox_id()])
            .await
            .unwrap();
        bola.sync_welcomes().await.unwrap();
        let bola_groups = bola.find_groups(None, None, None, None).unwrap();
        assert_eq!(bola_groups.len(), 1);
        let bola_group = bola_groups.first().unwrap();
        bola_group.sync(&bola).await.unwrap();
        let group_mutable_metadata = bola_group.mutable_metadata().unwrap();
        assert!(group_mutable_metadata
            .attributes
            .get(&MetadataField::GroupName.to_string())
            .unwrap()
            .eq(""));

        // Update group name
        amal_group
            .update_group_name(&amal, "New Group Name 1".to_string())
            .await
            .unwrap();

        amal_group
            .send_message("hello".as_bytes(), &amal)
            .await
            .unwrap();

        // Verify amal group sees update
        amal_group.sync(&amal).await.unwrap();
        let binding = amal_group.mutable_metadata().expect("msg");
        let amal_group_name: &String = binding
            .attributes
            .get(&MetadataField::GroupName.to_string())
            .unwrap();
        assert_eq!(amal_group_name, "New Group Name 1");

        // Verify bola group sees update
        bola_group.sync(&bola).await.unwrap();
        let binding = bola_group.mutable_metadata().expect("msg");
        let bola_group_name: &String = binding
            .attributes
            .get(&MetadataField::GroupName.to_string())
            .unwrap();
        assert_eq!(bola_group_name, "New Group Name 1");

        // Verify that bola can not update the group name since they are not the creator
        bola_group
            .update_group_name(&bola, "New Group Name 2".to_string())
            .await
            .expect_err("expected err");

        // Verify bola group does not see an update
        bola_group.sync(&bola).await.unwrap();
        let binding = bola_group.mutable_metadata().expect("msg");
        let bola_group_name: &String = binding
            .attributes
            .get(&MetadataField::GroupName.to_string())
            .unwrap();
        assert_eq!(bola_group_name, "New Group Name 1");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_update_group_image_url_square() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        // Create a group and verify it has the default group name
        let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
        let amal_group: MlsGroup = amal
            .create_group(policy_set, GroupMetadataOptions::default())
            .unwrap();
        amal_group.sync(&amal).await.unwrap();

        let group_mutable_metadata = amal_group.mutable_metadata().unwrap();
        assert!(group_mutable_metadata
            .attributes
            .get(&MetadataField::GroupImageUrlSquare.to_string())
            .unwrap()
            .eq(""));

        // Update group name
        amal_group
            .update_group_image_url_square(&amal, "a url".to_string())
            .await
            .unwrap();

        // Verify amal group sees update
        amal_group.sync(&amal).await.unwrap();
        let binding = amal_group.mutable_metadata().expect("msg");
        let amal_group_image_url: &String = binding
            .attributes
            .get(&MetadataField::GroupImageUrlSquare.to_string())
            .unwrap();
        assert_eq!(amal_group_image_url, "a url");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_update_group_pinned_frame_url() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        // Create a group and verify it has the default group name
        let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
        let amal_group: MlsGroup = amal
            .create_group(policy_set, GroupMetadataOptions::default())
            .unwrap();
        amal_group.sync(&amal).await.unwrap();

        let group_mutable_metadata = amal_group.mutable_metadata().unwrap();
        assert!(group_mutable_metadata
            .attributes
            .get(&MetadataField::GroupPinnedFrameUrl.to_string())
            .unwrap()
            .eq(""));

        // Update group name
        amal_group
            .update_group_pinned_frame_url(&amal, "a frame url".to_string())
            .await
            .unwrap();

        // Verify amal group sees update
        amal_group.sync(&amal).await.unwrap();
        let binding = amal_group.mutable_metadata().expect("msg");
        let amal_group_pinned_frame_url: &String = binding
            .attributes
            .get(&MetadataField::GroupPinnedFrameUrl.to_string())
            .unwrap();
        assert_eq!(amal_group_pinned_frame_url, "a frame url");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_group_mutable_data_group_permissions() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola_wallet = generate_local_wallet();
        let bola = ClientBuilder::new_test_client(&bola_wallet).await;

        // Create a group and verify it has the default group name
        let policy_set = Some(PreconfiguredPolicies::AllMembers.to_policy_set());
        let amal_group: MlsGroup = amal
            .create_group(policy_set, GroupMetadataOptions::default())
            .unwrap();
        amal_group.sync(&amal).await.unwrap();

        let group_mutable_metadata = amal_group.mutable_metadata().unwrap();
        assert!(group_mutable_metadata
            .attributes
            .get(&MetadataField::GroupName.to_string())
            .unwrap()
            .eq(""));

        // Add bola to the group
        amal_group
            .add_members(&amal, vec![bola_wallet.get_address()])
            .await
            .unwrap();
        bola.sync_welcomes().await.unwrap();
        let bola_groups = bola.find_groups(None, None, None, None).unwrap();
        assert_eq!(bola_groups.len(), 1);
        let bola_group = bola_groups.first().unwrap();
        bola_group.sync(&bola).await.unwrap();
        let group_mutable_metadata = bola_group.mutable_metadata().unwrap();
        assert!(group_mutable_metadata
            .attributes
            .get(&MetadataField::GroupName.to_string())
            .unwrap()
            .eq(""));

        // Update group name
        amal_group
            .update_group_name(&amal, "New Group Name 1".to_string())
            .await
            .unwrap();

        // Verify amal group sees update
        amal_group.sync(&amal).await.unwrap();
        let binding = amal_group.mutable_metadata().unwrap();
        let amal_group_name: &String = binding
            .attributes
            .get(&MetadataField::GroupName.to_string())
            .unwrap();
        assert_eq!(amal_group_name, "New Group Name 1");

        // Verify bola group sees update
        bola_group.sync(&bola).await.unwrap();
        let binding = bola_group.mutable_metadata().expect("msg");
        let bola_group_name: &String = binding
            .attributes
            .get(&MetadataField::GroupName.to_string())
            .unwrap();
        assert_eq!(bola_group_name, "New Group Name 1");

        // Verify that bola CAN update the group name since everyone is admin for this group
        bola_group
            .update_group_name(&bola, "New Group Name 2".to_string())
            .await
            .expect("non creator failed to udpate group name");

        // Verify amal group sees an update
        amal_group.sync(&amal).await.unwrap();
        let binding = amal_group.mutable_metadata().expect("msg");
        let amal_group_name: &String = binding
            .attributes
            .get(&MetadataField::GroupName.to_string())
            .unwrap();
        assert_eq!(amal_group_name, "New Group Name 2");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_group_admin_list_update() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola_wallet = generate_local_wallet();
        let bola = ClientBuilder::new_test_client(&bola_wallet).await;
        let caro = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let charlie = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
        let amal_group = amal
            .create_group(policy_set, GroupMetadataOptions::default())
            .unwrap();
        amal_group.sync(&amal).await.unwrap();

        // Add bola to the group
        amal_group
            .add_members(&amal, vec![bola_wallet.get_address()])
            .await
            .unwrap();
        bola.sync_welcomes().await.unwrap();
        let bola_groups = bola.find_groups(None, None, None, None).unwrap();
        assert_eq!(bola_groups.len(), 1);
        let bola_group = bola_groups.first().unwrap();
        bola_group.sync(&bola).await.unwrap();

        // Verify Amal is the only admin and super admin
        let admin_list = amal_group.admin_list().unwrap();
        let super_admin_list = amal_group.super_admin_list().unwrap();
        assert_eq!(admin_list.len(), 0);
        assert_eq!(super_admin_list.len(), 1);
        assert!(super_admin_list.contains(&amal.inbox_id()));

        // Verify that bola can not add caro because they are not an admin
        bola.sync_welcomes().await.unwrap();
        let bola_groups = bola.find_groups(None, None, None, None).unwrap();
        assert_eq!(bola_groups.len(), 1);
        let bola_group: &MlsGroup = bola_groups.first().unwrap();
        bola_group.sync(&bola).await.unwrap();
        bola_group
            .add_members_by_inbox_id(&bola, vec![caro.inbox_id()])
            .await
            .expect_err("expected err");

        // Add bola as an admin
        amal_group
            .update_admin_list(&amal, UpdateAdminListType::Add, bola.inbox_id())
            .await
            .unwrap();
        amal_group.sync(&amal).await.unwrap();
        bola_group.sync(&bola).await.unwrap();
        assert_eq!(bola_group.admin_list().unwrap().len(), 1);
        assert!(bola_group.admin_list().unwrap().contains(&bola.inbox_id()));

        // Verify that bola can now add caro because they are an admin
        bola_group
            .add_members_by_inbox_id(&bola, vec![caro.inbox_id()])
            .await
            .unwrap();

        bola_group.sync(&bola).await.unwrap();

        // Verify that bola can not remove amal as a super admin, because
        // Remove admin is super admin only permissions
        bola_group
            .update_admin_list(&bola, UpdateAdminListType::RemoveSuper, amal.inbox_id())
            .await
            .expect_err("expected err");

        // Now amal removes bola as an admin
        amal_group
            .update_admin_list(&amal, UpdateAdminListType::Remove, bola.inbox_id())
            .await
            .unwrap();
        amal_group.sync(&amal).await.unwrap();
        bola_group.sync(&bola).await.unwrap();
        assert_eq!(bola_group.admin_list().unwrap().len(), 0);
        assert!(!bola_group.admin_list().unwrap().contains(&bola.inbox_id()));

        // Verify that bola can not add charlie because they are not an admin
        bola.sync_welcomes().await.unwrap();
        let bola_groups = bola.find_groups(None, None, None, None).unwrap();
        assert_eq!(bola_groups.len(), 1);
        let bola_group: &MlsGroup = bola_groups.first().unwrap();
        bola_group.sync(&bola).await.unwrap();
        bola_group
            .add_members_by_inbox_id(&bola, vec![charlie.inbox_id()])
            .await
            .expect_err("expected err");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_group_super_admin_list_update() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let caro = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
        let amal_group = amal
            .create_group(policy_set, GroupMetadataOptions::default())
            .unwrap();
        amal_group.sync(&amal).await.unwrap();

        // Add bola to the group
        amal_group
            .add_members_by_inbox_id(&amal, vec![bola.inbox_id()])
            .await
            .unwrap();
        bola.sync_welcomes().await.unwrap();
        let bola_groups = bola.find_groups(None, None, None, None).unwrap();
        assert_eq!(bola_groups.len(), 1);
        let bola_group = bola_groups.first().unwrap();
        bola_group.sync(&bola).await.unwrap();

        // Verify Amal is the only super admin
        let admin_list = amal_group.admin_list().unwrap();
        let super_admin_list = amal_group.super_admin_list().unwrap();
        assert_eq!(admin_list.len(), 0);
        assert_eq!(super_admin_list.len(), 1);
        assert!(super_admin_list.contains(&amal.inbox_id()));

        // Verify that bola can not add caro as an admin because they are not a super admin
        bola.sync_welcomes().await.unwrap();
        let bola_groups = bola.find_groups(None, None, None, None).unwrap();
        assert_eq!(bola_groups.len(), 1);
        let bola_group: &MlsGroup = bola_groups.first().unwrap();
        bola_group.sync(&bola).await.unwrap();
        bola_group
            .update_admin_list(&bola, UpdateAdminListType::Add, caro.inbox_id())
            .await
            .expect_err("expected err");

        // Add bola as a super admin
        amal_group
            .update_admin_list(&amal, UpdateAdminListType::AddSuper, bola.inbox_id())
            .await
            .unwrap();
        amal_group.sync(&amal).await.unwrap();
        bola_group.sync(&bola).await.unwrap();
        assert_eq!(bola_group.super_admin_list().unwrap().len(), 2);
        assert!(bola_group
            .super_admin_list()
            .unwrap()
            .contains(&bola.inbox_id()));

        // Verify that bola can now add caro as an admin
        bola_group
            .update_admin_list(&bola, UpdateAdminListType::Add, caro.inbox_id())
            .await
            .unwrap();
        bola_group.sync(&bola).await.unwrap();
        assert_eq!(bola_group.admin_list().unwrap().len(), 1);
        assert!(bola_group.admin_list().unwrap().contains(&caro.inbox_id()));

        // Verify that no one can remove a super admin from a group
        amal_group
            .remove_members(&amal, vec![bola.inbox_id()])
            .await
            .expect_err("expected err");

        // Verify that bola can now remove themself as a super admin
        bola_group
            .update_admin_list(&bola, UpdateAdminListType::RemoveSuper, bola.inbox_id())
            .await
            .unwrap();
        bola_group.sync(&bola).await.unwrap();
        assert_eq!(bola_group.super_admin_list().unwrap().len(), 1);
        assert!(!bola_group
            .super_admin_list()
            .unwrap()
            .contains(&bola.inbox_id()));

        // Verify that amal can NOT remove themself as a super admin because they are the only remaining
        amal_group
            .update_admin_list(&amal, UpdateAdminListType::RemoveSuper, amal.inbox_id())
            .await
            .expect_err("expected err");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_group_members_permission_level_update() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let caro = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
        let amal_group = amal
            .create_group(policy_set, GroupMetadataOptions::default())
            .unwrap();
        amal_group.sync(&amal).await.unwrap();

        // Add Bola and Caro to the group
        amal_group
            .add_members_by_inbox_id(&amal, vec![bola.inbox_id(), caro.inbox_id()])
            .await
            .unwrap();
        amal_group.sync(&amal).await.unwrap();

        // Initial checks for group members
        let initial_members = amal_group.members().unwrap();
        let mut count_member = 0;
        let mut count_admin = 0;
        let mut count_super_admin = 0;

        for member in &initial_members {
            match member.permission_level {
                PermissionLevel::Member => count_member += 1,
                PermissionLevel::Admin => count_admin += 1,
                PermissionLevel::SuperAdmin => count_super_admin += 1,
            }
        }

        assert_eq!(
            count_super_admin, 1,
            "Only Amal should be super admin initially"
        );
        assert_eq!(count_admin, 0, "no members are admin only");
        assert_eq!(count_member, 2, "two members have no admin status");

        // Add Bola as an admin
        amal_group
            .update_admin_list(&amal, UpdateAdminListType::Add, bola.inbox_id())
            .await
            .unwrap();
        amal_group.sync(&amal).await.unwrap();

        // Check after adding Bola as an admin
        let members = amal_group.members().unwrap();
        let mut count_member = 0;
        let mut count_admin = 0;
        let mut count_super_admin = 0;

        for member in &members {
            match member.permission_level {
                PermissionLevel::Member => count_member += 1,
                PermissionLevel::Admin => count_admin += 1,
                PermissionLevel::SuperAdmin => count_super_admin += 1,
            }
        }

        assert_eq!(
            count_super_admin, 1,
            "Only Amal should be super admin initially"
        );
        assert_eq!(count_admin, 1, "bola is admin");
        assert_eq!(count_member, 1, "caro has no admin status");

        // Add Caro as a super admin
        amal_group
            .update_admin_list(&amal, UpdateAdminListType::AddSuper, caro.inbox_id())
            .await
            .unwrap();
        amal_group.sync(&amal).await.unwrap();

        // Check after adding Caro as a super admin
        let members = amal_group.members().unwrap();
        let mut count_member = 0;
        let mut count_admin = 0;
        let mut count_super_admin = 0;

        for member in &members {
            match member.permission_level {
                PermissionLevel::Member => count_member += 1,
                PermissionLevel::Admin => count_admin += 1,
                PermissionLevel::SuperAdmin => count_super_admin += 1,
            }
        }

        assert_eq!(
            count_super_admin, 2,
            "Amal and Caro should be super admin initially"
        );
        assert_eq!(count_admin, 1, "bola is admin");
        assert_eq!(count_member, 0, "no members have no admin status");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_staged_welcome() {
        // Create Clients
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        // Amal creates a group
        let amal_group = amal
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();

        // Amal adds Bola to the group
        amal_group
            .add_members_by_inbox_id(&amal, vec![bola.inbox_id()])
            .await
            .unwrap();

        // Bola syncs groups - this will decrypt the Welcome, identify who added Bola
        // and then store that value on the group and insert into the database
        let bola_groups = bola.sync_welcomes().await.unwrap();

        // Bola gets the group id. This will be needed to fetch the group from
        // the database.
        let bola_group = bola_groups.first().unwrap();
        let bola_group_id = bola_group.group_id.clone();

        // Bola fetches group from the database
        let bola_fetched_group = bola.group(bola_group_id).unwrap();

        // Check Bola's group for the added_by_inbox_id of the inviter
        let added_by_inbox = bola_fetched_group.added_by_inbox_id().unwrap();

        // Verify the welcome host_credential is equal to Amal's
        assert_eq!(
            amal.inbox_id(),
            added_by_inbox,
            "The Inviter and added_by_address do not match!"
        );
    }

    #[tokio::test]
    async fn test_can_read_group_creator_inbox_id() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let policy_set = Some(PreconfiguredPolicies::AllMembers.to_policy_set());
        let amal_group = amal
            .create_group(policy_set, GroupMetadataOptions::default())
            .unwrap();
        amal_group.sync(&amal).await.unwrap();

        let mutable_metadata = amal_group.mutable_metadata().unwrap();
        assert_eq!(mutable_metadata.super_admin_list.len(), 1);
        assert_eq!(mutable_metadata.super_admin_list[0], amal.inbox_id());

        let protected_metadata: GroupMetadata = amal_group.metadata().unwrap();
        assert_eq!(
            protected_metadata.conversation_type,
            ConversationType::Group
        );

        assert_eq!(protected_metadata.creator_inbox_id, amal.inbox_id());
    }

    #[tokio::test]
    async fn test_can_update_gce_after_failed_commit() {
        // Step 1: Amal creates a group
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let policy_set = Some(PreconfiguredPolicies::AllMembers.to_policy_set());
        let amal_group = amal
            .create_group(policy_set, GroupMetadataOptions::default())
            .unwrap();
        amal_group.sync(&amal).await.unwrap();

        // Step 2:  Amal adds Bola to the group
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        amal_group
            .add_members_by_inbox_id(&amal, vec![bola.inbox_id()])
            .await
            .unwrap();

        // Step 3: Verify that Bola can update the group name, and amal sees the update
        bola.sync_welcomes().await.unwrap();
        let bola_groups = bola.find_groups(None, None, None, None).unwrap();
        let bola_group: &MlsGroup = bola_groups.first().unwrap();
        bola_group.sync(&bola).await.unwrap();
        bola_group
            .update_group_name(&bola, "Name Update 1".to_string())
            .await
            .unwrap();
        amal_group.sync(&amal).await.unwrap();
        let name = amal_group.group_name().unwrap();
        assert_eq!(name, "Name Update 1");

        // Step 4:  Bola attempts an action that they do not have permissions for like add admin, fails as expected
        let result = bola_group
            .update_admin_list(&bola, UpdateAdminListType::Add, bola.inbox_id())
            .await;
        if let Err(e) = &result {
            eprintln!("Error updating admin list: {:?}", e);
        }
        // Step 5: Now have Bola attempt to update the group name again
        bola_group
            .update_group_name(&bola, "Name Update 2".to_string())
            .await
            .unwrap();

        // Step 6: Verify that both clients can sync without error and that the group name has been updated
        amal_group.sync(&amal).await.unwrap();
        bola_group.sync(&bola).await.unwrap();
        let binding = amal_group.mutable_metadata().expect("msg");
        let amal_group_name: &String = binding
            .attributes
            .get(&MetadataField::GroupName.to_string())
            .unwrap();
        assert_eq!(amal_group_name, "Name Update 2");
        let binding = bola_group.mutable_metadata().expect("msg");
        let bola_group_name: &String = binding
            .attributes
            .get(&MetadataField::GroupName.to_string())
            .unwrap();
        assert_eq!(bola_group_name, "Name Update 2");
    }

    #[tokio::test]
    async fn test_can_update_permissions_after_group_creation() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
        let amal_group: MlsGroup = amal
            .create_group(policy_set, GroupMetadataOptions::default())
            .unwrap();

        // Step 2:  Amal adds Bola to the group
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        amal_group
            .add_members_by_inbox_id(&amal, vec![bola.inbox_id()])
            .await
            .unwrap();

        // Step 3: Bola attemps to add Caro, but fails because group is admin only
        let caro = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        bola.sync_welcomes().await.unwrap();
        let bola_groups = bola.find_groups(None, None, None, None).unwrap();
        let bola_group: &MlsGroup = bola_groups.first().unwrap();
        bola_group.sync(&bola).await.unwrap();
        let result = bola_group
            .add_members_by_inbox_id(&bola, vec![caro.inbox_id()])
            .await;
        if let Err(e) = &result {
            eprintln!("Error adding member: {:?}", e);
        } else {
            panic!("Expected error adding member");
        }

        // Step 4: Bola attempts to update permissions but fails because they are not a super admin
        let result = bola_group
            .update_permission_policy(
                &bola,
                PermissionUpdateType::AddMember,
                PermissionPolicyOption::Allow,
                None,
            )
            .await;
        if let Err(e) = &result {
            eprintln!("Error updating permissions: {:?}", e);
        } else {
            panic!("Expected error updating permissions");
        }

        // Step 5: Amal updates group permissions so that all members can add
        amal_group
            .update_permission_policy(
                &amal,
                PermissionUpdateType::AddMember,
                PermissionPolicyOption::Allow,
                None,
            )
            .await
            .unwrap();

        // Step 6: Bola can now add Caro to the group
        bola_group
            .add_members_by_inbox_id(&bola, vec![caro.inbox_id()])
            .await
            .unwrap();
        bola_group.sync(&bola).await.unwrap();
        let members = bola_group.members().unwrap();
        assert_eq!(members.len(), 3);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_optimistic_send() {
        let amal = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let bola_wallet = generate_local_wallet();
        let bola = Arc::new(ClientBuilder::new_test_client(&bola_wallet).await);
        let amal_group = amal
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        amal_group.sync(&amal).await.unwrap();
        // Add bola to the group
        amal_group
            .add_members(&amal, vec![bola_wallet.get_address()])
            .await
            .unwrap();
        let bola_group = receive_group_invite(&bola).await;

        let ids = vec![
            amal_group.send_message_optimistic(b"test one").unwrap(),
            amal_group.send_message_optimistic(b"test two").unwrap(),
            amal_group.send_message_optimistic(b"test three").unwrap(),
            amal_group.send_message_optimistic(b"test four").unwrap(),
        ];

        let messages = amal_group
            .find_messages(Some(GroupMessageKind::Application), None, None, None, None)
            .unwrap()
            .into_iter()
            .collect::<Vec<StoredGroupMessage>>();

        let text = messages
            .iter()
            .cloned()
            .map(|m| String::from_utf8_lossy(&m.decrypted_message_bytes).to_string())
            .collect::<Vec<String>>();
        assert_eq!(
            ids,
            messages
                .iter()
                .cloned()
                .map(|m| m.id)
                .collect::<Vec<Vec<u8>>>()
        );
        assert_eq!(
            text,
            vec![
                "test one".to_string(),
                "test two".to_string(),
                "test three".to_string(),
                "test four".to_string(),
            ]
        );

        let delivery = messages
            .iter()
            .cloned()
            .map(|m| m.delivery_status)
            .collect::<Vec<DeliveryStatus>>();
        assert_eq!(
            delivery,
            vec![
                DeliveryStatus::Unpublished,
                DeliveryStatus::Unpublished,
                DeliveryStatus::Unpublished,
                DeliveryStatus::Unpublished,
            ]
        );

        amal_group.publish_messages(&amal).await.unwrap();
        bola_group.sync(&bola).await.unwrap();

        let messages = bola_group
            .find_messages(None, None, None, None, None)
            .unwrap();
        let delivery = messages
            .iter()
            .cloned()
            .map(|m| m.delivery_status)
            .collect::<Vec<DeliveryStatus>>();
        assert_eq!(
            delivery,
            vec![
                DeliveryStatus::Published,
                DeliveryStatus::Published,
                DeliveryStatus::Published,
                DeliveryStatus::Published,
            ]
        );
    }
}
