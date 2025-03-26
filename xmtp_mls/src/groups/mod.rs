pub mod device_sync;
pub mod group_membership;
pub mod group_metadata;
pub mod group_mutable_metadata;
pub mod group_permissions;
pub mod intents;
pub mod members;
pub mod scoped_client;

mod disappearing_messages;
pub(super) mod mls_ext;
pub(super) mod mls_sync;
pub(super) mod subscriptions;
pub mod validated_commit;

#[cfg(test)]
pub mod test_utils;

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
use crate::groups::group_mutable_metadata::{
    extract_group_mutable_metadata, MessageDisappearingSettings,
};
use crate::groups::intents::UpdateGroupMembershipResult;
use crate::storage::consent_record::ConsentType;
use crate::storage::{
    group::DmIdExt,
    group_message::{ContentType, StoredGroupMessageWithReactions},
    refresh_state::EntityKind,
    NotFound, ProviderTransactions, StorageError,
};
use crate::subscriptions::SyncEvent;
use crate::GroupCommitLock;
use crate::{
    client::{ClientError, XmtpMlsLocalContext},
    configuration::{
        CIPHERSUITE, GROUP_MEMBERSHIP_EXTENSION_ID, GROUP_PERMISSIONS_EXTENSION_ID, MAX_GROUP_SIZE,
        MAX_PAST_EPOCHS, MUTABLE_METADATA_EXTENSION_ID,
        SEND_MESSAGE_UPDATE_INSTALLATIONS_INTERVAL_NS,
    },
    hpke::HpkeError,
    identity::IdentityError,
    identity_updates::{load_identity_updates, InstallationDiffError},
    intents::ProcessIntentError,
    storage::xmtp_openmls_provider::XmtpOpenMlsProvider,
    storage::{
        consent_record::{ConsentState, StoredConsentRecord},
        db_connection::DbConnection,
        group::{ConversationType, GroupMembershipState, StoredGroup},
        group_intent::IntentKind,
        group_message::{DeliveryStatus, GroupMessageKind, MsgQueryArgs, StoredGroupMessage},
        sql_key_store,
    },
    subscriptions::{LocalEventError, LocalEvents},
    utils::id::calculate_message_id,
    Store,
};
use device_sync::preference_sync::UserPreferenceUpdate;
use intents::SendMessageIntentData;
use mls_ext::DecryptedWelcome;
use mls_sync::GroupMessageProcessingError;
use openmls::{
    credentials::CredentialType,
    error::LibraryError,
    extensions::{
        Extension, ExtensionType, Extensions, Metadata, RequiredCapabilitiesExtension,
        UnknownExtension,
    },
    group::{CreateGroupContextExtProposalError, MlsGroupCreateConfig},
    messages::proposals::ProposalType,
    prelude::{
        BasicCredentialError, Capabilities, CredentialWithKey, Error as TlsCodecError, GroupId,
        MlsGroup as OpenMlsGroup, StagedWelcome, WireFormatPolicy,
    },
};
use openmls_traits::OpenMlsProvider;
use prost::Message;
use std::collections::HashMap;
use std::future::Future;
use std::{collections::HashSet, sync::Arc};
use thiserror::Error;
use tokio::sync::Mutex;
use validated_commit::LibXMTPVersion;
use xmtp_common::retry::RetryableError;
use xmtp_common::time::now_ns;
use xmtp_content_types::reaction::{LegacyReaction, ReactionCodec};
use xmtp_content_types::should_push;
use xmtp_cryptography::signature::IdentifierValidationError;
use xmtp_id::associations::Identifier;
use xmtp_id::{AsIdRef, InboxId, InboxIdRef};
use xmtp_proto::xmtp::mls::{
    api::v1::welcome_message,
    message_contents::{
        content_types::ReactionV2,
        plaintext_envelope::{Content, V1},
        EncodedContent, PlaintextEnvelope,
    },
};

const MAX_GROUP_DESCRIPTION_LENGTH: usize = 1000;
const MAX_GROUP_NAME_LENGTH: usize = 100;
const MAX_GROUP_IMAGE_URL_LENGTH: usize = 2048;

