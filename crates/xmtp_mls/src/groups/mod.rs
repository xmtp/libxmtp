pub mod app_data;
mod builders;
mod lifecycle;
mod membership;
mod messages;
mod metadata;
mod state;
pub use builders::*;
pub use state::{ConversationStateSnapshot, GroupMetadataSnapshot};
pub mod change_callbacks;
pub mod commit_log;
pub mod commit_log_key;
mod error;
pub mod intents;
pub mod members;
pub mod message_list;
pub(super) mod mls_ext;
pub(super) mod mls_sync;
pub mod oneshot;
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
    group_permissions::GroupMutablePermissions,
    group_permissions::PolicySet,
    intents::{
        AdminListActionType, PermissionPolicyOption, PermissionUpdateType,
        UpdateAdminListIntentData, UpdateMetadataIntentData, UpdatePermissionIntentData,
    },
};
#[cfg(test)]
use crate::GroupCommitLock;
use crate::context::XmtpSharedContext;
use crate::groups::{
    intents::{QueueIntent, ReaddInstallationsIntentData},
    mls_ext::CommitLogStorer,
};
use crate::state_tx::state_write;
use crate::{client::ClientError, utils::id::calculate_message_id};
use crate::{
    groups::send_message_opts::SendMessageOpts,
    worker::device_sync::preference_sync::PreferenceUpdate,
};
pub use error::*;
use intents::SendMessageIntentData;
pub use intents::UpdateGroupMembershipResult;
#[cfg(test)]
use openmls::extensions::Metadata;
use openmls::{
    credentials::CredentialType,
    extensions::{
        Extension, ExtensionType, Extensions, RequiredCapabilitiesExtension, UnknownExtension,
    },
    group::MlsGroupCreateConfig,
    messages::proposals::ProposalType,
    prelude::{Capabilities, MlsGroup as OpenMlsGroup, WireFormatPolicy},
};
use prost::Message;
use std::collections::HashMap;
use std::{collections::HashSet, sync::Arc};
use tokio::sync::Mutex;
use xmtp_common::{Event, log_event, time::now_ns};
#[cfg(test)]
use xmtp_configuration::GROUP_PERMISSIONS_EXTENSION_ID;
use xmtp_configuration::{
    CIPHERSUITE, GROUP_MEMBERSHIP_EXTENSION_ID, MAX_PAST_EPOCHS, MUTABLE_METADATA_EXTENSION_ID,
    SEND_MESSAGE_UPDATE_INSTALLATIONS_INTERVAL_NS,
    WELCOME_POINTEE_ENCRYPTION_AEAD_TYPES_EXTENSION_ID, WELCOME_WRAPPER_ENCRYPTION_EXTENSION_ID,
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
    group_intent::{IntentState, StoredGroupIntent},
    group_message::ContentType,
    refresh_state::EntityKind,
};
use xmtp_db::{Store, StoreOrIgnore};
use xmtp_db::{TransactionOutcome, TransactionOutcome::Continue, XmtpOpenMlsProviderRef};
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
use xmtp_mls_common::libxmtp_version::LibXMTPVersion;
use xmtp_mls_common::{
    app_data::components::{
        inbox_id_set::{AdminListComponent, SuperAdminListComponent},
        metadata_attributes::{
            AppDataComponent, GroupDescriptionComponent, GroupImageUrlComponent, GroupNameComponent,
        },
    },
    group::{DMMetadataOptions, GroupMetadataOptions},
    group_metadata::{DmMembers, GroupMetadata, GroupMetadataError, extract_group_metadata},
    group_mutable_metadata::{
        GroupMutableMetadata, GroupMutableMetadataError, MessageDisappearingSettings, MetadataField,
    },
};
pub use xmtp_mls_validation::{group_membership, group_permissions};
use xmtp_proto::xmtp::mls::message_contents::content_types::{DeleteMessage, LeaveRequest};
use xmtp_proto::{
    types::{Cursor, GroupId},
    xmtp::mls::message_contents::{
        EncodedContent, OneshotMessage, PlaintextEnvelope,
        content_types::ReactionV2,
        plaintext_envelope::{Content, V1},
    },
};

use xmtp_mls_common::app_data::components::metadata_attributes::{
    MAX_APP_DATA_LENGTH, MAX_GROUP_DESCRIPTION_LENGTH, MAX_GROUP_IMAGE_URL_LENGTH,
    MAX_GROUP_NAME_LENGTH,
};
const DEFAULT_IDEMPOTENCY_KEY_BYTES: usize = 16;

/// An LibXMTP MlsGroup
/// _NOTE:_ The Eq implementation compares [`GroupId`], so a dm group with the same identity will be
/// different.
/// the Hash implementation hashes the [`GroupId`]
pub struct MlsGroup<Context> {
    pub group_id: GroupId,
    pub dm_id: Option<String>,
    pub conversation_type: ConversationType,
    pub created_at_ns: i64,
    pub context: Context,
    #[cfg(test)]
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
        let id = xmtp_common::fmt::truncate_hex(hex::encode(self.group_id));
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
            group_id: self.group_id,
            dm_id: self.dm_id.clone(),
            conversation_type: self.conversation_type,
            created_at_ns: self.created_at_ns,
            context: self.context.clone(),
            mutex: self.mutex.clone(),
            #[cfg(test)]
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

#[derive(Debug, Clone, Copy)]
enum AdminListKind {
    Admin,
    SuperAdmin,
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

