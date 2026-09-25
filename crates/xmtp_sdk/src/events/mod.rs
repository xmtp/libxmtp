pub(crate) mod dispatch;
mod filter;
mod listener;
mod reader;

pub use filter::EventFilter;
pub use listener::{EventListener, ListenerError};
pub use reader::EventReader;

use crate::{ContentTypeId, ConversationID, InboxID, InstallationID, MessageID};
use xmtp_events as core;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ListenerID(pub u64);
uniffi::custom_newtype!(ListenerID, u64);

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
mapped_enum!(ConnectionState => core::ConnectionState { Connecting, Connected, Reconnecting, Failed, Closed });

#[derive(Clone, Debug, uniffi::Enum)]
pub enum ClientEvent {
    ConversationJoined {
        conversation_id: ConversationID,
        conversation_type: EventConversationType,
        origin: JoinOrigin,
        adder_inbox_id: Option<InboxID>,
    },
    ConversationRemoved {
        conversation_id: ConversationID,
        cause: RemovalCause,
    },
    ConversationMembershipChanged {
        conversation_id: ConversationID,
        added_inbox_ids: Vec<InboxID>,
        removed_inbox_ids: Vec<InboxID>,
    },
    ConversationMetadataChanged {
        conversation_id: ConversationID,
        changed: Vec<String>,
    },
    ConversationPaused {
        conversation_id: ConversationID,
        floor: String,
    },
    MessageReceived {
        conversation_id: ConversationID,
        message_id: MessageID,
        content_type: Option<ContentTypeId>,
        sender_inbox_id: InboxID,
    },
    MessageStatusChanged {
        conversation_id: ConversationID,
        message_id: MessageID,
        previous: EventMessageStatus,
        current: EventMessageStatus,
    },
    MessageDeleted {
        conversation_id: ConversationID,
        message_id: MessageID,
        cause: DeletionCause,
    },
    MessageExpired {
        conversation_id: ConversationID,
        message_id: MessageID,
    },
    ConsentChanged {
        entity_kind: ConsentEntityKind,
        entity: String,
        state: EventConsentState,
    },
    HmacKeysUpdated,
    IdentityRegistered {
        inbox_id: InboxID,
        installation_id: InstallationID,
    },
    IdentityOwnInstallationAdded {
        installation_id: InstallationID,
    },
    IdentityOwnInstallationRevoked {
        installation_id: InstallationID,
        is_this_installation: bool,
    },
    ClientRejectedByServer {
        cause: RejectionCause,
        min_libxmtp_version: Option<String>,
    },
    ClientLockoutChanged {
        change: LockoutChange,
    },
    ConversationForkDetected {
        conversation_id: ConversationID,
    },
    NotificationsFailed {
        cause: String,
    },
    ArchiveRestored {
        complete: bool,
    },
    ConnectionStateChanged {
        previous: ConnectionState,
        current: ConnectionState,
    },
    Lagged {
        discarded: u64,
    },
}

fn conversation_id(bytes: Vec<u8>) -> ConversationID {
    ConversationID(hex::encode(bytes))
}
fn message_id(bytes: Vec<u8>) -> MessageID {
    MessageID(hex::encode(bytes))
}
fn installation_id(bytes: Vec<u8>) -> InstallationID {
    InstallationID(hex::encode(bytes))
}
fn content_type(value: xmtp_events::ContentTypeId) -> ContentTypeId {
    ContentTypeId {
        authority_id: value.authority_id,
        type_id: value.type_id,
        version_major: value.version_major,
        version_minor: 0,
    }
}

