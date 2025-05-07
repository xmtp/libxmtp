pub mod device_sync;
pub mod device_sync_legacy;
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
pub mod summary;
#[cfg(test)]
mod tests;
pub mod validated_commit;

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
use crate::subscriptions::SyncWorkerEvent;
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
    subscriptions::{LocalEventError, LocalEvents},
    utils::id::calculate_message_id,
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
use summary::SyncSummary;
use thiserror::Error;
use tokio::sync::Mutex;
use validated_commit::LibXMTPVersion;
use xmtp_common::retry::RetryableError;
use xmtp_common::time::now_ns;
use xmtp_content_types::reaction::{LegacyReaction, ReactionCodec};
use xmtp_content_types::should_push;
use xmtp_cryptography::signature::IdentifierValidationError;
use xmtp_db::consent_record::ConsentType;
use xmtp_db::user_preferences::HmacKey;
use xmtp_db::xmtp_openmls_provider::XmtpOpenMlsProvider;
use xmtp_db::Store;
use xmtp_db::{
    consent_record::{ConsentState, StoredConsentRecord},
    db_connection::DbConnection,
    group::{ConversationType, GroupMembershipState, StoredGroup},
    group_intent::IntentKind,
    group_message::{DeliveryStatus, GroupMessageKind, MsgQueryArgs, StoredGroupMessage},
    sql_key_store,
};
use xmtp_db::{
    group_message::{ContentType, StoredGroupMessageWithReactions},
    refresh_state::EntityKind,
    NotFound, ProviderTransactions, StorageError,
};
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

#[derive(Error, Debug)]
pub struct ReceiveErrors {
    /// list of message ids we received
    ids: Vec<u64>,
    errors: Vec<GroupMessageProcessingError>,
}

impl RetryableError for ReceiveErrors {
    fn is_retryable(&self) -> bool {
        self.errors.iter().any(|e| e.is_retryable())
    }
}

impl ReceiveErrors {
    pub fn new(errors: Vec<GroupMessageProcessingError>, ids: Vec<u64>) -> Self {
        Self { ids, errors }
    }
}

