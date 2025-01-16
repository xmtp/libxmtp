pub mod device_sync;
pub mod group_membership;
pub mod group_metadata;
pub mod group_mutable_metadata;
pub mod group_permissions;
pub mod intents;
pub mod members;
pub mod scoped_client;

pub(super) mod mls_sync;
pub(super) mod subscriptions;
pub mod validated_commit;

use device_sync::preference_sync::UserPreferenceUpdate;
use intents::SendMessageIntentData;
use mls_sync::GroupMessageProcessingError;
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
use tokio::sync::Mutex;
use xmtp_content_types::reaction::ReactionCodec;

use self::device_sync::DeviceSyncError;
pub use self::group_permissions::PreconfiguredPolicies;
use self::scoped_client::ScopedGroupClient;
use self::{
    group_membership::GroupMembership,
    group_metadata::{extract_group_metadata, DmMembers},
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
    group_metadata::{GroupMetadata, GroupMetadataError},
    group_permissions::PolicySet,
    intents::IntentError,
    validated_commit::CommitValidationError,
};
use crate::storage::{
    group::DmIdExt,
    group_message::{ContentType, StoredGroupMessageWithReactions},
    NotFound, StorageError,
};
use xmtp_common::time::now_ns;
use xmtp_proto::xmtp::mls::{
    api::v1::{
        group_message::{Version as GroupMessageVersion, V1 as GroupMessageV1},
        GroupMessage,
    },
    message_contents::{
        content_types::ReactionV2,
        plaintext_envelope::{Content, V1},
        EncodedContent, PlaintextEnvelope,
    },
};

use crate::{
    api::WrappedApiError,
    client::{deserialize_welcome, ClientError, XmtpMlsLocalContext},
    configuration::{
        CIPHERSUITE, GROUP_MEMBERSHIP_EXTENSION_ID, GROUP_PERMISSIONS_EXTENSION_ID, MAX_GROUP_SIZE,
        MAX_PAST_EPOCHS, MUTABLE_METADATA_EXTENSION_ID,
        SEND_MESSAGE_UPDATE_INSTALLATIONS_INTERVAL_NS,
    },
    hpke::{decrypt_welcome, HpkeError},
    identity::{parse_credential, IdentityError},
    identity_updates::{load_identity_updates, InstallationDiffError},
    intents::ProcessIntentError,
    storage::xmtp_openmls_provider::XmtpOpenMlsProvider,
    storage::{
        consent_record::{ConsentState, ConsentType, StoredConsentRecord},
        db_connection::DbConnection,
        group::{ConversationType, GroupMembershipState, StoredGroup},
        group_intent::IntentKind,
        group_message::{DeliveryStatus, GroupMessageKind, MsgQueryArgs, StoredGroupMessage},
        sql_key_store,
    },
    subscriptions::{LocalEventError, LocalEvents},
    utils::id::calculate_message_id,
    Store, MLS_COMMIT_LOCK,
};
use std::future::Future;
use std::{collections::HashSet, sync::Arc};
use xmtp_cryptography::signature::{sanitize_evm_addresses, AddressValidationError};
use xmtp_id::{InboxId, InboxIdRef};

use xmtp_common::retry::RetryableError;

