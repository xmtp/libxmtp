pub mod commit_log;
pub mod device_sync;
pub mod device_sync_legacy;
mod error;
pub mod group_membership;
pub mod group_permissions;
pub mod intents;
pub mod members;
pub mod welcome_sync;

pub mod disappearing_messages;
pub mod key_package_cleaner_worker;
pub(super) mod mls_ext;
pub(super) mod mls_sync;
pub(super) mod subscriptions;
pub mod summary;
#[cfg(test)]
mod tests;
pub mod validated_commit;

pub use self::group_permissions::PreconfiguredPolicies;
use self::{
    group_membership::GroupMembership,
    group_permissions::PolicySet,
    group_permissions::{extract_group_permissions, GroupMutablePermissions},
    intents::{
        AdminListActionType, PermissionPolicyOption, PermissionUpdateType,
        UpdateAdminListIntentData, UpdateMetadataIntentData, UpdatePermissionIntentData,
    },
    validated_commit::extract_group_membership,
};
use crate::groups::{intents::QueueIntent, mls_ext::CommitLogStorer};
use crate::{
    client::ClientError,
    configuration::{
        CIPHERSUITE, MAX_GROUP_SIZE, MAX_PAST_EPOCHS, SEND_MESSAGE_UPDATE_INSTALLATIONS_INTERVAL_NS,
    },
    identity_updates::load_identity_updates,
    intents::ProcessIntentError,
    subscriptions::LocalEvents,
    utils::id::calculate_message_id,
};
use crate::{context::XmtpSharedContext, GroupCommitLock};
use crate::{subscriptions::SyncWorkerEvent, track};
use device_sync::preference_sync::PreferenceUpdate;
pub use error::*;
use intents::{SendMessageIntentData, UpdateGroupMembershipResult};
use mls_ext::DecryptedWelcome;
use mls_sync::GroupMessageProcessingError;
use openmls::{
    credentials::CredentialType,
    extensions::{
        Extension, ExtensionType, Extensions, Metadata, RequiredCapabilitiesExtension,
        UnknownExtension,
    },
    group::MlsGroupCreateConfig,
    messages::proposals::ProposalType,
    prelude::{Capabilities, GroupId, MlsGroup as OpenMlsGroup, StagedWelcome, WireFormatPolicy},
};
use openmls_traits::storage::CURRENT_VERSION;
use prost::Message;
use std::collections::HashMap;
use std::future::Future;
use std::{collections::HashSet, sync::Arc};
use tokio::sync::Mutex;
use validated_commit::LibXMTPVersion;
use xmtp_common::time::now_ns;
use xmtp_content_types::should_push;
use xmtp_content_types::ContentCodec;
use xmtp_content_types::{
    group_updated::GroupUpdatedCodec,
    reaction::{LegacyReaction, ReactionCodec},
};
use xmtp_db::local_commit_log::LocalCommitLog;
use xmtp_db::user_preferences::HmacKey;
use xmtp_db::xmtp_openmls_provider::{XmtpOpenMlsProvider, XmtpOpenMlsProviderRef};
use xmtp_db::XmtpMlsStorageProvider;
use xmtp_db::{consent_record::ConsentType, Fetch};
use xmtp_db::{
    consent_record::{ConsentState, StoredConsentRecord},
    group::{ConversationType, GroupMembershipState, StoredGroup},
    group_message::{DeliveryStatus, GroupMessageKind, MsgQueryArgs, StoredGroupMessage},
};
use xmtp_db::{
    group_message::{ContentType, StoredGroupMessageWithReactions},
    refresh_state::EntityKind,
    NotFound, StorageError,
};
use xmtp_db::{prelude::*, ConnectionExt};
use xmtp_db::{Store, StoreOrIgnore};
use xmtp_id::associations::Identifier;
use xmtp_id::{AsIdRef, InboxId, InboxIdRef};
use xmtp_mls_common::{
    config::{
        GROUP_MEMBERSHIP_EXTENSION_ID, GROUP_PERMISSIONS_EXTENSION_ID,
        MUTABLE_METADATA_EXTENSION_ID,
    },
    group::{DMMetadataOptions, GroupMetadataOptions},
    group_metadata::{extract_group_metadata, DmMembers, GroupMetadata, GroupMetadataError},
    group_mutable_metadata::{
        extract_group_mutable_metadata, GroupMutableMetadata, GroupMutableMetadataError,
        MessageDisappearingSettings, MetadataField,
    },
};
use xmtp_proto::xmtp::mls::{
    api::v1::welcome_message,
    message_contents::{
        content_types::ReactionV2,
        group_updated::Inbox,
        plaintext_envelope::{Content, V1},
        ContentTypeId, EncodedContent, GroupUpdated, PlaintextEnvelope,
    },
};

const MAX_GROUP_DESCRIPTION_LENGTH: usize = 1000;
const MAX_GROUP_NAME_LENGTH: usize = 100;
const MAX_GROUP_IMAGE_URL_LENGTH: usize = 2048;

pub struct MlsGroup<Context> {
    pub group_id: Vec<u8>,
    pub dm_id: Option<String>,
    pub conversation_type: ConversationType,
    pub created_at_ns: i64,
    pub context: Context,
    mls_commit_lock: Arc<GroupCommitLock>,
    mutex: Arc<Mutex<()>>,
}

impl<Context> std::fmt::Debug for MlsGroup<Context>
where
    Context: XmtpSharedContext,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        let id = xmtp_common::fmt::truncate_hex(hex::encode(&self.group_id));
        let inbox_id = self.context.inbox_id();
        let installation = self.context.installation_id().to_string();
        let time = chrono::DateTime::from_timestamp_nanos(self.created_at_ns);
        write!(
            f,
            "Group {{ id: [{}], created: [{}], client: [{}], installation: [{}] }}",
            id,
            time.format("%H:%M:%S"),
            inbox_id,
            installation
        )
    }
}

pub struct ConversationListItem<Context> {
    pub group: MlsGroup<Context>,
    pub last_message: Option<StoredGroupMessage>,
}

