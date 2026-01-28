pub mod commit_log;
pub mod commit_log_key;
pub mod device_sync;
pub mod disappearing_messages;
mod error;
pub mod group_membership;
pub mod group_permissions;
pub mod intents;
pub mod key_package_cleaner_worker;
pub mod members;
pub mod message_list;
pub(super) mod mls_ext;
pub(super) mod mls_sync;
pub mod oneshot;
pub(crate) mod pending_self_remove_worker;
pub mod send_message_opts;
pub(super) mod subscriptions;
pub mod summary;
#[cfg(test)]
mod tests;
pub mod validated_commit;
pub mod welcome_pointer;
pub mod welcome_sync;
mod welcomes;
pub use welcomes::*;

pub use self::group_permissions::PreconfiguredPolicies;
use self::{
    group_membership::GroupMembership,
    group_permissions::PolicySet,
    group_permissions::{GroupMutablePermissions, extract_group_permissions},
    intents::{
        AdminListActionType, PermissionPolicyOption, PermissionUpdateType,
        UpdateAdminListIntentData, UpdateMetadataIntentData, UpdatePermissionIntentData,
    },
};
use crate::groups::send_message_opts::SendMessageOpts;
use crate::groups::{
    intents::{QueueIntent, ReaddInstallationsIntentData},
    mls_ext::CommitLogStorer,
    validated_commit::LibXMTPVersion,
};
use crate::messages::enrichment::EnrichMessageError;
use crate::subscriptions::SyncWorkerEvent;
use crate::{GroupCommitLock, context::XmtpSharedContext};
use crate::{client::ClientError, subscriptions::LocalEvents, utils::id::calculate_message_id};
use device_sync::preference_sync::PreferenceUpdate;
pub use error::*;
use intents::{SendMessageIntentData, UpdateGroupMembershipResult};
use openmls::{
    credentials::CredentialType,
    extensions::{
        Extension, ExtensionType, Extensions, Metadata, RequiredCapabilitiesExtension,
        UnknownExtension,
    },
    group::{GroupContext, MlsGroupCreateConfig},
    messages::proposals::ProposalType,
    prelude::{Capabilities, GroupId, MlsGroup as OpenMlsGroup, WireFormatPolicy},
};
use prost::Message;
use std::collections::HashMap;
use std::{collections::HashSet, sync::Arc};
use tokio::sync::Mutex;
use xmtp_common::{Event, log_event, time::now_ns};
use xmtp_configuration::{
    CIPHERSUITE, GROUP_MEMBERSHIP_EXTENSION_ID, GROUP_PERMISSIONS_EXTENSION_ID, MAX_GROUP_SIZE,
    MAX_PAST_EPOCHS, MUTABLE_METADATA_EXTENSION_ID, Originators, PROPOSAL_SUPPORT_EXTENSION_ID,
    SEND_MESSAGE_UPDATE_INSTALLATIONS_INTERVAL_NS,
};
use xmtp_content_types::delete_message::DeleteMessageCodec;
use xmtp_content_types::leave_request::LeaveRequestCodec;
use xmtp_content_types::{ContentCodec, encoded_content_to_bytes};
use xmtp_content_types::{
    reaction::{LegacyReaction, ReactionCodec},
    reply::ReplyCodec,
};
use xmtp_cryptography::configuration::ED25519_KEY_LENGTH;
use xmtp_db::group_message::Deletable;
use xmtp_db::message_deletion::{QueryMessageDeletion, StoredMessageDeletion};
use xmtp_db::pending_remove::QueryPendingRemove;
use xmtp_db::prelude::*;
use xmtp_db::user_preferences::HmacKey;
use xmtp_db::{Fetch, consent_record::ConsentType};
use xmtp_db::{
    NotFound, StorageError,
    group_message::{ContentType, StoredGroupMessageWithReactions},
    refresh_state::EntityKind,
};
use xmtp_db::{Store, StoreOrIgnore};
use xmtp_db::{
    XmtpMlsStorageProvider,
    remote_commit_log::{RemoteCommitLog, RemoteCommitLogOrder},
};
use xmtp_db::{
    consent_record::{ConsentState, StoredConsentRecord},
    group::{ConversationType, GroupMembershipState, StoredGroup},
    group_message::{DeliveryStatus, GroupMessageKind, MsgQueryArgs, StoredGroupMessage},
};
use xmtp_db::{group_message::LatestMessageTimeBySender, local_commit_log::LocalCommitLog};
use xmtp_id::associations::Identifier;
use xmtp_id::{AsIdRef, InboxId, InboxIdRef};
use xmtp_mls_common::{
    group::{DMMetadataOptions, GroupMetadataOptions},
    group_metadata::{DmMembers, GroupMetadata, GroupMetadataError, extract_group_metadata},
    group_mutable_metadata::{
        GroupMutableMetadata, GroupMutableMetadataError, MessageDisappearingSettings, MetadataField,
    },
};
use xmtp_proto::xmtp::mls::message_contents::content_types::{DeleteMessage, LeaveRequest};
use xmtp_proto::{
    types::Cursor,
    xmtp::mls::message_contents::{
        EncodedContent, OneshotMessage, PlaintextEnvelope,
        content_types::ReactionV2,
        plaintext_envelope::{Content, V1},
    },
};

const MAX_GROUP_DESCRIPTION_LENGTH: usize = 1000;
const MAX_GROUP_NAME_LENGTH: usize = 100;
const MAX_GROUP_IMAGE_URL_LENGTH: usize = 2048;
const MAX_APP_DATA_LENGTH: usize = 8192;

/// An LibXMTP MlsGroup
/// _NOTE:_ The Eq implementation compares [`GroupId`], so a dm group with the same identity will be
/// different.
/// the Hash implementation hashes the [`GroupId`]
pub struct MlsGroup<Context> {
    pub group_id: Vec<u8>,
    pub dm_id: Option<String>,
    pub conversation_type: ConversationType,
    pub created_at_ns: i64,
    pub context: Context,
    mls_commit_lock: Arc<GroupCommitLock>,
    mutex: Arc<Mutex<()>>,
}

