pub(crate) mod dispatch;
mod filter;
mod listener;
mod reader;

pub use filter::EventFilter;
pub use listener::{EventListener, ListenerError};
pub use reader::EventReader;

use crate::{ConnectionState, InboxId};
use xmtp_events as core;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ListenerId(pub u64);
uniffi::custom_newtype!(ListenerId, u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
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
    ConnectionStateChanged,
    AttachmentUploadStarted,
    AttachmentUploadCompleted,
    AttachmentUploadFailed,
    AttachmentDownloadStarted,
    AttachmentDownloadCompleted,
    AttachmentDownloadFailed,
    AttachmentDeleted,
    Lagged,
}

impl From<EventKind> for core::EventKind {
    fn from(value: EventKind) -> Self {
        match value {
            EventKind::ConversationJoined => Self::ConversationJoined,
            EventKind::ConversationRemoved => Self::ConversationRemoved,
            EventKind::ConversationMembershipChanged => Self::ConversationMembershipChanged,
            EventKind::ConversationMetadataChanged => Self::ConversationMetadataChanged,
            EventKind::ConversationPaused => Self::ConversationPaused,
            EventKind::MessageReceived => Self::MessageReceived,
            EventKind::MessageStatusChanged => Self::MessageStatusChanged,
            EventKind::MessageDeleted => Self::MessageDeleted,
            EventKind::MessageExpired => Self::MessageExpired,
            EventKind::ConsentChanged => Self::ConsentChanged,
            EventKind::HmacKeysUpdated => Self::HmacKeysUpdated,
            EventKind::IdentityRegistered => Self::IdentityRegistered,
            EventKind::IdentityOwnInstallationAdded => Self::IdentityOwnInstallationAdded,
            EventKind::IdentityOwnInstallationRevoked => Self::IdentityOwnInstallationRevoked,
            EventKind::ClientRejectedByServer => Self::ClientRejectedByServer,
            EventKind::ClientLockoutChanged => Self::ClientLockoutChanged,
            EventKind::ConversationForkDetected => Self::ConversationForkDetected,
            EventKind::NotificationsFailed => Self::NotificationsFailed,
            EventKind::ArchiveRestored => Self::ArchiveRestored,
            EventKind::ConnectionStateChanged => Self::ConnectionStateChanged,
            EventKind::AttachmentUploadStarted => Self::AttachmentUploadStarted,
            EventKind::AttachmentUploadCompleted => Self::AttachmentUploadCompleted,
            EventKind::AttachmentUploadFailed => Self::AttachmentUploadFailed,
            EventKind::AttachmentDownloadStarted => Self::AttachmentDownloadStarted,
            EventKind::AttachmentDownloadCompleted => Self::AttachmentDownloadCompleted,
            EventKind::AttachmentDownloadFailed => Self::AttachmentDownloadFailed,
            EventKind::AttachmentDeleted => Self::AttachmentDeleted,
            EventKind::Lagged => Self::Lagged,
        }
    }
}

macro_rules! mapped_enum {
    ($name:ident => $source:ty { $($variant:ident),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
        pub enum $name { $($variant),+ }
        impl From<$source> for $name {
            fn from(value: $source) -> Self {
                match value { $(<$source>::$variant => Self::$variant),+ }
            }
        }
    };
}

mapped_enum!(EventConversationType => core::ConversationType { Group, Dm });
mapped_enum!(JoinOrigin => core::JoinOrigin { Created, Welcomed });
mapped_enum!(RemovalCause => core::RemovalCause { Removed, Left });
mapped_enum!(DeletionCause => core::DeletionCause { Deleted, DeletedLocally });
mapped_enum!(EventMessageStatus => core::MessageStatus { Unpublished, Published, Failed });
mapped_enum!(ConsentEntityKind => core::ConsentEntityKind { Conversation, Inbox });
mapped_enum!(EventConsentState => core::ConsentState { Unknown, Allowed, Denied });
mapped_enum!(RejectionCause => core::RejectionCause { BackendMismatch, VersionTooOld });
mapped_enum!(LockoutChange => core::LockoutChange { Entered, Left });
impl From<core::ConnectionState> for ConnectionState {
    fn from(value: core::ConnectionState) -> Self {
        match value {
            core::ConnectionState::Connecting => Self::Connecting,
            core::ConnectionState::Connected => Self::Connected,
            core::ConnectionState::Reconnecting => Self::Reconnecting,
            core::ConnectionState::Failed => Self::Failed,
            core::ConnectionState::Closed => Self::Closed,
        }
    }
}

/// The attachment an `attachment.*` event reports.
#[derive(Clone, PartialEq, Eq, uniffi::Record)]
pub struct AttachmentRef {
    pub attachment_key: String,
    pub url: String,
    pub content_digest: String,
}

/// The attachment a failed transfer reports, with its failure cause.
#[derive(Clone, PartialEq, Eq, uniffi::Record)]
pub struct AttachmentFailed {
    pub attachment_key: String,
    pub url: String,
    pub content_digest: String,
    pub cause: String,
}

impl std::fmt::Debug for AttachmentRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AttachmentRef")
            .field("attachment_key", &self.attachment_key)
            .field("url", &"<redacted>")
            .field("content_digest", &self.content_digest)
            .finish()
    }
}

