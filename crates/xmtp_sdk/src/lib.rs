//! The shared UniFFI surface for XMTP SDKs.

#![recursion_limit = "512"]

// Keep the WASM allocator, free function, and panic hook in the cdylib.
#[cfg(target_arch = "wasm32")]
extern crate uniffi_runtime_wasm as _;

#[cfg(not(feature = "pure-only"))]
mod archives;
#[cfg(not(feature = "pure-only"))]
mod client;
#[cfg(not(feature = "pure-only"))]
mod client_identity;
#[cfg(not(feature = "pure-only"))]
mod configuration;
mod content;
#[cfg(not(feature = "pure-only"))]
mod conversation;
#[cfg(not(feature = "pure-only"))]
mod conversations;
#[cfg(not(feature = "pure-only"))]
mod credentials;
#[cfg(not(feature = "pure-only"))]
mod crypto;
#[cfg(not(feature = "pure-only"))]
mod diagnostics;
mod error;
#[cfg(not(feature = "pure-only"))]
mod foreign;
#[cfg(not(feature = "pure-only"))]
mod identity;
mod ids;
#[cfg(not(feature = "pure-only"))]
mod logging;
#[cfg(not(feature = "pure-only"))]
mod message;
#[cfg(all(not(feature = "pure-only"), not(target_arch = "wasm32")))]
mod notifications;
#[cfg(not(feature = "pure-only"))]
mod preferences;
#[cfg(not(feature = "pure-only"))]
mod reader;
#[cfg(not(feature = "pure-only"))]
mod signer;
#[cfg(not(feature = "pure-only"))]
mod state;
#[cfg(not(feature = "pure-only"))]
mod static_helpers;
#[cfg(not(feature = "pure-only"))]
mod storage;

#[cfg(not(feature = "pure-only"))]
pub use archives::{ArchiveElement, ArchiveMetadata, ArchiveOptions, Archives};
#[cfg(not(feature = "pure-only"))]
pub use client::{Client, ClientOptions, StorageLocation, StorageOptions};
#[cfg(not(feature = "pure-only"))]
pub use configuration::{
    AuthConfiguration, LimitsConfiguration, MlsConfiguration, RetentionConfiguration,
    ServerConfiguration, SigningKeyDescription,
};
#[cfg(feature = "conformance")]
pub use content::StandardCodecSample;
pub use content::{
    Action, ActionStyle, Actions, Attachment, Compression, DeletedBy, DeletedMessage,
    EncodedContent, GroupUpdated, Intent, LeaveRequest, MetadataFieldChange, MultiRemoteAttachment,
    Reaction, ReactionAction, ReactionSchema, RemoteAttachment, SendOptions, StandardContent,
    StandardContentKind, TransactionMetadata, TransactionReference, WalletCall, WalletCallMetadata,
    WalletSendCalls, decode_standard, encode_standard, encode_text, standard_content_type,
};
#[cfg(not(feature = "pure-only"))]
pub use conversation::{Conversation, Conversations, Dm, Group};
#[cfg(not(feature = "pure-only"))]
pub use conversations::{
    ConversationKind, ConversationOrder, CreateDmOptions, CreateGroupOptions, GroupPermissionMode,
    ListConversationsOptions, ListMessagesOptions, MessageOrder, MessageSortBy,
};
#[cfg(not(feature = "pure-only"))]
pub use credentials::{
    Backend, BackendOptions, BackendSource, Credential, CredentialError, CredentialSource,
};
#[cfg(not(feature = "pure-only"))]
pub use crypto::{EncryptedEncodedContent, EncryptionKeys};
#[cfg(not(feature = "pure-only"))]
pub use diagnostics::{ApiStats, Diagnostics, IdentityStats};
pub use error::{ErrorCategory, ErrorDetails, XmtpError};
#[cfg(not(feature = "pure-only"))]
pub use identity::{
    CanMessageEntry, CatchUpSummary, GroupSyncSummary, InboxCountEntry, InboxState, Installation,
    KeyPackageLifetime, KeyPackageStatus, KeyPackageStatusEntry, SignatureKind, SignatureRequest,
};
pub use ids::{ConversationID, InboxID, InstallationID, MessageID, Timestamp};
#[cfg(not(feature = "pure-only"))]
pub use logging::{LogLevel, LoggingOptions, OtelOptions};
#[cfg(all(not(feature = "pure-only"), not(target_arch = "wasm32")))]
pub use logging::{LogProcessType, LogRecord, LogRotation, LogSink, LogSinkError};
#[cfg(not(feature = "pure-only"))]
pub use message::{
    ContentTypeId, DeliveryStatus, Message, MessageBody, MessageContent, MessageData, MessageKind,
    ReactionMessage, ReplyParent,
};
#[cfg(all(not(feature = "pure-only"), not(target_arch = "wasm32")))]
pub use notifications::{
    NotificationChannel, NotificationConfig, NotificationFailure, NotificationState,
};
#[cfg(not(feature = "pure-only"))]
pub use preferences::{ConsentEntity, ConsentRecord, ConsentState, Preferences};
#[cfg(not(feature = "pure-only"))]
pub use reader::MessageReader;
#[cfg(not(feature = "pure-only"))]
pub use signer::{
    PublicIdentity, PublicIdentityKind, Signature, Signer, SignerError, SignerKind, SigningRequest,
    generate_local_signer, local_signer_from_private_key,
};
#[cfg(not(feature = "pure-only"))]
pub use state::{
    CommitLogForkStatus, ConversationDebugInfo, ConversationHmacKeys, ConversationState,
    DisappearingSettings, GroupMembershipCapabilities, GroupPermissions, GroupPolicyType,
    GroupState, HmacKey, InboxCapabilities, InstallationCapabilities, LastReadTimeEntry, Member,
    MembershipResult, MembershipState, MetadataFieldKind, MlsExtensionType, NotificationOverride,
    PermissionLevel, PermissionPolicy, PermissionPolicySet, PermissionUpdateKind,
};
#[cfg(not(feature = "pure-only"))]
pub use static_helpers::MessageMetadataEntry;
#[cfg(not(feature = "pure-only"))]
pub use storage::Storage;

#[cfg(feature = "pure-only")]
#[derive(Clone, Debug, uniffi::Record)]
pub struct ContentTypeId {
    pub authority_id: String,
    pub type_id: String,
    pub version_major: u32,
    pub version_minor: u32,
}

uniffi::setup_scaffolding!();

#[xmtp_macro::sdk_export(pure)]
pub fn sdk_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

/// An empty asynchronous call for measuring FFI scheduling cost.
#[cfg(all(feature = "bench", not(feature = "pure-only")))]
#[xmtp_macro::sdk_export]
pub async fn sdk_empty_call() {}

#[cfg(all(feature = "bridge-panic-test", not(feature = "pure-only")))]
#[uniffi::export]
pub async fn bridge_test_panic() -> Result<(), XmtpError> {
    panic!("bridge panic proof");
}

#[cfg(all(
    feature = "bridge-panic-test",
    target_arch = "wasm32",
    not(feature = "pure-only")
))]
#[uniffi::export]
pub async fn bridge_test_background_panic() -> Result<(), XmtpError> {
    wasm_bindgen_futures::spawn_local(async {
        panic!("bridge background panic proof");
    });
    Ok(())
}

#[cfg(all(test, not(feature = "pure-only")))]
mod tests;