impl<C> std::hash::Hash for MlsGroup<C> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.group_id.hash(state);
    }
}

impl<C> PartialEq for MlsGroup<C> {
    fn eq(&self, other: &Self) -> bool {
        self.group_id == other.group_id
    }
}

impl<C> Eq for MlsGroup<C> {}

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
    pub is_commit_log_forked: Option<bool>,
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
    pub is_commit_log_forked: Option<bool>,
    pub local_commit_log: String,
    pub remote_commit_log: String,
    pub cursor: Vec<Cursor>,
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
        let content_type_id = content.r#type.clone().unwrap_or_default();

        let type_id_str = content_type_id.type_id.clone();

        let reference_id = match (type_id_str.as_str(), content_type_id.version_major) {
            (ReplyCodec::TYPE_ID, 1) => ReplyCodec::decode(content)
                .ok()
                .and_then(|reply| hex::decode(reply.reference).ok()),
            (ReactionCodec::TYPE_ID, major) if major >= 2 => {
                ReactionV2::decode(content.content.as_slice())
                    .ok()
                    .and_then(|reaction| hex::decode(reaction.reference).ok())
            }
            (ReactionCodec::TYPE_ID, _) => LegacyReaction::decode(&content.content)
                .and_then(|legacy_reaction| hex::decode(legacy_reaction.reference).ok()),
            (DeleteMessageCodec::TYPE_ID, DeleteMessageCodec::MAJOR_VERSION) => {
                DeleteMessage::decode(content.content.as_slice())
                    .ok()
                    .and_then(|delete_msg| hex::decode(delete_msg.message_id).ok())
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

impl<Context: Clone> From<MlsGroup<&Context>> for MlsGroup<Context> {
    fn from(group: MlsGroup<&Context>) -> MlsGroup<Context> {
        MlsGroup::<Context> {
            context: group.context.clone(),
            group_id: group.group_id,
            dm_id: group.dm_id,
            created_at_ns: group.created_at_ns,
            mls_commit_lock: group.mls_commit_lock,
            mutex: group.mutex,
            conversation_type: group.conversation_type,
        }
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
                    group.conversation_type,
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
    pub(crate) async fn load_mls_group_with_lock_async<R, E>(
        &self,
        operation: impl AsyncFnOnce(OpenMlsGroup) -> Result<R, E>,
    ) -> Result<R, E>
    where
        E: From<crate::StorageError> + From<xmtp_db::sql_key_store::SqlKeyStoreError>,
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

    /// Check if all members in the group support the proposal-by-reference flow.
    ///
    /// This checks if all members have the `PROPOSAL_SUPPORT_EXTENSION_ID` in their
    /// leaf node capabilities using OpenMLS's `check_extension_support` method.
    ///
    /// Returns `true` if all members support proposals, `false` otherwise.
    pub fn all_members_support_proposals(&self, mls_group: &OpenMlsGroup) -> bool {
        let extension_type = ExtensionType::Unknown(PROPOSAL_SUPPORT_EXTENSION_ID);
        mls_group.check_extension_support(&[extension_type]).is_ok()
    }

    /// Force-check proposal support, assuming all members support proposals.
    ///
    /// This method returns `true` unconditionally, bypassing the backward
    /// compatibility check. Use this only when:
    /// - All clients in the group are known to support proposals
    /// - Testing proposal-by-reference functionality
    /// - Operating in a controlled environment
    ///
    /// # Warning
    ///
    /// Using this when some members don't support proposals will cause those
    /// members to fail to process standalone proposal messages.
    #[cfg(any(test, feature = "test-utils"))]
    #[allow(unused_variables)]
    pub fn all_members_support_proposals_unchecked(&self, mls_group: &OpenMlsGroup) -> bool {
        true
    }

    // Create a new group and save it to the DB
    pub(crate) fn create_and_insert(
        context: Context,
        conversation_type: ConversationType,
        permissions_policy_set: PolicySet,
        opts: GroupMetadataOptions,
        oneshot_message: Option<OneshotMessage>,
    ) -> Result<Self, GroupError> {
        assert!(conversation_type != ConversationType::Dm);
        let stored_group = Self::insert(
            &context,
            None,
            GroupMembershipState::Allowed,
            conversation_type,
            permissions_policy_set,
            opts,
            oneshot_message,
        )?;
        let new_group = Self::new_from_arc(
            context.clone(),
            stored_group.id,
            stored_group.dm_id,
            conversation_type,
            stored_group.created_at_ns,
        );

        // Consent state defaults to allowed when the user creates the group
        if !conversation_type.is_virtual() {
            new_group.update_consent_state(ConsentState::Allowed)?;
        }

        Ok(new_group)
    }

    pub(crate) fn insert(
        context: &Context,
        existing_group_id: Option<&[u8]>,
        membership_state: GroupMembershipState,
        conversation_type: ConversationType,
        permissions_policy_set: PolicySet,
        opts: GroupMetadataOptions,
        oneshot_message: Option<OneshotMessage>,
    ) -> Result<StoredGroup, GroupError> {
        assert!(conversation_type != ConversationType::Dm);

        let creator_inbox_id = context.inbox_id();
        let protected_metadata = build_protected_metadata_extension(
            creator_inbox_id,
            conversation_type,
            oneshot_message,
        )?;
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
            .conversation_type(conversation_type)
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
        existing_group_id: Option<&[u8]>,
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

        let mls_group = if let Some(group_id) = existing_group_id {
            OpenMlsGroup::from_backup_stub_logged(
                &provider,
                context.identity(),
                &group_config,
                GroupId::from_slice(group_id),
            )?
        } else {
            OpenMlsGroup::from_creation_logged(&provider, context.identity(), &group_config)?
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
        Ok(new_group)
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

    /// Send a message on this users XMTP [`Client`](crate::client::Client).
    #[cfg_attr(any(test, feature = "test-utils"), tracing::instrument(level = "info", skip_all, fields(who = self.context.inbox_id(), message = %String::from_utf8_lossy(&message[..message.len().min(100)]))))]
    #[cfg_attr(
        not(any(test, feature = "test-utils")),
        tracing::instrument(level = "trace", skip_all)
    )]
    pub async fn send_message(
        &self,
        message: &[u8],
        opts: send_message_opts::SendMessageOpts,
    ) -> Result<Vec<u8>, GroupError> {
        if !self.is_active()? {
            tracing::warn!("Unable to send a message on an inactive group.");
            return Err(GroupError::GroupInactive);
        }

        self.ensure_not_paused().await?;
        let update_interval_ns = Some(SEND_MESSAGE_UPDATE_INSTALLATIONS_INTERVAL_NS);
        self.maybe_update_installations(update_interval_ns).await?;

        let message_id =
            self.prepare_message(message, opts, |now| Self::into_envelope(message, now))?;

        self.sync_until_last_intent_resolved().await?;

        // implicitly set group consent state to allowed
        self.update_consent_state(ConsentState::Allowed)?;

        Ok(message_id)
    }

    /// Publish all unpublished messages. This happens by calling `sync_until_last_intent_resolved`
    /// which publishes all pending intents and reads them back from the network.
    #[cfg_attr(any(test, feature = "test-utils"), tracing::instrument(level = "info", fields(who = self.context.inbox_id()), skip(self)))]
    #[cfg_attr(
        not(any(test, feature = "test-utils")),
        tracing::instrument(level = "trace", skip(self))
    )]
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
    pub fn send_message_optimistic(
        &self,
        message: &[u8],
        opts: send_message_opts::SendMessageOpts,
    ) -> Result<Vec<u8>, GroupError> {
        let message_id =
            self.prepare_message(message, opts, |now| Self::into_envelope(message, now))?;
        Ok(message_id)
    }

    /// Prepare a message for later publishing.
    ///
    /// Stores the message locally with `Unpublished` delivery status but does NOT
    /// create an intent to publish. Use `publish_stored_message` to publish later.
    ///
    /// # Arguments
    /// * `message` - The message content bytes
    /// * `should_push` - Whether to send a push notification when publishing
    ///
    /// Returns the message ID.
    pub fn prepare_message_for_later_publish(
        &self,
        message: &[u8],
        should_push: bool,
    ) -> Result<Vec<u8>, GroupError> {
        let now = now_ns();
        let queryable_content_fields = Self::extract_queryable_content_fields(message);

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
            sequence_id: 0,
            originator_id: 0,
            expire_at_ns: None,
            inserted_at_ns: 0,
            should_push,
        };
        group_message.store(&self.context.db())?;

        Ok(message_id)
    }

    /// Publish a previously stored message by ID.
    ///
    /// Creates an intent for the message and publishes it to the network.
    /// Uses the `should_push` value that was stored with the message.
    /// This is a no-op if the message is already published.
    ///
    /// Returns an error if the message is not found.
    #[cfg_attr(
        not(any(test, feature = "test-utils")),
        tracing::instrument(level = "trace", skip(self))
    )]
    pub async fn publish_stored_message(&self, message_id: &[u8]) -> Result<(), GroupError> {
        if !self.is_active()? {
            return Err(GroupError::GroupInactive);
        }
        self.ensure_not_paused().await?;

        // Fetch the message
        let message = self
            .context
            .db()
            .get_group_message(message_id)?
            .ok_or_else(|| GroupError::NotFound(NotFound::MessageById(message_id.to_vec())))?;

        // Silent no-op if already published
        if message.delivery_status == DeliveryStatus::Published {
            return Ok(());
        }

        // Create envelope from stored message
        let plain_envelope =
            Self::into_envelope(&message.decrypted_message_bytes, message.sent_at_ns);
        let mut encoded_envelope = vec![];
        plain_envelope.encode(&mut encoded_envelope)?;

        // Queue the intent (use should_push from stored message)
        let intent_data: Vec<u8> = SendMessageIntentData::new(encoded_envelope).into();
        QueueIntent::send_message()
            .data(intent_data)
            .should_push(message.should_push)
            .queue(self)?;

        // Publish
        self.maybe_update_installations(Some(SEND_MESSAGE_UPDATE_INSTALLATIONS_INTERVAL_NS))
            .await?;
        self.sync_until_last_intent_resolved().await?;

        // Implicitly set group consent state to allowed
        self.update_consent_state(ConsentState::Allowed)?;

        Ok(())
    }

    /// Delete a message by its ID. Returns the ID of the deletion message.
    ///
    /// Only the original sender or a super admin can delete a message.
    ///
    /// # Wire Protocol
    /// The `DeleteMessage` protobuf encodes `message_id` as a hex-encoded string for wire
    /// transmission, while the database stores message IDs as raw bytes. This function handles
    /// the conversion: it accepts raw bytes, hex-encodes them for the wire protocol, and when
    /// processing incoming deletions (in `process_delete_message`), the hex string is decoded
    /// back to bytes for database lookups.
    ///
    /// # Arguments
    /// * `message_id` - The message ID as bytes
    ///
    /// # Returns
    /// The ID of the deletion message
    pub fn delete_message(&self, message_id: Vec<u8>) -> Result<Vec<u8>, GroupError> {
        use error::DeleteMessageError;

        let conn = self.context.db();

        // Load the original message
        let original_msg = conn
            .get_group_message(&message_id)?
            .ok_or_else(|| DeleteMessageError::MessageNotFound(hex::encode(&message_id)))?;

        // Validate message belongs to this group (prevent cross-group deletion)
        if original_msg.group_id != self.group_id {
            return Err(DeleteMessageError::NotAuthorized.into());
        }

        // Check if message is already deleted
        if conn.is_message_deleted(&message_id)? {
            return Err(DeleteMessageError::MessageAlreadyDeleted.into());
        }

        let sender_inbox_id = self.context.inbox_id();
        let is_sender = original_msg.sender_inbox_id == sender_inbox_id;
        let is_super_admin = self.is_super_admin(sender_inbox_id.to_string())?;

        if !is_sender && !is_super_admin {
            return Err(DeleteMessageError::NotAuthorized.into());
        }

        if !original_msg.kind.is_deletable() || !original_msg.content_type.is_deletable() {
            return Err(DeleteMessageError::NonDeletableMessage.into());
        }

        let delete_msg = DeleteMessage {
            message_id: hex::encode(&message_id),
        };

        let encoded_delete = DeleteMessageCodec::encode(delete_msg)?;
        let mut buf = Vec::new();
        encoded_delete.encode(&mut buf)?;

        let deletion_message_id = self.send_message_optimistic(&buf, SendMessageOpts::default())?;

        let is_super_admin_deletion = !is_sender && is_super_admin;

        let deletion = StoredMessageDeletion {
            id: deletion_message_id.clone(),
            group_id: self.group_id.clone(),
            deleted_message_id: message_id,
            deleted_by_inbox_id: sender_inbox_id.to_string(),
            is_super_admin_deletion,
            deleted_at_ns: now_ns(),
        };

        deletion.store(&conn)?;

        Ok(deletion_message_id)
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
    /// * opts: Options for sending the message
    /// * envelope: closure that returns context-specific [`PlaintextEnvelope`]. Closure accepts
    ///   timestamp attached to intent & stored message.
    #[tracing::instrument(skip_all, level = "trace")]
    pub(crate) fn prepare_message<F>(
        &self,
        message: &[u8],
        opts: send_message_opts::SendMessageOpts,
        envelope: F,
    ) -> Result<Vec<u8>, GroupError>
    where
        F: FnOnce(i64) -> PlaintextEnvelope,
    {
        // Store the message locally first (with should_push preference)
        let message_id = self.prepare_message_for_later_publish(message, opts.should_push)?;

        // Fetch the stored message to get the sent_at_ns timestamp
        let stored_message = self
            .context
            .db()
            .get_group_message(&message_id)?
            .ok_or_else(|| GroupError::NotFound(NotFound::MessageById(message_id.clone())))?;

        // Create envelope using the stored timestamp for consistency
        let plain_envelope = envelope(stored_message.sent_at_ns);
        let mut encoded_envelope = vec![];
        plain_envelope.encode(&mut encoded_envelope)?;

        // Queue the intent (use should_push from stored message)
        let intent_data: Vec<u8> = SendMessageIntentData::new(encoded_envelope).into();
        QueueIntent::send_message()
            .data(intent_data)
            .should_push(stored_message.should_push)
            .queue(self)?;

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

    /// Count the number of stored messages matching the given criteria
    pub fn count_messages(&self, args: &MsgQueryArgs) -> Result<i64, GroupError> {
        let conn = self.context.db();
        let count = conn.count_group_messages(&self.group_id, args)?;
        Ok(count)
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

    /// Query for enriched messages (with reactions, replies, and deletion status)
    pub fn find_enriched_messages(
        &self,
        args: &MsgQueryArgs,
    ) -> Result<Vec<crate::messages::decoded_message::DecodedMessage>, EnrichMessageError> {
        let conn = self.context.db();
        let messages = conn.get_group_messages(&self.group_id, args)?;
        let enriched =
            crate::messages::enrichment::enrich_messages(conn, &self.group_id, messages)?;
        Ok(enriched)
    }

    pub fn get_last_read_times(&self) -> Result<LatestMessageTimeBySender, GroupError> {
        let conn = self.context.db();
        let latest_read_receipt =
            conn.get_latest_message_times_by_sender(&self.group_id, &[ContentType::ReadReceipt])?;
        Ok(latest_read_receipt)
    }

    /// Load the group reference stored in the local database
    pub fn load(&self) -> Result<StoredGroup, StorageError> {
        let conn = self.context.db();
        if let Some(group) = conn.find_group(&self.group_id)? {
            Ok(group)
        } else {
            tracing::error!("group {} does not exist", hex::encode(&self.group_id));
            Err(NotFound::GroupById(self.group_id.to_vec()).into())
        }
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
    pub async fn add_members_by_identity(
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

        self.add_members(&inbox_id_map.into_values().collect::<Vec<_>>())
            .await
    }

    #[cfg_attr(any(test, feature = "test-utils"), tracing::instrument(level = "info", skip_all, fields(who = %self.context.inbox_id(), inbox_ids = ?inbox_ids.as_ref().iter().map(|i| i.as_ref()).collect::<Vec<_>>())))]
    #[cfg_attr(
        not(any(test, feature = "test-utils")),
        tracing::instrument(level = "trace", skip_all)
    )]
    pub async fn add_members<S: AsIdRef>(
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
        let epoch = self.epoch().await?;

        log_event!(
            Event::AddedMembers,
            self.context.installation_id(),
            group_id = self.group_id,
            members = ?ids,
            epoch
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
    pub async fn remove_members_by_identity(
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
        self.remove_members(ids.as_slice()).await
    }

    /// Removes members from the group by their inbox IDs.
    ///
    /// # Arguments
    /// * `client` - The XMTP client.
    /// * `inbox_ids` - A vector of inbox IDs to remove from the group.
    ///
    /// # Returns
    /// A `Result` indicating success or failure of the operation.
    #[cfg_attr(any(test, feature = "test-utils"), tracing::instrument(level = "info", skip_all, fields(who = %self.context.inbox_id(), inbox_ids = ?inbox_ids)))]
    #[cfg_attr(
        not(any(test, feature = "test-utils")),
        tracing::instrument(level = "trace", skip_all)
    )]
    pub async fn remove_members(&self, inbox_ids: &[InboxIdRef<'_>]) -> Result<(), GroupError> {
        self.ensure_not_paused().await?;
        let intent_data = self.get_membership_update_intent(&[], inbox_ids).await?;
        let intent = QueueIntent::update_group_membership()
            .data(intent_data)
            .queue(self)?;

        let _ = self.sync_until_intent_resolved(intent.id).await?;

        Ok(())
    }

    /// Removes and readds installations from the MLS tree.
    ///
    /// The installation list should be validated beforehand - invalid installations
    /// will simply be omitted at the time that the intent's publish data is computed.
    ///
    /// # Arguments
    /// * `installations` - A vector of installations to readd.
    ///
    /// # Returns
    /// A `Result` indicating success or failure of the operation.
    #[allow(dead_code)]
    pub(crate) async fn readd_installations(
        &self,
        installations: Vec<Vec<u8>>,
    ) -> Result<(), GroupError> {
        self.ensure_not_paused().await?;

        let readd_min_version =
            LibXMTPVersion::parse(xmtp_configuration::MIN_RECOVERY_REQUEST_VERSION)?;
        let metadata = self.mutable_metadata()?;
        let group_version = metadata
            .attributes
            .get(MetadataField::MinimumSupportedProtocolVersion.as_str());
        let group_min_version =
            LibXMTPVersion::parse(group_version.unwrap_or(&"0.0.0".to_string()))?;

        if readd_min_version > group_min_version {
            self.update_group_min_version(xmtp_configuration::MIN_RECOVERY_REQUEST_VERSION)
                .await?;
        }

        let intent_data: Vec<u8> = ReaddInstallationsIntentData::new(installations.clone()).into();
        let intent = QueueIntent::readd_installations()
            .data(intent_data)
            .queue(self)?;

        let _ = self.sync_until_intent_resolved(intent.id).await?;

        Ok(())
    }

    /// Removes all members from the group who are currently in the pending removal list.
    ///
    /// Only admins and super admins can call this function. Validates permissions, filters
    /// out invalid removal requests and performs batch removal of valid pending members.
    ///
    /// # Returns
    /// * `Ok(())` - All valid pending members were successfully removed
    /// * `Err(GroupError)` - Failed to retrieve metadata, validate permissions or execute removals
    pub async fn remove_members_pending_removal(&self) -> Result<(), GroupError> {
        let pending_removal_list = self.pending_remove_list()?;

        if pending_removal_list.is_empty() {
            tracing::debug!(
                group_id = hex::encode(&self.group_id),
                inbox_id = %self.context.inbox_id(),
                "Group has no pending removal members"
            );
            return Ok(());
        }

        let is_super_admin = self.is_super_admin(self.context.inbox_id().to_string())?;
        if !is_super_admin {
            tracing::debug!(
                group_id = hex::encode(&self.group_id),
                inbox_id = %self.context.inbox_id(),
                "Current inbox ID is not in admin or super admin list, skipping pending removal processing"
            );
            return Ok(());
        }

        // Get current group members to validate which ones actually exist
        let members = self.members().await?;
        let member_inbox_ids: HashSet<String> =
            members.iter().map(|m| m.inbox_id.clone()).collect();

        // Filter pending removals to only include actual group members
        let valid_removals: Vec<&str> = pending_removal_list
            .iter()
            .filter(|inbox_id| member_inbox_ids.contains(*inbox_id))
            .map(|s| s.as_str())
            .collect();

        if valid_removals.is_empty() {
            tracing::warn!(
                group_id = hex::encode(&self.group_id),
                pending_count = pending_removal_list.len(),
                "No valid members found in pending removal list"
            );
            return Ok(());
        }
        // Log members that are in pending list but not in group
        let invalid_removals: Vec<&String> = pending_removal_list
            .iter()
            .filter(|inbox_id| !member_inbox_ids.contains(*inbox_id))
            .collect();

        if !invalid_removals.is_empty() {
            tracing::warn!(
                group_id = hex::encode(&self.group_id),
                invalid_members = ?invalid_removals,
                "Some members in pending removal list are not in the group"
            );
        }

        // Remove all valid members at once
        tracing::info!(
            group_id = hex::encode(&self.group_id),
            removing_count = valid_removals.len(),
            members_to_remove = ?valid_removals,
            "Removing pending members from group"
        );

        match self.remove_members(&valid_removals).await {
            Ok(_) => {
                tracing::info!(
                    group_id = hex::encode(&self.group_id),
                    removed_count = valid_removals.len(),
                    removed_members = ?valid_removals,
                    "Successfully removed all pending members from group"
                );
            }
            Err(e) => {
                tracing::error!(
                    group_id = hex::encode(&self.group_id),
                    members = ?valid_removals,
                    error = %e,
                    "Failed to remove pending members from group"
                );
                return Err(e);
            }
        }

        Ok(())
    }

    /// Removes members from the pending removal list who are no longer in the group.
    ///
    /// Iterates through all members in the pending removal list, checking each one to see
    /// if they're still in the group. If a member is no longer in the group, they are
    /// removed from the pending list. The pending list is refreshed after each removal
    /// to ensure we're working with the most current data.
    ///
    /// # Returns
    /// * `Ok(())` - Successfully processed all pending removal members
    /// * `Err(GroupError)` - Failed to retrieve data or update the pending list
    pub async fn cleanup_pending_removal_list(&self) -> Result<(), GroupError> {
        tracing::debug!(
            group_id = hex::encode(&self.group_id),
            "Starting pending removal list cleanup"
        );

        // Get both lists upfront
        let pending_removal_list = self.pending_remove_list()?;

        if pending_removal_list.is_empty() {
            tracing::debug!(
                group_id = hex::encode(&self.group_id),
                "No pending removals to clean up"
            );
            // Clear the pending leave request status
            self.context
                .db()
                .set_group_has_pending_leave_request_status(&self.group_id, Some(false))?;
            return Ok(());
        }

        // Get current group members
        let current_members = self.members().await?;
        let current_member_ids: Vec<String> = current_members
            .iter()
            .map(|member| member.inbox_id.clone())
            .collect();

        // Calculate removed members: users in pending list but not in current group
        let removed_members: Vec<String> = pending_removal_list
            .iter()
            .filter(|pending_user| !current_member_ids.contains(pending_user))
            .cloned()
            .collect();

        if !removed_members.is_empty() {
            tracing::info!(
                group_id = hex::encode(&self.group_id),
                removed_count = removed_members.len(),
                removed_members = ?removed_members,
                "Removing members from pending removal list - they are no longer in the group"
            );

            // Remove all users who are no longer in the group from pending list
            self.context
                .db()
                .delete_pending_remove_users(&self.group_id, removed_members)?;
        }

        // After cleanup, check if there are any pending removals left
        let remaining_pending_list = self.pending_remove_list()?;
        if remaining_pending_list.is_empty() {
            // Clear the pending leave request status if no pending removals remain
            self.context
                .db()
                .set_group_has_pending_leave_request_status(&self.group_id, Some(false))?;
        }

        tracing::info!(
            group_id = hex::encode(&self.group_id),
            remaining_pending = remaining_pending_list.len(),
            "Finished cleaning up pending removal list"
        );

        Ok(())
    }

    pub async fn leave_group(&self) -> Result<(), GroupError> {
        self.ensure_not_paused().await?;

        // Check if user is a member
        let is_member = self.is_member().await?;
        if !is_member {
            return Err(GroupLeaveValidationError::NotAGroupMember.into());
        }

        //check member size
        let members = self.members().await?;

        // check if the group has other members
        if members.len() == 1 {
            return Err(GroupLeaveValidationError::SingleMemberLeaveRejected.into());
        }

        // check if the conversation is not a DM
        if self.metadata().await?.conversation_type == ConversationType::Dm {
            return Err(GroupLeaveValidationError::DmLeaveForbidden.into());
        }

        let is_super_admin = self.is_super_admin(self.context.inbox_id().to_string())?;

        // super-admin cannot leave a group; must be demoted first
        // since SuperAdmins can't remove other SuperAdmins they need to be demoted first
        if is_super_admin {
            return Err(GroupLeaveValidationError::SuperAdminLeaveForbidden.into());
        }

        if !self.is_in_pending_remove(self.context.inbox_id())? {
            let content = LeaveRequestCodec::encode(LeaveRequest {
                authenticated_note: None,
            })?;
            self.send_message(
                &encoded_content_to_bytes(content),
                SendMessageOpts::default(),
            )
            .await?;
        };
        Ok(())
    }

    /// Checks if the current user is a member of the group.
    /// Returns true if the user is a member, false otherwise.
    #[tracing::instrument(level = "debug", skip(self))]
    async fn is_member(&self) -> Result<bool, GroupError> {
        let members = self.members().await?;
        Ok(members
            .iter()
            .any(|m| m.inbox_id == self.context.inbox_id()))
    }

    /// Updates the name of the group. Will error if the user does not have the appropriate permissions
    /// to perform these updates.
    #[cfg_attr(any(test, feature = "test-utils"), tracing::instrument(level = "info", fields(who = %self.context.inbox_id()), skip(self)))]
    #[cfg_attr(
        not(any(test, feature = "test-utils")),
        tracing::instrument(level = "trace", skip(self))
    )]
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

    #[cfg_attr(any(test, feature = "test-utils"), tracing::instrument(level = "info", fields(who = %self.context.inbox_id()), skip(self)))]
    #[cfg_attr(
        not(any(test, feature = "test-utils")),
        tracing::instrument(level = "trace", skip(self))
    )]
    pub async fn update_app_data(&self, app_data: String) -> Result<(), GroupError> {
        self.ensure_not_paused().await?;

        if app_data.len() > MAX_APP_DATA_LENGTH {
            return Err(GroupError::TooManyCharacters {
                length: MAX_APP_DATA_LENGTH,
            });
        }
        if self.metadata().await?.conversation_type == ConversationType::Dm {
            return Err(MetadataPermissionsError::DmGroupMetadataForbidden.into());
        }
        let intent_data: Vec<u8> = UpdateMetadataIntentData::new_update_app_data(app_data).into();
        let intent = QueueIntent::metadata_update()
            .data(intent_data)
            .queue(self)?;

        let _ = self.sync_until_intent_resolved(intent.id).await?;
        Ok(())
    }

    /// Updates min version of the group to match this client's version.
    /// Not publicly exposed because:
    /// - Setting the min version to pre-release versions may not behave as expected
    /// - When the version is not explicitly specified, unexpected behavior may arise,
    ///   for example if the code is left in across multiple version bumps.
    #[cfg_attr(any(test, feature = "test-utils"), tracing::instrument(level = "info", fields(who = %self.context.inbox_id()), skip(self)))]
    #[cfg_attr(
        not(any(test, feature = "test-utils")),
        tracing::instrument(level = "trace", skip(self))
    )]
    #[allow(dead_code)]
    pub(crate) async fn update_group_min_version_to_match_self(&self) -> Result<(), GroupError> {
        let version = self.context.version_info().pkg_version();
        self.update_group_min_version(version).await
    }

    /// Updates min version of the group to match the given version.
    ///
    /// # Arguments
    /// * `version` - The libxmtp version to update the group min version to.
    ///   This is a semver-formatted string matching the Cargo.toml in the
    ///   libxmtp dependency, and does not match mobile or web release versions.
    ///   Do NOT include pre-release metadata like "1.0.0-alpha",
    ///   "1.0.0-beta", etc, as the version comparison may not match what
    ///   is expected. For historical reasons, "1.0.0-alpha" is considered to be
    ///   > "1.0.0", so it is better to just specify "1.0.0".
    ///
    /// # Returns
    /// A `Result` indicating success or failure of the operation.
    pub async fn update_group_min_version(&self, version: &str) -> Result<(), GroupError> {
        self.ensure_not_paused().await?;
        tracing::info!("updating group min version to match self: {}", version);
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

    /// Updates the commit log signer of the group. Will error if the user does not have the appropriate permissions
    /// to perform these updates.
    pub async fn update_commit_log_signer(
        &self,
        commit_log_signer: xmtp_cryptography::Secret,
    ) -> Result<(), GroupError> {
        self.ensure_not_paused().await?;

        if self.metadata().await?.conversation_type == ConversationType::Dm {
            return Err(MetadataPermissionsError::DmGroupMetadataForbidden.into());
        }
        let intent_data: Vec<u8> =
            UpdateMetadataIntentData::new_update_commit_log_signer(commit_log_signer).into();
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
    #[cfg_attr(any(test, feature = "test-utils"), tracing::instrument(level = "info", fields(who = %self.context.inbox_id()), skip(self)))]
    #[cfg_attr(
        not(any(test, feature = "test-utils")),
        tracing::instrument(level = "trace", skip(self))
    )]
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

    /// Retrieves the app_data field from the group's mutable metadata extension
    pub fn app_data(&self) -> Result<String, GroupError> {
        let mutable_metadata = self.mutable_metadata()?;
        match mutable_metadata
            .attributes
            .get(&MetadataField::AppData.to_string())
        {
            Some(app_data) => Ok(app_data.clone()),
            None => Err(MetadataPermissionsError::from(
                GroupMutableMetadataError::MissingExtension,
            )
            .into()),
        }
    }

    /// Updates the description of the group.
    #[cfg_attr(any(test, feature = "test-utils"), tracing::instrument(level = "info", fields(who = %self.context.inbox_id()), skip(self)))]
    #[cfg_attr(
        not(any(test, feature = "test-utils")),
        tracing::instrument(level = "trace", skip(self))
    )]
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
    #[cfg_attr(any(test, feature = "test-utils"), tracing::instrument(level = "info", fields(who = %self.context.inbox_id()), skip(self)))]
    #[cfg_attr(
        not(any(test, feature = "test-utils")),
        tracing::instrument(level = "trace", skip(self))
    )]
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

    #[cfg_attr(any(test, feature = "test-utils"), tracing::instrument(level = "info", fields(who = %self.context.inbox_id()), skip(self)))]
    #[cfg_attr(
        not(any(test, feature = "test-utils")),
        tracing::instrument(level = "trace", skip(self))
    )]
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

    #[cfg_attr(any(test, feature = "test-utils"), tracing::instrument(level = "info", fields(who = %self.context.inbox_id()), skip(self)))]
    #[cfg_attr(
        not(any(test, feature = "test-utils")),
        tracing::instrument(level = "trace", skip(self))
    )]
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

    pub fn pending_remove_list(&self) -> Result<Vec<String>, GroupError> {
        self.context
            .db()
            .get_pending_remove_users(&self.group_id)
            .map_err(Into::into)
    }

    /// Checks if the given inbox ID is the pending-remove list of the group at the most recently synced epoch.
    pub fn is_in_pending_remove(&self, inbox_id: &str) -> Result<bool, GroupError> {
        self.context
            .db()
            .get_user_pending_remove_status(&self.group_id, inbox_id)
            .map_err(Into::into)
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

    /// Checks if the given inbox ID is a super admin of the group at the most recently synced epoch
    pub fn is_super_admin_without_lock(
        &self,
        mls_group: &OpenMlsGroup,
        inbox_id: String,
    ) -> Result<bool, GroupMutableMetadataError> {
        let mutable_metadata = GroupMutableMetadata::try_from(mls_group)?;
        Ok(mutable_metadata.super_admin_list.contains(&inbox_id))
    }

    /// Retrieves the conversation type of the group from the group's metadata extension.
    pub async fn conversation_type(&self) -> Result<ConversationType, GroupError> {
        let conversation_type = self.context.db().get_conversation_type(&self.group_id)?;
        Ok(conversation_type)
    }

    /// Updates the admin list of the group and syncs the changes to the network.
    #[cfg_attr(any(test, feature = "test-utils"), tracing::instrument(level = "info", fields(who = %self.context.inbox_id()), skip(self)))]
    #[cfg_attr(
        not(any(test, feature = "test-utils")),
        tracing::instrument(level = "trace", skip(self))
    )]
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

        Ok(db.insert_or_replace_consent_records(std::slice::from_ref(&consent_record))?)
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
        self.load_mls_group_with_lock_async(async |mls_group| Ok(mls_group.epoch().as_u64()))
            .await
    }

    /// Get the encryption state of the current epoch. Should match for all installations
    /// in the same epoch.
    #[cfg(test)]
    #[allow(unused)]
    pub(crate) async fn epoch_authenticator(&self) -> Result<Vec<u8>, GroupError> {
        self.load_mls_group_with_lock_async(async |mls_group| {
            Ok(mls_group.epoch_authenticator().as_slice().to_vec())
        })
        .await
    }

    pub async fn cursor(&self) -> Result<[Cursor; 2], GroupError> {
        let db = self.context.db();
        let msgs = db.get_last_cursor_for_originator(
            &self.group_id,
            EntityKind::ApplicationMessage,
            Originators::APPLICATION_MESSAGES,
        )?;
        let commits = db.get_last_cursor_for_originator(
            &self.group_id,
            EntityKind::CommitMessage,
            Originators::MLS_COMMITS,
        )?;
        Ok([msgs, commits])
    }

    pub async fn local_commit_log(&self) -> Result<Vec<LocalCommitLog>, GroupError> {
        Ok(self.context.db().get_group_logs(&self.group_id)?)
    }

    pub async fn remote_commit_log(&self) -> Result<Vec<RemoteCommitLog>, GroupError> {
        Ok(self.context.db().get_remote_commit_log_after_cursor(
            &self.group_id,
            0,
            RemoteCommitLogOrder::AscendingByRowid,
        )?)
    }

    pub async fn debug_info(&self) -> Result<ConversationDebugInfo, GroupError> {
        let epoch = self.epoch().await?;
        let cursor = self.cursor().await?;
        let commit_log = self.local_commit_log().await?;
        let remote_commit_log = self.remote_commit_log().await?;
        let db = self.context.db();

        let stored_group = match db.find_group(&self.group_id)? {
            Some(group) => group,
            None => {
                return Err(GroupError::NotFound(NotFound::GroupById(
                    self.group_id.clone(),
                )));
            }
        };

        Ok(ConversationDebugInfo {
            epoch,
            maybe_forked: stored_group.maybe_forked,
            fork_details: stored_group.fork_details,
            is_commit_log_forked: stored_group.is_commit_log_forked,
            local_commit_log: format!("{:?}", commit_log),
            remote_commit_log: format!("{:?}", remote_commit_log),
            cursor: cursor.to_vec(),
        })
    }

    /// Update this installation's leaf key in the group by creating a key update commit
    #[cfg_attr(any(test, feature = "test-utils"), tracing::instrument(level = "info", fields(who = %self.context.inbox_id()), skip(self)))]
    #[cfg_attr(
        not(any(test, feature = "test-utils")),
        tracing::instrument(level = "trace", skip(self))
    )]
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

    /// Returns the membership state of the current user in this group.
    #[tracing::instrument(skip_all, level = "trace")]
    pub fn membership_state(&self) -> Result<GroupMembershipState, GroupError> {
        let stored_group = self
            .context
            .db()
            .find_group(&self.group_id)?
            .ok_or_else(|| GroupError::NotFound(NotFound::GroupById(self.group_id.clone())))?;
        Ok(stored_group.membership_state)
    }

    /// Get the `GroupMetadata` of the group.
    pub async fn metadata(&self) -> Result<GroupMetadata, GroupError> {
        self.load_mls_group_with_lock_async(async |mls_group| {
            extract_group_metadata(mls_group.extensions())
                .map_err(MetadataPermissionsError::from)
                .map_err(Into::into)
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

pub(crate) fn build_protected_metadata_extension(
    creator_inbox_id: &str,
    conversation_type: ConversationType,
    oneshot_message: Option<OneshotMessage>,
) -> Result<Extension, MetadataPermissionsError> {
    assert!(conversation_type != ConversationType::Dm);
    let metadata = GroupMetadata::new(
        conversation_type,
        creator_inbox_id.to_string(),
        None,
        oneshot_message,
    );
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
        None,
    );
    let protected_metadata = Metadata::new(
        metadata
            .try_into()
            .map_err(MetadataPermissionsError::from)?,
    );

    Ok(Extension::ImmutableMetadata(protected_metadata))
}

pub(crate) fn build_mutable_permissions_extension(
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
    let mut commit_log_signer = None;
    if xmtp_configuration::ENABLE_COMMIT_LOG {
        // Optional TODO(rich): Plumb in provider and use traits in commit_log_key.rs to generate and store secret
        commit_log_signer = Some(xmtp_cryptography::rand::rand_secret::<ED25519_KEY_LENGTH>());
    }
    let mutable_metadata: Vec<u8> =
        GroupMutableMetadata::new_default(creator_inbox_id.to_string(), commit_log_signer, opts)
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
    let mut commit_log_signer = None;
    if xmtp_configuration::ENABLE_COMMIT_LOG {
        commit_log_signer = Some(xmtp_cryptography::rand::rand_secret::<ED25519_KEY_LENGTH>());
    }
    let mutable_metadata: Vec<u8> = GroupMutableMetadata::new_dm_default(
        creator_inbox_id.to_string(),
        dm_target_inbox_id,
        commit_log_signer,
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
) -> Result<Extensions<GroupContext>, MetadataPermissionsError> {
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
    extensions.add_or_replace(extension)?;
    Ok(extensions)
}

#[tracing::instrument(level = "trace", skip_all)]
pub fn build_extensions_for_permissions_update(
    group: &OpenMlsGroup,
    update_permissions_intent: UpdatePermissionIntentData,
) -> Result<Extensions<GroupContext>, MetadataPermissionsError> {
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
    extensions.add_or_replace(extension)?;
    Ok(extensions)
}

#[tracing::instrument(level = "trace", skip_all)]
pub fn build_extensions_for_admin_lists_update(
    group: &OpenMlsGroup,
    admin_lists_update: UpdateAdminListIntentData,
) -> Result<Extensions<GroupContext>, MetadataPermissionsError> {
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
    extensions.add_or_replace(extension)?;
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

pub(crate) fn build_group_config(
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
        .with_group_context_extensions(extensions)
        .capabilities(capabilities)
        .ciphersuite(CIPHERSUITE)
        .wire_format_policy(WireFormatPolicy::default())
        .max_past_epochs(MAX_PAST_EPOCHS)
        .use_ratchet_tree_extension(true)
        .build())
}

pub fn filter_inbox_ids_needing_updates<'a>(
    conn: &impl DbQuery,
    filters: &[(&'a str, i64)],
) -> Result<Vec<&'a str>, xmtp_db::ConnectionError> {
    let existing_sequence_ids =
        conn.get_latest_sequence_id(&filters.iter().map(|f| f.0).collect::<Vec<&str>>())?;

    let needs_update = filters
        .iter()
        .filter_map(|&(inbox_id, seq)| {
            let existing_sequence_id = existing_sequence_ids.get(inbox_id);
            if existing_sequence_id.is_some_and(|&s| s >= seq) {
                return None;
            }

            Some(inbox_id)
        })
        .collect();
    Ok(needs_update)
}

fn validate_dm_group(
    context: impl XmtpSharedContext,
    mls_group: &OpenMlsGroup,
    added_by_inbox: &str,
) -> Result<(), MetadataPermissionsError> {
    // Validate dm specific immutable metadata
    let metadata = extract_group_metadata(mls_group.extensions())?;

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