impl std::fmt::Display for ReceiveErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let errs: HashSet<String> = self.errors.iter().map(|e| e.to_string()).collect();
        let mut sorted = self.ids.clone();
        sorted.sort();
        writeln!(
            f,
            "\n=========================== Receive Errors  =====================\n\
            total of [{}] errors processing [{}] messages in cursor range [{:?} ... {:?}]\n\
            [{}] unique errors:",
            self.errors.len(),
            self.ids.len(),
            sorted.first(),
            sorted.last(),
            errs.len(),
        )?;
        for err in errs.iter() {
            writeln!(f, "{}", err)?;
        }
        writeln!(
            f,
            "================================================================="
        )?;
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum GroupError {
    #[error(transparent)]
    NotFound(#[from] NotFound),
    #[error("Max user limit exceeded.")]
    UserLimitExceeded,
    #[error("api error: {0}")]
    WrappedApi(#[from] xmtp_api::ApiError),
    #[error("invalid group membership")]
    InvalidGroupMembership,
    #[error("storage error: {0}")]
    Storage(#[from] xmtp_db::StorageError),
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
    #[error("Receive errors: {0}")]
    ReceiveErrors(ReceiveErrors),
    #[error("generic: {0}")]
    Generic(String),
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
    SyncFailedToWait(SyncSummary),
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
    #[error("Group is inactive")]
    GroupInactive,
    #[error("{}", _0.to_string())]
    Sync(#[from] SyncSummary),
}

impl RetryableError for GroupError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::ReceiveErrors(errors) => errors.is_retryable(),
            Self::Client(client_error) => client_error.is_retryable(),
            Self::Storage(storage) => storage.is_retryable(),
            Self::ReceiveError(msg) => msg.is_retryable(),
            Self::Hpke(hpke) => hpke.is_retryable(),
            Self::Identity(identity) => identity.is_retryable(),
            Self::UpdateGroupMembership(update) => update.is_retryable(),
            Self::GroupCreate(group) => group.is_retryable(),
            Self::SelfUpdate(update) => update.is_retryable(),
            Self::WelcomeError(welcome) => welcome.is_retryable(),
            Self::SqlKeyStore(sql) => sql.is_retryable(),
            Self::InstallationDiff(diff) => diff.is_retryable(),
            Self::CreateGroupContextExtProposalError(create) => create.is_retryable(),
            Self::CommitValidation(err) => err.is_retryable(),
            Self::WrappedApi(err) => err.is_retryable(),
            Self::MessageHistory(err) => err.is_retryable(),
            Self::ProcessIntent(err) => err.is_retryable(),
            Self::LocalEvent(err) => err.is_retryable(),
            Self::LockUnavailable => true,
            Self::LockFailedToAcquire => true,
            Self::SyncFailedToWait(_) => true,
            Self::Sync(s) => s.is_retryable(),
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
            | Self::GroupPausedUntilUpdate(_)
            | Self::GroupInactive => false,
        }
    }
}

pub struct MlsGroup<C> {
    pub group_id: Vec<u8>,
    pub dm_id: Option<String>,
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
            dm_id: self.dm_id.clone(),
            created_at_ns: self.created_at_ns,
            client: self.client.clone(),
            mutex: self.mutex.clone(),
            mls_commit_lock: self.mls_commit_lock.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConversationDebugInfo {
    pub epoch: u64,
    pub maybe_forked: bool,
    pub fork_details: String,
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
    pub fn new(
        client: ScopedClient,
        group_id: Vec<u8>,
        dm_id: Option<String>,
        created_at_ns: i64,
    ) -> Self {
        Self::new_from_arc(Arc::new(client), group_id, dm_id, created_at_ns)
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
                Self::new_from_arc(
                    Arc::new(client),
                    group_id,
                    group.dm_id.clone(),
                    group.created_at_ns,
                ),
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
        dm_id: Option<String>,
        created_at_ns: i64,
    ) -> Self {
        let mut mutexes = client.context().mutexes.clone();
        let context = client.context();
        Self {
            group_id: group_id.clone(),
            dm_id,
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
        let stored_group = Self::insert(
            &client,
            provider,
            None,
            membership_state,
            permissions_policy_set,
            opts,
        )?;
        let new_group = Self::new_from_arc(
            client.clone(),
            stored_group.id,
            stored_group.dm_id,
            stored_group.created_at_ns,
        );

        // Consent state defaults to allowed when the user creates the group
        new_group.update_consent_state(ConsentState::Allowed)?;
        Ok(new_group)
    }

    // Save a new group to the db
    pub(crate) fn insert(
        client: &ScopedClient,
        provider: &XmtpOpenMlsProvider,
        group_id: Option<&[u8]>,
        membership_state: GroupMembershipState,
        permissions_policy_set: PolicySet,
        opts: GroupMetadataOptions,
    ) -> Result<StoredGroup, GroupError> {
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

        let mls_group = if let Some(group_id) = group_id {
            OpenMlsGroup::new_with_group_id(
                provider,
                &context.identity.installation_keys,
                &group_config,
                GroupId::from_slice(group_id),
                CredentialWithKey {
                    credential: context.identity.credential(),
                    signature_key: context.identity.installation_keys.public_slice().into(),
                },
            )?
        } else {
            OpenMlsGroup::new(
                provider,
                &context.identity.installation_keys,
                &group_config,
                CredentialWithKey {
                    credential: context.identity.credential(),
                    signature_key: context.identity.installation_keys.public_slice().into(),
                },
            )?
        };

        let group_id = mls_group.group_id().to_vec();
        let stored_group = StoredGroup::builder()
            .id(group_id.clone())
            .created_at_ns(now_ns())
            .membership_state(membership_state)
            .added_by_inbox_id(context.inbox_id().to_string())
            .message_disappear_from_ns(
                opts.message_disappearing_settings
                    .as_ref()
                    .map(|m| m.from_ns),
            )
            .message_disappear_in_ns(opts.message_disappearing_settings.as_ref().map(|m| m.in_ns))
            .build()?;

        stored_group.store(provider.conn_ref())?;

        Ok(stored_group)
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
        let stored_group = StoredGroup::builder()
            .id(group_id.clone())
            .created_at_ns(now_ns())
            .membership_state(membership_state)
            .added_by_inbox_id(context.inbox_id().to_string())
            .message_disappear_from_ns(
                opts.message_disappearing_settings
                    .as_ref()
                    .map(|m| m.from_ns),
            )
            .message_disappear_in_ns(opts.message_disappearing_settings.as_ref().map(|m| m.in_ns))
            .dm_id(Some(
                DmMembers {
                    member_one_inbox_id: dm_target_inbox_id,
                    member_two_inbox_id: client.inbox_id().to_string(),
                }
                .to_string(),
            ))
            .build()?;

        stored_group.store(provider.conn_ref())?;
        let new_group = Self::new_from_arc(
            client.clone(),
            group_id,
            stored_group.dm_id.clone(),
            stored_group.created_at_ns,
        );
        // Consent state defaults to allowed when the user creates the group
        new_group.update_consent_state(ConsentState::Allowed)?;
        Ok(new_group)
    }

    /// Create a group from a decrypted and decoded welcome message.
    /// If the group already exists in the store, overwrite the MLS state and do not update the group entry
    ///
    /// # Parameters
    /// * `client` - The client context to use for group operations
    /// * `provider` - The OpenMLS provider for database access
    /// * `welcome` - The encrypted welcome message
    /// * `allow_cursor_increment` - Controls whether to allow cursor increments during processing.
    ///   Set to `true` when processing messages from trusted ordered sources (queries), and `false` when
    ///   processing from potentially out-of-order sources like streams.
    #[tracing::instrument(skip_all, level = "debug")]
    pub(super) async fn create_from_welcome(
        client: &ScopedClient,
        provider: &XmtpOpenMlsProvider,
        welcome: &welcome_message::V1,
        allow_cursor_increment: bool,
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
            let group = Self::new(client.clone(), group.id, group.dm_id, group.created_at_ns);

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

            let requires_processing = if allow_cursor_increment {
                tracing::info!(
                    "calling update cursor for welcome {}, allow_cursor_increment is true",
                    welcome.id
                );
                provider.conn_ref().update_cursor(
                    client.context().installation_public_key(),
                    EntityKind::Welcome,
                    welcome.id as i64,
                )?
            } else {
                tracing::info!(
                    "will not call update cursor for welcome {}, allow_cursor_increment is false",
                    welcome.id
                );
                let current_cursor = provider
                    .conn_ref()
                    .get_last_cursor_for_id(client.context().installation_public_key(), EntityKind::Welcome)?;
                current_cursor < welcome.id as i64
            };
            if !requires_processing {
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

            let mut group = StoredGroup::builder();
            group.id(group_id)
                .created_at_ns(now_ns())
                .added_by_inbox_id(&added_by_inbox_id)
                .welcome_id(welcome.id as i64)
                .conversation_type(conversation_type)
                .dm_id(dm_members.map(String::from))
                .message_disappear_from_ns(disappearing_settings.as_ref().map(|m| m.from_ns))
                .message_disappear_in_ns(disappearing_settings.as_ref().map(|m| m.in_ns));

            let to_store = match conversation_type {
                ConversationType::Group => {
                    group
                        .membership_state(GroupMembershipState::Pending)
                        .paused_for_version(paused_for_version)
                        .build()?
                },
                ConversationType::Dm => {
                    validate_dm_group(client, &mls_group, &added_by_inbox_id)?;
                    group
                        .membership_state(GroupMembershipState::Pending)
                        .last_message_ns(welcome.created_ns as i64)
                        .build()?
                }
                ConversationType::Sync => {
                    // Let the DeviceSync worker know about the presence of a new
                    // sync group that came in from a welcome.3
                    let group_id = mls_group.group_id().to_vec();
                    let _ = client.local_events().send(LocalEvents::SyncWorkerEvent(SyncWorkerEvent::NewSyncGroupFromWelcome(group_id)));

                    group
                        .membership_state(GroupMembershipState::Allowed)
                        .build()?
                },
            };

            // Insert or replace the group in the database.
            // Replacement can happen in the case that the user has been removed from and subsequently re-added to the group.
            let stored_group = provider.conn_ref().insert_or_replace_group(to_store)?;

            StoredConsentRecord::persist_consent(provider.conn_ref(), &stored_group)?;

            Ok(Self::new(
                client.clone(),
                stored_group.id,
                stored_group.dm_id,
                stored_group.created_at_ns,
            ))
        })
    }

    pub(crate) fn create_and_insert_sync_group(
        client: Arc<ScopedClient>,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<MlsGroup<ScopedClient>, GroupError> {
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
        let stored_group = StoredGroup::create_sync_group(
            provider.conn_ref(),
            group_id,
            now_ns(),
            GroupMembershipState::Allowed,
        )?;

        let group = Self::new_from_arc(client, stored_group.id, None, stored_group.created_at_ns);

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
        if !self.is_active(provider)? {
            tracing::warn!("Unable to send a message on an inactive group.");
            return Err(GroupError::GroupInactive);
        }

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
    ///   timestamp attached to intent & stored message.
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

        let _ = self
            .sync_until_intent_resolved(&provider, intent.id)
            .await?;
        Ok(())
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

        let _ = self
            .sync_until_intent_resolved(&provider, intent.id)
            .await?;
        Ok(())
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

        let _ = self
            .sync_until_intent_resolved(&provider, intent.id)
            .await?;
        Ok(())
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

        let _ = self
            .sync_until_intent_resolved(&provider, intent.id)
            .await?;
        Ok(())
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

        let _ = self
            .sync_until_intent_resolved(&provider, intent.id)
            .await?;
        Ok(())
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

        let _ = self
            .sync_until_intent_resolved(&provider, intent.id)
            .await?;
        Ok(())
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
        let _ = self.sync_until_intent_resolved(provider, intent.id).await?;
        Ok(())
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
        let _ = self.sync_until_intent_resolved(provider, intent.id).await?;
        Ok(())
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

        let _ = self
            .sync_until_intent_resolved(&provider, intent.id)
            .await?;
        Ok(())
    }

    /// Find the `inbox_id` of the group member who added the member to the group
    pub fn added_by_inbox_id(&self) -> Result<String, GroupError> {
        let conn = self.context().store().conn()?;
        let group = conn
            .find_group(&self.group_id)?
            .ok_or_else(|| NotFound::GroupById(self.group_id.clone()))?;
        Ok(group.added_by_inbox_id)
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
            .map(UserPreferenceUpdate::Consent)
            .collect();

        if !new_records.is_empty() {
            // Dispatch an update event so it can be synced across devices
            let _ = self
                .client
                .local_events()
                .send(LocalEvents::SyncWorkerEvent(
                    SyncWorkerEvent::SyncPreferences(new_records.clone()),
                ));
            // Broadcast the changes
            let _ = self
                .client
                .local_events()
                .send(LocalEvents::PreferencesChanged(new_records));
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

    pub async fn debug_info(&self) -> Result<ConversationDebugInfo, GroupError> {
        let provider = self.client.mls_provider()?;
        let epoch =
            self.load_mls_group_with_lock(&provider, |mls_group| Ok(mls_group.epoch().as_u64()))?;

        let stored_group = match provider.conn_ref().find_group(&self.group_id)? {
            Some(group) => group,
            None => {
                return Err(GroupError::NotFound(NotFound::GroupById(
                    self.group_id.clone(),
                )))
            }
        };

        Ok(ConversationDebugInfo {
            epoch,
            maybe_forked: stored_group.maybe_forked,
            fork_details: stored_group.fork_details,
        })
    }

    /// Update this installation's leaf key in the group by creating a key update commit
    pub async fn key_update(&self) -> Result<(), GroupError> {
        let provider = self.client.mls_provider()?;
        let intent = self.queue_intent(&provider, IntentKind::KeyUpdate, vec![], false)?;
        let _ = self
            .sync_until_intent_resolved(&provider, intent.id)
            .await?;
        Ok(())
    }

    /// Checks if the current user is active in the group.
    ///
    /// If the current user has been kicked out of the group, `is_active` will return `false`
    pub fn is_active(&self, provider: &XmtpOpenMlsProvider) -> Result<bool, GroupError> {
        // Restored groups that are not yet added are inactive
        let Some(stored_group) = provider.conn_ref().find_group(&self.group_id)? else {
            return Err(GroupError::NotFound(NotFound::GroupById(
                self.group_id.clone(),
            )));
        };
        if matches!(
            stored_group.membership_state,
            GroupMembershipState::Restored
        ) {
            return Ok(false);
        }

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
        opts: Option<DMMetadataOptions>,
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
                opts.unwrap_or_default(),
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
        let stored_group = StoredGroup::builder()
            .id(group_id.clone())
            .created_at_ns(now_ns())
            .membership_state(GroupMembershipState::Allowed)
            .added_by_inbox_id(context.inbox_id().to_string())
            .dm_id(Some(
                DmMembers {
                    member_one_inbox_id: client.inbox_id().to_string(),
                    member_two_inbox_id: dm_target_inbox_id,
                }
                .to_string(),
            ))
            .build()?;

        stored_group.store(provider.conn_ref())?;
        Ok(Self::new_from_arc(
            client,
            group_id,
            stored_group.dm_id.clone(),
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
