//! The shared UniFFI surface for XMTP SDKs.

#![recursion_limit = "512"]

// Keep the WASM allocator, free function, and panic hook in the cdylib.
#[cfg(target_arch = "wasm32")]
extern crate uniffi_runtime_wasm as _;

#[cfg(not(feature = "pure-only"))]
mod archives;
#[cfg(not(feature = "pure-only"))]
mod attachments;
#[cfg(not(feature = "pure-only"))]
mod client;
#[cfg(all(not(feature = "pure-only"), not(target_arch = "wasm32")))]
mod client_discard;
#[cfg(all(not(feature = "pure-only"), not(target_arch = "wasm32")))]
pub use client_discard::sdk_discard_unreturned_client;
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
mod delivery;
#[cfg(not(feature = "pure-only"))]
mod diagnostics;
mod error;
#[cfg(not(feature = "pure-only"))]
mod events;
#[cfg(all(feature = "conformance", not(feature = "pure-only")))]
pub use client::SdkConformanceListenerCounts;
#[cfg(all(
    feature = "conformance",
    not(feature = "pure-only"),
    not(target_arch = "wasm32")
))]
pub use client::{SdkConformanceConstructorProbe, SdkConformanceConstructorState};
#[cfg(not(feature = "pure-only"))]
mod foreign;
#[cfg(all(feature = "conformance", not(feature = "pure-only")))]
mod foreign_conformance;
#[cfg(all(feature = "conformance", not(feature = "pure-only")))]
pub use foreign_conformance::{
    SdkConformanceForeignCallCounts, sdk_conformance_foreign_call_counts,
};
#[cfg(not(feature = "pure-only"))]
mod identity;
mod ids;
#[cfg(not(feature = "pure-only"))]
mod logging;
#[cfg(not(feature = "pure-only"))]
mod message;
mod metadata;
#[cfg(all(not(feature = "pure-only"), not(target_arch = "wasm32")))]
mod notifications;
#[cfg(not(feature = "pure-only"))]
mod preferences;
mod signer;
#[cfg(not(feature = "pure-only"))]
mod state;
#[cfg(not(feature = "pure-only"))]
mod static_helpers;
#[cfg(not(feature = "pure-only"))]
mod storage;
#[cfg(all(target_arch = "wasm32", not(feature = "pure-only")))]
mod storage_admin;