#[derive(Debug, Error)]
pub enum GroupError {
    #[error("group not found")]
    GroupNotFound,
    #[error("Max user limit exceeded.")]
    UserLimitExceeded,
    #[error("api error: {0}")]
    Api(#[from] xmtp_proto::Error),
    #[error("api error: {0}")]
    WrappedApi(#[from] WrappedApiError),
    #[error("invalid group membership")]
    InvalidGroupMembership,
    #[error("storage error: {0}")]
    Storage(#[from] crate::storage::StorageError),
    #[error("intent error: {0}")]
    Intent(#[from] IntentError),
    #[error("create message: {0}")]
    CreateMessage(#[from] openmls::prelude::CreateMessageError),
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
    ReceiveError(#[from] GroupMessageProcessingError),
    #[error("Receive errors: {0:?}")]
    ReceiveErrors(Vec<GroupMessageProcessingError>),
    #[error("generic: {0}")]
    Generic(String),
    #[error("diesel error {0}")]
    Diesel(#[from] diesel::result::Error),
    #[error(transparent)]
    AddressValidation(#[from] AddressValidationError),
    #[error(transparent)]
    LocalEvent(#[from] LocalEventError),
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
    MessageHistory(#[from] Box<DeviceSyncError>),
    #[error("Installation diff error: {0}")]
    InstallationDiff(#[from] InstallationDiffError),
    #[error("PSKs are not support")]
    NoPSKSupport,
    #[error("Metadata update must specify a metadata field")]
    InvalidPermissionUpdate,
    #[error("dm requires target inbox_id")]
    InvalidDmMissingInboxId,
    #[error("Missing metadata field {name}")]
    MissingMetadataField { name: String },
    #[error("sql key store error: {0}")]
    SqlKeyStore(#[from] sql_key_store::SqlKeyStoreError),
    #[error("Sync failed to wait for intent")]
    SyncFailedToWait,
    #[error("cannot change metadata of DM")]
    DmGroupMetadataForbidden,
    #[error("Missing pending commit")]
    MissingPendingCommit,
    #[error("Intent not committed")]
    IntentNotCommitted,
    #[error(transparent)]
    ProcessIntent(#[from] ProcessIntentError),
    #[error("Failed to load lock")]
    LockUnavailable,
    #[error("Failed to acquire semaphore lock")]
    LockFailedToAcquire,
}

impl RetryableError for GroupError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Api(api_error) => api_error.is_retryable(),
            Self::ReceiveErrors(errors) => errors.iter().any(|e| e.is_retryable()),
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
            Self::SqlKeyStore(sql) => sql.is_retryable(),
            Self::Sync(errs) => errs.iter().any(|e| e.is_retryable()),
            Self::InstallationDiff(diff) => diff.is_retryable(),
            Self::CreateGroupContextExtProposalError(create) => create.is_retryable(),
            Self::CommitValidation(err) => err.is_retryable(),
            Self::WrappedApi(err) => err.is_retryable(),
            Self::MessageHistory(err) => err.is_retryable(),
            Self::ProcessIntent(err) => err.is_retryable(),
            Self::LocalEvent(err) => err.is_retryable(),
            Self::LockUnavailable => true,
            Self::LockFailedToAcquire => true,
            Self::SyncFailedToWait => true,
            Self::GroupNotFound
            | Self::GroupMetadata(_)
            | Self::GroupMutableMetadata(_)
            | Self::GroupMutablePermissions(_)
            | Self::UserLimitExceeded
            | Self::InvalidGroupMembership
            | Self::Intent(_)
            | Self::CreateMessage(_)
            | Self::TlsError(_)
            | Self::IntentNotCommitted
            | Self::Generic(_)
            | Self::InvalidDmMissingInboxId
            | Self::MissingSequenceId
            | Self::AddressNotFound(_)
            | Self::InvalidExtension(_)
            | Self::MissingMetadataField { .. }
            | Self::DmGroupMetadataForbidden
            | Self::Signature(_)
            | Self::LeafNodeError(_)
            | Self::NoPSKSupport
            | Self::MissingPendingCommit
            | Self::InvalidPermissionUpdate
            | Self::AddressValidation(_)
            | Self::InvalidPublicKeys(_)
            | Self::CredentialError(_)
            | Self::EncodeError(_) => false,
        }
    }
}

pub struct MlsGroup<C> {
    pub group_id: Vec<u8>,
    pub created_at_ns: i64,
    pub client: Arc<C>,
    mutex: Arc<Mutex<()>>,
}

pub struct ConversationListItem<C> {
    pub group: MlsGroup<C>,
    pub last_message: Option<StoredGroupMessage>,
}

#[derive(Default)]
pub struct GroupMetadataOptions {
    pub name: Option<String>,
    pub image_url_square: Option<String>,
    pub description: Option<String>,
    pub pinned_frame_url: Option<String>,
    pub message_expiration_from_ms: Option<i64>,
    pub message_expiration_ms: Option<i64>,
}

impl<C> Clone for MlsGroup<C> {
    fn clone(&self) -> Self {
        Self {
            group_id: self.group_id.clone(),
            created_at_ns: self.created_at_ns,
            client: self.client.clone(),
            mutex: self.mutex.clone(),
        }
    }
}

pub struct HmacKey {
    pub key: [u8; 42],
    // # of 30 day periods since unix epoch
    pub epoch: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UpdateAdminListType {
    Add,
    Remove,
    AddSuper,
    RemoveSuper,
}

/// Fields extracted from content of a message that should be stored in the DB
pub struct QueryableContentFields {
    pub content_type: ContentType,
    pub version_major: i32,
    pub version_minor: i32,
    pub authority_id: String,
    pub reference_id: Option<Vec<u8>>,
}

impl Default for QueryableContentFields {
    fn default() -> Self {
        Self {
            content_type: ContentType::Unknown, // Or whatever the appropriate default is
            version_major: 0,
            version_minor: 0,
            authority_id: String::new(),
            reference_id: None,
        }
    }
}

impl TryFrom<EncodedContent> for QueryableContentFields {
    type Error = prost::DecodeError;

    fn try_from(content: EncodedContent) -> Result<Self, Self::Error> {
        let content_type_id = content.r#type.unwrap_or_default();
        let reference_id = match (
            content_type_id.type_id.as_str(),
            content_type_id.version_major,
        ) {
            (ReactionCodec::TYPE_ID, major) if major >= 2 => {
                let reaction = ReactionV2::decode(content.content.as_slice())?;
                hex::decode(reaction.reference).ok()
            }
            (ReactionCodec::TYPE_ID, _) => {
                // TODO: Implement JSON deserialization for legacy reaction format
                None
            }
            _ => None,
        };

        Ok(QueryableContentFields {
            content_type: content_type_id.type_id.into(),
            version_major: content_type_id.version_major as i32,
            version_minor: content_type_id.version_minor as i32,
            authority_id: content_type_id.authority_id.to_string(),
            reference_id,
        })
    }
}

/// Represents a group, which can contain anywhere from 1 to MAX_GROUP_SIZE inboxes.
///
/// This is a wrapper around OpenMLS's `MlsGroup` that handles our application-level configuration
/// and validations.
impl<ScopedClient: ScopedGroupClient> MlsGroup<ScopedClient> {
    // Creates a new group instance. Does not validate that the group exists in the DB
    pub fn new(client: ScopedClient, group_id: Vec<u8>, created_at_ns: i64) -> Self {
        Self::new_from_arc(Arc::new(client), group_id, created_at_ns)
    }

    pub fn new_from_arc(client: Arc<ScopedClient>, group_id: Vec<u8>, created_at_ns: i64) -> Self {
        let mut mutexes = client.context().mutexes.clone();
        Self {
            group_id: group_id.clone(),
            created_at_ns,
            mutex: mutexes.get_mutex(group_id),
            client,
        }
    }

    pub(self) fn context(&self) -> Arc<XmtpMlsLocalContext> {
        self.client.context()
    }

    /// Instantiate a new [`XmtpOpenMlsProvider`] pulling a connection from the database.
    /// prefer to use an already-instantiated mls provider if possible.
    pub fn mls_provider(&self) -> Result<XmtpOpenMlsProvider, StorageError> {
        self.context().mls_provider()
    }

    // Load the stored OpenMLS group from the OpenMLS provider's keystore
    #[tracing::instrument(level = "trace", skip_all)]
    pub(crate) fn load_mls_group_with_lock<F, R>(
        &self,
        provider: impl OpenMlsProvider,
        operation: F,
    ) -> Result<R, GroupError>
    where
        F: FnOnce(OpenMlsGroup) -> Result<R, GroupError>,
    {
        // Get the group ID for locking
        let group_id = self.group_id.clone();

        // Acquire the lock synchronously using blocking_lock
        let _lock = MLS_COMMIT_LOCK.get_lock_sync(group_id.clone());
        // Load the MLS group
        let mls_group =
            OpenMlsGroup::load(provider.storage(), &GroupId::from_slice(&self.group_id))
                .map_err(|_| GroupError::GroupNotFound)?
                .ok_or(GroupError::GroupNotFound)?;

        // Perform the operation with the MLS group
        operation(mls_group)
    }

    // Load the stored OpenMLS group from the OpenMLS provider's keystore
    #[tracing::instrument(level = "trace", skip_all)]
    pub(crate) async fn load_mls_group_with_lock_async<F, E, R, Fut>(
        &self,
        provider: &XmtpOpenMlsProvider,
        operation: F,
    ) -> Result<R, E>
    where
        F: FnOnce(OpenMlsGroup) -> Fut,
        Fut: Future<Output = Result<R, E>>,
        E: From<GroupMessageProcessingError> + From<crate::StorageError>,
    {
        // Get the group ID for locking
        let group_id = self.group_id.clone();

        // Acquire the lock asynchronously
        let _lock = MLS_COMMIT_LOCK.get_lock_async(group_id.clone()).await;

        // Load the MLS group
        let mls_group =
            OpenMlsGroup::load(provider.storage(), &GroupId::from_slice(&self.group_id))
                .map_err(crate::StorageError::from)?
                .ok_or(StorageError::from(NotFound::GroupById(
                    self.group_id.to_vec(),
                )))?;

        // Perform the operation with the MLS group
        operation(mls_group).await.map_err(Into::into)
    }

    // Create a new group and save it to the DB
    pub(crate) fn create_and_insert(
        client: Arc<ScopedClient>,
        provider: &XmtpOpenMlsProvider,
        membership_state: GroupMembershipState,
        permissions_policy_set: PolicySet,
        opts: GroupMetadataOptions,
    ) -> Result<Self, GroupError> {
        let context = client.context();
        let creator_inbox_id = context.inbox_id();
        let protected_metadata =
            build_protected_metadata_extension(creator_inbox_id, ConversationType::Group)?;
        let mutable_metadata = build_mutable_metadata_extension_default(creator_inbox_id, opts)?;
        let group_membership = build_starting_group_membership_extension(creator_inbox_id, 0);
        let mutable_permissions = build_mutable_permissions_extension(permissions_policy_set)?;
        let group_config = build_group_config(
            protected_metadata,
            mutable_metadata,
            group_membership,
            mutable_permissions,
        )?;

        let mls_group = OpenMlsGroup::new(
            provider,
            &context.identity.installation_keys,
            &group_config,
            CredentialWithKey {
                credential: context.identity.credential(),
                signature_key: context.identity.installation_keys.public_slice().into(),
            },
        )?;

        let group_id = mls_group.group_id().to_vec();
        let stored_group = StoredGroup::new(
            group_id.clone(),
            now_ns(),
            membership_state,
            context.inbox_id().to_string(),
            None,
        );

        stored_group.store(provider.conn_ref())?;
        let new_group = Self::new_from_arc(client.clone(), group_id, stored_group.created_at_ns);

        // Consent state defaults to allowed when the user creates the group
        new_group.update_consent_state(ConsentState::Allowed)?;
        Ok(new_group)
    }

    // Create a new DM and save it to the DB
    pub(crate) fn create_dm_and_insert(
        provider: &XmtpOpenMlsProvider,
        client: Arc<ScopedClient>,
        membership_state: GroupMembershipState,
        dm_target_inbox_id: InboxId,
    ) -> Result<Self, GroupError> {
        let context = client.context();
        let protected_metadata =
            build_dm_protected_metadata_extension(context.inbox_id(), dm_target_inbox_id.clone())?;
        let mutable_metadata =
            build_dm_mutable_metadata_extension_default(context.inbox_id(), &dm_target_inbox_id)?;
        let group_membership = build_starting_group_membership_extension(context.inbox_id(), 0);
        let mutable_permissions = PolicySet::new_dm();
        let mutable_permission_extension =
            build_mutable_permissions_extension(mutable_permissions)?;
        let group_config = build_group_config(
            protected_metadata,
            mutable_metadata,
            group_membership,
            mutable_permission_extension,
        )?;

        let mls_group = OpenMlsGroup::new(
            &provider,
            &context.identity.installation_keys,
            &group_config,
            CredentialWithKey {
                credential: context.identity.credential(),
                signature_key: context.identity.installation_keys.public_slice().into(),
            },
        )?;

        let group_id = mls_group.group_id().to_vec();

        let stored_group = StoredGroup::new(
            group_id.clone(),
            now_ns(),
            membership_state,
            context.inbox_id().to_string(),
            Some(DmMembers {
                member_one_inbox_id: dm_target_inbox_id,
                member_two_inbox_id: client.inbox_id().to_string(),
            }),
        );

        stored_group.store(provider.conn_ref())?;
        let new_group = Self::new_from_arc(client.clone(), group_id, stored_group.created_at_ns);
        // Consent state defaults to allowed when the user creates the group
        new_group.update_consent_state(ConsentState::Allowed)?;
        Ok(new_group)
    }

    // Create a group from a decrypted and decoded welcome message
    // If the group already exists in the store, overwrite the MLS state and do not update the group entry
    async fn create_from_welcome(
        client: Arc<ScopedClient>,
        provider: &XmtpOpenMlsProvider,
        welcome: MlsWelcome,
        added_by_inbox: String,
        welcome_id: i64,
    ) -> Result<Self, GroupError> {
        tracing::info!("Creating from welcome");
        let mls_welcome =
            StagedWelcome::new_from_welcome(provider, &build_group_join_config(), welcome, None)?;

        let mls_group = mls_welcome.into_group(provider)?;
        let group_id = mls_group.group_id().to_vec();
        let metadata = extract_group_metadata(&mls_group)?;
        let dm_members = metadata.dm_members;

        let conversation_type = metadata.conversation_type;

        let to_store = match conversation_type {
            ConversationType::Group => StoredGroup::new_from_welcome(
                group_id.clone(),
                now_ns(),
                GroupMembershipState::Pending,
                added_by_inbox,
                welcome_id,
                conversation_type,
                dm_members,
            ),
            ConversationType::Dm => {
                validate_dm_group(client.as_ref(), &mls_group, &added_by_inbox)?;
                StoredGroup::new_from_welcome(
                    group_id.clone(),
                    now_ns(),
                    GroupMembershipState::Pending,
                    added_by_inbox,
                    welcome_id,
                    conversation_type,
                    dm_members,
                )
            }
            ConversationType::Sync => StoredGroup::new_from_welcome(
                group_id.clone(),
                now_ns(),
                GroupMembershipState::Allowed,
                added_by_inbox,
                welcome_id,
                conversation_type,
                dm_members,
            ),
        };

        // Ensure that the list of members in the group's MLS tree matches the list of inboxes specified
        // in the `GroupMembership` extension.
        validate_initial_group_membership(client.as_ref(), provider.conn_ref(), &mls_group).await?;

        // Insert or replace the group in the database.
        // Replacement can happen in the case that the user has been removed from and subsequently re-added to the group.
        let stored_group = provider.conn_ref().insert_or_replace_group(to_store)?;

        Ok(Self::new_from_arc(
            client.clone(),
            stored_group.id,
            stored_group.created_at_ns,
        ))
    }

    /// Decrypt a welcome message using HPKE and then create and save a group from the stored message
    pub async fn create_from_encrypted_welcome(
        client: Arc<ScopedClient>,
        provider: &XmtpOpenMlsProvider,
        hpke_public_key: &[u8],
        encrypted_welcome_bytes: &[u8],
        welcome_id: i64,
    ) -> Result<Self, GroupError> {
        tracing::info!("Trying to decrypt welcome");
        let welcome_bytes = decrypt_welcome(provider, hpke_public_key, encrypted_welcome_bytes)?;

        let welcome = deserialize_welcome(&welcome_bytes)?;

        let join_config = build_group_join_config();

        let processed_welcome =
            ProcessedWelcome::new_from_welcome(provider, &join_config, welcome.clone())?;
        let psks = processed_welcome.psks();
        if !psks.is_empty() {
            tracing::error!("No PSK support for welcome");
            return Err(GroupError::NoPSKSupport);
        }
        let staged_welcome = processed_welcome.into_staged_welcome(provider, None)?;

        let added_by_node = staged_welcome.welcome_sender()?;

        let added_by_credential = BasicCredential::try_from(added_by_node.credential().clone())?;
        let inbox_id = parse_credential(added_by_credential.identity())?;

        Self::create_from_welcome(client, provider, welcome, inbox_id, welcome_id).await
    }

    pub(crate) fn create_and_insert_sync_group(
        client: Arc<ScopedClient>,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<MlsGroup<ScopedClient>, GroupError> {
        let context = client.context();
        let creator_inbox_id = context.inbox_id();

        let protected_metadata =
            build_protected_metadata_extension(creator_inbox_id, ConversationType::Sync)?;
        let mutable_metadata = build_mutable_metadata_extension_default(
            creator_inbox_id,
            GroupMetadataOptions::default(),
        )?;
        let group_membership = build_starting_group_membership_extension(creator_inbox_id, 0);
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
                signature_key: context.identity.installation_keys.public_slice().into(),
            },
        )?;

        let group_id = mls_group.group_id().to_vec();
        let stored_group =
            StoredGroup::new_sync_group(group_id.clone(), now_ns(), GroupMembershipState::Allowed);

        stored_group.store(provider.conn_ref())?;

        Ok(Self::new_from_arc(
            client,
            stored_group.id,
            stored_group.created_at_ns,
        ))
    }

    /// Send a message on this users XMTP [`Client`].
    pub async fn send_message(&self, message: &[u8]) -> Result<Vec<u8>, GroupError> {
        tracing::debug!(inbox_id = self.client.inbox_id(), "sending message");
        let conn = self.context().store().conn()?;
        let provider = XmtpOpenMlsProvider::from(conn);
        self.send_message_with_provider(message, &provider).await
    }

    /// Send a message with the given [`XmtpOpenMlsProvider`]
    pub async fn send_message_with_provider(
        &self,
        message: &[u8],
        provider: &XmtpOpenMlsProvider,
    ) -> Result<Vec<u8>, GroupError> {
        let update_interval_ns = Some(SEND_MESSAGE_UPDATE_INSTALLATIONS_INTERVAL_NS);
        self.maybe_update_installations(provider, update_interval_ns)
            .await?;

        let message_id =
            self.prepare_message(message, provider, |now| Self::into_envelope(message, now))?;

        self.sync_until_last_intent_resolved(provider).await?;

        // implicitly set group consent state to allowed
        self.update_consent_state(ConsentState::Allowed)?;

        Ok(message_id)
    }

    /// Publish all unpublished messages. This happens by calling `sync_until_last_intent_resolved`
    /// which publishes all pending intents and reads them back from the network.
    pub async fn publish_messages(&self) -> Result<(), GroupError> {
        let conn = self.context().store().conn()?;
        let provider = XmtpOpenMlsProvider::from(conn);
        let update_interval_ns = Some(SEND_MESSAGE_UPDATE_INSTALLATIONS_INTERVAL_NS);
        self.maybe_update_installations(&provider, update_interval_ns)
            .await?;
        self.sync_until_last_intent_resolved(&provider).await?;

        // implicitly set group consent state to allowed
        self.update_consent_state(ConsentState::Allowed)?;

        Ok(())
    }

    /// Checks the network to see if any group members have identity updates that would cause installations
    /// to be added or removed from the group.
    ///
    /// If so, adds/removes those group members
    pub async fn update_installations(&self) -> Result<(), GroupError> {
        let provider = self.client.mls_provider()?;
        self.maybe_update_installations(&provider, Some(0)).await?;
        Ok(())
    }

    /// Send a message, optimistically returning the ID of the message before the result of a message publish.
    pub fn send_message_optimistic(&self, message: &[u8]) -> Result<Vec<u8>, GroupError> {
        let provider = self.mls_provider()?;
        let message_id =
            self.prepare_message(message, &provider, |now| Self::into_envelope(message, now))?;
        Ok(message_id)
    }

    /// Helper function to extract queryable content fields from a message
    fn extract_queryable_content_fields(message: &[u8]) -> QueryableContentFields {
        // Return early with default if decoding fails or type is missing
        EncodedContent::decode(message)
            .inspect_err(|e| tracing::debug!("Failed to decode message as EncodedContent: {}", e))
            .and_then(|content| {
                QueryableContentFields::try_from(content).inspect_err(|e| {
                    tracing::debug!(
                        "Failed to convert EncodedContent to QueryableContentFields: {}",
                        e
                    )
                })
            })
            .unwrap_or_default()
    }

    /// Prepare a [`IntentKind::SendMessage`] intent, and [`StoredGroupMessage`] on this users XMTP [`Client`].
    ///
    /// # Arguments
    /// * message: UTF-8 or encoded message bytes
    /// * conn: Connection to SQLite database
    /// * envelope: closure that returns context-specific [`PlaintextEnvelope`]. Closure accepts
    ///     timestamp attached to intent & stored message.
    fn prepare_message<F>(
        &self,
        message: &[u8],
        provider: &XmtpOpenMlsProvider,
        envelope: F,
    ) -> Result<Vec<u8>, GroupError>
    where
        F: FnOnce(i64) -> PlaintextEnvelope,
    {
        let now = now_ns();
        let plain_envelope = envelope(now);
        let mut encoded_envelope = vec![];
        plain_envelope
            .encode(&mut encoded_envelope)
            .map_err(GroupError::EncodeError)?;

        let intent_data: Vec<u8> = SendMessageIntentData::new(encoded_envelope).into();
        self.queue_intent(provider, IntentKind::SendMessage, intent_data)?;

        // store this unpublished message locally before sending
        let message_id = calculate_message_id(&self.group_id, message, &now.to_string());
        let queryable_content_fields = Self::extract_queryable_content_fields(message);
        let group_message = StoredGroupMessage {
            id: message_id.clone(),
            group_id: self.group_id.clone(),
            decrypted_message_bytes: message.to_vec(),
            sent_at_ns: now,
            kind: GroupMessageKind::Application,
            sender_installation_id: self.context().installation_public_key().into(),
            sender_inbox_id: self.context().inbox_id().to_string(),
            delivery_status: DeliveryStatus::Unpublished,
            content_type: queryable_content_fields.content_type,
            version_major: queryable_content_fields.version_major,
            version_minor: queryable_content_fields.version_minor,
            authority_id: queryable_content_fields.authority_id,
            reference_id: queryable_content_fields.reference_id,
        };
        group_message.store(provider.conn_ref())?;

        Ok(message_id)
    }

    fn into_envelope(encoded_msg: &[u8], idempotency_key: i64) -> PlaintextEnvelope {
        PlaintextEnvelope {
            content: Some(Content::V1(V1 {
                content: encoded_msg.to_vec(),
                idempotency_key: idempotency_key.to_string(),
            })),
        }
    }

    /// Query the database for stored messages. Optionally filtered by time, kind, delivery_status
    /// and limit
    pub fn find_messages(
        &self,
        args: &MsgQueryArgs,
    ) -> Result<Vec<StoredGroupMessage>, GroupError> {
        let conn = self.context().store().conn()?;
        let messages = conn.get_group_messages(&self.group_id, args)?;
        Ok(messages)
    }

    /// Query the database for stored messages. Optionally filtered by time, kind, delivery_status
    /// and limit
    pub fn find_messages_with_reactions(
        &self,
        args: &MsgQueryArgs,
    ) -> Result<Vec<StoredGroupMessageWithReactions>, GroupError> {
        let conn = self.context().store().conn()?;
        let messages = conn.get_group_messages_with_reactions(&self.group_id, args)?;
        Ok(messages)
    }

    ///
    /// Add members to the group by account address
    ///
    /// If any existing members have new installations that have not been added or removed, the
    /// group membership will be updated to include those changes as well.
    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn add_members(&self, account_addresses_to_add: &[String]) -> Result<(), GroupError> {
        let account_addresses = sanitize_evm_addresses(account_addresses_to_add)?;
        let inbox_id_map = self
            .client
            .api()
            .get_inbox_ids(account_addresses.clone())
            .await?;
        let provider = self.mls_provider()?;
        // get current number of users in group
        let member_count = self.members_with_provider(&provider).await?.len();
        if member_count + inbox_id_map.len() > MAX_GROUP_SIZE {
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

        self.add_members_by_inbox_id_with_provider(
            &provider,
            &inbox_id_map.into_values().collect::<Vec<_>>(),
        )
        .await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn add_members_by_inbox_id<S: AsRef<str>>(
        &self,
        inbox_ids: &[S],
    ) -> Result<(), GroupError> {
        let provider = self.client.mls_provider()?;
        self.add_members_by_inbox_id_with_provider(&provider, inbox_ids)
            .await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn add_members_by_inbox_id_with_provider<S: AsRef<str>>(
        &self,
        provider: &XmtpOpenMlsProvider,
        inbox_ids: &[S],
    ) -> Result<(), GroupError> {
        let ids = inbox_ids.iter().map(AsRef::as_ref).collect::<Vec<&str>>();
        let intent_data = self
            .get_membership_update_intent(provider, ids.as_slice(), &[])
            .await?;

        // TODO:nm this isn't the best test for whether the request is valid
        // If some existing group member has an update, this will return an intent with changes
        // when we really should return an error
        if intent_data.is_empty() {
            tracing::warn!("Member already added");
            return Ok(());
        }

        let intent = self.queue_intent(
            provider,
            IntentKind::UpdateGroupMembership,
            intent_data.into(),
        )?;

        tracing::warn!("This makes it here?");

        self.sync_until_intent_resolved(provider, intent.id).await
    }

    /// Removes members from the group by their account addresses.
    ///
    /// # Arguments
    /// * `client` - The XMTP client.
    /// * `account_addresses_to_remove` - A vector of account addresses to remove from the group.
    ///
    /// # Returns
    /// A `Result` indicating success or failure of the operation.
    pub async fn remove_members(
        &self,
        account_addresses_to_remove: &[InboxId],
    ) -> Result<(), GroupError> {
        let account_addresses = sanitize_evm_addresses(account_addresses_to_remove)?;
        let inbox_id_map = self.client.api().get_inbox_ids(account_addresses).await?;

        let ids = inbox_id_map
            .values()
            .map(AsRef::as_ref)
            .collect::<Vec<&str>>();
        self.remove_members_by_inbox_id(ids.as_slice()).await
    }

    /// Removes members from the group by their inbox IDs.
    ///
    /// # Arguments
    /// * `client` - The XMTP client.
    /// * `inbox_ids` - A vector of inbox IDs to remove from the group.
    ///
    /// # Returns
    /// A `Result` indicating success or failure of the operation.
    pub async fn remove_members_by_inbox_id(
        &self,
        inbox_ids: &[InboxIdRef<'_>],
    ) -> Result<(), GroupError> {
        let provider = self.client.store().conn()?.into();

        let intent_data = self
            .get_membership_update_intent(&provider, &[], inbox_ids)
            .await?;

        let intent = self.queue_intent(
            &provider,
            IntentKind::UpdateGroupMembership,
            intent_data.into(),
        )?;

        self.sync_until_intent_resolved(&provider, intent.id).await
    }

    /// Updates the name of the group. Will error if the user does not have the appropriate permissions
    /// to perform these updates.
    pub async fn update_group_name(&self, group_name: String) -> Result<(), GroupError> {
        let provider = self.client.mls_provider()?;
        if self.metadata(&provider).await?.conversation_type == ConversationType::Dm {
            return Err(GroupError::DmGroupMetadataForbidden);
        }
        let intent_data: Vec<u8> =
            UpdateMetadataIntentData::new_update_group_name(group_name).into();
        let intent = self.queue_intent(&provider, IntentKind::MetadataUpdate, intent_data)?;

        self.sync_until_intent_resolved(&provider, intent.id).await
    }

    /// Updates the permission policy of the group. This requires super admin permissions.
    pub async fn update_permission_policy(
        &self,
        permission_update_type: PermissionUpdateType,
        permission_policy: PermissionPolicyOption,
        metadata_field: Option<MetadataField>,
    ) -> Result<(), GroupError> {
        let provider = self.client.mls_provider()?;
        if self.metadata(&provider).await?.conversation_type == ConversationType::Dm {
            return Err(GroupError::DmGroupMetadataForbidden);
        }
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

        let intent = self.queue_intent(&provider, IntentKind::UpdatePermission, intent_data)?;

        self.sync_until_intent_resolved(&provider, intent.id).await
    }

    /// Retrieves the group name from the group's mutable metadata extension.
    pub fn group_name(&self, provider: &XmtpOpenMlsProvider) -> Result<String, GroupError> {
        let mutable_metadata = self.mutable_metadata(provider)?;
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

    /// Updates the description of the group.
    pub async fn update_group_description(
        &self,
        group_description: String,
    ) -> Result<(), GroupError> {
        let provider = self.client.mls_provider()?;
        if self.metadata(&provider).await?.conversation_type == ConversationType::Dm {
            return Err(GroupError::DmGroupMetadataForbidden);
        }
        let intent_data: Vec<u8> =
            UpdateMetadataIntentData::new_update_group_description(group_description).into();
        let intent = self.queue_intent(&provider, IntentKind::MetadataUpdate, intent_data)?;

        self.sync_until_intent_resolved(&provider, intent.id).await
    }

    pub fn group_description(&self, provider: &XmtpOpenMlsProvider) -> Result<String, GroupError> {
        let mutable_metadata = self.mutable_metadata(provider)?;
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

    /// Updates the image URL (square) of the group.
    pub async fn update_group_image_url_square(
        &self,
        group_image_url_square: String,
    ) -> Result<(), GroupError> {
        let provider = self.client.mls_provider()?;
        if self.metadata(&provider).await?.conversation_type == ConversationType::Dm {
            return Err(GroupError::DmGroupMetadataForbidden);
        }
        let intent_data: Vec<u8> =
            UpdateMetadataIntentData::new_update_group_image_url_square(group_image_url_square)
                .into();
        let intent = self.queue_intent(&provider, IntentKind::MetadataUpdate, intent_data)?;

        self.sync_until_intent_resolved(&provider, intent.id).await
    }

    /// Retrieves the image URL (square) of the group from the group's mutable metadata extension.
    pub fn group_image_url_square(
        &self,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<String, GroupError> {
        let mutable_metadata = self.mutable_metadata(provider)?;
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

    pub async fn update_group_pinned_frame_url(
        &self,
        pinned_frame_url: String,
    ) -> Result<(), GroupError> {
        let provider = self.client.mls_provider()?;
        if self.metadata(&provider).await?.conversation_type == ConversationType::Dm {
            return Err(GroupError::DmGroupMetadataForbidden);
        }
        let intent_data: Vec<u8> =
            UpdateMetadataIntentData::new_update_group_pinned_frame_url(pinned_frame_url).into();
        let intent = self.queue_intent(&provider, IntentKind::MetadataUpdate, intent_data)?;

        self.sync_until_intent_resolved(&provider, intent.id).await
    }

    pub fn group_pinned_frame_url(
        &self,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<String, GroupError> {
        let mutable_metadata = self.mutable_metadata(provider)?;
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

    /// Retrieves the admin list of the group from the group's mutable metadata extension.
    pub fn admin_list(&self, provider: &XmtpOpenMlsProvider) -> Result<Vec<String>, GroupError> {
        let mutable_metadata = self.mutable_metadata(provider)?;
        Ok(mutable_metadata.admin_list)
    }

    /// Retrieves the super admin list of the group from the group's mutable metadata extension.
    pub fn super_admin_list(
        &self,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<Vec<String>, GroupError> {
        let mutable_metadata = self.mutable_metadata(provider)?;
        Ok(mutable_metadata.super_admin_list)
    }

    /// Checks if the given inbox ID is an admin of the group at the most recently synced epoch.
    pub fn is_admin(
        &self,
        inbox_id: String,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<bool, GroupError> {
        let mutable_metadata = self.mutable_metadata(provider)?;
        Ok(mutable_metadata.admin_list.contains(&inbox_id))
    }

    /// Checks if the given inbox ID is a super admin of the group at the most recently synced epoch.
    pub fn is_super_admin(
        &self,
        inbox_id: String,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<bool, GroupError> {
        let mutable_metadata = self.mutable_metadata(provider)?;
        Ok(mutable_metadata.super_admin_list.contains(&inbox_id))
    }

    /// Retrieves the conversation type of the group from the group's metadata extension.
    pub async fn conversation_type(
        &self,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<ConversationType, GroupError> {
        let metadata = self.metadata(provider).await?;
        Ok(metadata.conversation_type)
    }

    /// Updates the admin list of the group and syncs the changes to the network.
    pub async fn update_admin_list(
        &self,
        action_type: UpdateAdminListType,
        inbox_id: String,
    ) -> Result<(), GroupError> {
        let provider = self.client.mls_provider()?;
        if self.metadata(&provider).await?.conversation_type == ConversationType::Dm {
            return Err(GroupError::DmGroupMetadataForbidden);
        }
        let intent_action_type = match action_type {
            UpdateAdminListType::Add => AdminListActionType::Add,
            UpdateAdminListType::Remove => AdminListActionType::Remove,
            UpdateAdminListType::AddSuper => AdminListActionType::AddSuper,
            UpdateAdminListType::RemoveSuper => AdminListActionType::RemoveSuper,
        };
        let intent_data: Vec<u8> =
            UpdateAdminListIntentData::new(intent_action_type, inbox_id).into();
        let intent = self.queue_intent(&provider, IntentKind::UpdateAdminList, intent_data)?;

        self.sync_until_intent_resolved(&provider, intent.id).await
    }

    /// Find the `inbox_id` of the group member who added the member to the group
    pub fn added_by_inbox_id(&self) -> Result<String, GroupError> {
        let conn = self.context().store().conn()?;
        conn.find_group(self.group_id.clone())
            .map_err(GroupError::from)
            .and_then(|fetch_result| {
                fetch_result
                    .map(|group| group.added_by_inbox_id.clone())
                    .ok_or_else(|| GroupError::GroupNotFound)
            })
    }

    /// Find the `inbox_id` of the group member who is the peer of this dm
    pub fn dm_inbox_id(&self) -> Result<String, GroupError> {
        let conn = self.context().store().conn()?;
        let group = conn
            .find_group(self.group_id.clone())?
            .ok_or(GroupError::GroupNotFound)?;
        let inbox_id = self.client.inbox_id();
        let dm_id = &group.dm_id.ok_or(GroupError::GroupNotFound)?;
        Ok(dm_id.other_inbox_id(inbox_id))
    }

    /// Find the `consent_state` of the group
    pub fn consent_state(&self) -> Result<ConsentState, GroupError> {
        let conn = self.context().store().conn()?;
        let record = conn.get_consent_record(
            hex::encode(self.group_id.clone()),
            ConsentType::ConversationId,
        )?;

        match record {
            Some(rec) => Ok(rec.state),
            None => Ok(ConsentState::Unknown),
        }
    }

    pub fn update_consent_state(&self, state: ConsentState) -> Result<(), GroupError> {
        let conn = self.context().store().conn()?;

        let consent_record = StoredConsentRecord::new(
            ConsentType::ConversationId,
            state,
            hex::encode(self.group_id.clone()),
        );
        let new_records: Vec<_> = conn
            .insert_or_replace_consent_records(&[consent_record.clone()])?
            .into_iter()
            .map(UserPreferenceUpdate::ConsentUpdate)
            .collect();

        if !new_records.is_empty() && self.client.history_sync_url().is_some() {
            // Dispatch an update event so it can be synced across devices
            let _ = self
                .client
                .local_events()
                .send(LocalEvents::OutgoingPreferenceUpdates(new_records));
        }

        Ok(())
    }

    /// Update this installation's leaf key in the group by creating a key update commit
    pub async fn key_update(&self) -> Result<(), GroupError> {
        let provider = self.client.mls_provider()?;
        let intent = self.queue_intent(&provider, IntentKind::KeyUpdate, vec![])?;
        self.sync_until_intent_resolved(&provider, intent.id).await
    }

    /// Checks if the current user is active in the group.
    ///
    /// If the current user has been kicked out of the group, `is_active` will return `false`
    pub fn is_active(&self, provider: &XmtpOpenMlsProvider) -> Result<bool, GroupError> {
        self.load_mls_group_with_lock(provider, |mls_group| Ok(mls_group.is_active()))
    }

    /// Get the `GroupMetadata` of the group.
    pub async fn metadata(
        &self,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<GroupMetadata, GroupError> {
        self.load_mls_group_with_lock_async(provider, |mls_group| {
            futures::future::ready(extract_group_metadata(&mls_group).map_err(Into::into))
        })
        .await
    }

    /// Get the `GroupMutableMetadata` of the group.
    pub fn mutable_metadata(
        &self,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<GroupMutableMetadata, GroupError> {
        self.load_mls_group_with_lock(provider, |mls_group| {
            Ok(GroupMutableMetadata::try_from(&mls_group)?)
        })
    }

    pub fn permissions(&self) -> Result<GroupMutablePermissions, GroupError> {
        let provider = self.mls_provider()?;

        self.load_mls_group_with_lock(&provider, |mls_group| {
            Ok(extract_group_permissions(&mls_group)?)
        })
    }

    /// Used for testing that dm group validation works as expected.
    ///
    /// See the `test_validate_dm_group` test function for more details.
    #[cfg(test)]
    pub fn create_test_dm_group(
        client: Arc<ScopedClient>,
        dm_target_inbox_id: InboxId,
        custom_protected_metadata: Option<Extension>,
        custom_mutable_metadata: Option<Extension>,
        custom_group_membership: Option<Extension>,
        custom_mutable_permissions: Option<PolicySet>,
    ) -> Result<Self, GroupError> {
        let context = client.context();
        let conn = context.store().conn()?;
        let provider = XmtpOpenMlsProvider::new(conn);

        let protected_metadata = custom_protected_metadata.unwrap_or_else(|| {
            build_dm_protected_metadata_extension(context.inbox_id(), dm_target_inbox_id.clone())
                .unwrap()
        });
        let mutable_metadata = custom_mutable_metadata.unwrap_or_else(|| {
            build_dm_mutable_metadata_extension_default(context.inbox_id(), &dm_target_inbox_id)
                .unwrap()
        });
        let group_membership = custom_group_membership
            .unwrap_or_else(|| build_starting_group_membership_extension(context.inbox_id(), 0));
        let mutable_permissions = custom_mutable_permissions.unwrap_or_else(PolicySet::new_dm);
        let mutable_permission_extension =
            build_mutable_permissions_extension(mutable_permissions)?;

        let group_config = build_group_config(
            protected_metadata,
            mutable_metadata,
            group_membership,
            mutable_permission_extension,
        )?;

        let mls_group = OpenMlsGroup::new(
            &provider,
            &context.identity.installation_keys,
            &group_config,
            CredentialWithKey {
                credential: context.identity.credential(),
                signature_key: context.identity.installation_keys.public_slice().into(),
            },
        )?;

        let group_id = mls_group.group_id().to_vec();
        let stored_group = StoredGroup::new(
            group_id.clone(),
            now_ns(),
            GroupMembershipState::Allowed, // Use Allowed as default for tests
            context.inbox_id().to_string(),
            Some(DmMembers {
                member_one_inbox_id: client.inbox_id().to_string(),
                member_two_inbox_id: dm_target_inbox_id,
            }),
        );

        stored_group.store(provider.conn_ref())?;
        Ok(Self::new_from_arc(
            client,
            group_id,
            stored_group.created_at_ns,
        ))
    }
}

fn extract_message_v1(
    message: GroupMessage,
) -> Result<GroupMessageV1, GroupMessageProcessingError> {
    match message.version {
        Some(GroupMessageVersion::V1(value)) => Ok(value),
        _ => Err(GroupMessageProcessingError::InvalidPayload),
    }
}

pub fn extract_group_id(message: &GroupMessage) -> Result<Vec<u8>, GroupMessageProcessingError> {
    match &message.version {
        Some(GroupMessageVersion::V1(value)) => Ok(value.group_id.clone()),
        _ => Err(GroupMessageProcessingError::InvalidPayload),
    }
}

fn build_protected_metadata_extension(
    creator_inbox_id: &str,
    conversation_type: ConversationType,
) -> Result<Extension, GroupError> {
    let metadata = GroupMetadata::new(conversation_type, creator_inbox_id.to_string(), None);
    let protected_metadata = Metadata::new(metadata.try_into()?);

    Ok(Extension::ImmutableMetadata(protected_metadata))
}

fn build_dm_protected_metadata_extension(
    creator_inbox_id: &str,
    dm_inbox_id: InboxId,
) -> Result<Extension, GroupError> {
    let dm_members = Some(DmMembers {
        member_one_inbox_id: creator_inbox_id.to_string(),
        member_two_inbox_id: dm_inbox_id,
    });

    let metadata = GroupMetadata::new(
        ConversationType::Dm,
        creator_inbox_id.to_string(),
        dm_members,
    );
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
    creator_inbox_id: &str,
    opts: GroupMetadataOptions,
) -> Result<Extension, GroupError> {
    let mutable_metadata: Vec<u8> =
        GroupMutableMetadata::new_default(creator_inbox_id.to_string(), opts).try_into()?;
    let unknown_gc_extension = UnknownExtension(mutable_metadata);

    Ok(Extension::Unknown(
        MUTABLE_METADATA_EXTENSION_ID,
        unknown_gc_extension,
    ))
}

pub fn build_dm_mutable_metadata_extension_default(
    creator_inbox_id: &str,
    dm_target_inbox_id: &str,
) -> Result<Extension, GroupError> {
    let mutable_metadata: Vec<u8> =
        GroupMutableMetadata::new_dm_default(creator_inbox_id.to_string(), dm_target_inbox_id)
            .try_into()?;
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
                update_permissions_intent.metadata_field_name.ok_or(
                    GroupError::MissingMetadataField {
                        name: "metadata_field_name".into(),
                    },
                )?,
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

pub fn build_starting_group_membership_extension(inbox_id: &str, sequence_id: u64) -> Extension {
    let mut group_membership = GroupMembership::new();
    group_membership.add(inbox_id.to_string(), sequence_id);
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
        .max_past_epochs(MAX_PAST_EPOCHS)
        .use_ratchet_tree_extension(true)
        .build())
}

/**
 * Ensures that the membership in the MLS tree matches the inboxes specified in the `GroupMembership` extension.
 */
async fn validate_initial_group_membership(
    client: impl ScopedGroupClient,
    conn: &DbConnection,
    mls_group: &OpenMlsGroup,
) -> Result<(), GroupError> {
    tracing::info!("Validating initial group membership");
    let membership = extract_group_membership(mls_group.extensions())?;
    let needs_update = conn.filter_inbox_ids_needing_updates(membership.to_filters().as_slice())?;
    if !needs_update.is_empty() {
        let ids = needs_update.iter().map(AsRef::as_ref).collect::<Vec<_>>();
        load_identity_updates(client.api(), conn, ids.as_slice()).await?;
    }

    let mut expected_installation_ids = HashSet::<Vec<u8>>::new();

    let futures: Vec<_> = membership
        .members
        .iter()
        .map(|(inbox_id, sequence_id)| {
            client.get_association_state(conn, inbox_id, Some(*sequence_id as i64))
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

    tracing::info!("Group membership validated");
    Ok(())
}

fn validate_dm_group(
    client: impl ScopedGroupClient,
    mls_group: &OpenMlsGroup,
    added_by_inbox: &str,
) -> Result<(), GroupError> {
    let metadata = extract_group_metadata(mls_group)?;

    // Check if the conversation type is DM
    if metadata.conversation_type != ConversationType::Dm {
        return Err(GroupError::Generic(
            "Invalid conversation type for DM group".to_string(),
        ));
    }

    // Check if DmMembers are set and validate their contents
    if let Some(dm_members) = metadata.dm_members {
        let our_inbox_id = client.inbox_id();
        if !((dm_members.member_one_inbox_id == added_by_inbox
            && dm_members.member_two_inbox_id == our_inbox_id)
            || (dm_members.member_one_inbox_id == our_inbox_id
                && dm_members.member_two_inbox_id == added_by_inbox))
        {
            return Err(GroupError::Generic(
                "DM members do not match expected inboxes".to_string(),
            ));
        }
    } else {
        return Err(GroupError::Generic(
            "DM group must have DmMembers set".to_string(),
        ));
    }

    // Validate mutable metadata
    let mutable_metadata: GroupMutableMetadata = mls_group.try_into()?;

    // Check if the admin list and super admin list are empty
    if !mutable_metadata.admin_list.is_empty() || !mutable_metadata.super_admin_list.is_empty() {
        return Err(GroupError::Generic(
            "DM group must have empty admin and super admin lists".to_string(),
        ));
    }

    // Validate permissions
    let permissions = extract_group_permissions(mls_group)?;
    if permissions != GroupMutablePermissions::new(PolicySet::new_dm()) {
        return Err(GroupError::Generic(
            "Invalid permissions for DM group".to_string(),
        ));
    }

    Ok(())
}

fn build_group_join_config() -> MlsGroupJoinConfig {
    MlsGroupJoinConfig::builder()
        .wire_format_policy(WireFormatPolicy::default())
        .max_past_epochs(MAX_PAST_EPOCHS)
        .use_ratchet_tree_extension(true)
        .build()
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use diesel::connection::SimpleConnection;
    use diesel::RunQueryDsl;
    use futures::future::join_all;
    use prost::Message;
    use std::sync::Arc;
    use wasm_bindgen_test::wasm_bindgen_test;
    use xmtp_common::assert_err;
    use xmtp_common::time::now_ns;
    use xmtp_content_types::{group_updated::GroupUpdatedCodec, ContentCodec};
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_proto::xmtp::mls::api::v1::group_message::Version;
    use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

    use crate::storage::group::StoredGroup;
    use crate::storage::schema::groups;
    use crate::{
        builder::ClientBuilder,
        groups::{
            build_dm_protected_metadata_extension, build_mutable_metadata_extension_default,
            build_protected_metadata_extension,
            group_metadata::GroupMetadata,
            group_mutable_metadata::MetadataField,
            intents::{PermissionPolicyOption, PermissionUpdateType},
            members::{GroupMember, PermissionLevel},
            mls_sync::GroupMessageProcessingError,
            validate_dm_group, DeliveryStatus, GroupError, GroupMetadataOptions,
            PreconfiguredPolicies, UpdateAdminListType,
        },
        storage::{
            consent_record::ConsentState,
            group::{ConversationType, GroupQueryArgs},
            group_intent::{IntentKind, IntentState},
            group_message::{GroupMessageKind, MsgQueryArgs, StoredGroupMessage},
            xmtp_openmls_provider::XmtpOpenMlsProvider,
        },
        utils::test::FullXmtpClient,
        InboxOwner, StreamHandle as _,
    };

    use super::{group_permissions::PolicySet, MlsGroup};

    async fn receive_group_invite(client: &FullXmtpClient) -> MlsGroup<FullXmtpClient> {
        client
            .sync_welcomes(&client.mls_provider().unwrap())
            .await
            .unwrap();
        let mut groups = client.find_groups(GroupQueryArgs::default()).unwrap();

        groups.remove(0)
    }

    async fn get_latest_message(group: &MlsGroup<FullXmtpClient>) -> StoredGroupMessage {
        group.sync().await.unwrap();
        let mut messages = group.find_messages(&MsgQueryArgs::default()).unwrap();
        messages.pop().unwrap()
    }

    // Adds a member to the group without the usual validations on group membership
    // Used for testing adversarial scenarios
    #[cfg(not(target_arch = "wasm32"))]
    async fn force_add_member(
        sender_client: &FullXmtpClient,
        new_member_client: &FullXmtpClient,
        sender_group: &MlsGroup<FullXmtpClient>,
        sender_mls_group: &mut openmls::prelude::MlsGroup,
        sender_provider: &XmtpOpenMlsProvider,
    ) {
        use super::intents::{Installation, SendWelcomesAction};
        use openmls::prelude::tls_codec::Serialize;
        let new_member_provider = new_member_client.mls_provider().unwrap();

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
                installation_key: new_member_client.installation_public_key().into(),
                hpke_public_key: hpke_init_key,
            }],
            serialized_welcome,
        );
        let messages = sender_group
            .prepare_group_messages(vec![serialized_commit.as_slice()])
            .unwrap();
        sender_client
            .api_client
            .send_group_messages(messages)
            .await
            .unwrap();
        sender_group
            .send_welcomes(send_welcomes_action)
            .await
            .unwrap();
    }

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "current_thread"))]
    async fn test_send_message() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;
        let group = client
            .create_group(None, GroupMetadataOptions::default())
            .expect("create group");
        group.send_message(b"hello").await.expect("send message");

        let messages = client
            .api_client
            .query_group_messages(group.group_id, None)
            .await
            .expect("read topic");
        assert_eq!(messages.len(), 2);
    }

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "current_thread"))]
    async fn test_receive_self_message() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;
        let group = client
            .create_group(None, GroupMetadataOptions::default())
            .expect("create group");
        let msg = b"hello";
        group.send_message(msg).await.expect("send message");

        group
            .receive(&client.store().conn().unwrap().into())
            .await
            .unwrap();
        // Check for messages
        let messages = group.find_messages(&MsgQueryArgs::default()).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages.first().unwrap().decrypted_message_bytes, msg);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test(flavor = "current_thread"))]
    async fn test_receive_message_from_other() {
        let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let alix_group = alix
            .create_group(None, GroupMetadataOptions::default())
            .expect("create group");
        alix_group
            .add_members_by_inbox_id(&[bo.inbox_id()])
            .await
            .unwrap();
        let alix_message = b"hello from alix";
        alix_group
            .send_message(alix_message)
            .await
            .expect("send message");

        let bo_group = receive_group_invite(&bo).await;
        let message = get_latest_message(&bo_group).await;
        assert_eq!(message.decrypted_message_bytes, alix_message);

        let bo_message = b"hello from bo";
        bo_group
            .send_message(bo_message)
            .await
            .expect("send message");

        let message = get_latest_message(&alix_group).await;
        assert_eq!(message.decrypted_message_bytes, bo_message);
    }

    // Test members function from non group creator
    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "current_thread"))]
    async fn test_members_func_from_non_creator() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let amal_group = amal
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        amal_group
            .add_members_by_inbox_id(&[bola.inbox_id()])
            .await
            .unwrap();

        // Get bola's version of the same group
        let bola_groups = bola
            .sync_welcomes(&bola.mls_provider().unwrap())
            .await
            .unwrap();
        let bola_group = bola_groups.first().unwrap();

        // Call sync for both
        amal_group.sync().await.unwrap();
        bola_group.sync().await.unwrap();

        // Verify bola can see the group name
        let bola_group_name = bola_group
            .group_name(&bola_group.mls_provider().unwrap())
            .unwrap();
        assert_eq!(bola_group_name, "");

        // Check if both clients can see the members correctly
        let amal_members: Vec<GroupMember> = amal_group.members().await.unwrap();
        let bola_members: Vec<GroupMember> = bola_group.members().await.unwrap();

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
    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "current_thread"))]
    async fn test_add_member_conflict() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let charlie = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let amal_group = amal
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        // Add bola
        amal_group
            .add_members_by_inbox_id(&[bola.inbox_id()])
            .await
            .unwrap();

        // Get bola's version of the same group
        let bola_groups = bola
            .sync_welcomes(&bola.mls_provider().unwrap())
            .await
            .unwrap();
        let bola_group = bola_groups.first().unwrap();
        bola_group.sync().await.unwrap();

        tracing::info!("Adding charlie from amal");
        // Have amal and bola both invite charlie.
        amal_group
            .add_members_by_inbox_id(&[charlie.inbox_id()])
            .await
            .expect("failed to add charlie");
        tracing::info!("Adding charlie from bola");
        bola_group
            .add_members_by_inbox_id(&[charlie.inbox_id()])
            .await
            .expect("bola's add should succeed in a no-op");

        amal_group
            .receive(&amal.store().conn().unwrap().into())
            .await
            .expect_err("expected error");

        // Check Amal's MLS group state.
        let amal_db = XmtpOpenMlsProvider::from(amal.context.store().conn().unwrap());
        let amal_members_len = amal_group
            .load_mls_group_with_lock(&amal_db, |mls_group| Ok(mls_group.members().count()))
            .unwrap();

        assert_eq!(amal_members_len, 3);

        // Check Bola's MLS group state.
        let bola_db = XmtpOpenMlsProvider::from(bola.context.store().conn().unwrap());
        let bola_members_len = bola_group
            .load_mls_group_with_lock(&bola_db, |mls_group| Ok(mls_group.members().count()))
            .unwrap();

        assert_eq!(bola_members_len, 3);

        let amal_uncommitted_intents = amal_db
            .conn_ref()
            .find_group_intents(
                amal_group.group_id.clone(),
                Some(vec![
                    IntentState::ToPublish,
                    IntentState::Published,
                    IntentState::Error,
                ]),
                None,
            )
            .unwrap();
        assert_eq!(amal_uncommitted_intents.len(), 0);

        let bola_failed_intents = bola_db
            .conn_ref()
            .find_group_intents(
                bola_group.group_id.clone(),
                Some(vec![IntentState::Error]),
                None,
            )
            .unwrap();
        // Bola's attempted add should be deleted, since it will have been a no-op on the second try
        assert_eq!(bola_failed_intents.len(), 0);

        // Make sure sending and receiving both worked
        amal_group
            .send_message("hello from amal".as_bytes())
            .await
            .unwrap();
        bola_group
            .send_message("hello from bola".as_bytes())
            .await
            .unwrap();

        let bola_messages = bola_group.find_messages(&MsgQueryArgs::default()).unwrap();
        let matching_message = bola_messages
            .iter()
            .find(|m| m.decrypted_message_bytes == "hello from amal".as_bytes());
        tracing::info!("found message: {:?}", bola_messages);
        assert!(matching_message.is_some());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg(not(target_arch = "wasm32"))]
    fn test_create_from_welcome_validation() {
        use crate::groups::{build_group_membership_extension, group_membership::GroupMembership};
        use xmtp_common::assert_logged;
        xmtp_common::traced_test!(async {
            tracing::info!("TEST");
            let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
            let bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;

            let alix_group = alix
                .create_group(None, GroupMetadataOptions::default())
                .unwrap();
            let provider = alix.mls_provider().unwrap();
            // Doctor the group membership
            let mut mls_group = alix_group
                .load_mls_group_with_lock(&provider, |mut mls_group| {
                    let mut existing_extensions = mls_group.extensions().clone();
                    let mut group_membership = GroupMembership::new();
                    group_membership.add("deadbeef".to_string(), 1);
                    existing_extensions
                        .add_or_replace(build_group_membership_extension(&group_membership));

                    mls_group
                        .update_group_context_extensions(
                            &provider,
                            existing_extensions.clone(),
                            &alix.identity().installation_keys,
                        )
                        .unwrap();
                    mls_group.merge_pending_commit(&provider).unwrap();

                    Ok(mls_group) // Return the updated group if necessary
                })
                .unwrap();

            // Now add bo to the group
            force_add_member(&alix, &bo, &alix_group, &mut mls_group, &provider).await;

            // Bo should not be able to actually read this group
            bo.sync_welcomes(&bo.mls_provider().unwrap()).await.unwrap();
            let groups = bo.find_groups(GroupQueryArgs::default()).unwrap();
            assert_eq!(groups.len(), 0);
            assert_logged!("failed to create group from welcome", 1);
        });
    }

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "current_thread"))]
    async fn test_dm_stitching() {
        let alix_wallet = generate_local_wallet();
        let alix = ClientBuilder::new_test_client(&alix_wallet).await;
        let alix_provider = alix.mls_provider().unwrap();
        let alix_conn = alix_provider.conn_ref();

        let bo_wallet = generate_local_wallet();
        let bo = ClientBuilder::new_test_client(&bo_wallet).await;

        let bo_dm = bo
            .create_dm_by_inbox_id(alix.inbox_id().to_string())
            .await
            .unwrap();
        let alix_dm = alix
            .create_dm_by_inbox_id(bo.inbox_id().to_string())
            .await
            .unwrap();

        bo_dm.send_message(b"Hello there").await.unwrap();
        alix_dm
            .send_message(b"No, let's use this dm")
            .await
            .unwrap();

        alix.sync_all_welcomes_and_groups(&alix_provider, None)
            .await
            .unwrap();

        // The dm shows up
        let alix_groups = alix_conn
            .raw_query_read( |conn| groups::table.load::<StoredGroup>(conn))
            .unwrap();
        assert_eq!(alix_groups.len(), 2);
        // They should have the same ID
        assert_eq!(alix_groups[0].dm_id, alix_groups[1].dm_id);

        // The dm is filtered out up
        let mut alix_filtered_groups = alix_conn.find_groups(GroupQueryArgs::default()).unwrap();
        assert_eq!(alix_filtered_groups.len(), 1);

        let dm_group = alix_filtered_groups.pop().unwrap();

        let now = now_ns();
        let one_second = 1_000_000_000;
        assert!(
            ((now - one_second)..(now + one_second)).contains(&dm_group.last_message_ns.unwrap())
        );

        let dm_group = alix.group(dm_group.id).unwrap();
        let alix_msgs = dm_group
            .find_messages(&MsgQueryArgs {
                kind: Some(GroupMessageKind::Application),
                ..Default::default()
            })
            .unwrap();

        assert_eq!(alix_msgs.len(), 2);

        let msg = String::from_utf8_lossy(&alix_msgs[0].decrypted_message_bytes);
        assert_eq!(msg, "Hello there");

        let msg = String::from_utf8_lossy(&alix_msgs[1].decrypted_message_bytes);
        assert_eq!(msg, "No, let's use this dm");
    }

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "current_thread"))]
    async fn test_add_inbox() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let client_2 = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let group = client
            .create_group(None, GroupMetadataOptions::default())
            .expect("create group");

        group
            .add_members_by_inbox_id(&[client_2.inbox_id()])
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

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "current_thread"))]
    async fn test_add_invalid_member() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let group = client
            .create_group(None, GroupMetadataOptions::default())
            .expect("create group");

        let result = group.add_members_by_inbox_id(&["1234".to_string()]).await;

        assert!(result.is_err());
    }

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "current_thread"))]
    async fn test_add_unregistered_member() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let unconnected_wallet_address = generate_local_wallet().get_address();
        let group = amal
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        let result = group.add_members(&[unconnected_wallet_address]).await;

        assert!(result.is_err());
    }

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "current_thread"))]
    async fn test_remove_inbox() {
        let client_1 = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        // Add another client onto the network
        let client_2 = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let group = client_1
            .create_group(None, GroupMetadataOptions::default())
            .expect("create group");
        group
            .add_members_by_inbox_id(&[client_2.inbox_id()])
            .await
            .expect("group create failure");

        let messages_with_add = group.find_messages(&MsgQueryArgs::default()).unwrap();
        assert_eq!(messages_with_add.len(), 1);

        // Try and add another member without merging the pending commit
        group
            .remove_members_by_inbox_id(&[client_2.inbox_id()])
            .await
            .expect("group remove members failure");

        let messages_with_remove = group.find_messages(&MsgQueryArgs::default()).unwrap();
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

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "current_thread"))]
    async fn test_key_update() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola_client = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let group = client
            .create_group(None, GroupMetadataOptions::default())
            .expect("create group");
        group
            .add_members_by_inbox_id(&[bola_client.inbox_id()])
            .await
            .unwrap();

        group.key_update().await.unwrap();

        let messages = client
            .api_client
            .query_group_messages(group.group_id.clone(), None)
            .await
            .unwrap();
        assert_eq!(messages.len(), 2);

        let provider: XmtpOpenMlsProvider = client.context.store().conn().unwrap().into();
        let pending_commit_is_none = group
            .load_mls_group_with_lock(&provider, |mls_group| {
                Ok(mls_group.pending_commit().is_none())
            })
            .unwrap();

        assert!(pending_commit_is_none);

        group.send_message(b"hello").await.expect("send message");

        bola_client
            .sync_welcomes(&bola_client.mls_provider().unwrap())
            .await
            .unwrap();
        let bola_groups = bola_client.find_groups(GroupQueryArgs::default()).unwrap();
        let bola_group = bola_groups.first().unwrap();
        bola_group.sync().await.unwrap();
        let bola_messages = bola_group.find_messages(&MsgQueryArgs::default()).unwrap();
        assert_eq!(bola_messages.len(), 1);
    }

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "current_thread"))]
    async fn test_post_commit() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let client_2 = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let group = client
            .create_group(None, GroupMetadataOptions::default())
            .expect("create group");

        group
            .add_members_by_inbox_id(&[client_2.inbox_id()])
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

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "current_thread"))]
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
            .add_members(&[bola_wallet.get_address(), charlie_wallet.get_address()])
            .await
            .unwrap();
        tracing::info!("created the group with 2 additional members");
        assert_eq!(group.members().await.unwrap().len(), 3);
        let messages = group.find_messages(&MsgQueryArgs::default()).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].kind, GroupMessageKind::MembershipChange);
        let encoded_content =
            EncodedContent::decode(messages[0].decrypted_message_bytes.as_slice()).unwrap();
        let group_update = GroupUpdatedCodec::decode(encoded_content).unwrap();
        assert_eq!(group_update.added_inboxes.len(), 2);
        assert_eq!(group_update.removed_inboxes.len(), 0);

        group
            .remove_members(&[bola_wallet.get_address()])
            .await
            .unwrap();
        assert_eq!(group.members().await.unwrap().len(), 2);
        tracing::info!("removed bola");
        let messages = group.find_messages(&MsgQueryArgs::default()).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[1].kind, GroupMessageKind::MembershipChange);
        let encoded_content =
            EncodedContent::decode(messages[1].decrypted_message_bytes.as_slice()).unwrap();
        let group_update = GroupUpdatedCodec::decode(encoded_content).unwrap();
        assert_eq!(group_update.added_inboxes.len(), 0);
        assert_eq!(group_update.removed_inboxes.len(), 1);

        let bola_group = receive_group_invite(&bola).await;
        bola_group.sync().await.unwrap();
        assert!(!bola_group
            .is_active(&bola_group.mls_provider().unwrap())
            .unwrap())
    }

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "current_thread"))]
    async fn test_removed_members_cannot_send_message_to_others() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola_wallet = &generate_local_wallet();
        let bola = ClientBuilder::new_test_client(bola_wallet).await;
        let charlie_wallet = &generate_local_wallet();
        let charlie = ClientBuilder::new_test_client(charlie_wallet).await;

        let amal_group = amal
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        amal_group
            .add_members(&[bola_wallet.get_address(), charlie_wallet.get_address()])
            .await
            .unwrap();
        assert_eq!(amal_group.members().await.unwrap().len(), 3);

        amal_group
            .remove_members(&[bola_wallet.get_address()])
            .await
            .unwrap();
        assert_eq!(amal_group.members().await.unwrap().len(), 2);
        assert!(amal_group
            .members()
            .await
            .unwrap()
            .iter()
            .all(|m| m.inbox_id != bola.inbox_id()));
        assert!(amal_group
            .members()
            .await
            .unwrap()
            .iter()
            .any(|m| m.inbox_id == charlie.inbox_id()));

        amal_group.sync().await.expect("sync failed");

        let message_text = b"hello";

        let bola_group = MlsGroup::<FullXmtpClient>::new(
            bola.clone(),
            amal_group.group_id.clone(),
            amal_group.created_at_ns,
        );
        bola_group
            .send_message(message_text)
            .await
            .expect_err("expected send_message to fail");

        amal_group.sync().await.expect("sync failed");
        amal_group.sync().await.expect("sync failed");

        let amal_messages = amal_group
            .find_messages(&MsgQueryArgs {
                kind: Some(GroupMessageKind::Application),
                ..Default::default()
            })
            .unwrap()
            .into_iter()
            .collect::<Vec<StoredGroupMessage>>();

        assert!(amal_messages.is_empty());
    }

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "current_thread"))]
    async fn test_add_missing_installations() {
        // Setup for test
        let amal_wallet = generate_local_wallet();
        let amal = ClientBuilder::new_test_client(&amal_wallet).await;
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let group = amal
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        group
            .add_members_by_inbox_id(&[bola.inbox_id()])
            .await
            .unwrap();

        assert_eq!(group.members().await.unwrap().len(), 2);

        let provider: XmtpOpenMlsProvider = amal.context.store().conn().unwrap().into();
        // Finished with setup

        // add a second installation for amal using the same wallet
        let _amal_2nd = ClientBuilder::new_test_client(&amal_wallet).await;

        // test if adding the new installation(s) worked
        let new_installations_were_added = group.add_missing_installations(&provider).await;
        assert!(new_installations_were_added.is_ok());

        group.sync().await.unwrap();
        let num_members = group
            .load_mls_group_with_lock(&provider, |mls_group| {
                Ok(mls_group.members().collect::<Vec<_>>().len())
            })
            .unwrap();

        assert_eq!(num_members, 3);
    }

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "multi_thread", worker_threads = 10))]
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
            .add_members_by_inbox_id(&[bola.inbox_id()])
            .await
            .unwrap();

        let bola_group = receive_group_invite(&bola).await;
        bola_group.sync().await.unwrap();
        // Both Amal and Bola are up to date on the group state. Now each of them want to add someone else
        amal_group
            .add_members_by_inbox_id(&[charlie.inbox_id()])
            .await
            .unwrap();

        bola_group
            .add_members_by_inbox_id(&[dave.inbox_id()])
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

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "current_thread"))]
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
            .add_members_by_inbox_id(&[bola.inbox_id()])
            .await
            .unwrap();

        let bola_group = receive_group_invite(&bola).await;
        bola_group.sync().await.unwrap();
        assert!(bola_group
            .add_members_by_inbox_id(&[charlie.inbox_id()])
            .await
            .is_err(),);
    }

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "current_thread"))]
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
                    message_expiration_from_ms: None,
                    message_expiration_ms: None,
                },
            )
            .unwrap();

        let binding = amal_group
            .mutable_metadata(&amal_group.mls_provider().unwrap())
            .expect("msg");
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

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "current_thread"))]
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
        amal_group.add_members(&clients).await.unwrap();
        let bola_wallet = generate_local_wallet();
        ClientBuilder::new_test_client(&bola_wallet).await;
        assert!(amal_group
            .add_members_by_inbox_id(&[bola_wallet.get_address()])
            .await
            .is_err(),);
    }

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "current_thread"))]
    async fn test_group_mutable_data() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        // Create a group and verify it has the default group name
        let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
        let amal_group = amal
            .create_group(policy_set, GroupMetadataOptions::default())
            .unwrap();
        amal_group.sync().await.unwrap();

        let group_mutable_metadata = amal_group
            .mutable_metadata(&amal_group.mls_provider().unwrap())
            .unwrap();
        assert!(group_mutable_metadata.attributes.len().eq(&4));
        assert!(group_mutable_metadata
            .attributes
            .get(&MetadataField::GroupName.to_string())
            .unwrap()
            .is_empty());

        // Add bola to the group
        amal_group
            .add_members_by_inbox_id(&[bola.inbox_id()])
            .await
            .unwrap();
        bola.sync_welcomes(&bola.mls_provider().unwrap())
            .await
            .unwrap();

        let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
        assert_eq!(bola_groups.len(), 1);
        let bola_group = bola_groups.first().unwrap();
        bola_group.sync().await.unwrap();
        let group_mutable_metadata = bola_group
            .mutable_metadata(&bola_group.mls_provider().unwrap())
            .unwrap();
        assert!(group_mutable_metadata
            .attributes
            .get(&MetadataField::GroupName.to_string())
            .unwrap()
            .is_empty());

        // Update group name
        amal_group
            .update_group_name("New Group Name 1".to_string())
            .await
            .unwrap();

        amal_group.send_message("hello".as_bytes()).await.unwrap();

        // Verify amal group sees update
        amal_group.sync().await.unwrap();
        let binding = amal_group
            .mutable_metadata(&amal_group.mls_provider().unwrap())
            .expect("msg");
        let amal_group_name: &String = binding
            .attributes
            .get(&MetadataField::GroupName.to_string())
            .unwrap();
        assert_eq!(amal_group_name, "New Group Name 1");

        // Verify bola group sees update
        bola_group.sync().await.unwrap();
        let binding = bola_group
            .mutable_metadata(&bola_group.mls_provider().unwrap())
            .expect("msg");
        let bola_group_name: &String = binding
            .attributes
            .get(&MetadataField::GroupName.to_string())
            .unwrap();
        assert_eq!(bola_group_name, "New Group Name 1");

        // Verify that bola can not update the group name since they are not the creator
        bola_group
            .update_group_name("New Group Name 2".to_string())
            .await
            .expect_err("expected err");

        // Verify bola group does not see an update
        bola_group.sync().await.unwrap();
        let binding = bola_group
            .mutable_metadata(&bola_group.mls_provider().unwrap())
            .expect("msg");
        let bola_group_name: &String = binding
            .attributes
            .get(&MetadataField::GroupName.to_string())
            .unwrap();
        assert_eq!(bola_group_name, "New Group Name 1");
    }

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "current_thread"))]
    async fn test_update_policies_empty_group() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola_wallet = generate_local_wallet();
        let _bola = ClientBuilder::new_test_client(&bola_wallet).await;

        // Create a group with amal and bola
        let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
        let amal_group = amal
            .create_group_with_members(
                &[bola_wallet.get_address()],
                policy_set,
                GroupMetadataOptions::default(),
            )
            .await
            .unwrap();

        // Verify we can update the group name without syncing first
        amal_group
            .update_group_name("New Group Name 1".to_string())
            .await
            .unwrap();

        // Verify the name is updated
        amal_group.sync().await.unwrap();
        let group_mutable_metadata = amal_group
            .mutable_metadata(&amal_group.mls_provider().unwrap())
            .unwrap();
        let group_name_1 = group_mutable_metadata
            .attributes
            .get(&MetadataField::GroupName.to_string())
            .unwrap();
        assert_eq!(group_name_1, "New Group Name 1");

        // Create a group with just amal
        let policy_set_2 = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
        let amal_group_2 = amal
            .create_group(policy_set_2, GroupMetadataOptions::default())
            .unwrap();

        // Verify empty group fails to update metadata before syncing
        amal_group_2
            .update_group_name("New Group Name 2".to_string())
            .await
            .expect_err("Should fail to update group name before first sync");

        // Sync the group
        amal_group_2.sync().await.unwrap();

        //Verify we can now update the group name
        amal_group_2
            .update_group_name("New Group Name 2".to_string())
            .await
            .unwrap();

        // Verify the name is updated
        amal_group_2.sync().await.unwrap();
        let group_mutable_metadata = amal_group_2
            .mutable_metadata(&amal_group_2.mls_provider().unwrap())
            .unwrap();
        let group_name_2 = group_mutable_metadata
            .attributes
            .get(&MetadataField::GroupName.to_string())
            .unwrap();
        assert_eq!(group_name_2, "New Group Name 2");
    }

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "current_thread"))]
    async fn test_update_group_image_url_square() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        // Create a group and verify it has the default group name
        let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
        let amal_group = amal
            .create_group(policy_set, GroupMetadataOptions::default())
            .unwrap();
        amal_group.sync().await.unwrap();

        let group_mutable_metadata = amal_group
            .mutable_metadata(&amal_group.mls_provider().unwrap())
            .unwrap();
        assert!(group_mutable_metadata
            .attributes
            .get(&MetadataField::GroupImageUrlSquare.to_string())
            .unwrap()
            .is_empty());

        // Update group name
        amal_group
            .update_group_image_url_square("a url".to_string())
            .await
            .unwrap();

        // Verify amal group sees update
        amal_group.sync().await.unwrap();
        let binding = amal_group
            .mutable_metadata(&amal_group.mls_provider().unwrap())
            .expect("msg");
        let amal_group_image_url: &String = binding
            .attributes
            .get(&MetadataField::GroupImageUrlSquare.to_string())
            .unwrap();
        assert_eq!(amal_group_image_url, "a url");
    }

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "current_thread"))]
    async fn test_update_group_pinned_frame_url() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        // Create a group and verify it has the default group name
        let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
        let amal_group = amal
            .create_group(policy_set, GroupMetadataOptions::default())
            .unwrap();
        amal_group.sync().await.unwrap();

        let group_mutable_metadata = amal_group
            .mutable_metadata(&amal_group.mls_provider().unwrap())
            .unwrap();
        assert!(group_mutable_metadata
            .attributes
            .get(&MetadataField::GroupPinnedFrameUrl.to_string())
            .unwrap()
            .is_empty());

        // Update group name
        amal_group
            .update_group_pinned_frame_url("a frame url".to_string())
            .await
            .unwrap();

        // Verify amal group sees update
        amal_group.sync().await.unwrap();
        let binding = amal_group
            .mutable_metadata(&amal_group.mls_provider().unwrap())
            .expect("msg");
        let amal_group_pinned_frame_url: &String = binding
            .attributes
            .get(&MetadataField::GroupPinnedFrameUrl.to_string())
            .unwrap();
        assert_eq!(amal_group_pinned_frame_url, "a frame url");
    }

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "current_thread"))]
    async fn test_group_mutable_data_group_permissions() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola_wallet = generate_local_wallet();
        let bola = ClientBuilder::new_test_client(&bola_wallet).await;

        // Create a group and verify it has the default group name
        let policy_set = Some(PreconfiguredPolicies::Default.to_policy_set());
        let amal_group = amal
            .create_group(policy_set, GroupMetadataOptions::default())
            .unwrap();
        amal_group.sync().await.unwrap();

        let group_mutable_metadata = amal_group
            .mutable_metadata(&amal_group.mls_provider().unwrap())
            .unwrap();
        assert!(group_mutable_metadata
            .attributes
            .get(&MetadataField::GroupName.to_string())
            .unwrap()
            .is_empty());

        // Add bola to the group
        amal_group
            .add_members(&[bola_wallet.get_address()])
            .await
            .unwrap();
        bola.sync_welcomes(&bola.mls_provider().unwrap())
            .await
            .unwrap();
        let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
        assert_eq!(bola_groups.len(), 1);
        let bola_group = bola_groups.first().unwrap();
        bola_group.sync().await.unwrap();
        let group_mutable_metadata = bola_group
            .mutable_metadata(&bola_group.mls_provider().unwrap())
            .unwrap();
        assert!(group_mutable_metadata
            .attributes
            .get(&MetadataField::GroupName.to_string())
            .unwrap()
            .is_empty());

        // Update group name
        amal_group
            .update_group_name("New Group Name 1".to_string())
            .await
            .unwrap();

        // Verify amal group sees update
        amal_group.sync().await.unwrap();
        let binding = amal_group
            .mutable_metadata(&amal_group.mls_provider().unwrap())
            .unwrap();
        let amal_group_name: &String = binding
            .attributes
            .get(&MetadataField::GroupName.to_string())
            .unwrap();
        assert_eq!(amal_group_name, "New Group Name 1");

        // Verify bola group sees update
        bola_group.sync().await.unwrap();
        let binding = bola_group
            .mutable_metadata(&bola_group.mls_provider().unwrap())
            .expect("msg");
        let bola_group_name: &String = binding
            .attributes
            .get(&MetadataField::GroupName.to_string())
            .unwrap();
        assert_eq!(bola_group_name, "New Group Name 1");

        // Verify that bola CAN update the group name since everyone is admin for this group
        bola_group
            .update_group_name("New Group Name 2".to_string())
            .await
            .expect("non creator failed to udpate group name");

        // Verify amal group sees an update
        amal_group.sync().await.unwrap();
        let binding = amal_group
            .mutable_metadata(&amal_group.mls_provider().unwrap())
            .expect("msg");
        let amal_group_name: &String = binding
            .attributes
            .get(&MetadataField::GroupName.to_string())
            .unwrap();
        assert_eq!(amal_group_name, "New Group Name 2");
    }

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "current_thread"))]
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
        amal_group.sync().await.unwrap();

        // Add bola to the group
        amal_group
            .add_members(&[bola_wallet.get_address()])
            .await
            .unwrap();
        bola.sync_welcomes(&bola.mls_provider().unwrap())
            .await
            .unwrap();
        let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
        assert_eq!(bola_groups.len(), 1);
        let bola_group = bola_groups.first().unwrap();
        bola_group.sync().await.unwrap();

        // Verify Amal is the only admin and super admin
        let provider = amal_group.mls_provider().unwrap();
        let admin_list = amal_group.admin_list(&provider).unwrap();
        let super_admin_list = amal_group.super_admin_list(&provider).unwrap();
        drop(provider); // allow connection to be cleaned
        assert_eq!(admin_list.len(), 0);
        assert_eq!(super_admin_list.len(), 1);
        assert!(super_admin_list.contains(&amal.inbox_id().to_string()));

        // Verify that bola can not add caro because they are not an admin
        bola.sync_welcomes(&bola.mls_provider().unwrap())
            .await
            .unwrap();
        let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
        assert_eq!(bola_groups.len(), 1);
        let bola_group: &MlsGroup<_> = bola_groups.first().unwrap();
        bola_group.sync().await.unwrap();
        bola_group
            .add_members_by_inbox_id(&[caro.inbox_id()])
            .await
            .expect_err("expected err");

        // Add bola as an admin
        amal_group
            .update_admin_list(UpdateAdminListType::Add, bola.inbox_id().to_string())
            .await
            .unwrap();
        amal_group.sync().await.unwrap();
        bola_group.sync().await.unwrap();
        assert_eq!(
            bola_group
                .admin_list(&bola_group.mls_provider().unwrap())
                .unwrap()
                .len(),
            1
        );
        assert!(bola_group
            .admin_list(&bola_group.mls_provider().unwrap())
            .unwrap()
            .contains(&bola.inbox_id().to_string()));

        // Verify that bola can now add caro because they are an admin
        bola_group
            .add_members_by_inbox_id(&[caro.inbox_id()])
            .await
            .unwrap();

        bola_group.sync().await.unwrap();

        // Verify that bola can not remove amal as a super admin, because
        // Remove admin is super admin only permissions
        bola_group
            .update_admin_list(
                UpdateAdminListType::RemoveSuper,
                amal.inbox_id().to_string(),
            )
            .await
            .expect_err("expected err");

        // Now amal removes bola as an admin
        amal_group
            .update_admin_list(UpdateAdminListType::Remove, bola.inbox_id().to_string())
            .await
            .unwrap();
        amal_group.sync().await.unwrap();
        bola_group.sync().await.unwrap();
        assert_eq!(
            bola_group
                .admin_list(&bola_group.mls_provider().unwrap())
                .unwrap()
                .len(),
            0
        );
        assert!(!bola_group
            .admin_list(&bola_group.mls_provider().unwrap())
            .unwrap()
            .contains(&bola.inbox_id().to_string()));

        // Verify that bola can not add charlie because they are not an admin
        bola.sync_welcomes(&bola.mls_provider().unwrap())
            .await
            .unwrap();
        let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
        assert_eq!(bola_groups.len(), 1);
        let bola_group: &MlsGroup<_> = bola_groups.first().unwrap();
        bola_group.sync().await.unwrap();
        bola_group
            .add_members_by_inbox_id(&[charlie.inbox_id()])
            .await
            .expect_err("expected err");
    }

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "current_thread"))]
    async fn test_group_super_admin_list_update() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let caro = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
        let amal_group = amal
            .create_group(policy_set, GroupMetadataOptions::default())
            .unwrap();
        amal_group.sync().await.unwrap();

        // Add bola to the group
        amal_group
            .add_members_by_inbox_id(&[bola.inbox_id()])
            .await
            .unwrap();
        bola.sync_welcomes(&bola.mls_provider().unwrap())
            .await
            .unwrap();
        let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
        assert_eq!(bola_groups.len(), 1);
        let bola_group = bola_groups.first().unwrap();
        bola_group.sync().await.unwrap();

        // Verify Amal is the only super admin
        let provider = amal_group.mls_provider().unwrap();
        let admin_list = amal_group.admin_list(&provider).unwrap();
        let super_admin_list = amal_group.super_admin_list(&provider).unwrap();
        drop(provider); // allow connection to be re-added to pool
        assert_eq!(admin_list.len(), 0);
        assert_eq!(super_admin_list.len(), 1);
        assert!(super_admin_list.contains(&amal.inbox_id().to_string()));

        // Verify that bola can not add caro as an admin because they are not a super admin
        bola.sync_welcomes(&bola.mls_provider().unwrap())
            .await
            .unwrap();
        let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();

        assert_eq!(bola_groups.len(), 1);
        let bola_group: &MlsGroup<_> = bola_groups.first().unwrap();
        bola_group.sync().await.unwrap();
        bola_group
            .update_admin_list(UpdateAdminListType::Add, caro.inbox_id().to_string())
            .await
            .expect_err("expected err");

        // Add bola as a super admin
        amal_group
            .update_admin_list(UpdateAdminListType::AddSuper, bola.inbox_id().to_string())
            .await
            .unwrap();
        amal_group.sync().await.unwrap();
        bola_group.sync().await.unwrap();
        let provider = bola_group.mls_provider().unwrap();
        assert_eq!(bola_group.super_admin_list(&provider).unwrap().len(), 2);
        assert!(bola_group
            .super_admin_list(&provider)
            .unwrap()
            .contains(&bola.inbox_id().to_string()));
        drop(provider); // allow connection to be re-added to pool

        // Verify that bola can now add caro as an admin
        bola_group
            .update_admin_list(UpdateAdminListType::Add, caro.inbox_id().to_string())
            .await
            .unwrap();
        bola_group.sync().await.unwrap();
        let provider = bola_group.mls_provider().unwrap();
        assert_eq!(bola_group.admin_list(&provider).unwrap().len(), 1);
        assert!(bola_group
            .admin_list(&provider)
            .unwrap()
            .contains(&caro.inbox_id().to_string()));
        drop(provider); // allow connection to be re-added to pool

        // Verify that no one can remove a super admin from a group
        amal_group
            .remove_members(&[bola.inbox_id().to_string()])
            .await
            .expect_err("expected err");

        // Verify that bola can now remove themself as a super admin
        bola_group
            .update_admin_list(
                UpdateAdminListType::RemoveSuper,
                bola.inbox_id().to_string(),
            )
            .await
            .unwrap();
        bola_group.sync().await.unwrap();
        let provider = bola_group.mls_provider().unwrap();
        assert_eq!(bola_group.super_admin_list(&provider).unwrap().len(), 1);
        assert!(!bola_group
            .super_admin_list(&provider)
            .unwrap()
            .contains(&bola.inbox_id().to_string()));
        drop(provider); // allow connection to be re-added to pool

        // Verify that amal can NOT remove themself as a super admin because they are the only remaining
        amal_group
            .update_admin_list(
                UpdateAdminListType::RemoveSuper,
                amal.inbox_id().to_string(),
            )
            .await
            .expect_err("expected err");
    }

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "current_thread"))]
    async fn test_group_members_permission_level_update() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let caro = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
        let amal_group = amal
            .create_group(policy_set, GroupMetadataOptions::default())
            .unwrap();
        amal_group.sync().await.unwrap();

        // Add Bola and Caro to the group
        amal_group
            .add_members_by_inbox_id(&[bola.inbox_id(), caro.inbox_id()])
            .await
            .unwrap();
        amal_group.sync().await.unwrap();

        // Initial checks for group members
        let initial_members = amal_group.members().await.unwrap();
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
            .update_admin_list(UpdateAdminListType::Add, bola.inbox_id().to_string())
            .await
            .unwrap();
        amal_group.sync().await.unwrap();

        // Check after adding Bola as an admin
        let members = amal_group.members().await.unwrap();
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
            .update_admin_list(UpdateAdminListType::AddSuper, caro.inbox_id().to_string())
            .await
            .unwrap();
        amal_group.sync().await.unwrap();

        // Check after adding Caro as a super admin
        let members = amal_group.members().await.unwrap();
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

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "current_thread"))]
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
            .add_members_by_inbox_id(&[bola.inbox_id()])
            .await
            .unwrap();

        // Bola syncs groups - this will decrypt the Welcome, identify who added Bola
        // and then store that value on the group and insert into the database
        let bola_groups = bola
            .sync_welcomes(&bola.mls_provider().unwrap())
            .await
            .unwrap();

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

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "current_thread"))]
    async fn test_can_read_group_creator_inbox_id() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let policy_set = Some(PreconfiguredPolicies::Default.to_policy_set());
        let amal_group = amal
            .create_group(policy_set, GroupMetadataOptions::default())
            .unwrap();
        amal_group.sync().await.unwrap();

        let mutable_metadata = amal_group
            .mutable_metadata(&amal_group.mls_provider().unwrap())
            .unwrap();
        assert_eq!(mutable_metadata.super_admin_list.len(), 1);
        assert_eq!(mutable_metadata.super_admin_list[0], amal.inbox_id());

        let protected_metadata: GroupMetadata = amal_group
            .metadata(&amal_group.mls_provider().unwrap())
            .await
            .unwrap();
        assert_eq!(
            protected_metadata.conversation_type,
            ConversationType::Group
        );

        assert_eq!(protected_metadata.creator_inbox_id, amal.inbox_id());
    }

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "current_thread"))]
    async fn test_can_update_gce_after_failed_commit() {
        // Step 1: Amal creates a group
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let policy_set = Some(PreconfiguredPolicies::Default.to_policy_set());
        let amal_group = amal
            .create_group(policy_set, GroupMetadataOptions::default())
            .unwrap();
        amal_group.sync().await.unwrap();

        // Step 2:  Amal adds Bola to the group
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        amal_group
            .add_members_by_inbox_id(&[bola.inbox_id()])
            .await
            .unwrap();

        // Step 3: Verify that Bola can update the group name, and amal sees the update
        bola.sync_welcomes(&bola.mls_provider().unwrap())
            .await
            .unwrap();
        let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
        let bola_group: &MlsGroup<_> = bola_groups.first().unwrap();
        bola_group.sync().await.unwrap();
        bola_group
            .update_group_name("Name Update 1".to_string())
            .await
            .unwrap();
        amal_group.sync().await.unwrap();
        let name = amal_group
            .group_name(&amal_group.mls_provider().unwrap())
            .unwrap();
        assert_eq!(name, "Name Update 1");

        // Step 4:  Bola attempts an action that they do not have permissions for like add admin, fails as expected
        let result = bola_group
            .update_admin_list(UpdateAdminListType::Add, bola.inbox_id().to_string())
            .await;
        if let Err(e) = &result {
            eprintln!("Error updating admin list: {:?}", e);
        }
        // Step 5: Now have Bola attempt to update the group name again
        bola_group
            .update_group_name("Name Update 2".to_string())
            .await
            .unwrap();

        // Step 6: Verify that both clients can sync without error and that the group name has been updated
        amal_group.sync().await.unwrap();
        bola_group.sync().await.unwrap();
        let binding = amal_group
            .mutable_metadata(&amal_group.mls_provider().unwrap())
            .expect("msg");
        let amal_group_name: &String = binding
            .attributes
            .get(&MetadataField::GroupName.to_string())
            .unwrap();
        assert_eq!(amal_group_name, "Name Update 2");
        let binding = bola_group
            .mutable_metadata(&bola_group.mls_provider().unwrap())
            .expect("msg");
        let bola_group_name: &String = binding
            .attributes
            .get(&MetadataField::GroupName.to_string())
            .unwrap();
        assert_eq!(bola_group_name, "Name Update 2");
    }

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "current_thread"))]
    async fn test_can_update_permissions_after_group_creation() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
        let amal_group: MlsGroup<_> = amal
            .create_group(policy_set, GroupMetadataOptions::default())
            .unwrap();

        // Step 2:  Amal adds Bola to the group
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        amal_group
            .add_members_by_inbox_id(&[bola.inbox_id()])
            .await
            .unwrap();

        // Step 3: Bola attemps to add Caro, but fails because group is admin only
        let caro = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        bola.sync_welcomes(&bola.mls_provider().unwrap())
            .await
            .unwrap();
        let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();

        let bola_group: &MlsGroup<_> = bola_groups.first().unwrap();
        bola_group.sync().await.unwrap();
        let result = bola_group.add_members_by_inbox_id(&[caro.inbox_id()]).await;
        if let Err(e) = &result {
            eprintln!("Error adding member: {:?}", e);
        } else {
            panic!("Expected error adding member");
        }

        // Step 4: Bola attempts to update permissions but fails because they are not a super admin
        let result = bola_group
            .update_permission_policy(
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
                PermissionUpdateType::AddMember,
                PermissionPolicyOption::Allow,
                None,
            )
            .await
            .unwrap();

        // Step 6: Bola can now add Caro to the group
        bola_group
            .add_members_by_inbox_id(&[caro.inbox_id()])
            .await
            .unwrap();
        bola_group.sync().await.unwrap();
        let members = bola_group.members().await.unwrap();
        assert_eq!(members.len(), 3);
    }

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "current_thread"))]
    async fn test_optimistic_send() {
        let amal = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let bola_wallet = generate_local_wallet();
        let bola = Arc::new(ClientBuilder::new_test_client(&bola_wallet).await);
        let amal_group = amal
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        amal_group.sync().await.unwrap();
        // Add bola to the group
        amal_group
            .add_members(&[bola_wallet.get_address()])
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
            .find_messages(&MsgQueryArgs {
                kind: Some(GroupMessageKind::Application),
                ..Default::default()
            })
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

        amal_group.publish_messages().await.unwrap();
        bola_group.sync().await.unwrap();

        let messages = bola_group.find_messages(&MsgQueryArgs::default()).unwrap();
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

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "current_thread"))]
    async fn test_dm_creation() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let caro = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        // Amal creates a dm group targetting bola
        let amal_dm = amal
            .create_dm_by_inbox_id(bola.inbox_id().to_string())
            .await
            .unwrap();

        // Amal can not add caro to the dm group
        let result = amal_dm.add_members_by_inbox_id(&[caro.inbox_id()]).await;
        assert!(result.is_err());

        // Bola is already a member
        let result = amal_dm
            .add_members_by_inbox_id(&[bola.inbox_id(), caro.inbox_id()])
            .await;
        assert!(result.is_err());
        amal_dm.sync().await.unwrap();
        let members = amal_dm.members().await.unwrap();
        assert_eq!(members.len(), 2);

        // Bola can message amal
        let _ = bola.sync_welcomes(&bola.mls_provider().unwrap()).await;
        let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();

        let bola_dm: &MlsGroup<_> = bola_groups.first().unwrap();
        bola_dm.send_message(b"test one").await.unwrap();

        // Amal sync and reads message
        amal_dm.sync().await.unwrap();
        let messages = amal_dm.find_messages(&MsgQueryArgs::default()).unwrap();
        assert_eq!(messages.len(), 2);
        let message = messages.last().unwrap();
        assert_eq!(message.decrypted_message_bytes, b"test one");

        // Amal can not remove bola
        let result = amal_dm.remove_members_by_inbox_id(&[bola.inbox_id()]).await;
        assert!(result.is_err());
        amal_dm.sync().await.unwrap();
        let members = amal_dm.members().await.unwrap();
        assert_eq!(members.len(), 2);

        // Neither Amal nor Bola is an admin or super admin
        amal_dm.sync().await.unwrap();
        bola_dm.sync().await.unwrap();
        let is_amal_admin = amal_dm
            .is_admin(amal.inbox_id().to_string(), &amal.mls_provider().unwrap())
            .unwrap();
        let is_bola_admin = amal_dm
            .is_admin(bola.inbox_id().to_string(), &bola.mls_provider().unwrap())
            .unwrap();
        let is_amal_super_admin = amal_dm
            .is_super_admin(amal.inbox_id().to_string(), &amal.mls_provider().unwrap())
            .unwrap();
        let is_bola_super_admin = amal_dm
            .is_super_admin(bola.inbox_id().to_string(), &bola.mls_provider().unwrap())
            .unwrap();
        assert!(!is_amal_admin);
        assert!(!is_bola_admin);
        assert!(!is_amal_super_admin);
        assert!(!is_bola_super_admin);
    }

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "current_thread"))]
    async fn process_messages_abort_on_retryable_error() {
        let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let alix_group = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();

        alix_group
            .add_members_by_inbox_id(&[bo.inbox_id()])
            .await
            .unwrap();

        // Create two commits
        alix_group
            .update_group_name("foo".to_string())
            .await
            .unwrap();
        alix_group
            .update_group_name("bar".to_string())
            .await
            .unwrap();

        let bo_group = receive_group_invite(&bo).await;
        // Get the group messages before we lock the DB, simulating an error that happens
        // in the middle of a sync instead of the beginning
        let bo_messages = bo
            .query_group_messages(&bo_group.group_id, &bo.store().conn().unwrap())
            .await
            .unwrap();

        let conn_1: XmtpOpenMlsProvider = bo.store().conn().unwrap().into();
        let conn_2 = bo.store().conn().unwrap();
        conn_2
            .raw_query_read( |c| {
                c.batch_execute("BEGIN EXCLUSIVE").unwrap();
                Ok::<_, diesel::result::Error>(())
            })
            .unwrap();

        let process_result = bo_group.process_messages(bo_messages, &conn_1).await;
        if let Some(GroupError::ReceiveErrors(errors)) = process_result.err() {
            assert_eq!(errors.len(), 1);
            assert!(errors
                .first()
                .unwrap()
                .to_string()
                .contains("database is locked"));
        } else {
            panic!("Expected error")
        }
    }

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "current_thread"))]
    async fn skip_already_processed_messages() {
        let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let bo_wallet = generate_local_wallet();
        let bo_client = ClientBuilder::new_test_client(&bo_wallet).await;

        let alix_group = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();

        alix_group
            .add_members_by_inbox_id(&[bo_client.inbox_id()])
            .await
            .unwrap();

        let alix_message = vec![1];
        alix_group.send_message(&alix_message).await.unwrap();
        bo_client
            .sync_welcomes(&bo_client.mls_provider().unwrap())
            .await
            .unwrap();
        let bo_groups = bo_client.find_groups(GroupQueryArgs::default()).unwrap();
        let bo_group = bo_groups.first().unwrap();

        let mut bo_messages_from_api = bo_client
            .query_group_messages(&bo_group.group_id, &bo_client.store().conn().unwrap())
            .await
            .unwrap();

        // override the messages to contain already processed messaged
        for msg in &mut bo_messages_from_api {
            if let Some(Version::V1(ref mut v1)) = msg.version {
                v1.id = 0;
            }
        }

        let process_result = bo_group
            .process_messages(bo_messages_from_api, &bo_client.mls_provider().unwrap())
            .await;
        if let Some(GroupError::ReceiveErrors(errors)) = process_result.err() {
            assert_eq!(errors.len(), 2);
            assert!(errors
                .first()
                .unwrap()
                .to_string()
                .contains("already processed"));
        } else {
            panic!("Expected error")
        }
    }

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "multi_thread", worker_threads = 5))]
    async fn test_parallel_syncs() {
        let wallet = generate_local_wallet();
        let alix1 = Arc::new(ClientBuilder::new_test_client(&wallet).await);
        let alix1_group = alix1
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();

        let alix2 = ClientBuilder::new_test_client(&wallet).await;

        let sync_tasks: Vec<_> = (0..10)
            .map(|_| {
                let group_clone = alix1_group.clone();
                // Each of these syncs is going to trigger the client to invite alix2 to the group
                // because of the race
                crate::spawn(None, async move { group_clone.sync().await }).join()
            })
            .collect();

        let results = join_all(sync_tasks).await;

        // Check if any of the syncs failed
        for result in results.into_iter() {
            assert!(result.is_ok(), "Sync error {:?}", result.err());
        }

        // Make sure that only one welcome was sent
        let alix2_welcomes = alix1
            .api_client
            .query_welcome_messages(alix2.installation_public_key(), None)
            .await
            .unwrap();
        assert_eq!(alix2_welcomes.len(), 1);

        // Make sure that only one group message was sent
        let group_messages = alix1
            .api_client
            .query_group_messages(alix1_group.group_id.clone(), None)
            .await
            .unwrap();
        assert_eq!(group_messages.len(), 1);

        let alix2_group = receive_group_invite(&alix2).await;

        // Send a message from alix1
        alix1_group
            .send_message("hi from alix1".as_bytes())
            .await
            .unwrap();
        // Send a message from alix2
        alix2_group
            .send_message("hi from alix2".as_bytes())
            .await
            .unwrap();

        // Sync both clients
        alix1_group.sync().await.unwrap();
        alix2_group.sync().await.unwrap();

        let alix1_messages = alix1_group.find_messages(&MsgQueryArgs::default()).unwrap();
        let alix2_messages = alix2_group.find_messages(&MsgQueryArgs::default()).unwrap();
        assert_eq!(alix1_messages.len(), alix2_messages.len());

        assert!(alix1_messages
            .iter()
            .any(|m| m.decrypted_message_bytes == "hi from alix2".as_bytes()));
        assert!(alix2_messages
            .iter()
            .any(|m| m.decrypted_message_bytes == "hi from alix1".as_bytes()));
    }

    // Create a membership update intent, but don't sync it yet
    async fn create_membership_update_no_sync(
        group: &MlsGroup<FullXmtpClient>,
        provider: &XmtpOpenMlsProvider,
    ) {
        let intent_data = group
            .get_membership_update_intent(provider, &[], &[])
            .await
            .unwrap();

        // If there is nothing to do, stop here
        if intent_data.is_empty() {
            return;
        }

        group
            .queue_intent(
                provider,
                IntentKind::UpdateGroupMembership,
                intent_data.into(),
            )
            .unwrap();
    }

    /**
     * This test case simulates situations where adding missing
     * installations gets interrupted before the sync part happens
     *
     * We need to be safe even in situations where there are multiple
     * intents that do the same thing, leading to conflicts
     */
    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "multi_thread", worker_threads = 5))]
    async fn add_missing_installs_reentrancy() {
        let wallet = generate_local_wallet();
        let alix1 = ClientBuilder::new_test_client(&wallet).await;
        let alix1_group = alix1
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();

        let alix1_provider = alix1.mls_provider().unwrap();

        let alix2 = ClientBuilder::new_test_client(&wallet).await;

        // We are going to run add_missing_installations TWICE
        // which will create two intents to add the installations
        create_membership_update_no_sync(&alix1_group, &alix1_provider).await;
        create_membership_update_no_sync(&alix1_group, &alix1_provider).await;

        // Now I am going to run publish intents multiple times
        alix1_group
            .publish_intents(&alix1_provider)
            .await
            .expect("Expect publish to be OK");
        alix1_group
            .publish_intents(&alix1_provider)
            .await
            .expect("Expected publish to be OK");

        // Now I am going to sync twice
        alix1_group.sync_with_conn(&alix1_provider).await.unwrap();
        alix1_group.sync_with_conn(&alix1_provider).await.unwrap();

        // Make sure that only one welcome was sent
        let alix2_welcomes = alix1
            .api_client
            .query_welcome_messages(alix2.installation_public_key(), None)
            .await
            .unwrap();
        assert_eq!(alix2_welcomes.len(), 1);

        // We expect two group messages to have been sent,
        // but only the first is valid
        let group_messages = alix1
            .api_client
            .query_group_messages(alix1_group.group_id.clone(), None)
            .await
            .unwrap();
        assert_eq!(group_messages.len(), 2);

        let alix2_group = receive_group_invite(&alix2).await;

        // Send a message from alix1
        alix1_group
            .send_message("hi from alix1".as_bytes())
            .await
            .unwrap();
        // Send a message from alix2
        alix2_group
            .send_message("hi from alix2".as_bytes())
            .await
            .unwrap();

        // Sync both clients
        alix1_group.sync().await.unwrap();
        alix2_group.sync().await.unwrap();

        let alix1_messages = alix1_group.find_messages(&MsgQueryArgs::default()).unwrap();
        let alix2_messages = alix2_group.find_messages(&MsgQueryArgs::default()).unwrap();
        assert_eq!(alix1_messages.len(), alix2_messages.len());

        assert!(alix1_messages
            .iter()
            .any(|m| m.decrypted_message_bytes == "hi from alix2".as_bytes()));
        assert!(alix2_messages
            .iter()
            .any(|m| m.decrypted_message_bytes == "hi from alix1".as_bytes()));
    }

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "multi_thread", worker_threads = 5))]
    async fn respect_allow_epoch_increment() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;

        let group = client
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();

        let _client_2 = ClientBuilder::new_test_client(&wallet).await;

        // Sync the group to get the message adding client_2 published to the network
        group.sync().await.unwrap();

        // Retrieve the envelope for the commit from the network
        let messages = client
            .api_client
            .query_group_messages(group.group_id.clone(), None)
            .await
            .unwrap();

        let first_envelope = messages.first().unwrap();

        let Some(xmtp_proto::xmtp::mls::api::v1::group_message::Version::V1(first_message)) =
            first_envelope.clone().version
        else {
            panic!("wrong message format")
        };
        let provider = client.mls_provider().unwrap();
        let process_result = group
            .process_message(&provider, &first_message, false)
            .await;

        assert_err!(
            process_result,
            GroupMessageProcessingError::EpochIncrementNotAllowed
        );
    }

    #[wasm_bindgen_test(unsupported = tokio::test)]
    async fn test_get_and_set_consent() {
        let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let caro = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let alix_group = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();

        // group consent state should be allowed if user created it
        assert_eq!(alix_group.consent_state().unwrap(), ConsentState::Allowed);

        alix_group
            .update_consent_state(ConsentState::Denied)
            .unwrap();
        assert_eq!(alix_group.consent_state().unwrap(), ConsentState::Denied);

        alix_group
            .add_members_by_inbox_id(&[bola.inbox_id()])
            .await
            .unwrap();

        bola.sync_welcomes(&bola.mls_provider().unwrap())
            .await
            .unwrap();
        let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
        let bola_group = bola_groups.first().unwrap();
        // group consent state should default to unknown for users who did not create the group
        assert_eq!(bola_group.consent_state().unwrap(), ConsentState::Unknown);

        bola_group
            .send_message("hi from bola".as_bytes())
            .await
            .unwrap();

        // group consent state should be allowed if user sends a message to the group
        assert_eq!(bola_group.consent_state().unwrap(), ConsentState::Allowed);

        alix_group
            .add_members_by_inbox_id(&[caro.inbox_id()])
            .await
            .unwrap();

        caro.sync_welcomes(&caro.mls_provider().unwrap())
            .await
            .unwrap();
        let caro_groups = caro.find_groups(GroupQueryArgs::default()).unwrap();
        let caro_group = caro_groups.first().unwrap();

        caro_group
            .send_message_optimistic("hi from caro".as_bytes())
            .unwrap();

        caro_group.publish_messages().await.unwrap();

        // group consent state should be allowed if user publishes a message to the group
        assert_eq!(caro_group.consent_state().unwrap(), ConsentState::Allowed);
    }

    #[wasm_bindgen_test(unsupported = tokio::test)]
    // TODO(rich): Generalize the test once fixed - test messages that are 0, 1, 2, 3, 4, 5 epochs behind
    async fn test_max_past_epochs() {
        // Create group with two members
        let bo_wallet = generate_local_wallet();
        let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bo = ClientBuilder::new_test_client(&bo_wallet).await;
        let alix_group = alix
            .create_group_with_members(
                &[bo_wallet.get_address()],
                None,
                GroupMetadataOptions::default(),
            )
            .await
            .unwrap();

        bo.sync_welcomes(&bo.mls_provider().unwrap()).await.unwrap();
        let bo_groups = bo.find_groups(GroupQueryArgs::default()).unwrap();
        let bo_group = bo_groups.first().unwrap();

        // Both members see the same amount of messages to start
        alix_group.send_message("alix 1".as_bytes()).await.unwrap();
        bo_group.send_message("bo 1".as_bytes()).await.unwrap();
        alix_group.sync().await.unwrap();
        bo_group.sync().await.unwrap();

        let alix_messages = alix_group
            .find_messages(&MsgQueryArgs {
                kind: Some(GroupMessageKind::Application),
                ..Default::default()
            })
            .unwrap();
        let bo_messages = bo_group
            .find_messages(&MsgQueryArgs {
                kind: Some(GroupMessageKind::Application),
                ..Default::default()
            })
            .unwrap();

        assert_eq!(alix_messages.len(), 2);
        assert_eq!(bo_messages.len(), 2);

        // Alix moves the group forward by 1 epoch
        alix_group
            .update_group_name("new name".to_string())
            .await
            .unwrap();

        // Bo sends a message while 1 epoch behind
        bo_group.send_message("bo 2".as_bytes()).await.unwrap();

        // If max_past_epochs is working, Alix should be able to decrypt Bo's message
        alix_group.sync().await.unwrap();
        bo_group.sync().await.unwrap();

        let alix_messages = alix_group
            .find_messages(&MsgQueryArgs {
                kind: Some(GroupMessageKind::Application),
                ..Default::default()
            })
            .unwrap();
        let bo_messages = bo_group
            .find_messages(&MsgQueryArgs {
                kind: Some(GroupMessageKind::Application),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(bo_messages.len(), 3);
        assert_eq!(alix_messages.len(), 3); // Fails here, 2 != 3
    }

    #[wasm_bindgen_test(unsupported = tokio::test)]
    async fn test_validate_dm_group() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let added_by_inbox = "added_by_inbox_id";
        let creator_inbox_id = client.context.identity.inbox_id();
        let dm_target_inbox_id = added_by_inbox.to_string();

        // Test case 1: Valid DM group
        let valid_dm_group = MlsGroup::<FullXmtpClient>::create_test_dm_group(
            client.clone().into(),
            dm_target_inbox_id.clone(),
            None,
            None,
            None,
            None,
        )
        .unwrap();
        assert!(valid_dm_group
            .load_mls_group_with_lock(client.mls_provider().unwrap(), |mls_group| {
                validate_dm_group(&client, &mls_group, added_by_inbox)
            })
            .is_ok());

        // Test case 2: Invalid conversation type
        let invalid_protected_metadata =
            build_protected_metadata_extension(creator_inbox_id, ConversationType::Group).unwrap();
        let invalid_type_group = MlsGroup::<FullXmtpClient>::create_test_dm_group(
            client.clone().into(),
            dm_target_inbox_id.clone(),
            Some(invalid_protected_metadata),
            None,
            None,
            None,
        )
        .unwrap();
        assert!(matches!(
            invalid_type_group.load_mls_group_with_lock(client.mls_provider().unwrap(), |mls_group|
                validate_dm_group(&client, &mls_group, added_by_inbox)
            ),
            Err(GroupError::Generic(msg)) if msg.contains("Invalid conversation type")
        ));
        // Test case 3: Missing DmMembers
        // This case is not easily testable with the current structure, as DmMembers are set in the protected metadata

        // Test case 4: Mismatched DM members
        let mismatched_dm_members =
            build_dm_protected_metadata_extension(creator_inbox_id, "wrong_inbox_id".to_string())
                .unwrap();
        let mismatched_dm_members_group = MlsGroup::<FullXmtpClient>::create_test_dm_group(
            client.clone().into(),
            dm_target_inbox_id.clone(),
            Some(mismatched_dm_members),
            None,
            None,
            None,
        )
        .unwrap();
        assert!(matches!(
            mismatched_dm_members_group.load_mls_group_with_lock(client.mls_provider().unwrap(), |mls_group|
                validate_dm_group(&client, &mls_group, added_by_inbox)
            ),
            Err(GroupError::Generic(msg)) if msg.contains("DM members do not match expected inboxes")
        ));

        // Test case 5: Non-empty admin list
        let non_empty_admin_list = build_mutable_metadata_extension_default(
            creator_inbox_id,
            GroupMetadataOptions::default(),
        )
        .unwrap();
        let non_empty_admin_list_group = MlsGroup::<FullXmtpClient>::create_test_dm_group(
            client.clone().into(),
            dm_target_inbox_id.clone(),
            None,
            Some(non_empty_admin_list),
            None,
            None,
        )
        .unwrap();
        assert!(matches!(
            non_empty_admin_list_group.load_mls_group_with_lock(client.mls_provider().unwrap(), |mls_group|
                validate_dm_group(&client, &mls_group, added_by_inbox)
            ),
            Err(GroupError::Generic(msg)) if msg.contains("DM group must have empty admin and super admin lists")
        ));

        // Test case 6: Non-empty super admin list
        // Similar to test case 5, but with super_admin_list

        // Test case 7: Invalid permissions
        let invalid_permissions = PolicySet::default();
        let invalid_permissions_group = MlsGroup::<FullXmtpClient>::create_test_dm_group(
            client.clone().into(),
            dm_target_inbox_id.clone(),
            None,
            None,
            None,
            Some(invalid_permissions),
        )
        .unwrap();
        assert!(matches!(
            invalid_permissions_group.load_mls_group_with_lock(client.mls_provider().unwrap(), |mls_group|
                validate_dm_group(&client, &mls_group, added_by_inbox)
            ),
            Err(GroupError::Generic(msg)) if msg.contains("Invalid permissions for DM group")
        ));
    }
}
