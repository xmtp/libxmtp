//! The shared UniFFI surface for XMTP SDKs.

#![recursion_limit = "512"]

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

#[cfg(test)]
mod tests;
