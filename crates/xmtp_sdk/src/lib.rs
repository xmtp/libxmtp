//! The shared UniFFI surface for XMTP SDKs.

#![recursion_limit = "512"]

// Keep the WASM allocator, free function, and panic hook in the cdylib.
#[cfg(target_arch = "wasm32")]
extern crate uniffi_runtime_wasm as _;

mod client;
mod client_identity;
mod configuration;
mod conversation;
mod credentials;
mod crypto;
mod error;
mod foreign;
mod identity;
mod ids;
mod logging;
mod message;
#[cfg(not(target_arch = "wasm32"))]
mod notifications;
mod reader;
mod signer;
mod static_helpers;

pub use client::{Client, ClientOptions, StorageLocation, StorageOptions};
pub use configuration::{
    AuthConfiguration, LimitsConfiguration, MlsConfiguration, RetentionConfiguration,
    ServerConfiguration, SigningKeyDescription,
};
pub use conversation::{Conversations, Group};
pub use credentials::{
    Backend, BackendOptions, BackendSource, Credential, CredentialError, CredentialSource,
};
pub use crypto::{EncryptedEncodedContent, EncryptionKeys};
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
    ContentTypeId, DeliveryStatus, Message, MessageContent, MessageData, MessageKind,
};
#[cfg(not(target_arch = "wasm32"))]
pub use notifications::{
    ConsentState, NotificationChannel, NotificationConfig, NotificationFailure, NotificationState,
};
pub use reader::MessageReader;
pub use signer::{
    PublicIdentity, PublicIdentityKind, Signature, Signer, SignerError, SignerKind, SigningRequest,
    generate_local_signer, local_signer_from_private_key,
};
pub use static_helpers::MessageMetadataEntry;

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
