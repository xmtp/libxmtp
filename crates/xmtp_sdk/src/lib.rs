//! The shared UniFFI surface for XMTP SDKs.

#![recursion_limit = "512"]

// Keep the WASM allocator, free function, and panic hook in the cdylib.
#[cfg(target_arch = "wasm32")]
extern crate uniffi_runtime_wasm as _;

mod client;
mod conversation;
mod credentials;
mod error;
mod foreign;
mod ids;
mod message;
mod reader;
mod signer;

pub use client::{Client, ClientOptions, StorageLocation, StorageOptions};
pub use conversation::{Conversations, Group};
pub use credentials::{Backend, BackendOptions, Credential, CredentialError, CredentialSource};
pub use error::{ErrorCategory, ErrorDetails, XmtpError};
pub use ids::{ConversationID, InboxID, InstallationID, MessageID, Timestamp};
pub use message::{
    ContentTypeId, DeliveryStatus, Message, MessageContent, MessageData, MessageKind,
};
pub use reader::MessageReader;
pub use signer::{
    PublicIdentity, PublicIdentityKind, Signature, Signer, SignerError, SignerKind, SigningRequest,
};

uniffi::setup_scaffolding!();

#[xmtp_macro::sdk_export]
pub fn sdk_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

/// An empty asynchronous call for measuring FFI scheduling cost.
#[xmtp_macro::sdk_export]
pub async fn sdk_empty_call() {}

#[cfg(test)]
mod tests;