impl<Context: XmtpSharedContext> Clone for MlsGroup<Context> {
    fn clone(&self) -> Self {
        Self {
            group_id: self.group_id.clone(),
            dm_id: self.dm_id.clone(),
            conversation_type: self.conversation_type,
            created_at_ns: self.created_at_ns,
            context: self.context.clone(),
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
    pub local_commit_log: String,
    pub cursor: i64,
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
impl<Context> MlsGroup<Context>
where
    Context: XmtpSharedContext,
{
    // Creates a new group instance. Does not validate that the group exists in the DB
    pub fn new(
        context: Context,
        group_id: Vec<u8>,
        dm_id: Option<String>,
        conversation_type: ConversationType,
        created_at_ns: i64,
    ) -> Self {
        Self::new_from_arc(
            context.clone(),
            group_id,
            dm_id,
            conversation_type,
            created_at_ns,
        )
    }

    /// Creates a new group instance from the database. Validate that the group exists in the DB before constructing
    /// the group.
    ///
    /// # Returns
    ///
    /// Returns the Group and the stored group information as a tuple.
    pub fn new_cached(
        context: Context,
        group_id: &[u8],
    ) -> Result<(Self, StoredGroup), StorageError> {
        let conn = context.db();
        if let Some(group) = conn.find_group(group_id)? {
            Ok((
                Self::new_from_arc(
                    context,
                    group_id.to_vec(),
                    group.dm_id.clone(),
                    ConversationType::Group,
                    group.created_at_ns,
                ),
                group,
            ))
        } else {
            tracing::error!("group {} does not exist", hex::encode(group_id));
            Err(NotFound::GroupById(group_id.to_vec()).into())
        }
    }

    pub(crate) fn new_from_arc(
        context: Context,
        group_id: Vec<u8>,
        dm_id: Option<String>,
        conversation_type: ConversationType,
        created_at_ns: i64,
    ) -> Self {
        let mut mutexes = context.mutexes().clone();
        Self {
            group_id: group_id.clone(),
            dm_id,
            conversation_type,
            created_at_ns,
            mutex: mutexes.get_mutex(group_id),
            context: context.clone(),
            mls_commit_lock: Arc::clone(context.mls_commit_lock()),
        }
    }

    // Load the stored OpenMLS group from the OpenMLS provider's keystore
    #[tracing::instrument(level = "trace", skip_all)]
    pub(crate) fn load_mls_group_with_lock<F, R>(
        &self,
        storage: &impl XmtpMlsStorageProvider,
        operation: F,
    ) -> Result<R, GroupError>
    where
        F: Fn(OpenMlsGroup) -> Result<R, GroupError>,
    {
        // Get the group ID for locking
        let group_id = self.group_id.clone();

        // Acquire the lock synchronously using blocking_lock
        let _lock = self.mls_commit_lock.get_lock_sync(group_id.clone());
        // Load the MLS group
        let mls_group = OpenMlsGroup::load(storage, &GroupId::from_slice(&self.group_id))
            .map_err(|_| NotFound::MlsGroup)?
            .ok_or(NotFound::MlsGroup)?;

        // Perform the operation with the MLS group
        operation(mls_group)
    }

    // Load the stored OpenMLS group from the OpenMLS provider's keystore
    #[tracing::instrument(level = "trace", skip(operation))]
    pub(crate) async fn load_mls_group_with_lock_async<F, E, R, Fut>(
        &self,
        operation: F,
    ) -> Result<R, E>
    where
        F: FnOnce(OpenMlsGroup) -> Fut,
        Fut: Future<Output = Result<R, E>>,
        E:
            From<GroupMessageProcessingError>
                + From<crate::StorageError>
                + From<
                    <Context::MlsStorage as openmls_traits::storage::StorageProvider<
                        CURRENT_VERSION,
                    >>::Error,
                >,
    {
        let mls_storage = self.context.mls_storage();
        // Get the group ID for locking
        let group_id = self.group_id.clone();

        // Acquire the lock asynchronously
        let _lock = self.mls_commit_lock.get_lock_async(group_id.clone()).await;

        // Load the MLS group
        let mls_group = OpenMlsGroup::load(mls_storage, &GroupId::from_slice(&self.group_id))?
            .ok_or(StorageError::from(NotFound::GroupById(
                self.group_id.to_vec(),
            )))?;

        // Perform the operation with the MLS group
        operation(mls_group).await
    }

    // Create a new group and save it to the DB
    pub(crate) fn create_and_insert(
        context: Context,
        membership_state: GroupMembershipState,
        conversation_type: ConversationType,
        permissions_policy_set: PolicySet,
        opts: GroupMetadataOptions,
    ) -> Result<Self, GroupError> {
        let stored_group = Self::insert(
            &context,
            None,
            membership_state,
            permissions_policy_set,
            opts,
        )?;
        let new_group = Self::new_from_arc(
            context.clone(),
            stored_group.id,
            stored_group.dm_id,
            conversation_type,
            stored_group.created_at_ns,
        );

        // Consent state defaults to allowed when the user creates the group
        new_group.update_consent_state(ConsentState::Allowed)?;

        Ok(new_group)
    }

    pub(crate) fn insert(
        context: &Context,
        existing_group_id: Option<&[u8]>,
        membership_state: GroupMembershipState,
        permissions_policy_set: PolicySet,
        opts: GroupMetadataOptions,
    ) -> Result<StoredGroup, GroupError> {
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

        let provider = context.mls_provider();
        let mls_group = if let Some(existing_group_id) = existing_group_id {
            // TODO: For groups restored from backup, in order to support queries on metadata such as
            // the group title and description, a stubbed OpenMLS group is created, and later overwritten
            // when a welcome is received.
            // To avoid potentially operating on this encryption state elsewhere, it may instead be better
            // to store this metadata on the StoredGroup instead, and modify group metadata queries to also
            // check the StoredGroup.
            OpenMlsGroup::from_backup_stub_logged(
                &provider,
                context.identity(),
                &group_config,
                GroupId::from_slice(existing_group_id),
            )?
        } else {
            OpenMlsGroup::from_creation_logged(&provider, context.identity(), &group_config)?
        };

        let group_id = mls_group.group_id().to_vec();
        // If not an existing group, the creator is a super admin and should publish the commit log
        // Otherwise, for existing groups, we'll never publish the commit log until we receive a welcome message
        let should_publish_commit_log = existing_group_id.is_none();
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
            .should_publish_commit_log(should_publish_commit_log)
            .build()?;

        stored_group.store_or_ignore(&context.db())?;

        Ok(stored_group)
    }

    // Create a new DM and save it to the DB
    pub(crate) fn create_dm_and_insert(
        context: &Context,
        membership_state: GroupMembershipState,
        dm_target_inbox_id: InboxId,
        opts: DMMetadataOptions,
    ) -> Result<Self, GroupError> {
        let provider = context.mls_provider();
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

        let mls_group =
            OpenMlsGroup::from_creation_logged(&provider, context.identity(), &group_config)?;

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
                    member_two_inbox_id: context.identity().inbox_id().to_string(),
                }
                .to_string(),
            ))
            .build()?;

        stored_group.store(&context.db())?;
        let new_group = Self::new_from_arc(
            context.clone(),
            group_id.clone(),
            stored_group.dm_id,
            ConversationType::Dm,
            stored_group.created_at_ns,
        );
        // Consent state defaults to allowed when the user creates the group
        new_group.update_consent_state(ConsentState::Allowed)?;

        track!("Group Create", { "conversation_type": ConversationType::Dm }, group: &new_group.group_id);

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
    #[tracing::instrument(skip_all, level = "trace")]
    pub(super) async fn create_from_welcome(
        context: Context,
        welcome: &welcome_message::V1,
        cursor_increment: bool,
    ) -> Result<Self, GroupError> {
        let conn = &context.db();
        // Check if this welcome was already processed. Return the existing group if so.
        if conn.get_last_cursor_for_id(context.installation_id(), EntityKind::Welcome)?
            >= welcome.id as i64
        {
            let group = conn
                .find_group_by_welcome_id(welcome.id as i64)?
                // The welcome previously errored out, e.g. HPKE error, so it's not in the DB
                .ok_or(GroupError::NotFound(NotFound::GroupByWelcome(
                    welcome.id as i64,
                )))?;
            let group = Self::new(
                context,
                group.id,
                group.dm_id,
                group.conversation_type,
                group.created_at_ns,
            );

            tracing::warn!("Skipping old welcome {}", welcome.id);
            return Ok(group);
        };

        let mut decrypt_result: Result<DecryptedWelcome, GroupError> =
            Err(GroupError::UninitializedResult);
        let transaction_result = context.mls_storage().transaction(|conn| {
            let mls_storage = conn.key_store();
            decrypt_result = DecryptedWelcome::from_encrypted_bytes(
                &XmtpOpenMlsProvider::new(mls_storage),
                &welcome.hpke_public_key,
                &welcome.data,
                welcome.wrapper_algorithm.into(),
            );
            Err(StorageError::IntentionalRollback)
        });

        // TODO: Move cursor forward on non-retriable errors, but not on retriable errors
        let Err(StorageError::IntentionalRollback) = transaction_result else {
            return Err(transaction_result?);
        };

        let DecryptedWelcome { staged_welcome, .. } = decrypt_result?;
        // Ensure that the list of members in the group's MLS tree matches the list of inboxes specified
        // in the `GroupMembership` extension.
        validate_initial_group_membership(&context, &staged_welcome).await?;
        let group_id = staged_welcome.public_group().group_id();
        if conn.find_group(group_id.as_slice())?.is_some() {
            // Fetch the original MLS group, rather than the one from the welcome
            let result = MlsGroup::new_cached(context.clone(), group_id.as_slice());
            if result.is_err() {
                tracing::error!(
                    "Error fetching group while validating welcome: {:?}",
                    result.err()
                );
            } else {
                let (group, _) = result.expect("No error");
                // Check the group epoch as well, because we may not have synced the latest is_active state
                // TODO(rich): Design a better way to detect if incoming welcomes are valid
                if group.is_active()?
                    && staged_welcome
                        .public_group()
                        .group_context()
                        .epoch()
                        .as_u64()
                        <= group.epoch().await?
                {
                    tracing::error!(
                        "Skipping welcome {} because we are already in group {}",
                        welcome.id,
                        hex::encode(group_id.as_slice())
                    );
                    return Err(ProcessIntentError::WelcomeAlreadyProcessed(welcome.id).into());
                }
            }
        }

        context.mls_storage().transaction(|conn| {
            let storage = conn.key_store();
            let db = storage.db();
            let provider = XmtpOpenMlsProviderRef::new(&storage);
            let decrypted_welcome = DecryptedWelcome::from_encrypted_bytes(
                &provider,
                &welcome.hpke_public_key,
                &welcome.data,
                welcome.wrapper_algorithm.into(),
            )?;
            let DecryptedWelcome {
                staged_welcome,
                added_by_inbox_id,
                added_by_installation_id,
            } = decrypted_welcome;

            tracing::debug!(
                "calling update cursor for welcome {}",
                welcome.id
            );
            let requires_processing = {
                let current_cursor = db.get_last_cursor_for_id(context.installation_id(), EntityKind::Welcome)?;
                welcome.id > current_cursor as u64
            };
            if !requires_processing {
                tracing::error!("Skipping already processed welcome {}", welcome.id);
                return Err(ProcessIntentError::WelcomeAlreadyProcessed(welcome.id).into());
            }
            if cursor_increment {
                // TODO: We update the cursor if this welcome decrypts successfully, but if previous welcomes
                // failed due to retriable errors, this will permanently skip them.
                db.update_cursor(
                    context.installation_id(),
                    EntityKind::Welcome,
                    welcome.id as i64,
                )?;
            }


            let mls_group = OpenMlsGroup::from_welcome_logged(&provider, staged_welcome, &added_by_inbox_id, &added_by_installation_id)?;
            let group_id = mls_group.group_id().to_vec();
            let metadata = extract_group_metadata(&mls_group).map_err(MetadataPermissionsError::from)?;
            let dm_members = metadata.dm_members;
            let conversation_type = metadata.conversation_type;
            let mutable_metadata = extract_group_mutable_metadata(&mls_group).ok();
            let disappearing_settings = mutable_metadata.as_ref().and_then(|metadata| {
                Self::conversation_message_disappearing_settings_from_extensions(metadata).ok()
            });

            let paused_for_version: Option<String> = mutable_metadata.as_ref().and_then(|metadata| {
                let min_version = Self::min_protocol_version_from_extensions(metadata);
                if let Some(min_version) = min_version {
                    let current_version_str = context.version_info().pkg_version();
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
                .message_disappear_in_ns(disappearing_settings.as_ref().map(|m| m.in_ns))
                .should_publish_commit_log(Self::check_should_publish_commit_log(context.inbox_id().to_string(), mutable_metadata));



            let to_store = match conversation_type {
                ConversationType::Group => {
                    group
                        .membership_state(GroupMembershipState::Pending)
                        .paused_for_version(paused_for_version)
                        .build()?
                },
                ConversationType::Dm => {
                    validate_dm_group(&context, &mls_group, &added_by_inbox_id)?;
                    group
                        .membership_state(GroupMembershipState::Pending)
                        .last_message_ns(welcome.created_ns as i64)
                        .build()?
                }
                ConversationType::Sync => {
                    // Let the DeviceSync worker know about the presence of a new
                    // sync group that came in from a welcome.3
                    let group_id = mls_group.group_id().to_vec();
                    let _ = context.worker_events().send(SyncWorkerEvent::NewSyncGroupFromWelcome(group_id));

                    group
                        .membership_state(GroupMembershipState::Allowed)
                        .build()?
                },
            };

            tracing::warn!("storing group with welcome id {}", welcome.id);
            // Insert or replace the group in the database.
            // Replacement can happen in the case that the user has been removed from and subsequently re-added to the group.
            let stored_group = db.insert_or_replace_group(to_store)?;

            StoredConsentRecord::stitch_dm_consent(&db, &stored_group)?;
            track!(
                "Group Welcome",
                {
                    "conversation_type": stored_group.conversation_type,
                    "added_by_inbox_id": &stored_group.added_by_inbox_id
                },
                group: &stored_group.id
            );

            // Create a GroupUpdated payload
                let current_inbox_id = context.inbox_id().to_string();
                let added_payload = GroupUpdated {
                    initiated_by_inbox_id: added_by_inbox_id.clone(),
                    added_inboxes: vec![Inbox {
                        inbox_id: current_inbox_id.clone(),
                    }],
                    removed_inboxes: vec![],
                    metadata_field_changes: vec![],
                };

                let encoded_added_payload = GroupUpdatedCodec::encode(added_payload)?;
                let mut encoded_added_payload_bytes = Vec::new();
                encoded_added_payload
                    .encode(&mut encoded_added_payload_bytes)
                    .map_err(GroupError::EncodeError)?;

                let added_message_id = crate::utils::id::calculate_message_id(
                    &stored_group.id,
                    encoded_added_payload_bytes.as_slice(),
                    &format!("{}_welcome_added", welcome.created_ns),
                );

                let added_content_type = match encoded_added_payload.r#type {
                    Some(ct) => ct,
                    None => {
                        tracing::warn!(
                            "Missing content type in encoded added payload, using default values"
                        );
                        ContentTypeId {
                            authority_id: "unknown".to_string(),
                            type_id: "unknown".to_string(),
                            version_major: 0,
                            version_minor: 0,
                        }
                    }
                };

                let added_msg = StoredGroupMessage {
                    id: added_message_id,
                    group_id: stored_group.id.clone(),
                    decrypted_message_bytes: encoded_added_payload_bytes,
                    sent_at_ns: welcome.created_ns as i64,
                    kind: GroupMessageKind::MembershipChange,
                    sender_installation_id: welcome.installation_key.clone(),
                    sender_inbox_id: added_by_inbox_id,
                    delivery_status: DeliveryStatus::Published,
                    content_type: added_content_type.type_id.into(),
                    version_major: added_content_type.version_major as i32,
                    version_minor: added_content_type.version_minor as i32,
                    authority_id: added_content_type.authority_id,
                    reference_id: None,
                    sequence_id: Some(welcome.id as i64),
                    originator_id: None,
                    expire_at_ns: None,
                };

                added_msg.store_or_ignore(&db)?;

                tracing::info!(
                    "[{}]: Created GroupUpdated message for welcome",
                    current_inbox_id
                );

            let group = Self::new(
                context.clone(),
                stored_group.id,
                stored_group.dm_id,
                stored_group.conversation_type,
                stored_group.created_at_ns,
            );

            // If this group is created by us - auto-consent to it.
            if context.inbox_id() == metadata.creator_inbox_id {
                group.quietly_update_consent_state(ConsentState::Allowed, &db)?;
            }

            Ok(group)
        })
    }

    // Super admin status is only criteria for whether to publish the commit log for now
    fn check_should_publish_commit_log(
        inbox_id: String,
        mutable_metadata: Option<GroupMutableMetadata>,
    ) -> bool {
        mutable_metadata
            .as_ref()
            .map(|metadata| metadata.is_super_admin(&inbox_id))
            .unwrap_or(false) // Default to false if no mutable metadata
    }

    /// Create a sync group and insert it into the database.
    pub(crate) fn create_and_insert_sync_group(
        context: Context,
    ) -> Result<MlsGroup<Context>, GroupError> {
        let provider = context.mls_provider();

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
        let mls_group =
            OpenMlsGroup::from_creation_logged(&provider, context.identity(), &group_config)?;

        let group_id = mls_group.group_id().to_vec();
        let stored_group = StoredGroup::create_sync_group(
            &context.db(),
            group_id,
            now_ns(),
            GroupMembershipState::Allowed,
        )?;

        let group = Self::new_from_arc(
            context,
            stored_group.id,
            None,
            ConversationType::Sync,
            stored_group.created_at_ns,
        );

        Ok(group)
    }

    /// Send a message on this users XMTP [`Client`].
    #[tracing::instrument(skip_all, level = "trace")]
    pub async fn send_message(&self, message: &[u8]) -> Result<Vec<u8>, GroupError> {
        if !self.is_active()? {
            tracing::warn!("Unable to send a message on an inactive group.");
            return Err(GroupError::GroupInactive);
        }

        self.ensure_not_paused().await?;
        let update_interval_ns = Some(SEND_MESSAGE_UPDATE_INSTALLATIONS_INTERVAL_NS);
        self.maybe_update_installations(update_interval_ns).await?;

        let message_id = self.prepare_message(message, |now| Self::into_envelope(message, now))?;

        self.sync_until_last_intent_resolved().await?;

        // implicitly set group consent state to allowed
        self.update_consent_state(ConsentState::Allowed)?;

        Ok(message_id)
    }

    /// Publish all unpublished messages. This happens by calling `sync_until_last_intent_resolved`
    /// which publishes all pending intents and reads them back from the network.
    pub async fn publish_messages(&self) -> Result<(), GroupError> {
        self.ensure_not_paused().await?;
        let update_interval_ns = Some(SEND_MESSAGE_UPDATE_INSTALLATIONS_INTERVAL_NS);
        self.maybe_update_installations(update_interval_ns).await?;
        self.sync_until_last_intent_resolved().await?;

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
        self.maybe_update_installations(Some(0)).await?;
        Ok(())
    }

    /// Send a message, optimistically returning the ID of the message before the result of a message publish.
    pub fn send_message_optimistic(&self, message: &[u8]) -> Result<Vec<u8>, GroupError> {
        let message_id = self.prepare_message(message, |now| Self::into_envelope(message, now))?;
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
    #[tracing::instrument(skip_all, level = "trace")]
    pub(crate) fn prepare_message<F>(
        &self,
        message: &[u8],
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
        QueueIntent::send_message()
            .data(intent_data)
            .should_push(queryable_content_fields.should_push)
            .queue(self)?;

        // store this unpublished message locally before sending
        let message_id = calculate_message_id(&self.group_id, message, &now.to_string());
        let group_message = StoredGroupMessage {
            id: message_id.clone(),
            group_id: self.group_id.clone(),
            decrypted_message_bytes: message.to_vec(),
            sent_at_ns: now,
            kind: GroupMessageKind::Application,
            sender_installation_id: self.context.installation_id().into(),
            sender_inbox_id: self.context.inbox_id().to_string(),
            delivery_status: DeliveryStatus::Unpublished,
            content_type: queryable_content_fields.content_type,
            version_major: queryable_content_fields.version_major,
            version_minor: queryable_content_fields.version_minor,
            authority_id: queryable_content_fields.authority_id,
            reference_id: queryable_content_fields.reference_id,
            sequence_id: None,
            originator_id: None,
            expire_at_ns: None,
        };
        group_message.store(&self.context.db())?;

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
        let conn = self.context.db();
        let messages = conn.get_group_messages(&self.group_id, args)?;
        Ok(messages)
    }

    /// Query the database for stored messages. Optionally filtered by time, kind, delivery_status
    /// and limit
    pub fn find_messages_with_reactions(
        &self,
        args: &MsgQueryArgs,
    ) -> Result<Vec<StoredGroupMessageWithReactions>, GroupError> {
        let conn = self.context.db();
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
            .context
            .api()
            .get_inbox_ids(requests)
            .await?
            .into_iter()
            .filter_map(|(k, v)| Some((k.try_into().ok()?, v)))
            .collect();

        // get current number of users in group
        let member_count = self.members().await?.len();
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

        self.add_members_by_inbox_id(&inbox_id_map.into_values().collect::<Vec<_>>())
            .await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn add_members_by_inbox_id<S: AsIdRef>(
        &self,
        inbox_ids: impl AsRef<[S]>,
    ) -> Result<UpdateGroupMembershipResult, GroupError> {
        self.ensure_not_paused().await?;

        let ids = inbox_ids
            .as_ref()
            .iter()
            .map(AsIdRef::as_ref)
            .collect::<Vec<&str>>();
        let intent_data = self
            .get_membership_update_intent(ids.as_slice(), &[])
            .await?;

        // TODO:nm this isn't the best test for whether the request is valid
        // If some existing group member has an update, this will return an intent with changes
        // when we really should return an error
        let ok_result = Ok(UpdateGroupMembershipResult::from(intent_data.clone()));

        if intent_data.is_empty() {
            tracing::warn!("Member already added");
            return ok_result;
        }

        let intent = QueueIntent::update_group_membership()
            .data(intent_data)
            .queue(self)?;

        self.sync_until_intent_resolved(intent.id).await?;
        track!(
            "Group Membership Change",
            {
                "added": ids,
                "removed": ()
            },
            group: &self.group_id
        );

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
            .context
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
        let intent_data = self.get_membership_update_intent(&[], inbox_ids).await?;
        let intent = QueueIntent::update_group_membership()
            .data(intent_data)
            .queue(self)?;

        let _ = self.sync_until_intent_resolved(intent.id).await?;

        track!(
            "Group Membership Change",
            {
                "added": (),
                "removed": inbox_ids
            },
            group: &self.group_id
        );

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
        if self.metadata().await?.conversation_type == ConversationType::Dm {
            return Err(MetadataPermissionsError::DmGroupMetadataForbidden.into());
        }
        let intent_data: Vec<u8> =
            UpdateMetadataIntentData::new_update_group_name(group_name).into();
        let intent = QueueIntent::metadata_update()
            .data(intent_data)
            .queue(self)?;

        let _ = self.sync_until_intent_resolved(intent.id).await?;
        Ok(())
    }

    /// Updates min version of the group to match this client's version.
    pub async fn update_group_min_version_to_match_self(&self) -> Result<(), GroupError> {
        self.ensure_not_paused().await?;
        let version = self.context.version_info().pkg_version();
        let intent_data: Vec<u8> =
            UpdateMetadataIntentData::new_update_group_min_version_to_match_self(
                version.to_string(),
            )
            .into();
        let intent = QueueIntent::metadata_update()
            .data(intent_data)
            .queue(self)?;

        let _ = self.sync_until_intent_resolved(intent.id).await?;
        Ok(())
    }

    fn min_protocol_version_from_extensions(
        mutable_metadata: &GroupMutableMetadata,
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

        if self.metadata().await?.conversation_type == ConversationType::Dm {
            return Err(MetadataPermissionsError::DmGroupMetadataForbidden.into());
        }
        if permission_update_type == PermissionUpdateType::UpdateMetadata
            && metadata_field.is_none()
        {
            return Err(MetadataPermissionsError::InvalidPermissionUpdate.into());
        }

        let intent_data: Vec<u8> = UpdatePermissionIntentData::new(
            permission_update_type,
            permission_policy,
            metadata_field.as_ref().map(|field| field.to_string()),
        )
        .into();

        let intent = QueueIntent::update_permission()
            .data(intent_data)
            .queue(self)?;

        let _ = self.sync_until_intent_resolved(intent.id).await?;
        Ok(())
    }

    /// Retrieves the group name from the group's mutable metadata extension.
    pub fn group_name(&self) -> Result<String, GroupError> {
        let mutable_metadata = self.mutable_metadata()?;
        match mutable_metadata
            .attributes
            .get(&MetadataField::GroupName.to_string())
        {
            Some(group_name) => Ok(group_name.clone()),
            None => Err(MetadataPermissionsError::from(
                GroupMutableMetadataError::MissingExtension,
            )
            .into()),
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

        if self.metadata().await?.conversation_type == ConversationType::Dm {
            return Err(MetadataPermissionsError::DmGroupMetadataForbidden.into());
        }
        let intent_data: Vec<u8> =
            UpdateMetadataIntentData::new_update_group_description(group_description).into();
        let intent = QueueIntent::metadata_update()
            .data(intent_data)
            .queue(self)?;

        let _ = self.sync_until_intent_resolved(intent.id).await?;
        Ok(())
    }

    pub fn group_description(&self) -> Result<String, GroupError> {
        let mutable_metadata = self.mutable_metadata()?;
        match mutable_metadata
            .attributes
            .get(&MetadataField::Description.to_string())
        {
            Some(group_description) => Ok(group_description.clone()),
            None => Err(GroupError::MetadataPermissionsError(
                GroupMutableMetadataError::MissingExtension.into(),
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

        if self.metadata().await?.conversation_type == ConversationType::Dm {
            return Err(MetadataPermissionsError::DmGroupMetadataForbidden.into());
        }
        let intent_data: Vec<u8> =
            UpdateMetadataIntentData::new_update_group_image_url_square(group_image_url_square)
                .into();
        let intent = QueueIntent::metadata_update()
            .data(intent_data)
            .queue(self)?;

        let _ = self.sync_until_intent_resolved(intent.id).await?;
        Ok(())
    }

    /// Retrieves the image URL (square) of the group from the group's mutable metadata extension.
    pub fn group_image_url_square(&self) -> Result<String, GroupError> {
        let mutable_metadata = self.mutable_metadata()?;
        match mutable_metadata
            .attributes
            .get(&MetadataField::GroupImageUrlSquare.to_string())
        {
            Some(group_image_url_square) => Ok(group_image_url_square.clone()),
            None => Err(MetadataPermissionsError::Mutable(
                GroupMutableMetadataError::MissingExtension,
            )
            .into()),
        }
    }

    pub async fn update_conversation_message_disappearing_settings(
        &self,
        settings: MessageDisappearingSettings,
    ) -> Result<(), GroupError> {
        self.ensure_not_paused().await?;

        self.update_conversation_message_disappear_from_ns(settings.from_ns)
            .await?;
        self.update_conversation_message_disappear_in_ns(settings.in_ns)
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
        expire_from_ms: i64,
    ) -> Result<(), GroupError> {
        self.ensure_not_paused().await?;

        let intent_data: Vec<u8> =
            UpdateMetadataIntentData::new_update_conversation_message_disappear_from_ns(
                expire_from_ms,
            )
            .into();
        let intent = QueueIntent::metadata_update()
            .data(intent_data)
            .queue(self)?;
        let _ = self.sync_until_intent_resolved(intent.id).await?;
        Ok(())
    }

    async fn update_conversation_message_disappear_in_ns(
        &self,
        expire_in_ms: i64,
    ) -> Result<(), GroupError> {
        self.ensure_not_paused().await?;

        let intent_data: Vec<u8> =
            UpdateMetadataIntentData::new_update_conversation_message_disappear_in_ns(expire_in_ms)
                .into();
        let intent = QueueIntent::metadata_update()
            .data(intent_data)
            .queue(self)?;
        let _ = self.sync_until_intent_resolved(intent.id).await?;
        Ok(())
    }

    /// If group is not paused, will return None, otherwise will return the version that the group is paused for
    pub fn paused_for_version(&self) -> Result<Option<String>, GroupError> {
        let paused_for_version = self.context.db().get_group_paused_version(&self.group_id)?;
        Ok(paused_for_version)
    }

    #[tracing::instrument(skip_all, level = "trace")]
    async fn ensure_not_paused(&self) -> Result<(), GroupError> {
        if let Some(min_version) = self.context.db().get_group_paused_version(&self.group_id)? {
            Err(GroupError::GroupPausedUntilUpdate(min_version))
        } else {
            Ok(())
        }
    }

    pub fn conversation_message_disappearing_settings(
        &self,
    ) -> Result<MessageDisappearingSettings, GroupError> {
        let metadata = self.mutable_metadata()?;
        Self::conversation_message_disappearing_settings_from_extensions(&metadata)
    }

    pub fn conversation_message_disappearing_settings_from_extensions(
        mutable_metadata: &GroupMutableMetadata,
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
            Err(GroupError::MetadataPermissionsError(
                GroupMetadataError::MissingExtension.into(),
            ))
        }
    }

    /// Retrieves the admin list of the group from the group's mutable metadata extension.
    pub fn admin_list(&self) -> Result<Vec<String>, GroupError> {
        let mutable_metadata = self.mutable_metadata()?;
        Ok(mutable_metadata.admin_list)
    }

    /// Retrieves the super admin list of the group from the group's mutable metadata extension.
    pub fn super_admin_list(&self) -> Result<Vec<String>, GroupError> {
        let mutable_metadata = self.mutable_metadata()?;
        Ok(mutable_metadata.super_admin_list)
    }

    /// Checks if the given inbox ID is an admin of the group at the most recently synced epoch.
    pub fn is_admin(&self, inbox_id: String) -> Result<bool, GroupError> {
        let mutable_metadata = self.mutable_metadata()?;
        Ok(mutable_metadata.admin_list.contains(&inbox_id))
    }

    /// Checks if the given inbox ID is a super admin of the group at the most recently synced epoch.
    pub fn is_super_admin(&self, inbox_id: String) -> Result<bool, GroupError> {
        let mutable_metadata = self.mutable_metadata()?;
        Ok(mutable_metadata.super_admin_list.contains(&inbox_id))
    }

    /// Retrieves the conversation type of the group from the group's metadata extension.
    pub async fn conversation_type(&self) -> Result<ConversationType, GroupError> {
        let conversation_type = self.context.db().get_conversation_type(&self.group_id)?;
        Ok(conversation_type)
    }

    /// Updates the admin list of the group and syncs the changes to the network.
    pub async fn update_admin_list(
        &self,
        action_type: UpdateAdminListType,
        inbox_id: String,
    ) -> Result<(), GroupError> {
        if self.metadata().await?.conversation_type == ConversationType::Dm {
            return Err(MetadataPermissionsError::DmGroupMetadataForbidden.into());
        }
        let intent_action_type = match action_type {
            UpdateAdminListType::Add => AdminListActionType::Add,
            UpdateAdminListType::Remove => AdminListActionType::Remove,
            UpdateAdminListType::AddSuper => AdminListActionType::AddSuper,
            UpdateAdminListType::RemoveSuper => AdminListActionType::RemoveSuper,
        };
        let intent_data: Vec<u8> =
            UpdateAdminListIntentData::new(intent_action_type, inbox_id).into();
        let intent = QueueIntent::update_admin_list()
            .data(intent_data)
            .queue(self)?;

        let _ = self.sync_until_intent_resolved(intent.id).await?;
        Ok(())
    }

    /// Find the `inbox_id` of the group member who added the member to the group
    pub fn added_by_inbox_id(&self) -> Result<String, GroupError> {
        let conn = self.context.db();
        let group = conn
            .find_group(&self.group_id)?
            .ok_or_else(|| NotFound::GroupById(self.group_id.clone()))?;
        Ok(group.added_by_inbox_id)
    }

    /// Find the `consent_state` of the group
    pub fn consent_state(&self) -> Result<ConsentState, GroupError> {
        let conn = self.context.db();
        let record = conn.get_consent_record(
            hex::encode(self.group_id.clone()),
            ConsentType::ConversationId,
        )?;

        match record {
            Some(rec) => Ok(rec.state),
            None => Ok(ConsentState::Unknown),
        }
    }

    // Returns new consent records. Does not broadcast changes.
    pub fn quietly_update_consent_state(
        &self,
        state: ConsentState,
        db: &impl DbQuery,
    ) -> Result<Vec<StoredConsentRecord>, GroupError> {
        let consent_record = StoredConsentRecord::new(
            ConsentType::ConversationId,
            state,
            hex::encode(self.group_id.clone()),
        );

        Ok(db.insert_or_replace_consent_records(&[consent_record.clone()])?)
    }

    #[tracing::instrument(skip_all, level = "trace")]
    pub fn update_consent_state(&self, state: ConsentState) -> Result<(), GroupError> {
        let db = self.context.db();
        let new_records: Vec<PreferenceUpdate> = self
            .quietly_update_consent_state(state, &db)?
            .into_iter()
            .map(PreferenceUpdate::Consent)
            .collect();

        if !new_records.is_empty() {
            // Dispatch an update event so it can be synced across devices
            let _ = self
                .context
                .worker_events()
                .send(SyncWorkerEvent::SyncPreferences(new_records.clone()));
            // Broadcast the changes
            let _ = self
                .context
                .local_events()
                .send(LocalEvents::PreferencesChanged(new_records));
        }

        Ok(())
    }

    /// Get the current epoch number of the group.
    pub async fn epoch(&self) -> Result<u64, GroupError> {
        self.load_mls_group_with_lock_async(|mls_group| {
            futures::future::ready(Ok(mls_group.epoch().as_u64()))
        })
        .await
    }

    pub async fn cursor(&self) -> Result<i64, GroupError> {
        let db = self.context.db();
        Ok(db.get_last_cursor_for_id(&self.group_id, EntityKind::Group)?)
    }

    pub async fn local_commit_log(&self) -> Result<Vec<LocalCommitLog>, GroupError> {
        Ok(self.context.db().get_group_logs(&self.group_id)?)
    }

    pub async fn debug_info(&self) -> Result<ConversationDebugInfo, GroupError> {
        let epoch = self.epoch().await?;
        let cursor = self.cursor().await?;
        let commit_log = self.local_commit_log().await?;
        let db = self.context.db();

        let stored_group = match db.find_group(&self.group_id)? {
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
            local_commit_log: format!("{:?}", commit_log),
            cursor,
        })
    }

    /// Update this installation's leaf key in the group by creating a key update commit
    pub async fn key_update(&self) -> Result<(), GroupError> {
        let intent = QueueIntent::key_update().queue(self)?;
        let _ = self.sync_until_intent_resolved(intent.id).await?;
        Ok(())
    }

    /// Checks if the current user is active in the group.
    ///
    /// If the current user has been kicked out of the group, `is_active` will return `false`
    #[tracing::instrument(skip_all, level = "trace")]
    pub fn is_active(&self) -> Result<bool, GroupError> {
        // Restored groups that are not yet added are inactive
        let Some(stored_group) = self.context.db().find_group(&self.group_id)? else {
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

        self.load_mls_group_with_lock(self.context.mls_storage(), |mls_group| {
            Ok(mls_group.is_active())
        })
    }

    /// Get the `GroupMetadata` of the group.
    pub async fn metadata(&self) -> Result<GroupMetadata, GroupError> {
        self.load_mls_group_with_lock_async(|mls_group| {
            futures::future::ready(
                extract_group_metadata(&mls_group)
                    .map_err(MetadataPermissionsError::from)
                    .map_err(Into::into),
            )
        })
        .await
    }

    /// Get the `GroupMutableMetadata` of the group.
    pub fn mutable_metadata(&self) -> Result<GroupMutableMetadata, GroupError> {
        self.load_mls_group_with_lock(self.context.mls_storage(), |mls_group| {
            GroupMutableMetadata::try_from(&mls_group)
                .map_err(MetadataPermissionsError::from)
                .map_err(GroupError::from)
        })
    }

    pub fn permissions(&self) -> Result<GroupMutablePermissions, GroupError> {
        self.load_mls_group_with_lock(self.context.mls_storage(), |mls_group| {
            Ok(extract_group_permissions(&mls_group).map_err(MetadataPermissionsError::from)?)
        })
    }

    /// Fetches the message disappearing settings for a given group ID.
    ///
    /// Returns `Some(MessageDisappearingSettings)` if the group exists and has valid settings,
    /// `None` if the group or settings are missing, or `Err(ClientError)` on a database error.
    pub fn disappearing_settings(&self) -> Result<Option<MessageDisappearingSettings>, GroupError> {
        let conn = self.context.db();
        let stored_group: Option<StoredGroup> = conn.fetch(&self.group_id)?;

        let settings = stored_group.and_then(|group| {
            let from_ns = group.message_disappear_from_ns?;
            let in_ns = group.message_disappear_in_ns?;

            Some(MessageDisappearingSettings { from_ns, in_ns })
        });

        Ok(settings)
    }

    /// Find all the duplicate dms for this group
    pub fn find_duplicate_dms(&self) -> Result<Vec<MlsGroup<Context>>, ClientError> {
        let duplicates = self.context.db().other_dms(&self.group_id)?;

        let mls_groups = duplicates
            .into_iter()
            .map(|g| {
                MlsGroup::new(
                    self.context.clone(),
                    g.id,
                    g.dm_id,
                    g.conversation_type,
                    g.created_at_ns,
                )
            })
            .collect();

        Ok(mls_groups)
    }

    /// Used for testing that dm group validation works as expected.
    ///
    /// See the `test_validate_dm_group` test function for more details.
    #[cfg(test)]
    pub fn create_test_dm_group(
        context: Context,
        dm_target_inbox_id: InboxId,
        custom_protected_metadata: Option<Extension>,
        custom_mutable_metadata: Option<Extension>,
        custom_group_membership: Option<Extension>,
        custom_mutable_permissions: Option<PolicySet>,
        opts: Option<DMMetadataOptions>,
    ) -> Result<Self, GroupError> {
        let provider = context.mls_provider();

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

        let mls_group =
            OpenMlsGroup::from_creation_logged(&provider, context.identity(), &group_config)?;
        let group_id = mls_group.group_id().to_vec();
        let stored_group = StoredGroup::builder()
            .id(group_id.clone())
            .created_at_ns(now_ns())
            .membership_state(GroupMembershipState::Allowed)
            .added_by_inbox_id(context.inbox_id().to_string())
            .dm_id(Some(
                DmMembers {
                    member_one_inbox_id: context.inbox_id().to_string(),
                    member_two_inbox_id: dm_target_inbox_id,
                }
                .to_string(),
            ))
            .build()?;

        stored_group.store(&context.db())?;
        Ok(Self::new_from_arc(
            context,
            group_id,
            stored_group.dm_id.clone(),
            ConversationType::Dm,
            stored_group.created_at_ns,
        ))
    }
}

fn build_protected_metadata_extension(
    creator_inbox_id: &str,
    conversation_type: ConversationType,
) -> Result<Extension, MetadataPermissionsError> {
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
    let protected_metadata = Metadata::new(
        metadata
            .try_into()
            .map_err(MetadataPermissionsError::from)?,
    );

    Ok(Extension::ImmutableMetadata(protected_metadata))
}

fn build_mutable_permissions_extension(
    policies: PolicySet,
) -> Result<Extension, MetadataPermissionsError> {
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
        GroupMutableMetadata::new_default(creator_inbox_id.to_string(), opts)
            .try_into()
            .map_err(MetadataPermissionsError::from)?;
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
) -> Result<Extension, MetadataPermissionsError> {
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
) -> Result<Extensions, MetadataPermissionsError> {
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
) -> Result<Extensions, MetadataPermissionsError> {
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
                update_permissions_intent
                    .metadata_field_name
                    .ok_or(GroupMutableMetadataError::MissingMetadataField)?,
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
) -> Result<Extensions, MetadataPermissionsError> {
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
    context: impl XmtpSharedContext,
    staged_welcome: &StagedWelcome,
) -> Result<(), GroupError> {
    let db = context.db();
    tracing::info!("Validating initial group membership");
    let extensions = staged_welcome.public_group().group_context().extensions();
    let membership = extract_group_membership(extensions)?;
    let needs_update = filter_inbox_ids_needing_updates(&db, membership.to_filters().as_slice())?;
    if !needs_update.is_empty() {
        let ids = needs_update.iter().map(AsRef::as_ref).collect::<Vec<_>>();
        load_identity_updates(context.api(), &db, ids.as_slice()).await?;
    }

    let mut expected_installation_ids = HashSet::<Vec<u8>>::new();

    let identity_updates = crate::identity_updates::IdentityUpdates::new(&context);
    let futures: Vec<_> = membership
        .members
        .iter()
        .map(|(inbox_id, sequence_id)| {
            identity_updates.get_association_state(&db, inbox_id, Some(*sequence_id as i64))
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

pub fn filter_inbox_ids_needing_updates<'a>(
    conn: &impl DbQuery,
    filters: &[(&'a str, i64)],
) -> Result<Vec<&'a str>, xmtp_db::ConnectionError> {
    let existing_sequence_ids =
        conn.get_latest_sequence_id(&filters.iter().map(|f| f.0).collect::<Vec<&str>>())?;

    let needs_update = filters
        .iter()
        .filter_map(|filter| {
            let existing_sequence_id = existing_sequence_ids.get(filter.0);
            if let Some(sequence_id) = existing_sequence_id {
                if sequence_id.ge(&filter.1) {
                    return None;
                }
            }

            Some(filter.0)
        })
        .collect::<Vec<&str>>();
    Ok(needs_update)
}

fn validate_dm_group(
    context: impl XmtpSharedContext,
    mls_group: &OpenMlsGroup,
    added_by_inbox: &str,
) -> Result<(), MetadataPermissionsError> {
    // Validate dm specific immutable metadata
    let metadata = extract_group_metadata(mls_group)?;

    // 1) Check if the conversation type is DM
    if metadata.conversation_type != ConversationType::Dm {
        return Err(DmValidationError::InvalidConversationType.into());
    }

    // 2) If `dm_members` is not set, return an error immediately
    let dm_members = match &metadata.dm_members {
        Some(dm) => dm,
        None => {
            return Err(DmValidationError::MustHaveMembersSet.into());
        }
    };

    // 3) If the inbox that added this group is our inbox, make sure that
    //    one of the `dm_members` is our inbox id
    let identity = context.identity();
    if added_by_inbox == identity.inbox_id() {
        if !(dm_members.member_one_inbox_id == identity.inbox_id()
            || dm_members.member_two_inbox_id == identity.inbox_id())
        {
            return Err(DmValidationError::OurInboxMustBeMember.into());
        }
        return Ok(());
    }

    // 4) Otherwise, make sure one of the `dm_members` is ours, and the other is `added_by_inbox`
    let is_expected_pair = (dm_members.member_one_inbox_id == added_by_inbox
        && dm_members.member_two_inbox_id == identity.inbox_id())
        || (dm_members.member_one_inbox_id == identity.inbox_id()
            && dm_members.member_two_inbox_id == added_by_inbox);

    if !is_expected_pair {
        return Err(DmValidationError::ExpectedInboxesDoNotMatch.into());
    }

    // Validate mutable metadata
    let mutable_metadata: GroupMutableMetadata = mls_group.try_into()?;

    // Check if the admin list and super admin list are empty
    if !mutable_metadata.admin_list.is_empty() || !mutable_metadata.super_admin_list.is_empty() {
        return Err(DmValidationError::MustHaveEmptyAdminAndSuperAdmin.into());
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
        return Err(DmValidationError::InvalidPermissions.into());
    }

    Ok(())
}