impl std::fmt::Debug for AttachmentFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AttachmentFailed")
            .field("attachment_key", &self.attachment_key)
            .field("url", &"<redacted>")
            .field("content_digest", &self.content_digest)
            .field("cause", &self.cause)
            .finish()
    }
}

impl From<core::AttachmentRef> for AttachmentRef {
    fn from(value: core::AttachmentRef) -> Self {
        Self {
            attachment_key: value.attachment_key,
            url: value.url,
            content_digest: value.content_digest,
        }
    }
}

impl From<core::AttachmentFailed> for AttachmentFailed {
    fn from(value: core::AttachmentFailed) -> Self {
        Self {
            attachment_key: value.attachment_key,
            url: value.url,
            content_digest: value.content_digest,
            cause: value.cause,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct ConversationJoined {
    pub group_id: Vec<u8>,
    pub conversation_type: EventConversationType,
    pub origin: JoinOrigin,
    pub adder_inbox_id: Option<InboxId>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct ConversationRemoved {
    pub group_id: Vec<u8>,
    pub cause: RemovalCause,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct MembershipChanged {
    pub group_id: Vec<u8>,
    pub added_inbox_ids: Vec<InboxId>,
    pub removed_inbox_ids: Vec<InboxId>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct MetadataChanged {
    pub group_id: Vec<u8>,
    pub changed: Vec<String>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct ConversationPaused {
    pub group_id: Vec<u8>,
    pub floor: String,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct EventContentTypeId {
    pub authority_id: String,
    pub type_id: String,
    pub version_major: u32,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct MessageReceived {
    pub group_id: Vec<u8>,
    pub message_id: Vec<u8>,
    pub content_type: Option<EventContentTypeId>,
    pub sender_inbox_id: InboxId,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct MessageStatusChanged {
    pub group_id: Vec<u8>,
    pub message_id: Vec<u8>,
    pub previous: EventMessageStatus,
    pub current: EventMessageStatus,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct MessageDeleted {
    pub group_id: Vec<u8>,
    pub message_id: Vec<u8>,
    pub cause: DeletionCause,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct MessageRef {
    pub group_id: Vec<u8>,
    pub message_id: Vec<u8>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct ConsentChanged {
    pub entity_kind: ConsentEntityKind,
    pub entity: String,
    pub state: EventConsentState,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct HmacKeysUpdated {}

#[derive(Clone, Debug, uniffi::Record)]
pub struct IdentityRegistered {
    pub inbox_id: InboxId,
    pub installation_key: Vec<u8>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct InstallationRef {
    pub installation_key: Vec<u8>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct InstallationRevoked {
    pub installation_key: Vec<u8>,
    pub is_this_installation: bool,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct ClientRejectedByServer {
    pub cause: RejectionCause,
    pub min_libxmtp_version: Option<String>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct LockoutChanged {
    pub change: LockoutChange,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct GroupRef {
    pub group_id: Vec<u8>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct NotificationsFailed {
    pub cause: String,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct ArchiveRestored {
    pub complete: bool,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct ConnectionStateChanged {
    pub previous: ConnectionState,
    pub current: ConnectionState,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct Lagged {
    pub discarded: u64,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum ClientEvent {
    ConversationJoined {
        conversation_joined: ConversationJoined,
    },
    ConversationRemoved {
        conversation_removed: ConversationRemoved,
    },
    ConversationMembershipChanged {
        membership_changed: MembershipChanged,
    },
    ConversationMetadataChanged {
        metadata_changed: MetadataChanged,
    },
    ConversationPaused {
        conversation_paused: ConversationPaused,
    },
    MessageReceived {
        message_received: MessageReceived,
    },
    MessageStatusChanged {
        message_status_changed: MessageStatusChanged,
    },
    MessageDeleted {
        message_deleted: MessageDeleted,
    },
    MessageExpired {
        message_expired: MessageRef,
    },
    ConsentChanged {
        consent_changed: ConsentChanged,
    },
    HmacKeysUpdated {
        hmac_keys_updated: HmacKeysUpdated,
    },
    IdentityRegistered {
        identity_registered: IdentityRegistered,
    },
    IdentityOwnInstallationAdded {
        own_installation_added: InstallationRef,
    },
    IdentityOwnInstallationRevoked {
        own_installation_revoked: InstallationRevoked,
    },
    ClientRejectedByServer {
        rejected_by_server: ClientRejectedByServer,
    },
    ClientLockoutChanged {
        lockout_changed: LockoutChanged,
    },
    ConversationForkDetected {
        conversation_fork_detected: GroupRef,
    },
    NotificationsFailed {
        notifications_failed: NotificationsFailed,
    },
    ArchiveRestored {
        archive_restored: ArchiveRestored,
    },
    ConnectionStateChanged {
        connection_state_changed: ConnectionStateChanged,
    },
    AttachmentUploadStarted {
        attachment_upload_started: AttachmentRef,
    },
    AttachmentUploadCompleted {
        attachment_upload_completed: AttachmentRef,
    },
    AttachmentUploadFailed {
        attachment_upload_failed: AttachmentFailed,
    },
    AttachmentDownloadStarted {
        attachment_download_started: AttachmentRef,
    },
    AttachmentDownloadCompleted {
        attachment_download_completed: AttachmentRef,
    },
    AttachmentDownloadFailed {
        attachment_download_failed: AttachmentFailed,
    },
    AttachmentDeleted {
        attachment_deleted: AttachmentRef,
    },
    Lagged {
        lagged: Lagged,
    },
}

fn content_type(value: xmtp_events::ContentTypeId) -> EventContentTypeId {
    EventContentTypeId {
        authority_id: value.authority_id,
        type_id: value.type_id,
        version_major: value.version_major,
    }
}

impl ClientEvent {
    pub(crate) fn from_core(value: core::ClientEvent) -> Self {
        match value {
            core::ClientEvent::ConversationJoined(v) => Self::ConversationJoined {
                conversation_joined: ConversationJoined {
                    group_id: v.group_id,
                    conversation_type: v.conversation_type.into(),
                    origin: v.origin.into(),
                    adder_inbox_id: v.adder_inbox_id.map(InboxId::unchecked),
                },
            },
            core::ClientEvent::ConversationRemoved(v) => Self::ConversationRemoved {
                conversation_removed: ConversationRemoved {
                    group_id: v.group_id,
                    cause: v.cause.into(),
                },
            },
            core::ClientEvent::ConversationMembershipChanged(v) => {
                Self::ConversationMembershipChanged {
                    membership_changed: MembershipChanged {
                        group_id: v.group_id,
                        added_inbox_ids: v
                            .added_inbox_ids
                            .into_iter()
                            .map(InboxId::unchecked)
                            .collect(),
                        removed_inbox_ids: v
                            .removed_inbox_ids
                            .into_iter()
                            .map(InboxId::unchecked)
                            .collect(),
                    },
                }
            }
            core::ClientEvent::ConversationMetadataChanged(v) => {
                Self::ConversationMetadataChanged {
                    metadata_changed: MetadataChanged {
                        group_id: v.group_id,
                        changed: v.changed,
                    },
                }
            }
            core::ClientEvent::ConversationPaused(v) => Self::ConversationPaused {
                conversation_paused: ConversationPaused {
                    group_id: v.group_id,
                    floor: v.floor,
                },
            },
            core::ClientEvent::MessageReceived(v) => Self::MessageReceived {
                message_received: MessageReceived {
                    group_id: v.group_id,
                    message_id: v.message_id,
                    content_type: v.content_type.map(content_type),
                    sender_inbox_id: InboxId::unchecked(v.sender_inbox_id),
                },
            },
            core::ClientEvent::MessageStatusChanged(v) => Self::MessageStatusChanged {
                message_status_changed: MessageStatusChanged {
                    group_id: v.group_id,
                    message_id: v.message_id,
                    previous: v.previous.into(),
                    current: v.current.into(),
                },
            },
            core::ClientEvent::MessageDeleted(v) => Self::MessageDeleted {
                message_deleted: MessageDeleted {
                    group_id: v.group_id,
                    message_id: v.message_id,
                    cause: v.cause.into(),
                },
            },
            core::ClientEvent::MessageExpired(v) => Self::MessageExpired {
                message_expired: MessageRef {
                    group_id: v.group_id,
                    message_id: v.message_id,
                },
            },
            core::ClientEvent::ConsentChanged(v) => Self::ConsentChanged {
                consent_changed: ConsentChanged {
                    entity_kind: v.entity_kind.into(),
                    entity: v.entity,
                    state: v.state.into(),
                },
            },
            core::ClientEvent::HmacKeysUpdated(_) => Self::HmacKeysUpdated {
                hmac_keys_updated: HmacKeysUpdated {},
            },
            core::ClientEvent::IdentityRegistered(v) => Self::IdentityRegistered {
                identity_registered: IdentityRegistered {
                    inbox_id: InboxId::unchecked(v.inbox_id),
                    installation_key: v.installation_key,
                },
            },
            core::ClientEvent::IdentityOwnInstallationAdded(v) => {
                Self::IdentityOwnInstallationAdded {
                    own_installation_added: InstallationRef {
                        installation_key: v.installation_key,
                    },
                }
            }
            core::ClientEvent::IdentityOwnInstallationRevoked(v) => {
                Self::IdentityOwnInstallationRevoked {
                    own_installation_revoked: InstallationRevoked {
                        installation_key: v.installation_key,
                        is_this_installation: v.is_this_installation,
                    },
                }
            }
            core::ClientEvent::ClientRejectedByServer(v) => Self::ClientRejectedByServer {
                rejected_by_server: ClientRejectedByServer {
                    cause: v.cause.into(),
                    min_libxmtp_version: v.min_libxmtp_version,
                },
            },
            core::ClientEvent::ClientLockoutChanged(v) => Self::ClientLockoutChanged {
                lockout_changed: LockoutChanged {
                    change: v.change.into(),
                },
            },
            core::ClientEvent::ConversationForkDetected(v) => Self::ConversationForkDetected {
                conversation_fork_detected: GroupRef {
                    group_id: v.group_id,
                },
            },
            core::ClientEvent::NotificationsFailed(v) => Self::NotificationsFailed {
                notifications_failed: NotificationsFailed { cause: v.cause },
            },
            core::ClientEvent::ArchiveRestored(v) => Self::ArchiveRestored {
                archive_restored: ArchiveRestored {
                    complete: v.complete,
                },
            },
            core::ClientEvent::ConnectionStateChanged(v) => Self::ConnectionStateChanged {
                connection_state_changed: ConnectionStateChanged {
                    previous: v.previous.into(),
                    current: v.current.into(),
                },
            },
            core::ClientEvent::AttachmentUploadStarted(v) => Self::AttachmentUploadStarted {
                attachment_upload_started: v.into(),
            },
            core::ClientEvent::AttachmentUploadCompleted(v) => Self::AttachmentUploadCompleted {
                attachment_upload_completed: v.into(),
            },
            core::ClientEvent::AttachmentUploadFailed(v) => Self::AttachmentUploadFailed {
                attachment_upload_failed: v.into(),
            },
            core::ClientEvent::AttachmentDownloadStarted(v) => Self::AttachmentDownloadStarted {
                attachment_download_started: v.into(),
            },
            core::ClientEvent::AttachmentDownloadCompleted(v) => {
                Self::AttachmentDownloadCompleted {
                    attachment_download_completed: v.into(),
                }
            }
            core::ClientEvent::AttachmentDownloadFailed(v) => Self::AttachmentDownloadFailed {
                attachment_download_failed: v.into(),
            },
            core::ClientEvent::AttachmentDeleted(v) => Self::AttachmentDeleted {
                attachment_deleted: v.into(),
            },
            core::ClientEvent::Lagged(v) => Self::Lagged {
                lagged: Lagged {
                    discarded: v.discarded,
                },
            },
        }
    }
}

#[cfg(test)]
mod diagnostics;

#[cfg(test)]
mod schema_tests;
