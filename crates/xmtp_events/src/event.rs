//! The public event contract. Payloads contain identifiers and small values.

/// The order is the order of the EVENT kind table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EventKind {
    ConversationJoined,
    ConversationRemoved,
    ConversationMembershipChanged,
    ConversationMetadataChanged,
    ConversationPaused,
    MessageReceived,
    MessageStatusChanged,
    MessageDeleted,
    MessageExpired,
    ConsentChanged,
    HmacKeysUpdated,
    IdentityRegistered,
    IdentityOwnInstallationAdded,
    IdentityOwnInstallationRevoked,
    ClientRejectedByServer,
    ClientLockoutChanged,
    ConversationForkDetected,
    NotificationsFailed,
    ArchiveRestored,
    Lagged,
}

impl EventKind {
    pub const ALL: [Self; 20] = [
        Self::ConversationJoined,
        Self::ConversationRemoved,
        Self::ConversationMembershipChanged,
        Self::ConversationMetadataChanged,
        Self::ConversationPaused,
        Self::MessageReceived,
        Self::MessageStatusChanged,
        Self::MessageDeleted,
        Self::MessageExpired,
        Self::ConsentChanged,
        Self::HmacKeysUpdated,
        Self::IdentityRegistered,
        Self::IdentityOwnInstallationAdded,
        Self::IdentityOwnInstallationRevoked,
        Self::ClientRejectedByServer,
        Self::ClientLockoutChanged,
        Self::ConversationForkDetected,
        Self::NotificationsFailed,
        Self::ArchiveRestored,
        Self::Lagged,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::ConversationJoined => "conversation.joined",
            Self::ConversationRemoved => "conversation.removed",
            Self::ConversationMembershipChanged => "conversation.membership_changed",
            Self::ConversationMetadataChanged => "conversation.metadata_changed",
            Self::ConversationPaused => "conversation.paused",
            Self::MessageReceived => "message.received",
            Self::MessageStatusChanged => "message.status_changed",
            Self::MessageDeleted => "message.deleted",
            Self::MessageExpired => "message.expired",
            Self::ConsentChanged => "consent.changed",
            Self::HmacKeysUpdated => "hmac_keys.updated",
            Self::IdentityRegistered => "identity.registered",
            Self::IdentityOwnInstallationAdded => "identity.own_installation_added",
            Self::IdentityOwnInstallationRevoked => "identity.own_installation_revoked",
            Self::ClientRejectedByServer => "client.rejected_by_server",
            Self::ClientLockoutChanged => "client.lockout_changed",
            Self::ConversationForkDetected => "conversation.fork_detected",
            Self::NotificationsFailed => "notifications.failed",
            Self::ArchiveRestored => "archive.restored",
            Self::Lagged => "lagged",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConversationType {
    Group,
    Dm,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JoinOrigin {
    Created,
    Welcomed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemovalCause {
    Removed,
    Left,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeletionCause {
    Deleted,
    DeletedLocally,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageStatus {
    Unpublished,
    Published,
    Failed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsentEntityKind {
    Conversation,
    Inbox,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsentState {
    Unknown,
    Allowed,
    Denied,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RejectionCause {
    BackendMismatch,
    VersionTooOld,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockoutChange {
    Entered,
    Left,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupRef {
    pub group_id: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageRef {
    pub group_id: Vec<u8>,
    pub message_id: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConversationJoined {
    pub group_id: Vec<u8>,
    pub conversation_type: ConversationType,
    pub origin: JoinOrigin,
    pub adder_inbox_id: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConversationRemoved {
    pub group_id: Vec<u8>,
    pub cause: RemovalCause,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MembershipChanged {
    pub group_id: Vec<u8>,
    pub added_inbox_ids: Vec<String>,
    pub removed_inbox_ids: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataChanged {
    pub group_id: Vec<u8>,
    pub changed: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConversationPaused {
    pub group_id: Vec<u8>,
    pub floor: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentTypeId {
    pub authority_id: String,
    pub type_id: String,
    pub version_major: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageReceived {
    pub group_id: Vec<u8>,
    pub message_id: Vec<u8>,
    pub content_type: Option<ContentTypeId>,
    pub sender_inbox_id: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageStatusChanged {
    pub group_id: Vec<u8>,
    pub message_id: Vec<u8>,
    pub previous: MessageStatus,
    pub current: MessageStatus,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageDeleted {
    pub group_id: Vec<u8>,
    pub message_id: Vec<u8>,
    pub cause: DeletionCause,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsentChanged {
    pub entity_kind: ConsentEntityKind,
    pub entity: String,
    pub state: ConsentState,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HmacKeysUpdated;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdentityRegistered {
    pub inbox_id: String,
    pub installation_key: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstallationRef {
    pub installation_key: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstallationRevoked {
    pub installation_key: Vec<u8>,
    pub is_this_installation: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientRejectedByServer {
    pub cause: RejectionCause,
    pub min_libxmtp_version: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LockoutChanged {
    pub change: LockoutChange,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotificationsFailed {
    pub cause: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchiveRestored {
    pub complete: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lagged {
    pub discarded: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClientEvent {
    ConversationJoined(ConversationJoined),
    ConversationRemoved(ConversationRemoved),
    ConversationMembershipChanged(MembershipChanged),
    ConversationMetadataChanged(MetadataChanged),
    ConversationPaused(ConversationPaused),
    MessageReceived(MessageReceived),
    MessageStatusChanged(MessageStatusChanged),
    MessageDeleted(MessageDeleted),
    MessageExpired(MessageRef),
    ConsentChanged(ConsentChanged),
    HmacKeysUpdated(HmacKeysUpdated),
    IdentityRegistered(IdentityRegistered),
    IdentityOwnInstallationAdded(InstallationRef),
    IdentityOwnInstallationRevoked(InstallationRevoked),
    ClientRejectedByServer(ClientRejectedByServer),
    ClientLockoutChanged(LockoutChanged),
    ConversationForkDetected(GroupRef),
    NotificationsFailed(NotificationsFailed),
    ArchiveRestored(ArchiveRestored),
    Lagged(Lagged),
}

impl ClientEvent {
    pub const fn kind(&self) -> EventKind {
        match self {
            Self::ConversationJoined(_) => EventKind::ConversationJoined,
            Self::ConversationRemoved(_) => EventKind::ConversationRemoved,
            Self::ConversationMembershipChanged(_) => EventKind::ConversationMembershipChanged,
            Self::ConversationMetadataChanged(_) => EventKind::ConversationMetadataChanged,
            Self::ConversationPaused(_) => EventKind::ConversationPaused,
            Self::MessageReceived(_) => EventKind::MessageReceived,
            Self::MessageStatusChanged(_) => EventKind::MessageStatusChanged,
            Self::MessageDeleted(_) => EventKind::MessageDeleted,
            Self::MessageExpired(_) => EventKind::MessageExpired,
            Self::ConsentChanged(_) => EventKind::ConsentChanged,
            Self::HmacKeysUpdated(_) => EventKind::HmacKeysUpdated,
            Self::IdentityRegistered(_) => EventKind::IdentityRegistered,
            Self::IdentityOwnInstallationAdded(_) => EventKind::IdentityOwnInstallationAdded,
            Self::IdentityOwnInstallationRevoked(_) => EventKind::IdentityOwnInstallationRevoked,
            Self::ClientRejectedByServer(_) => EventKind::ClientRejectedByServer,
            Self::ClientLockoutChanged(_) => EventKind::ClientLockoutChanged,
            Self::ConversationForkDetected(_) => EventKind::ConversationForkDetected,
            Self::NotificationsFailed(_) => EventKind::NotificationsFailed,
            Self::ArchiveRestored(_) => EventKind::ArchiveRestored,
            Self::Lagged(_) => EventKind::Lagged,
        }
    }

    pub fn group_id(&self) -> Option<&[u8]> {
        match self {
            Self::ConversationJoined(v) => Some(&v.group_id),
            Self::ConversationRemoved(v) => Some(&v.group_id),
            Self::ConversationMembershipChanged(v) => Some(&v.group_id),
            Self::ConversationMetadataChanged(v) => Some(&v.group_id),
            Self::ConversationPaused(v) => Some(&v.group_id),
            Self::MessageReceived(v) => Some(&v.group_id),
            Self::MessageStatusChanged(v) => Some(&v.group_id),
            Self::MessageDeleted(v) => Some(&v.group_id),
            Self::MessageExpired(v) => Some(&v.group_id),
            Self::ConversationForkDetected(v) => Some(&v.group_id),
            _ => None,
        }
    }
}
