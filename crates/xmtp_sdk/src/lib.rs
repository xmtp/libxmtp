//! The shared UniFFI surface for XMTP SDKs.

#![recursion_limit = "512"]

// Keep the WASM allocator, free function, and panic hook in the cdylib.
#[cfg(target_arch = "wasm32")]
extern crate uniffi_runtime_wasm as _;

mod archives;
mod client;
mod client_identity;
mod configuration;
mod content;
mod conversation;
mod conversations;
mod credentials;
mod crypto;
mod diagnostics;
mod error;
mod foreign;
mod identity;
mod ids;
mod logging;
mod message;
#[cfg(not(target_arch = "wasm32"))]
mod notifications;
mod preferences;
mod reader;
mod signer;
mod state;
mod static_helpers;
mod storage;

pub use archives::{ArchiveElement, ArchiveMetadata, ArchiveOptions, Archives};
pub use client::{Client, ClientOptions, StorageLocation, StorageOptions};
pub use configuration::{
    AuthConfiguration, LimitsConfiguration, MlsConfiguration, RetentionConfiguration,
    ServerConfiguration, SigningKeyDescription,
};
pub use content::{
    Action, ActionStyle, Actions, Attachment, Compression, DeletedBy, DeletedMessage,
    EncodedContent, GroupUpdated, Intent, LeaveRequest, MetadataFieldChange, MultiRemoteAttachment,
    Reaction, ReactionAction, ReactionSchema, RemoteAttachment, SendOptions, TransactionMetadata,
    TransactionReference, WalletCall, WalletCallMetadata, WalletSendCalls, encode_text,
};
pub use conversation::{Conversation, Conversations, Dm, Group};
pub use conversations::{
    ConversationKind, ConversationOrder, CreateDmOptions, CreateGroupOptions, GroupPermissionMode,
    ListConversationsOptions, ListMessagesOptions, MessageOrder, MessageSortBy,
};
pub use credentials::{
    Backend, BackendOptions, BackendSource, Credential, CredentialError, CredentialSource,
};
pub use crypto::{EncryptedEncodedContent, EncryptionKeys};
pub use diagnostics::{ApiStats, Diagnostics, IdentityStats};
pub use error::{ErrorCategory, ErrorDetails, XmtpError};
pub use identity::{
    CanMessageEntry, CatchUpSummary, GroupSyncSummary, InboxCountEntry, InboxState, Installation,
    KeyPackageLifetime, KeyPackageStatus, KeyPackageStatusEntry, SignatureKind, SignatureRequest,
};
pub use ids::{ConversationID, InboxID, InstallationID, MessageID, Timestamp};
pub use logging::{LogLevel, LoggingOptions, OtelOptions};
#[cfg(not(target_arch = "wasm32"))]
pub use logging::{LogProcessType, LogRecord, LogRotation, LogSink, LogSinkError};
pub use message::{
    ContentTypeId, DeliveryStatus, Message, MessageBody, MessageContent, MessageData, MessageKind,
    ReactionMessage, ReplyParent,
};
#[cfg(not(target_arch = "wasm32"))]
pub use notifications::{
    NotificationChannel, NotificationConfig, NotificationFailure, NotificationState,
};
pub use preferences::{ConsentEntity, ConsentRecord, ConsentState, Preferences};
pub use reader::MessageReader;
pub use signer::{
    PublicIdentity, PublicIdentityKind, Signature, Signer, SignerError, SignerKind, SigningRequest,
    generate_local_signer, local_signer_from_private_key,
};
pub use state::{
    CommitLogForkStatus, ConversationDebugInfo, ConversationHmacKeys, ConversationState,
    DisappearingSettings, GroupMembershipCapabilities, GroupPermissions, GroupPolicyType,
    GroupState, HmacKey, InboxCapabilities, InstallationCapabilities, LastReadTimeEntry, Member,
    MembershipResult, MembershipState, MetadataFieldKind, MlsExtensionType, NotificationOverride,
    PermissionLevel, PermissionPolicy, PermissionPolicySet, PermissionUpdateKind,
};
pub use static_helpers::MessageMetadataEntry;
pub use storage::Storage;

uniffi::setup_scaffolding!();

#[xmtp_macro::sdk_export]
pub fn sdk_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

/// An empty asynchronous call for measuring FFI scheduling cost.
#[cfg(feature = "bench")]
#[xmtp_macro::sdk_export]
pub async fn sdk_empty_call() {}

#[cfg(test)]
mod tests;