        // Invalid compressed content has no searchable reference. Message
        // decoding still exposes the original envelope to the app.
        let decoded = xmtp_content_types::compression::decompress(content).ok();
        let reference_id = decoded.and_then(|content| {
            if content_type_id.authority_id != "xmtp.org" {
                return None;
            }
            match (type_id_str.as_str(), content_type_id.version_major) {
                (ReplyCodec::TYPE_ID, 1) => ReplyCodec::decode(content)
                    .ok()
                    .and_then(|reply| hex::decode(reply.reference).ok()),
                (ReactionCodec::TYPE_ID, ReactionCodec::MAJOR_VERSION) => {
                    ReactionV2::decode(content.content.as_slice())
                        .ok()
                        .and_then(|reaction| hex::decode(reaction.reference).ok())
                }
                (ReactionCodec::TYPE_ID, 1) => LegacyReaction::decode(&content.content)
                    .and_then(|legacy_reaction| hex::decode(legacy_reaction.reference).ok()),
                (DeleteMessageCodec::TYPE_ID, DeleteMessageCodec::MAJOR_VERSION) => {
                    DeleteMessage::decode(content.content.as_slice())
                        .ok()
                        .and_then(|delete_msg| hex::decode(delete_msg.message_id).ok())
                }
                _ => None,
            }
        });

        Ok(QueryableContentFields {
            content_type: ContentType::from_identifier(
                &content_type_id.authority_id,
                &content_type_id.type_id,
                content_type_id.version_major,
            ),
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
            #[cfg(test)]
            mls_commit_lock: group.mls_commit_lock,
            mutex: group.mutex,
            conversation_type: group.conversation_type,
        }
    }
}

/// An MLS extension type advertised by an installation's key package or
/// present in a group's context.
///
/// Mirrors openmls [`ExtensionType`]; unknown/forward-compatibility variants
/// are preserved verbatim so callers can match on what they understand and
/// ignore the rest. This is a generic capability primitive — callers filter
/// it to answer specific questions (e.g. "is this group migrated to the
/// proposal flow?" = a list contains [`MlsExtensionType::AppDataDictionary`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MlsExtensionType {
    ApplicationId,
    RatchetTree,
    RequiredCapabilities,
    ExternalPub,
    ExternalSenders,
    LastResort,
    ImmutableMetadata,
    AppDataDictionary,
    /// An extension type this build does not have a named variant for.
    Unknown(u16),
    /// A GREASE value used to exercise extensibility.
    Grease(u16),
}

impl From<ExtensionType> for MlsExtensionType {
    fn from(value: ExtensionType) -> Self {
        match value {
            ExtensionType::ApplicationId => MlsExtensionType::ApplicationId,
            ExtensionType::RatchetTree => MlsExtensionType::RatchetTree,
            ExtensionType::RequiredCapabilities => MlsExtensionType::RequiredCapabilities,
            ExtensionType::ExternalPub => MlsExtensionType::ExternalPub,
            ExtensionType::ExternalSenders => MlsExtensionType::ExternalSenders,
            ExtensionType::LastResort => MlsExtensionType::LastResort,
            ExtensionType::ImmutableMetadata => MlsExtensionType::ImmutableMetadata,
            ExtensionType::AppDataDictionary => MlsExtensionType::AppDataDictionary,
            ExtensionType::Unknown(id) => MlsExtensionType::Unknown(id),
            ExtensionType::Grease(id) => MlsExtensionType::Grease(id),
        }
    }
}

/// Capability snapshot for a single installation (device) of a member.
#[derive(Debug, Clone)]
pub struct InstallationCapabilities {
    pub installation_id: Vec<u8>,
    /// True for the local (this device's) installation.
    pub is_own: bool,
    /// The MLS extension types this installation advertises, taken from its
    /// *latest published* key package. Empty when `capabilities_known` is
    /// false.
    pub supported_extensions: Vec<MlsExtensionType>,
    /// Whether capabilities were determined. `false` means the key package
    /// could not be fetched or failed verification — distinct from an
    /// installation that advertises no extensions.
    pub capabilities_known: bool,
}

/// Per-inbox grouping of installation capabilities. Callers map `inbox_id`
/// back to a profile to attribute capabilities to a person.
#[derive(Debug, Clone)]
pub struct InboxCapabilities {
    pub inbox_id: InboxId,
    pub installations: Vec<InstallationCapabilities>,
}

/// A generic membership/capability snapshot for a group.
///
/// This intentionally reports raw facts rather than answers, so callers can
/// filter it to whatever question they care about. For the proposal
/// (app-data-dictionary) migration specifically: the group is already
/// migrated when `context_extensions` contains
/// [`MlsExtensionType::AppDataDictionary`], it is eligible to migrate when
/// every installation's `supported_extensions` contains it, and the inboxes
/// blocking migration are those with an installation that does not.
#[derive(Debug, Clone)]
pub struct GroupMembershipCapabilities {
    /// Extension types present in the group's context.
    pub context_extensions: Vec<MlsExtensionType>,
    /// Per-inbox, per-installation capability breakdown — one entry per member
    /// inbox, in no particular order.
    pub members: Vec<InboxCapabilities>,
}