impl From<core::ClientEvent> for ClientEvent {
    fn from(value: core::ClientEvent) -> Self {
        match value {
            core::ClientEvent::ConversationJoined(v) => Self::ConversationJoined {
                conversation_id: conversation_id(v.group_id),
                conversation_type: v.conversation_type.into(),
                origin: v.origin.into(),
                adder_inbox_id: v.adder_inbox_id.map(InboxID),
            },
            core::ClientEvent::ConversationRemoved(v) => Self::ConversationRemoved {
                conversation_id: conversation_id(v.group_id),
                cause: v.cause.into(),
            },
            core::ClientEvent::ConversationMembershipChanged(v) => {
                Self::ConversationMembershipChanged {
                    conversation_id: conversation_id(v.group_id),
                    added_inbox_ids: v.added_inbox_ids.into_iter().map(InboxID).collect(),
                    removed_inbox_ids: v.removed_inbox_ids.into_iter().map(InboxID).collect(),
                }
            }
            core::ClientEvent::ConversationMetadataChanged(v) => {
                Self::ConversationMetadataChanged {
                    conversation_id: conversation_id(v.group_id),
                    changed: v.changed,
                }
            }
            core::ClientEvent::ConversationPaused(v) => Self::ConversationPaused {
                conversation_id: conversation_id(v.group_id),
                floor: v.floor,
            },
            core::ClientEvent::MessageReceived(v) => Self::MessageReceived {
                conversation_id: conversation_id(v.group_id),
                message_id: message_id(v.message_id),
                content_type: v.content_type.map(content_type),
                sender_inbox_id: InboxID(v.sender_inbox_id),
            },
            core::ClientEvent::MessageStatusChanged(v) => Self::MessageStatusChanged {
                conversation_id: conversation_id(v.group_id),
                message_id: message_id(v.message_id),
                previous: v.previous.into(),
                current: v.current.into(),
            },
            core::ClientEvent::MessageDeleted(v) => Self::MessageDeleted {
                conversation_id: conversation_id(v.group_id),
                message_id: message_id(v.message_id),
                cause: v.cause.into(),
            },
            core::ClientEvent::MessageExpired(v) => Self::MessageExpired {
                conversation_id: conversation_id(v.group_id),
                message_id: message_id(v.message_id),
            },
            core::ClientEvent::ConsentChanged(v) => Self::ConsentChanged {
                entity_kind: v.entity_kind.into(),
                entity: v.entity,
                state: v.state.into(),
            },
            core::ClientEvent::HmacKeysUpdated(_) => Self::HmacKeysUpdated,
            core::ClientEvent::IdentityRegistered(v) => Self::IdentityRegistered {
                inbox_id: InboxID(v.inbox_id),
                installation_id: installation_id(v.installation_key),
            },
            core::ClientEvent::IdentityOwnInstallationAdded(v) => {
                Self::IdentityOwnInstallationAdded {
                    installation_id: installation_id(v.installation_key),
                }
            }
            core::ClientEvent::IdentityOwnInstallationRevoked(v) => {
                Self::IdentityOwnInstallationRevoked {
                    installation_id: installation_id(v.installation_key),
                    is_this_installation: v.is_this_installation,
                }
            }
            core::ClientEvent::ClientRejectedByServer(v) => Self::ClientRejectedByServer {
                cause: v.cause.into(),
                min_libxmtp_version: v.min_libxmtp_version,
            },
            core::ClientEvent::ClientLockoutChanged(v) => Self::ClientLockoutChanged {
                change: v.change.into(),
            },
            core::ClientEvent::ConversationForkDetected(v) => Self::ConversationForkDetected {
                conversation_id: conversation_id(v.group_id),
            },
            core::ClientEvent::NotificationsFailed(v) => {
                Self::NotificationsFailed { cause: v.cause }
            }
            core::ClientEvent::ArchiveRestored(v) => Self::ArchiveRestored {
                complete: v.complete,
            },
            core::ClientEvent::ConnectionStateChanged(v) => Self::ConnectionStateChanged {
                previous: v.previous.into(),
                current: v.current.into(),
            },
            core::ClientEvent::Lagged(v) => Self::Lagged {
                discarded: v.discarded,
            },
        }
    }
}