#[cfg(not(feature = "pure-only"))]
pub use archives::{ArchiveElement, ArchiveMetadata, ArchiveOptions, Archives};
#[cfg(not(feature = "pure-only"))]
pub use attachments::{
    AttachmentSource, Attachments, DownloadedAttachment, LocalAttachment, PendingAttachment,
    PendingAttachmentStatus,
};
#[cfg(not(feature = "pure-only"))]
pub use client::{
    AttachmentOptions, Client, ClientHandlers, ClientOptions, PreAuthenticate,
    PreAuthenticateError, StorageLocation, StorageOptions,
};
#[cfg(not(feature = "pure-only"))]
pub use configuration::{
    AttachmentsConfiguration, AuthConfiguration, LimitsConfiguration, MlsConfiguration,
    RetentionConfiguration, ServerConfiguration, SigningKeyDescription,
};
#[cfg(feature = "conformance")]
pub use content::StandardCodecSample;
pub use content::{
    Action, ActionStyle, Actions, Attachment, Compression, DeleteMessageContent, DeletedBy,
    DeletedMessage, EncodedContent, EncryptedEncodedContent, EncryptionKeys, GroupUpdated, Intent,
    LeaveRequest, MetadataFieldChange, MultiRemoteAttachment, Reaction, ReactionAction,
    ReactionSchema, ReactionV2Content, RemoteAttachment, ReplyContent, SendOptions,
    StandardContent, StandardContentKind, TransactionMetadata, TransactionReference, WalletCall,
    WalletCallMetadata, WalletSendCalls, catalogue_content_type_should_push, decode_encoded_content,
    decode_standard, encode_encoded_content, encode_standard, encode_text,
    remote_attachment_from_encrypted, standard_content_type,
};
#[cfg(not(feature = "pure-only"))]
pub use conversation::{Conversation, Conversations, Dm, Group};
#[cfg(not(feature = "pure-only"))]
pub use conversations::{
    ConversationKind, ConversationMessageReaderOptions, ConversationOrder,
    ConversationReaderOptions, CreateDmOptions, CreateGroupOptions, GroupPermissionMode,
    ListConversationsOptions, ListMessagesOptions, MessageOrder, MessageReaderOptions,
    MessageSortBy,
};
#[cfg(not(feature = "pure-only"))]
pub use credentials::{
    Backend, BackendOptions, BackendSource, Credential, CredentialError, CredentialSource,
};
#[cfg(all(test, not(feature = "pure-only")))]
use delivery as reader;
#[cfg(not(feature = "pure-only"))]
pub use delivery::{ConnectionState, ConversationReader, MessageHistorySnapshot, MessageReader};
#[cfg(not(feature = "pure-only"))]
pub use diagnostics::{ApiStats, Diagnostics, IdentityStats};
pub use error::{
    AttachmentFailure, AttachmentFailureCause, CredentialFailureKind, ErrorCategory, ErrorDetails,
    XmtpError,
};
#[cfg(not(feature = "pure-only"))]
pub use events::{
    AttachmentFailed, AttachmentRef, ClientEvent, EventFilter, EventKind, EventListener,
    EventReader, ListenerError, ListenerId,
};
#[cfg(not(feature = "pure-only"))]
pub use identity::{
    CatchUpSummary, GroupSyncSummary, InboxState, Installation, KeyPackageLifetime,
    KeyPackageStatus, SignatureKind, SignatureRequest,
};
pub use ids::{ConversationId, InboxId, InstallationId, MessageId, Timestamp};
#[cfg(not(feature = "pure-only"))]
pub use logging::{LogLevel, LogRecord, LogSink, LogSinkError, LoggingOptions, OtelOptions};
#[cfg(all(not(feature = "pure-only"), not(target_arch = "wasm32")))]
pub use logging::{LogProcessType, LogRotation};
#[cfg(not(feature = "pure-only"))]
pub use message::{
    ContentTypeId, DeliveryStatus, Message, MessageBody, MessageContent, MessageData, MessageKind,
    ReactionMessage, ReplyParent,
};
#[cfg(not(feature = "pure-only"))]
pub use metadata::{
    ApplicationComponentDefinition, ComponentMutation, ComponentPermissions, FieldKey, FieldValue,
    MapEntry, MapMutation, MetadataBasePolicy, MetadataComponentType, MetadataFieldDescriptor,
    MetadataFieldValue, MetadataKeyType, MetadataPolicy, MetadataScalarType, MetadataValue,
    SetMutation, UserFieldUpdate, UserFieldValue,
};
pub use metadata::{MetadataFieldRef, WellKnownMetadataField, metadata_field_ref};
#[cfg(all(not(feature = "pure-only"), not(target_arch = "wasm32")))]
pub use notifications::{
    NotificationChannel, NotificationConfig, NotificationFailure, NotificationState,
};
#[cfg(not(feature = "pure-only"))]
pub use preferences::{ConsentEntity, ConsentRecord, ConsentState, Preferences};
pub use signer::{PublicIdentity, PublicIdentityKind, generate_inbox_id};
#[cfg(not(feature = "pure-only"))]
pub use signer::{
    Signature, Signer, SignerError, SignerKind, SigningRequest, generate_local_signer,
    local_signer_from_private_key,
};
#[cfg(not(feature = "pure-only"))]
pub use state::{
    CommitLogForkStatus, ConversationDebugInfo, ConversationState, DisappearingSettings,
    GroupMembershipCapabilities, GroupPermissions, GroupPolicyType, GroupState, HmacKey,
    InboxCapabilities, InstallationCapabilities, Member, MembershipResult, MembershipState,
    MetadataFieldKind, MlsExtensionType, NotificationOverride, PermissionLevel, PermissionPolicy,
    PermissionPolicySet, PermissionUpdateKind,
};
#[cfg(not(feature = "pure-only"))]
pub use static_helpers::MessageMetadataEntry;
#[cfg(not(feature = "pure-only"))]
pub use storage::Storage;
#[cfg(all(target_arch = "wasm32", not(feature = "pure-only")))]
pub use storage_admin::StorageAdmin;

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