#[derive(Debug, Error)]
pub enum GroupError {
    #[error(transparent)]
    NotFound(#[from] NotFound),
    #[error("Max user limit exceeded.")]
    UserLimitExceeded,
    #[error("api error: {0}")]
    WrappedApi(#[from] xmtp_api::Error),
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
    AddressValidation(#[from] IdentifierValidationError),
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
    #[error("Exceeded max characters for this field. Must be under: {length}")]
    TooManyCharacters { length: usize },
    #[error("Group is paused until version {0} is available")]
    GroupPausedUntilUpdate(String),
}

impl RetryableError for GroupError {
    fn is_retryable(&self) -> bool {
        match self {
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
            Self::NotFound(_)
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
            | Self::EncodeError(_)
            | Self::TooManyCharacters { .. }
            | Self::GroupPausedUntilUpdate(_) => false,
        }
    }
}

pub struct MlsGroup<C> {
    pub group_id: Vec<u8>,
    pub created_at_ns: i64,
    pub client: Arc<C>,
    mls_commit_lock: Arc<GroupCommitLock>,
    mutex: Arc<Mutex<()>>,
}

pub struct ConversationListItem<C> {
    pub group: MlsGroup<C>,
    pub last_message: Option<StoredGroupMessage>,
}

#[derive(Default, Clone)]
pub struct GroupMetadataOptions {
    pub name: Option<String>,
    pub image_url_square: Option<String>,
    pub description: Option<String>,
    pub message_disappearing_settings: Option<MessageDisappearingSettings>,
}

#[derive(Default, Clone)]
pub struct DMMetadataOptions {
    pub message_disappearing_settings: Option<MessageDisappearingSettings>,
}

impl<C> Clone for MlsGroup<C> {
    fn clone(&self) -> Self {
        Self {
            group_id: self.group_id.clone(),
            created_at_ns: self.created_at_ns,
            client: self.client.clone(),
            mutex: self.mutex.clone(),
            mls_commit_lock: self.mls_commit_lock.clone(),
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
    pub should_push: bool,
}

impl Default for QueryableContentFields {
    fn default() -> Self {
        Self {
            content_type: ContentType::Unknown, // Or whatever the appropriate default is
            version_major: 0,
            version_minor: 0,
            authority_id: String::new(),
            reference_id: None,
            should_push: false,
        }
    }
}

impl TryFrom<EncodedContent> for QueryableContentFields {
    type Error = prost::DecodeError;

    fn try_from(content: EncodedContent) -> Result<Self, Self::Error> {
        let content_type_id = content.r#type.unwrap_or_default();

        let type_id_str = content_type_id.type_id.clone();

        let reference_id = match (type_id_str.as_str(), content_type_id.version_major) {
            (ReactionCodec::TYPE_ID, major) if major >= 2 => {
                ReactionV2::decode(content.content.as_slice())
                    .ok()
                    .and_then(|reaction| hex::decode(reaction.reference).ok())
            }
            (ReactionCodec::TYPE_ID, _) => LegacyReaction::decode(&content.content)
                .and_then(|legacy_reaction| hex::decode(legacy_reaction.reference).ok()),
            _ => None,
        };

        Ok(QueryableContentFields {
            content_type: content_type_id.type_id.into(),
            version_major: content_type_id.version_major as i32,
            version_minor: content_type_id.version_minor as i32,
            authority_id: content_type_id.authority_id.to_string(),
            reference_id,
            should_push: should_push(type_id_str),
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

    /// Creates a new group instance. Validate that the group exists in the DB before constructing
    /// the group.
    ///
    /// # Returns
    ///
    /// Returns the Group and the stored group information as a tuple.
    pub fn new_validated(
        client: ScopedClient,
        group_id: Vec<u8>,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<(Self, StoredGroup), GroupError> {
        if let Some(group) = provider.conn_ref().find_group(&group_id)? {
            Ok((
                Self::new_from_arc(Arc::new(client), group_id, group.created_at_ns),
                group,
            ))
        } else {
            tracing::error!("Failed to validate existence of group");
            Err(NotFound::GroupById(group_id).into())
        }
    }

    pub(crate) fn new_from_arc(
        client: Arc<ScopedClient>,
        group_id: Vec<u8>,
        created_at_ns: i64,
    ) -> Self {
        let mut mutexes = client.context().mutexes.clone();
        let context = client.context();
        Self {
            group_id: group_id.clone(),
            created_at_ns,
            mutex: mutexes.get_mutex(group_id),
            client,
            mls_commit_lock: Arc::clone(context.mls_commit_lock()),
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
        let _lock = self.mls_commit_lock.get_lock_sync(group_id.clone());
        // Load the MLS group
        let mls_group =
            OpenMlsGroup::load(provider.storage(), &GroupId::from_slice(&self.group_id))
                .map_err(|_| NotFound::MlsGroup)?
                .ok_or(NotFound::MlsGroup)?;

        // Perform the operation with the MLS group
        operation(mls_group)
    }

    // Load the stored OpenMLS group from the OpenMLS provider's keystore
    #[tracing::instrument(level = "debug", skip_all)]
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
        let _lock = self.mls_commit_lock.get_lock_async(group_id.clone()).await;

        // Load the MLS group
        let mls_group =
            OpenMlsGroup::load(provider.storage(), &GroupId::from_slice(&self.group_id))
                .map_err(crate::StorageError::from)?
                .ok_or(StorageError::from(NotFound::GroupById(
                    self.group_id.to_vec(),
                )))?;

        // Perform the operation with the MLS group
        operation(mls_group).await
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
        let mutable_metadata =
            build_mutable_metadata_extension_default(creator_inbox_id, opts.clone())?;
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
            opts.message_disappearing_settings,
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
        opts: DMMetadataOptions,
    ) -> Result<Self, GroupError> {
        let context = client.context();
        let protected_metadata =
            build_dm_protected_metadata_extension(context.inbox_id(), dm_target_inbox_id.clone())?;
        let mutable_metadata = build_dm_mutable_metadata_extension_default(
            context.inbox_id(),
            &dm_target_inbox_id,
            opts.clone(),
        )?;
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
            opts.message_disappearing_settings,
            None,
        );

        stored_group.store(provider.conn_ref())?;
        let new_group = Self::new_from_arc(client.clone(), group_id, stored_group.created_at_ns);
        // Consent state defaults to allowed when the user creates the group
        new_group.update_consent_state(ConsentState::Allowed)?;
        Ok(new_group)
    }

    // Create a group from a decrypted and decoded welcome message
    // If the group already exists in the store, overwrite the MLS state and do not update the group entry
    #[tracing::instrument(skip_all, level = "debug")]
    pub(super) async fn create_from_welcome(
        client: &ScopedClient,
        provider: &XmtpOpenMlsProvider,
        welcome: &welcome_message::V1,
    ) -> Result<Self, GroupError>
    where
        ScopedClient: Clone,
    {
        // Check if this welcome was already processed. Return the existing group if so.
        if provider
            .conn_ref()
            .get_last_cursor_for_id(client.installation_id(), EntityKind::Welcome)?
            >= welcome.id as i64
        {
            let group = provider
                .conn_ref()
                .find_group_by_welcome_id(welcome.id as i64)?
                .ok_or(GroupError::NotFound(NotFound::GroupByWelcome(
                    welcome.id as i64,
                )))?;
            let group = Self::new(client.clone(), group.id, group.created_at_ns);

            return Ok(group);
        };

        let mut decrypted_welcome = None;
        let result = provider.transaction(|provider| {
            let result = DecryptedWelcome::from_encrypted_bytes(
                provider,
                &welcome.hpke_public_key,
                &welcome.data,
            );
            decrypted_welcome = Some(result);
            Err(StorageError::IntentionalRollback)
        });
        let Err(StorageError::IntentionalRollback) = result else {
            return Err(result?);
        };

        let DecryptedWelcome { staged_welcome, .. } = decrypted_welcome.expect("Set to some")?;

        // Ensure that the list of members in the group's MLS tree matches the list of inboxes specified
        // in the `GroupMembership` extension.
        validate_initial_group_membership(client, provider.conn_ref(), &staged_welcome).await?;

        provider.transaction(|provider| {
            let decrypted_welcome = DecryptedWelcome::from_encrypted_bytes(
                provider,
                &welcome.hpke_public_key,
                &welcome.data,
            )?;
            let DecryptedWelcome {
                staged_welcome,
                added_by_inbox_id,
                ..
            } = decrypted_welcome;

            let is_updated = provider.conn_ref().update_cursor(
                client.context().installation_public_key(),
                EntityKind::Welcome,
                welcome.id as i64,
            )?;
            if !is_updated {
                return Err(ProcessIntentError::AlreadyProcessed(welcome.id).into());
            }

            let mls_group = staged_welcome.into_group(provider)?;
            let group_id = mls_group.group_id().to_vec();
            let metadata = extract_group_metadata(&mls_group)?;
            let dm_members = metadata.dm_members;
            let conversation_type = metadata.conversation_type;
            let mutable_metadata = extract_group_mutable_metadata(&mls_group).ok();
            let disappearing_settings = mutable_metadata.clone().and_then(|metadata| {
                Self::conversation_message_disappearing_settings_from_extensions(metadata).ok()
            });
            let paused_for_version: Option<String> = mutable_metadata.and_then(|metadata| {
                let min_version = Self::min_protocol_version_from_extensions(metadata);
                if let Some(min_version) = min_version {
                    let current_version_str = client.version_info().pkg_version();
                    let current_version =
                        LibXMTPVersion::parse(current_version_str).ok()?;
                    let required_min_version = LibXMTPVersion::parse(&min_version.clone()).ok()?;
                    if required_min_version > current_version {
                        tracing::warn!(
                            "Saving group from welcome as paused since version requirements are not met. \
                            Group ID: {}, \
                            Required version: {}, \
                            Current version: {}",
                            hex::encode(group_id.clone()),
                            min_version,
                            current_version_str
                        );
                        Some(min_version)
                    } else {
                        None
                    }
                } else {
                    None
                }
            });

            let to_store = match conversation_type {
                ConversationType::Group => StoredGroup::new_from_welcome(
                    group_id.clone(),
                    now_ns(),
                    GroupMembershipState::Pending,
                    added_by_inbox_id,
                    welcome.id as i64,
                    conversation_type,
                    dm_members,
                    disappearing_settings,
                    paused_for_version,
                ),
                ConversationType::Dm => {
                    validate_dm_group(client, &mls_group, &added_by_inbox_id)?;
                    StoredGroup::new_from_welcome(
                        group_id.clone(),
                        now_ns(),
                        GroupMembershipState::Pending,
                        added_by_inbox_id,
                        welcome.id as i64,
                        conversation_type,
                        dm_members,
                        disappearing_settings,
                        None,
                    )
                }
                ConversationType::Sync => {
                    // Let the DeviceSync worker know about the presence of a new
                    // sync group that came in from a welcome.
                    let _ = client.local_events().send(LocalEvents::SyncEvent(SyncEvent::NewSyncGroupFromWelcome));

                    StoredGroup::new_from_welcome(
                        group_id.clone(),
                        now_ns(),
                        GroupMembershipState::Allowed,
                        added_by_inbox_id,
                        welcome.id as i64,
                        conversation_type,
                        dm_members,
                        disappearing_settings,
                        None,
                    )
                }
            };

            // Insert or replace the group in the database.
            // Replacement can happen in the case that the user has been removed from and subsequently re-added to the group.
            let stored_group = provider.conn_ref().insert_or_replace_group(to_store)?;

            Ok(Self::new(
                client.clone(),
                stored_group.id,
                stored_group.created_at_ns,
            ))
        })
    }

    pub(crate) fn create_and_insert_sync_group(
        client: Arc<ScopedClient>,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<MlsGroup<ScopedClient>, GroupError> {
        tracing::info!("Creating sync group.");

        let context = client.context();

        let protected_metadata =
            build_protected_metadata_extension(context.inbox_id(), ConversationType::Sync)?;
        let mutable_metadata = build_mutable_metadata_extension_default(
            context.inbox_id(),
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
                signature_key: context.identity.installation_keys.public_slice().into(),
            },
        )?;

        let group_id = mls_group.group_id().to_vec();
        let stored_group =
            StoredGroup::new_sync_group(group_id.clone(), now_ns(), GroupMembershipState::Allowed);

        stored_group.store(provider.conn_ref())?;
        let group = Self::new_from_arc(client, stored_group.id, stored_group.created_at_ns);

        Ok(group)
    }

    /// Send a message on this users XMTP [`Client`].
    #[tracing::instrument(skip_all, level = "debug")]
    pub async fn send_message(&self, message: &[u8]) -> Result<Vec<u8>, GroupError> {
        let provider = self.mls_provider()?;
        self.send_message_with_provider(message, &provider).await
    }

    /// Send a message with the given [`XmtpOpenMlsProvider`]
    pub async fn send_message_with_provider(
        &self,
        message: &[u8],
        provider: &XmtpOpenMlsProvider,
    ) -> Result<Vec<u8>, GroupError> {
        self.ensure_not_paused().await?;
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
        self.ensure_not_paused().await?;
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
        self.ensure_not_paused().await?;
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
            .inspect_err(|_| {
                tracing::debug!("No queryable content fields, msg not formatted as encoded content")
            })
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
    pub(crate) fn prepare_message<F>(
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
        let queryable_content_fields: QueryableContentFields =
            Self::extract_queryable_content_fields(message);
        self.queue_intent(
            provider,
            IntentKind::SendMessage,
            intent_data,
            queryable_content_fields.should_push,
        )?;

        // store this unpublished message locally before sending
        let message_id = calculate_message_id(&self.group_id, message, &now.to_string());
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
    /// # Returns
    /// - `Ok(UpdateGroupMembershipResult)`: Contains details about the membership changes, including:
    ///   - `added_members`: list of added installations
    ///   - `removed_members`: A list of installations that were removed.
    ///   - `members_with_errors`: A list of members that encountered errors during the update.
    /// - `Err(GroupError)`: If the operation fails due to an error.
    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn add_members(
        &self,
        account_identifiers: &[Identifier],
    ) -> Result<UpdateGroupMembershipResult, GroupError> {
        // Fetch the associated inbox_ids
        let requests = account_identifiers.iter().map(Into::into).collect();
        let inbox_id_map: HashMap<Identifier, String> = self
            .client
            .api()
            .get_inbox_ids(requests)
            .await?
            .into_iter()
            .filter_map(|(k, v)| Some((k.try_into().ok()?, v)))
            .collect();

        let provider = self.mls_provider()?;
        // get current number of users in group
        let member_count = self.members_with_provider(&provider).await?.len();
        if member_count + inbox_id_map.len() > MAX_GROUP_SIZE {
            return Err(GroupError::UserLimitExceeded);
        }

        if inbox_id_map.len() != account_identifiers.len() {
            let found_addresses: HashSet<&Identifier> = inbox_id_map.keys().collect();
            let to_add_hashset = HashSet::from_iter(account_identifiers.iter());

            let missing_addresses = found_addresses.difference(&to_add_hashset);
            return Err(GroupError::AddressNotFound(
                missing_addresses
                    .into_iter()
                    .map(|ident| format!("{ident}"))
                    .collect(),
            ));
        }

        self.add_members_by_inbox_id_with_provider(
            &provider,
            &inbox_id_map.into_values().collect::<Vec<_>>(),
        )
        .await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn add_members_by_inbox_id<S: AsIdRef>(
        &self,
        inbox_ids: &[S],
    ) -> Result<UpdateGroupMembershipResult, GroupError> {
        let provider = self.client.mls_provider()?;
        self.add_members_by_inbox_id_with_provider(&provider, inbox_ids)
            .await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn add_members_by_inbox_id_with_provider<S: AsIdRef>(
        &self,
        provider: &XmtpOpenMlsProvider,
        inbox_ids: &[S],
    ) -> Result<UpdateGroupMembershipResult, GroupError> {
        self.ensure_not_paused().await?;
        let ids = inbox_ids.iter().map(AsIdRef::as_ref).collect::<Vec<&str>>();
        let intent_data = self
            .get_membership_update_intent(provider, ids.as_slice(), &[])
            .await?;

        // TODO:nm this isn't the best test for whether the request is valid
        // If some existing group member has an update, this will return an intent with changes
        // when we really should return an error
        let ok_result = Ok(UpdateGroupMembershipResult::from(intent_data.clone()));

        if intent_data.is_empty() {
            tracing::warn!("Member already added");
            return ok_result;
        }

        let intent = self.queue_intent(
            provider,
            IntentKind::UpdateGroupMembership,
            intent_data.into(),
            false,
        )?;

        self.sync_until_intent_resolved(provider, intent.id).await?;
        ok_result
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
        account_addresses_to_remove: &[Identifier],
    ) -> Result<(), GroupError> {
        let account_addresses_to_remove =
            account_addresses_to_remove.iter().map(Into::into).collect();

        let inbox_id_map = self
            .client
            .api()
            .get_inbox_ids(account_addresses_to_remove)
            .await?;

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
        self.ensure_not_paused().await?;

        let provider = self.client.store().conn()?.into();

        let intent_data = self
            .get_membership_update_intent(&provider, &[], inbox_ids)
            .await?;

        let intent = self.queue_intent(
            &provider,
            IntentKind::UpdateGroupMembership,
            intent_data.into(),
            false,
        )?;

        self.sync_until_intent_resolved(&provider, intent.id).await
    }

    /// Updates the name of the group. Will error if the user does not have the appropriate permissions
    /// to perform these updates.
    pub async fn update_group_name(&self, group_name: String) -> Result<(), GroupError> {
        self.ensure_not_paused().await?;

        if group_name.len() > MAX_GROUP_NAME_LENGTH {
            return Err(GroupError::TooManyCharacters {
                length: MAX_GROUP_NAME_LENGTH,
            });
        }
        let provider = self.client.mls_provider()?;
        if self.metadata(&provider).await?.conversation_type == ConversationType::Dm {
            return Err(GroupError::DmGroupMetadataForbidden);
        }
        let intent_data: Vec<u8> =
            UpdateMetadataIntentData::new_update_group_name(group_name).into();
        let intent =
            self.queue_intent(&provider, IntentKind::MetadataUpdate, intent_data, false)?;

        self.sync_until_intent_resolved(&provider, intent.id).await
    }

    /// Updates min version of the group to match this client's version.
    pub async fn update_group_min_version_to_match_self(&self) -> Result<(), GroupError> {
        self.ensure_not_paused().await?;

        let provider = self.client.mls_provider()?;

        let version = self.client.version_info().pkg_version();
        let intent_data: Vec<u8> =
            UpdateMetadataIntentData::new_update_group_min_version_to_match_self(
                version.to_string(),
            )
            .into();
        let intent =
            self.queue_intent(&provider, IntentKind::MetadataUpdate, intent_data, false)?;

        self.sync_until_intent_resolved(&provider, intent.id).await
    }

    fn min_protocol_version_from_extensions(
        mutable_metadata: GroupMutableMetadata,
    ) -> Option<String> {
        mutable_metadata
            .attributes
            .get(&MetadataField::MinimumSupportedProtocolVersion.to_string())
            .map(|v| v.to_string())
    }

    /// Updates the permission policy of the group. This requires super admin permissions.
    pub async fn update_permission_policy(
        &self,
        permission_update_type: PermissionUpdateType,
        permission_policy: PermissionPolicyOption,
        metadata_field: Option<MetadataField>,
    ) -> Result<(), GroupError> {
        self.ensure_not_paused().await?;

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

        let intent =
            self.queue_intent(&provider, IntentKind::UpdatePermission, intent_data, false)?;

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
        self.ensure_not_paused().await?;

        if group_description.len() > MAX_GROUP_DESCRIPTION_LENGTH {
            return Err(GroupError::TooManyCharacters {
                length: MAX_GROUP_DESCRIPTION_LENGTH,
            });
        }

        let provider = self.client.mls_provider()?;
        if self.metadata(&provider).await?.conversation_type == ConversationType::Dm {
            return Err(GroupError::DmGroupMetadataForbidden);
        }
        let intent_data: Vec<u8> =
            UpdateMetadataIntentData::new_update_group_description(group_description).into();
        let intent =
            self.queue_intent(&provider, IntentKind::MetadataUpdate, intent_data, false)?;

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
        self.ensure_not_paused().await?;

        if group_image_url_square.len() > MAX_GROUP_IMAGE_URL_LENGTH {
            return Err(GroupError::TooManyCharacters {
                length: MAX_GROUP_IMAGE_URL_LENGTH,
            });
        }

        let provider = self.client.mls_provider()?;
        if self.metadata(&provider).await?.conversation_type == ConversationType::Dm {
            return Err(GroupError::DmGroupMetadataForbidden);
        }
        let intent_data: Vec<u8> =
            UpdateMetadataIntentData::new_update_group_image_url_square(group_image_url_square)
                .into();
        let intent =
            self.queue_intent(&provider, IntentKind::MetadataUpdate, intent_data, false)?;

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

    pub async fn update_conversation_message_disappearing_settings(
        &self,
        settings: MessageDisappearingSettings,
    ) -> Result<(), GroupError> {
        self.ensure_not_paused().await?;

        let provider = self.client.mls_provider()?;

        self.update_conversation_message_disappear_from_ns(&provider, settings.from_ns)
            .await?;
        self.update_conversation_message_disappear_in_ns(&provider, settings.in_ns)
            .await
    }

    pub async fn remove_conversation_message_disappearing_settings(
        &self,
    ) -> Result<(), GroupError> {
        self.ensure_not_paused().await?;

        self.update_conversation_message_disappearing_settings(
            MessageDisappearingSettings::default(),
        )
        .await
    }

    async fn update_conversation_message_disappear_from_ns(
        &self,
        provider: &XmtpOpenMlsProvider,
        expire_from_ms: i64,
    ) -> Result<(), GroupError> {
        self.ensure_not_paused().await?;

        let intent_data: Vec<u8> =
            UpdateMetadataIntentData::new_update_conversation_message_disappear_from_ns(
                expire_from_ms,
            )
            .into();
        let intent = self.queue_intent(provider, IntentKind::MetadataUpdate, intent_data, false)?;
        self.sync_until_intent_resolved(provider, intent.id).await
    }

    async fn update_conversation_message_disappear_in_ns(
        &self,
        provider: &XmtpOpenMlsProvider,
        expire_in_ms: i64,
    ) -> Result<(), GroupError> {
        self.ensure_not_paused().await?;

        let intent_data: Vec<u8> =
            UpdateMetadataIntentData::new_update_conversation_message_disappear_in_ns(expire_in_ms)
                .into();
        let intent = self.queue_intent(provider, IntentKind::MetadataUpdate, intent_data, false)?;
        self.sync_until_intent_resolved(provider, intent.id).await
    }

    /// If group is not paused, will return None, otherwise will return the version that the group is paused for
    pub fn paused_for_version(
        &self,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<Option<String>, GroupError> {
        let paused_for_version = provider
            .conn_ref()
            .get_group_paused_version(&self.group_id)?;
        Ok(paused_for_version)
    }

    async fn ensure_not_paused(&self) -> Result<(), GroupError> {
        let conn = self.context().store().conn()?;
        let provider = XmtpOpenMlsProvider::from(conn);
        if let Some(min_version) = provider
            .conn_ref()
            .get_group_paused_version(&self.group_id)?
        {
            Err(GroupError::GroupPausedUntilUpdate(min_version))
        } else {
            Ok(())
        }
    }

    pub fn conversation_message_disappearing_settings(
        &self,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<MessageDisappearingSettings, GroupError> {
        Self::conversation_message_disappearing_settings_from_extensions(
            self.mutable_metadata(provider)?,
        )
    }

    pub fn conversation_message_disappearing_settings_from_extensions(
        mutable_metadata: GroupMutableMetadata,
    ) -> Result<MessageDisappearingSettings, GroupError> {
        let disappear_from_ns = mutable_metadata
            .attributes
            .get(&MetadataField::MessageDisappearFromNS.to_string());
        let disappear_in_ns = mutable_metadata
            .attributes
            .get(&MetadataField::MessageDisappearInNS.to_string());

        if let (Some(Ok(message_disappear_from_ns)), Some(Ok(message_disappear_in_ns))) = (
            disappear_from_ns.map(|s| s.parse::<i64>()),
            disappear_in_ns.map(|s| s.parse::<i64>()),
        ) {
            Ok(MessageDisappearingSettings::new(
                message_disappear_from_ns,
                message_disappear_in_ns,
            ))
        } else {
            Err(GroupError::GroupMetadata(
                GroupMetadataError::MissingExtension,
            ))
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
        let intent =
            self.queue_intent(&provider, IntentKind::UpdateAdminList, intent_data, false)?;

        self.sync_until_intent_resolved(&provider, intent.id).await
    }

    /// Find the `inbox_id` of the group member who added the member to the group
    pub fn added_by_inbox_id(&self) -> Result<String, GroupError> {
        let conn = self.context().store().conn()?;
        let group = conn
            .find_group(&self.group_id)?
            .ok_or_else(|| NotFound::GroupById(self.group_id.clone()))?;
        Ok(group.added_by_inbox_id)
    }

    /// Find the `inbox_id` of the group member who is the peer of this dm
    pub fn dm_inbox_id(&self) -> Result<String, GroupError> {
        let conn = self.context().store().conn()?;
        let group = conn
            .find_group(&self.group_id)?
            .ok_or_else(|| NotFound::GroupById(self.group_id.clone()))?;
        let inbox_id = self.client.inbox_id();
        let dm_id = &group
            .dm_id
            .ok_or_else(|| NotFound::GroupById(self.group_id.clone()))?;
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

        if !new_records.is_empty() && self.client.device_sync_server_url().is_some() {
            // Dispatch an update event so it can be synced across devices
            let _ = self
                .client
                .local_events()
                .send(LocalEvents::OutgoingPreferenceUpdates(new_records));
        }

        Ok(())
    }

    /// Get the current epoch number of the group.
    pub async fn epoch(&self, provider: &XmtpOpenMlsProvider) -> Result<u64, GroupError> {
        self.load_mls_group_with_lock_async(provider, |mls_group| {
            futures::future::ready(Ok(mls_group.epoch().as_u64()))
        })
        .await
    }

    /// Update this installation's leaf key in the group by creating a key update commit
    pub async fn key_update(&self) -> Result<(), GroupError> {
        let provider = self.client.mls_provider()?;
        let intent = self.queue_intent(&provider, IntentKind::KeyUpdate, vec![], false)?;
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
        opts: DMMetadataOptions,
    ) -> Result<Self, GroupError> {
        let context = client.context();
        let conn = context.store().conn()?;
        let provider = XmtpOpenMlsProvider::new(conn);

        let protected_metadata = custom_protected_metadata.unwrap_or_else(|| {
            build_dm_protected_metadata_extension(context.inbox_id(), dm_target_inbox_id.clone())
                .unwrap()
        });
        let mutable_metadata = custom_mutable_metadata.unwrap_or_else(|| {
            build_dm_mutable_metadata_extension_default(
                context.inbox_id(),
                &dm_target_inbox_id,
                opts,
            )
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
            None,
            None,
        );

        stored_group.store(provider.conn_ref())?;
        Ok(Self::new_from_arc(
            client,
            group_id,
            stored_group.created_at_ns,
        ))
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
    opts: DMMetadataOptions,
) -> Result<Extension, GroupError> {
    let mutable_metadata: Vec<u8> = GroupMutableMetadata::new_dm_default(
        creator_inbox_id.to_string(),
        dm_target_inbox_id,
        opts,
    )
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
    staged_welcome: &StagedWelcome,
) -> Result<(), GroupError> {
    tracing::info!("Validating initial group membership");
    let extensions = staged_welcome.public_group().group_context().extensions();
    let membership = extract_group_membership(extensions)?;
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

    let actual_installation_ids: HashSet<Vec<u8>> = staged_welcome
        .public_group()
        .members()
        .map(|member| member.signature_key)
        .collect();

    // exclude failed installations
    expected_installation_ids.retain(|id| !membership.failed_installations.contains(id));

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
    // Validate dm specific immutable metadata
    let metadata = extract_group_metadata(mls_group)?;

    // 1) Check if the conversation type is DM
    if metadata.conversation_type != ConversationType::Dm {
        return Err(GroupError::Generic(
            "Invalid conversation type for DM group".to_string(),
        ));
    }

    // 2) If `dm_members` is not set, return an error immediately
    let dm_members = match &metadata.dm_members {
        Some(dm) => dm,
        None => {
            return Err(GroupError::Generic(
                "DM group must have DmMembers set".to_string(),
            ));
        }
    };

    // 3) If the inbox that added this group is our inbox, make sure that
    //    one of the `dm_members` is our inbox id
    if added_by_inbox == client.inbox_id() {
        if !(dm_members.member_one_inbox_id == client.inbox_id()
            || dm_members.member_two_inbox_id == client.inbox_id())
        {
            return Err(GroupError::Generic(
                "DM group must have our inbox as one of the dm members".to_string(),
            ));
        }
        return Ok(());
    }

    // 4) Otherwise, make sure one of the `dm_members` is ours, and the other is `added_by_inbox`
    let is_expected_pair = (dm_members.member_one_inbox_id == added_by_inbox
        && dm_members.member_two_inbox_id == client.inbox_id())
        || (dm_members.member_one_inbox_id == client.inbox_id()
            && dm_members.member_two_inbox_id == added_by_inbox);

    if !is_expected_pair {
        return Err(GroupError::Generic(
            "DM members do not match expected inboxes".to_string(),
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

    // Validate permissions so no one adds us to a dm that they can unexpectedly add another member to
    // Note: we don't validate mutable metadata permissions, because they don't affect group membership
    let permissions = extract_group_permissions(mls_group)?;
    let expected_permissions = GroupMutablePermissions::new(PolicySet::new_dm());

    if permissions.policies.add_member_policy != expected_permissions.policies.add_member_policy
        && permissions.policies.remove_member_policy
            != expected_permissions.policies.remove_member_policy
        && permissions.policies.add_admin_policy != expected_permissions.policies.add_admin_policy
        && permissions.policies.remove_admin_policy
            != expected_permissions.policies.remove_admin_policy
        && permissions.policies.update_permissions_policy
            != expected_permissions.policies.update_permissions_policy
    {
        return Err(GroupError::Generic(
            "Invalid permissions for DM group".to_string(),
        ));
    }

    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    #[cfg(not(target_arch = "wasm32"))]
    use crate::groups::scoped_client::ScopedGroupClient;
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
    use xmtp_id::associations::test_utils::WalletTestExt;
    use xmtp_id::associations::Identifier;
    use xmtp_proto::xmtp::mls::api::v1::group_message::Version;
    use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

    use super::{group_permissions::PolicySet, DMMetadataOptions, MlsGroup};
    use crate::groups::group_mutable_metadata::MessageDisappearingSettings;
    use crate::groups::{
        MAX_GROUP_DESCRIPTION_LENGTH, MAX_GROUP_IMAGE_URL_LENGTH, MAX_GROUP_NAME_LENGTH,
    };
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
    };
    use xmtp_common::StreamHandle as _;

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
            .prepare_group_messages(vec![(serialized_commit.as_slice(), false)])
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

    #[xmtp_common::test]
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

    #[xmtp_common::test]
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

    #[xmtp_common::test]
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
    #[xmtp_common::test]
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
    #[xmtp_common::test]
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

    #[xmtp_common::test]
    async fn test_dm_stitching() {
        let alix_wallet = generate_local_wallet();
        let alix = ClientBuilder::new_test_client(&alix_wallet).await;
        let alix_provider = alix.mls_provider().unwrap();
        let alix_conn = alix_provider.conn_ref();

        let bo_wallet = generate_local_wallet();
        let bo = ClientBuilder::new_test_client(&bo_wallet).await;

        let bo_dm = bo
            .find_or_create_dm_by_inbox_id(
                alix.inbox_id().to_string(),
                DMMetadataOptions::default(),
            )
            .await
            .unwrap();
        let alix_dm = alix
            .find_or_create_dm_by_inbox_id(bo.inbox_id().to_string(), DMMetadataOptions::default())
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
            .raw_query_read(|conn| groups::table.load::<StoredGroup>(conn))
            .unwrap();
        assert_eq!(alix_groups.len(), 2);
        // They should have the same ID
        assert_eq!(alix_groups[0].dm_id, alix_groups[1].dm_id);

        // The dm is filtered out up
        let mut alix_filtered_groups = alix_conn.find_groups(GroupQueryArgs::default()).unwrap();
        assert_eq!(alix_filtered_groups.len(), 1);

        let dm_group = alix_filtered_groups.pop().unwrap();

        let now = now_ns();
        let ten_seconds = 10_000_000_000;
        assert!(
            ((now - ten_seconds)..(now + ten_seconds)).contains(&dm_group.last_message_ns.unwrap()),
            "last_message_ns {} was not within one second of current time {}",
            dm_group.last_message_ns.unwrap(),
            now
        );

        let dm_group = alix.group(&dm_group.id).unwrap();
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

    #[xmtp_common::test]
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

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test(flavor = "current_thread")]
    #[ignore] // ignoring for now due to flakiness
    async fn test_create_group_with_member_two_installations_one_malformed_keypackage() {
        use xmtp_id::associations::test_utils::WalletTestExt;

        use crate::utils::set_test_mode_upload_malformed_keypackage;
        // 1) Prepare clients
        let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola_wallet = generate_local_wallet();

        // bola has two installations
        let bola_1 = ClientBuilder::new_test_client(&bola_wallet).await;
        let bola_2 = ClientBuilder::new_test_client(&bola_wallet).await;

        // 2) Mark the second installation as malformed
        set_test_mode_upload_malformed_keypackage(
            true,
            Some(vec![bola_2.installation_id().to_vec()]),
        );

        // 3) Create the group, inviting bola (which internally includes bola_1 and bola_2)
        let group = alix
            .create_group_with_members(
                &[bola_wallet.identifier()],
                None,
                GroupMetadataOptions::default(),
            )
            .await
            .unwrap();

        // 4) Sync from Alix's side
        group.sync().await.unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // 5) Bola_1 syncs welcomes and checks for groups
        bola_1
            .sync_welcomes(&bola_1.mls_provider().unwrap())
            .await
            .unwrap();
        bola_2
            .sync_welcomes(&bola_2.mls_provider().unwrap())
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let bola_1_groups = bola_1.find_groups(GroupQueryArgs::default()).unwrap();
        let bola_2_groups = bola_2.find_groups(GroupQueryArgs::default()).unwrap();

        assert_eq!(bola_1_groups.len(), 1, "Bola_1 should see exactly 1 group");
        assert_eq!(bola_2_groups.len(), 0, "Bola_2 should see no groups!");

        let bola_1_group = bola_1_groups.first().unwrap();
        bola_1_group.sync().await.unwrap();

        // 6) Verify group membership from both sides
        //    Here we expect 2 *members* (Alix + Bola), though internally Bola might have 2 installations.
        assert_eq!(
            group.members().await.unwrap().len(),
            2,
            "Group should have 2 members"
        );
        assert_eq!(
            bola_1_group.members().await.unwrap().len(),
            2,
            "Bola_1 should also see 2 members in the group"
        );

        // 7) Send a message from Alix and confirm Bola_1 receives it
        let message = b"Hello";
        group.send_message(message).await.unwrap();
        bola_1_group.send_message(message).await.unwrap();

        // Sync both sides again
        group.sync().await.unwrap();
        bola_1_group.sync().await.unwrap();

        // Query messages from Bola_1's perspective
        let messages_bola_1 = bola_1
            .api_client
            .query_group_messages(group.clone().group_id.clone(), None)
            .await
            .unwrap();

        // The last message should be our "Hello from Alix"
        assert_eq!(messages_bola_1.len(), 4);

        // Query messages from Alix's perspective
        let messages_alix = alix
            .api_client
            .query_group_messages(group.clone().group_id, None)
            .await
            .unwrap();

        // The last message should be our "Hello from Alix"
        assert_eq!(messages_alix.len(), 4);
        assert_eq!(
            message.to_vec(),
            get_latest_message(&group).await.decrypted_message_bytes
        );
        assert_eq!(
            message.to_vec(),
            get_latest_message(bola_1_group)
                .await
                .decrypted_message_bytes
        );
    }
    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test(flavor = "current_thread")]
    #[ignore]
    async fn test_create_group_with_member_all_malformed_installations() {
        use xmtp_id::associations::test_utils::WalletTestExt;

        use crate::utils::set_test_mode_upload_malformed_keypackage;
        // 1) Prepare clients
        let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        // bola has two installations
        let bola_wallet = generate_local_wallet();
        let bola_1 = ClientBuilder::new_test_client(&bola_wallet).await;
        let bola_2 = ClientBuilder::new_test_client(&bola_wallet).await;

        // 2) Mark both installations as malformed
        set_test_mode_upload_malformed_keypackage(
            true,
            Some(vec![
                bola_1.installation_id().to_vec(),
                bola_2.installation_id().to_vec(),
            ]),
        );

        // 3) Attempt to create the group, which should fail
        let result = alix
            .create_group_with_members(
                &[bola_wallet.identifier()],
                None,
                GroupMetadataOptions::default(),
            )
            .await;
        // 4) Ensure group creation failed
        assert!(
            result.is_err(),
            "Group creation should fail when all installations have bad key packages"
        );

        // 5) Ensure Bola does not have any groups on either installation
        bola_1
            .sync_welcomes(&bola_1.mls_provider().unwrap())
            .await
            .unwrap();
        bola_2
            .sync_welcomes(&bola_2.mls_provider().unwrap())
            .await
            .unwrap();

        let bola_1_groups = bola_1.find_groups(GroupQueryArgs::default()).unwrap();
        let bola_2_groups = bola_2.find_groups(GroupQueryArgs::default()).unwrap();

        assert_eq!(
            bola_1_groups.len(),
            0,
            "Bola_1 should have no groups after failed creation"
        );
        assert_eq!(
            bola_2_groups.len(),
            0,
            "Bola_2 should have no groups after failed creation"
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test(flavor = "current_thread")]
    #[ignore] // ignoring for now due to flakiness
    async fn test_dm_creation_with_user_two_installations_one_malformed() {
        use crate::utils::set_test_mode_upload_malformed_keypackage;
        // 1) Prepare clients
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola_wallet = generate_local_wallet();

        // Bola has two installations
        let bola_1 = ClientBuilder::new_test_client(&bola_wallet).await;
        let bola_2 = ClientBuilder::new_test_client(&bola_wallet).await;

        // 2) Mark bola_2's installation as malformed
        assert_ne!(bola_1.installation_id(), bola_2.installation_id());
        set_test_mode_upload_malformed_keypackage(
            true,
            Some(vec![bola_2.installation_id().to_vec()]),
        );

        // 3) Amal creates a DM group targeting Bola
        let amal_dm = amal
            .find_or_create_dm_by_inbox_id(
                bola_1.inbox_id().to_string(),
                DMMetadataOptions::default(),
            )
            .await
            .unwrap();

        // 4) Ensure the DM is created with only 2 members (Amal + one valid Bola installation)
        // amal_dm.sync().await.unwrap();
        let members = amal_dm.members().await.unwrap();
        assert_eq!(
            members.len(),
            2,
            "DM should contain only Amal and one valid Bola installation"
        );

        // 5) Bola_1 syncs and confirms it has the DM
        bola_1
            .sync_welcomes(&bola_1.mls_provider().unwrap())
            .await
            .unwrap();
        // tokio::time::sleep(std::time::Duration::from_secs(4)).await;

        let bola_groups = bola_1.find_groups(GroupQueryArgs::default()).unwrap();

        assert_eq!(bola_groups.len(), 1, "Bola_1 should see the DM group");

        let bola_1_dm: &MlsGroup<_> = bola_groups.first().unwrap();
        bola_1_dm.sync().await.unwrap();

        // 6) Ensure Bola_2 does NOT have the group
        bola_2
            .sync_welcomes(&bola_2.mls_provider().unwrap())
            .await
            .unwrap();
        let bola_2_groups = bola_2.find_groups(GroupQueryArgs::default()).unwrap();
        assert_eq!(
            bola_2_groups.len(),
            0,
            "Bola_2 should not have the DM group due to malformed key package"
        );

        // 7) Send a message from Amal to Bola_1
        let message_text = b"Hello from Amal";
        amal_dm.send_message(message_text).await.unwrap();

        // 8) Sync both sides and check message delivery
        amal_dm.sync().await.unwrap();
        bola_1_dm.sync().await.unwrap();

        // Verify Bola_1 received the message
        let messages_bola_1 = bola_1_dm.find_messages(&MsgQueryArgs::default()).unwrap();
        assert_eq!(
            messages_bola_1.len(),
            1,
            "Bola_1 should have received Amal's message"
        );

        let last_message = messages_bola_1.last().unwrap();
        assert_eq!(
            last_message.decrypted_message_bytes, message_text,
            "Bola_1 should receive the correct message"
        );

        // 9) Bola_1 replies, and Amal confirms receipt
        let reply_text = b"Hey Amal!";
        bola_1_dm.send_message(reply_text).await.unwrap();

        amal_dm.sync().await.unwrap();
        let messages_amal = amal_dm.find_messages(&MsgQueryArgs::default()).unwrap();
        assert_eq!(messages_amal.len(), 3, "Amal should receive Bola_1's reply");

        let last_message_amal = messages_amal.last().unwrap();
        assert_eq!(
            last_message_amal.decrypted_message_bytes, reply_text,
            "Amal should receive the correct reply from Bola_1"
        );

        // 10) Ensure only valid installations are considered for the DM
        assert_eq!(
            amal_dm.members().await.unwrap().len(),
            2,
            "Only Amal and Bola_1 should be in the DM"
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test(flavor = "current_thread")]
    #[ignore]
    async fn test_dm_creation_with_user_all_malformed_installations() {
        use xmtp_id::associations::test_utils::WalletTestExt;

        use crate::utils::set_test_mode_upload_malformed_keypackage;
        // 1) Prepare clients
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola_wallet = generate_local_wallet();

        // Bola has two installations
        let bola_1 = ClientBuilder::new_test_client(&bola_wallet).await;
        let bola_2 = ClientBuilder::new_test_client(&bola_wallet).await;

        // 2) Mark all of Bola's installations as malformed
        set_test_mode_upload_malformed_keypackage(
            true,
            Some(vec![
                bola_1.installation_id().to_vec(),
                bola_2.installation_id().to_vec(),
            ]),
        );

        // 3) Attempt to create the DM group, which should fail
        let result = amal
            .find_or_create_dm(bola_wallet.identifier(), DMMetadataOptions::default())
            .await;

        // 4) Ensure DM creation fails with the correct error
        assert!(result.is_err());

        // 5) Ensure Bola_1 does not have any groups
        bola_1
            .sync_welcomes(&bola_1.mls_provider().unwrap())
            .await
            .unwrap();
        let bola_1_groups = bola_1.find_groups(GroupQueryArgs::default()).unwrap();
        assert_eq!(
            bola_1_groups.len(),
            0,
            "Bola_1 should have no DM group due to malformed key package"
        );

        // 6) Ensure Bola_2 does not have any groups
        bola_2
            .sync_welcomes(&bola_2.mls_provider().unwrap())
            .await
            .unwrap();
        let bola_2_groups = bola_2.find_groups(GroupQueryArgs::default()).unwrap();
        assert_eq!(
            bola_2_groups.len(),
            0,
            "Bola_2 should have no DM group due to malformed key package"
        );
    }

    #[xmtp_common::test]
    async fn test_add_invalid_member() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let group = client
            .create_group(None, GroupMetadataOptions::default())
            .expect("create group");

        let result = group.add_members_by_inbox_id(&["1234".to_string()]).await;

        assert!(result.is_err());
    }

    #[xmtp_common::test]
    async fn test_add_unregistered_member() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let unconnected_ident = Identifier::rand_ethereum();
        let group = amal
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        let result = group.add_members(&[unconnected_ident]).await;

        assert!(result.is_err());
    }

    #[xmtp_common::test]
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

    #[xmtp_common::test]
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

    #[xmtp_common::test]
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

    #[xmtp_common::test]
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
            .add_members(&[bola_wallet.identifier(), charlie_wallet.identifier()])
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
            .remove_members(&[bola_wallet.identifier()])
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

    #[xmtp_common::test]
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
            .add_members(&[bola_wallet.identifier(), charlie_wallet.identifier()])
            .await
            .unwrap();
        assert_eq!(amal_group.members().await.unwrap().len(), 3);

        amal_group
            .remove_members(&[bola_wallet.identifier()])
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

    #[xmtp_common::test]
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

    #[xmtp_common::test(flavor = "multi_thread")]
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

    #[xmtp_common::test]
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

    #[xmtp_common::test]
    async fn test_group_options() {
        let expected_group_message_disappearing_settings =
            MessageDisappearingSettings::new(100, 200);

        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let amal_group = amal
            .create_group(
                None,
                GroupMetadataOptions {
                    name: Some("Group Name".to_string()),
                    image_url_square: Some("url".to_string()),
                    description: Some("group description".to_string()),
                    message_disappearing_settings: Some(
                        expected_group_message_disappearing_settings.clone(),
                    ),
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
        let amal_group_message_disappear_from_ns = binding
            .attributes
            .get(&MetadataField::MessageDisappearFromNS.to_string())
            .unwrap();
        let amal_group_message_disappear_in_ns = binding
            .attributes
            .get(&MetadataField::MessageDisappearInNS.to_string())
            .unwrap();
        assert_eq!(amal_group_name, "Group Name");
        assert_eq!(amal_group_image_url, "url");
        assert_eq!(amal_group_description, "group description");
        assert_eq!(
            amal_group_message_disappear_from_ns.clone(),
            expected_group_message_disappearing_settings
                .from_ns
                .to_string()
        );
        assert_eq!(
            amal_group_message_disappear_in_ns.clone(),
            expected_group_message_disappearing_settings
                .in_ns
                .to_string()
        );
    }

    #[xmtp_common::test]
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
            clients.push(wallet.identifier());
        }
        amal_group.add_members(&clients).await.unwrap();
        let bola_wallet = generate_local_wallet();
        ClientBuilder::new_test_client(&bola_wallet).await;
        assert!(amal_group
            .add_members_by_inbox_id(&[bola_wallet.get_inbox_id(0)])
            .await
            .is_err(),);
    }

    #[xmtp_common::test]
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
        assert!(group_mutable_metadata.attributes.len().eq(&3));
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

    #[xmtp_common::test]
    async fn test_update_policies_empty_group() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola_wallet = generate_local_wallet();
        let _bola = ClientBuilder::new_test_client(&bola_wallet).await;

        // Create a group with amal and bola
        let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
        let amal_group = amal
            .create_group_with_members(
                &[bola_wallet.identifier()],
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

    #[xmtp_common::test]
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

    #[xmtp_common::test(flavor = "current_thread")]
    async fn test_update_group_message_expiration_settings() {
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
        assert_eq!(
            group_mutable_metadata
                .attributes
                .get(&MetadataField::MessageDisappearInNS.to_string()),
            None
        );
        assert_eq!(
            group_mutable_metadata
                .attributes
                .get(&MetadataField::MessageDisappearFromNS.to_string()),
            None
        );

        // Update group name
        let expected_group_message_expiration_settings = MessageDisappearingSettings::new(100, 200);

        amal_group
            .update_conversation_message_disappearing_settings(
                expected_group_message_expiration_settings.clone(),
            )
            .await
            .unwrap();

        // Verify amal group sees update
        amal_group.sync().await.unwrap();
        let binding = amal_group
            .mutable_metadata(&amal_group.mls_provider().unwrap())
            .expect("msg");
        let amal_message_expiration_from_ms: &String = binding
            .attributes
            .get(&MetadataField::MessageDisappearFromNS.to_string())
            .unwrap();
        let amal_message_disappear_in_ns: &String = binding
            .attributes
            .get(&MetadataField::MessageDisappearInNS.to_string())
            .unwrap();
        assert_eq!(
            amal_message_expiration_from_ms.clone(),
            expected_group_message_expiration_settings
                .from_ns
                .to_string()
        );
        assert_eq!(
            amal_message_disappear_in_ns.clone(),
            expected_group_message_expiration_settings.in_ns.to_string()
        );
    }

    #[xmtp_common::test(flavor = "current_thread")]
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
            .add_members(&[bola_wallet.identifier()])
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

    #[xmtp_common::test]
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
            .add_members(&[bola_wallet.identifier()])
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

    #[xmtp_common::test]
    async fn test_group_super_admin_list_update() {
        let bola_wallet = generate_local_wallet();
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola = ClientBuilder::new_test_client(&bola_wallet).await;
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
            .remove_members(&[bola_wallet.identifier()])
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

    #[xmtp_common::test]
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

    #[xmtp_common::test]
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
        let bola_fetched_group = bola.group(&bola_group_id).unwrap();

        // Check Bola's group for the added_by_inbox_id of the inviter
        let added_by_inbox = bola_fetched_group.added_by_inbox_id().unwrap();

        // Verify the welcome host_credential is equal to Amal's
        assert_eq!(
            amal.inbox_id(),
            added_by_inbox,
            "The Inviter and added_by_address do not match!"
        );
    }

    #[xmtp_common::test]
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

    #[xmtp_common::test]
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

    #[xmtp_common::test]
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

    #[xmtp_common::test]
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
            .add_members(&[bola_wallet.identifier()])
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

    #[xmtp_common::test]
    async fn test_dm_creation() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let caro = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        // Amal creates a dm group targetting bola
        let amal_dm = amal
            .find_or_create_dm_by_inbox_id(
                bola.inbox_id().to_string(),
                DMMetadataOptions::default(),
            )
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

    #[xmtp_common::test]
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
            .raw_query_write(|c| {
                c.batch_execute("BEGIN EXCLUSIVE").unwrap();
                Ok::<_, diesel::result::Error>(())
            })
            .unwrap();

        let process_result = bo_group.process_messages(bo_messages, &conn_1).await;
        if let Some(GroupError::ReceiveErrors(errors)) = process_result.err() {
            assert_eq!(errors.len(), 1);
            assert!(errors.iter().any(|err| err
                .to_string()
                .contains("cannot start a transaction within a transaction")));
        } else {
            panic!("Expected error")
        }
    }

    #[xmtp_common::test]
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
        let Some(GroupError::ReceiveErrors(errors)) = process_result.err() else {
            panic!("Expected error")
        };

        assert_eq!(errors.len(), 2);
        assert!(errors
            .iter()
            .any(|err| err.to_string().contains("already processed")));
    }

    #[xmtp_common::test(flavor = "multi_thread")]
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
                xmtp_common::spawn(None, async move { group_clone.sync().await }).join()
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
                false,
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
    #[xmtp_common::test(flavor = "multi_thread")]
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

    #[xmtp_common::test(flavor = "multi_thread")]
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

    #[xmtp_common::test]
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

    #[xmtp_common::test]
    // TODO(rich): Generalize the test once fixed - test messages that are 0, 1, 2, 3, 4, 5 epochs behind
    async fn test_max_past_epochs() {
        // Create group with two members
        let bo_wallet = generate_local_wallet();
        let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bo = ClientBuilder::new_test_client(&bo_wallet).await;
        let alix_group = alix
            .create_group_with_members(
                &[bo_wallet.identifier()],
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
            DMMetadataOptions::default(),
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
            DMMetadataOptions::default(),
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
            DMMetadataOptions::default(),
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
            DMMetadataOptions::default(),
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
            DMMetadataOptions::default(),
        )
        .unwrap();
        assert!(matches!(
            invalid_permissions_group.load_mls_group_with_lock(client.mls_provider().unwrap(), |mls_group|
                validate_dm_group(&client, &mls_group, added_by_inbox)
            ),
            Err(GroupError::Generic(msg)) if msg.contains("Invalid permissions for DM group")
        ));
    }

    #[xmtp_common::test]
    async fn test_respects_character_limits_for_group_metadata() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
        let amal_group = amal
            .create_group(policy_set, GroupMetadataOptions::default())
            .unwrap();
        amal_group.sync().await.unwrap();

        let overlong_name = "a".repeat(MAX_GROUP_NAME_LENGTH + 1);
        let overlong_description = "b".repeat(MAX_GROUP_DESCRIPTION_LENGTH + 1);
        let overlong_image_url =
            "http://example.com/".to_string() + &"c".repeat(MAX_GROUP_IMAGE_URL_LENGTH);

        // Verify that updating the name with an excessive length fails
        let result = amal_group.update_group_name(overlong_name).await;
        assert!(
            matches!(result, Err(GroupError::TooManyCharacters { length }) if length == MAX_GROUP_NAME_LENGTH)
        );

        // Verify that updating the description with an excessive length fails
        let result = amal_group
            .update_group_description(overlong_description)
            .await;
        assert!(
            matches!(result, Err(GroupError::TooManyCharacters { length }) if length == MAX_GROUP_DESCRIPTION_LENGTH)
        );

        // Verify that updating the image URL with an excessive length fails
        let result = amal_group
            .update_group_image_url_square(overlong_image_url)
            .await;
        assert!(
            matches!(result, Err(GroupError::TooManyCharacters { length }) if length == MAX_GROUP_IMAGE_URL_LENGTH)
        );

        // Verify updates with valid lengths are successful
        let valid_name = "Valid Group Name".to_string();
        let valid_description = "Valid group description within limit.".to_string();
        let valid_image_url = "http://example.com/image.png".to_string();

        amal_group
            .update_group_name(valid_name.clone())
            .await
            .unwrap();
        amal_group
            .update_group_description(valid_description.clone())
            .await
            .unwrap();
        amal_group
            .update_group_image_url_square(valid_image_url.clone())
            .await
            .unwrap();

        // Sync and verify stored values
        amal_group.sync().await.unwrap();

        let provider = amal_group.mls_provider().unwrap();
        let metadata = amal_group.mutable_metadata(&provider).unwrap();

        assert_eq!(
            metadata
                .attributes
                .get(&MetadataField::GroupName.to_string())
                .unwrap(),
            &valid_name
        );
        assert_eq!(
            metadata
                .attributes
                .get(&MetadataField::Description.to_string())
                .unwrap(),
            &valid_description
        );
        assert_eq!(
            metadata
                .attributes
                .get(&MetadataField::GroupImageUrlSquare.to_string())
                .unwrap(),
            &valid_image_url
        );
    }

    fn increment_patch_version(version: &str) -> Option<String> {
        // Split version into numeric part and suffix (if any)
        let (version_part, suffix) = match version.split_once('-') {
            Some((v, s)) => (v, Some(s)),
            None => (version, None),
        };

        // Split numeric version string into components
        let mut parts: Vec<&str> = version_part.split('.').collect();

        // Ensure we have exactly 3 parts (major.minor.patch)
        if parts.len() != 3 {
            return None;
        }

        // Parse the patch number and increment it
        let patch = parts[2].parse::<u32>().ok()?;
        let new_patch = patch + 1;

        // Replace the patch number with the incremented value
        let binding = new_patch.to_string();
        parts[2] = &binding;

        // Join the parts back together with dots and add suffix if it existed
        let new_version = parts.join(".");
        match suffix {
            Some(s) => Some(format!("{}-{}", new_version, s)),
            None => Some(new_version),
        }
    }

    #[xmtp_common::test]
    fn test_increment_patch_version() {
        assert_eq!(increment_patch_version("1.2.3"), Some("1.2.4".to_string()));
        assert_eq!(increment_patch_version("0.0.9"), Some("0.0.10".to_string()));
        assert_eq!(increment_patch_version("1.0.0"), Some("1.0.1".to_string()));
        assert_eq!(
            increment_patch_version("1.0.0-alpha"),
            Some("1.0.1-alpha".to_string())
        );

        // Invalid inputs should return None
        assert_eq!(increment_patch_version("1.2"), None);
        assert_eq!(increment_patch_version("1.2.3.4"), None);
        assert_eq!(increment_patch_version("invalid"), None);
    }

    #[xmtp_common::test]
    async fn test_can_set_min_supported_protocol_version_for_commit() {
        // Step 1: Create two clients, amal is one version ahead of bo
        let mut amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let amal_version = amal.version_info().pkg_version();
        amal.test_update_version(increment_patch_version(amal_version).unwrap().as_str());

        let mut bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        // Step 2: Amal creates a group and adds bo as a member
        let amal_group = amal
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        amal_group
            .add_members_by_inbox_id(&[bo.context.identity.inbox_id()])
            .await
            .unwrap();

        // Step 3: Amal updates the group name and sends a message to the group
        amal_group
            .update_group_name("new name".to_string())
            .await
            .unwrap();
        amal_group
            .send_message("Hello, world!".as_bytes())
            .await
            .unwrap();

        // Step 4: Verify that bo can read the message even though they are on different client versions
        bo.sync_welcomes(&bo.mls_provider().unwrap()).await.unwrap();
        let binding = bo.find_groups(GroupQueryArgs::default()).unwrap();
        let bo_group = binding.first().unwrap();
        bo_group.sync().await.unwrap();
        let messages = bo_group.find_messages(&MsgQueryArgs::default()).unwrap();
        assert_eq!(messages.len(), 2);

        let message_text = String::from_utf8_lossy(&messages[1].decrypted_message_bytes);
        assert_eq!(message_text, "Hello, world!");

        // Step 5: Amal updates the group version to match their client version
        amal_group
            .update_group_min_version_to_match_self()
            .await
            .unwrap();
        amal_group.sync().await.unwrap();
        amal_group
            .send_message("new version only!".as_bytes())
            .await
            .unwrap();

        // Step 6: Bo should now be unable to sync messages for the group
        let _ = bo_group.sync().await;
        let messages = bo_group.find_messages(&MsgQueryArgs::default()).unwrap();
        assert_eq!(messages.len(), 2);

        // Step 7: Bo updates their client, and see if we can then download latest messages
        let bo_version = bo.version_info().pkg_version();
        bo.test_update_version(increment_patch_version(bo_version).unwrap().as_str());

        // Refresh Bo's group context
        let binding = bo.find_groups(GroupQueryArgs::default()).unwrap();
        let bo_group = binding.first().unwrap();

        bo_group.sync().await.unwrap();
        let _ = bo_group.sync().await;
        let messages = bo_group.find_messages(&MsgQueryArgs::default()).unwrap();
        assert_eq!(messages.len(), 4);
    }

    #[xmtp_common::test]
    async fn test_client_on_old_version_pauses_after_joining_min_version_group() {
        // Step 1: Create three clients, amal and bo are one version ahead of caro
        let mut amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let amal_version = amal.version_info().pkg_version();
        amal.test_update_version(increment_patch_version(amal_version).unwrap().as_str());

        let mut bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bo_version = bo.version_info().pkg_version();
        bo.test_update_version(increment_patch_version(bo_version).unwrap().as_str());

        let mut caro = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        assert!(caro.version_info().pkg_version() != amal.version_info().pkg_version());
        assert!(bo.version_info().pkg_version() == amal.version_info().pkg_version());

        // Step 2: Amal creates a group and adds bo as a member
        let amal_group = amal
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        amal_group
            .add_members_by_inbox_id(&[bo.context.identity.inbox_id()])
            .await
            .unwrap();

        // Step 3: Amal sends a message to the group
        amal_group
            .send_message("Hello, world!".as_bytes())
            .await
            .unwrap();

        // Step 4: Verify that bo can read the message
        bo.sync_welcomes(&bo.mls_provider().unwrap()).await.unwrap();
        let binding = bo.find_groups(GroupQueryArgs::default()).unwrap();
        let bo_group = binding.first().unwrap();
        bo_group.sync().await.unwrap();
        let messages = bo_group.find_messages(&MsgQueryArgs::default()).unwrap();
        assert_eq!(messages.len(), 1);

        let message_text = String::from_utf8_lossy(&messages[0].decrypted_message_bytes);
        assert_eq!(message_text, "Hello, world!");

        // Step 5: Amal updates the group to have a min version of current version + 1
        amal_group
            .update_group_min_version_to_match_self()
            .await
            .unwrap();
        amal_group.sync().await.unwrap();
        amal_group
            .send_message("new version only!".as_bytes())
            .await
            .unwrap();

        // Step 6: Bo should still be able to sync messages for the group
        let _ = bo_group.sync().await;
        let messages = bo_group.find_messages(&MsgQueryArgs::default()).unwrap();
        assert_eq!(messages.len(), 3);

        // Step 7: Amal adds caro as a member
        amal_group
            .add_members_by_inbox_id(&[caro.context.identity.inbox_id()])
            .await
            .unwrap();

        // Caro received the invite for the group
        caro.sync_welcomes(&caro.mls_provider().unwrap())
            .await
            .unwrap();
        let binding = caro.find_groups(GroupQueryArgs::default()).unwrap();
        let caro_group = binding.first().unwrap();
        assert!(caro_group.group_id == amal_group.group_id);

        // Caro group is paused immediately after joining
        let is_paused = caro_group
            .paused_for_version(&caro.mls_provider().unwrap())
            .unwrap()
            .is_some();
        assert!(is_paused);
        let result = caro_group.send_message("Hello from Caro".as_bytes()).await;
        assert!(matches!(result, Err(GroupError::GroupPausedUntilUpdate(_))));

        // Caro updates their client to the same version as amal and syncs to unpause the group
        let caro_version = caro.version_info().pkg_version();
        caro.test_update_version(increment_patch_version(caro_version).unwrap().as_str());
        let binding = caro.find_groups(GroupQueryArgs::default()).unwrap();
        let caro_group = binding.first().unwrap();
        assert!(caro_group.group_id == amal_group.group_id);
        caro_group.sync().await.unwrap();

        // Caro should now be able to send a message
        caro_group
            .send_message("Hello from Caro".as_bytes())
            .await
            .unwrap();
        amal_group.sync().await.unwrap();
        let messages = amal_group.find_messages(&MsgQueryArgs::default()).unwrap();
        assert_eq!(
            messages[messages.len() - 1].decrypted_message_bytes,
            "Hello from Caro".as_bytes()
        );
    }

    #[xmtp_common::test]
    async fn test_only_super_admins_can_set_min_supported_protocol_version() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let amal_group = amal
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        amal_group
            .add_members_by_inbox_id(&[bo.context.identity.inbox_id()])
            .await
            .unwrap();
        amal_group
            .update_admin_list(
                UpdateAdminListType::Add,
                bo.context.identity.inbox_id().to_string(),
            )
            .await
            .unwrap();
        amal_group.sync().await.unwrap();

        let is_bo_admin = amal_group
            .is_admin(
                bo.context.identity.inbox_id().to_string(),
                &amal.mls_provider().unwrap(),
            )
            .unwrap();
        assert!(is_bo_admin);

        let is_bo_super_admin = amal_group
            .is_super_admin(
                bo.context.identity.inbox_id().to_string(),
                &amal.mls_provider().unwrap(),
            )
            .unwrap();
        assert!(!is_bo_super_admin);

        bo.sync_welcomes(&bo.mls_provider().unwrap()).await.unwrap();
        let binding = bo.find_groups(GroupQueryArgs::default()).unwrap();
        let bo_group = binding.first().unwrap();
        bo_group.sync().await.unwrap();

        let metadata = bo_group
            .mutable_metadata(&amal_group.mls_provider().unwrap())
            .unwrap();
        let min_version = metadata
            .attributes
            .get(&MetadataField::MinimumSupportedProtocolVersion.to_string());
        assert_eq!(min_version, None);

        let result = bo_group.update_group_min_version_to_match_self().await;
        assert!(result.is_err());
        bo_group.sync().await.unwrap();

        let metadata = bo_group
            .mutable_metadata(&bo_group.mls_provider().unwrap())
            .unwrap();
        let min_version = metadata
            .attributes
            .get(&MetadataField::MinimumSupportedProtocolVersion.to_string());
        assert_eq!(min_version, None);

        amal_group.sync().await.unwrap();
        let result = amal_group.update_group_min_version_to_match_self().await;
        assert!(result.is_ok());
        bo_group.sync().await.unwrap();

        let metadata = bo_group
            .mutable_metadata(&bo_group.mls_provider().unwrap())
            .unwrap();
        let min_version = metadata
            .attributes
            .get(&MetadataField::MinimumSupportedProtocolVersion.to_string());
        assert_eq!(min_version.unwrap(), amal.version_info().pkg_version());
    }

    #[xmtp_common::test]
    async fn test_send_message_while_paused_after_welcome_returns_expected_error() {
        // Create two clients with different versions
        let mut amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let amal_version = amal.version_info().pkg_version();
        amal.test_update_version(increment_patch_version(amal_version).unwrap().as_str());

        let bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        // Amal creates a group and adds bo
        let amal_group = amal
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        amal_group
            .add_members_by_inbox_id(&[bo.context.identity.inbox_id()])
            .await
            .unwrap();

        // Amal sets minimum version requirement
        amal_group
            .update_group_min_version_to_match_self()
            .await
            .unwrap();
        amal_group.sync().await.unwrap();

        // Bo joins group and attempts to send message
        bo.sync_welcomes(&bo.mls_provider().unwrap()).await.unwrap();
        let binding = bo.find_groups(GroupQueryArgs::default()).unwrap();
        let bo_group = binding.first().unwrap();

        // If bo tries to send a message before syncing the group, we get a SyncFailedToWait error
        let result = bo_group.send_message("Hello from Bo".as_bytes()).await;
        assert!(
            matches!(result, Err(GroupError::SyncFailedToWait)),
            "Expected SyncFailedToWait error, got {:?}",
            result
        );

        bo_group.sync().await.unwrap();

        // After syncing if we attempt to send message - should fail with GroupPausedUntilUpdate error
        let result = bo_group.send_message("Hello from Bo".as_bytes()).await;
        if let Err(GroupError::GroupPausedUntilUpdate(version)) = result {
            assert_eq!(version, amal.version_info().pkg_version());
        } else {
            panic!("Expected GroupPausedUntilUpdate error, got {:?}", result);
        }
    }

    #[xmtp_common::test]
    async fn test_send_message_after_min_version_update_gets_expected_error() {
        // Create two clients with different versions
        let mut amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let amal_version = amal.version_info().pkg_version();
        amal.test_update_version(increment_patch_version(amal_version).unwrap().as_str());

        let mut bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        // Amal creates a group and adds bo
        let amal_group = amal
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        amal_group
            .add_members_by_inbox_id(&[bo.context.identity.inbox_id()])
            .await
            .unwrap();

        // Bo joins group and successfully sends initial message
        bo.sync_welcomes(&bo.mls_provider().unwrap()).await.unwrap();
        let binding = bo.find_groups(GroupQueryArgs::default()).unwrap();
        let bo_group = binding.first().unwrap();
        bo_group.sync().await.unwrap();

        bo_group
            .send_message("Hello from Bo".as_bytes())
            .await
            .unwrap();

        // Amal sets new minimum version requirement
        amal_group
            .update_group_min_version_to_match_self()
            .await
            .unwrap();
        amal_group.sync().await.unwrap();

        // Bo's attempt to send message before syncing should now fail with SyncFailedToWait error
        let result = bo_group
            .send_message("Second message from Bo".as_bytes())
            .await;
        assert!(
            matches!(result, Err(GroupError::SyncFailedToWait)),
            "Expected SyncFailedToWait error, got {:?}",
            result
        );

        // Bo syncs to get the version update
        bo_group.sync().await.unwrap();

        // After syncing if we attempt to send message - should fail with GroupPausedUntilUpdate error
        let result = bo_group.send_message("Hello from Bo".as_bytes()).await;
        if let Err(GroupError::GroupPausedUntilUpdate(version)) = result {
            assert_eq!(version, amal.version_info().pkg_version());
        } else {
            panic!("Expected GroupPausedUntilUpdate error, got {:?}", result);
        }

        // Verify Bo can send again after updating their version
        let bo_version = bo.version_info().pkg_version();
        bo.test_update_version(increment_patch_version(bo_version).unwrap().as_str());

        // Need to get fresh group reference after version update
        let binding = bo.find_groups(GroupQueryArgs::default()).unwrap();
        let bo_group = binding.first().unwrap();
        bo_group.sync().await.unwrap();

        // Should now succeed
        let result = bo_group
            .send_message("Message after update".as_bytes())
            .await;
        assert!(result.is_ok());
    }
}
