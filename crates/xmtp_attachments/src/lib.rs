//! Streaming remote attachment primitives.

mod address;
mod crypto;
mod derive;
mod encoding;
mod http;
mod sanitize;
mod store;

pub use crypto::{GcmDecryptor, GcmEncryptor, KeyMaterial};
pub use derive::{attachment_key, download_url, plaintext_rel_path, remote_attachment};
pub use encoding::{AttachmentDecoder, ContentChunk, DecodedMeta, ciphertext_len, encoded_prefix};
pub use http::{PutOutcome, Transfer, UploadRequest, download_cap};
pub use sanitize::{local_file_name, sanitize_path_component, sanitize_path_component_with_limit};
#[cfg(not(target_arch = "wasm32"))]
pub use store::NativeStore;
#[cfg(target_arch = "wasm32")]
pub use store::OpfsStore;
pub use store::{
    AttachmentOptions, DownloadSink, LocalStore, StagedFile, StoreFile, StoreWriter, staged_path,
    temporary_path,
};

#[cfg(all(test, target_arch = "wasm32"))]
wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

/// Stable cause of an attachment failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentFailureCause {
    NotOffered,
    TooLarge,
    SourceUnreadable,
    LocalStorage,
    StagedUnusable,
    ConnectionBlocked,
    Credential,
    BackendRejected,
    BackendUnavailable,
    TargetRejected,
    Network,
    InsecureUrl,
    BlockedAddress,
    TooManyRedirects,
    NotFound,
    HttpStatus,
    Malformed,
    DigestMismatch,
    DecryptionFailed,
    NotAnAttachment,
    Deleted,
}

impl AttachmentFailureCause {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotOffered => "not_offered",
            Self::TooLarge => "too_large",
            Self::SourceUnreadable => "source_unreadable",
            Self::LocalStorage => "local_storage",
            Self::StagedUnusable => "staged_unusable",
            Self::ConnectionBlocked => "connection_blocked",
            Self::Credential => "credential",
            Self::BackendRejected => "backend_rejected",
            Self::BackendUnavailable => "backend_unavailable",
            Self::TargetRejected => "target_rejected",
            Self::Network => "network",
            Self::InsecureUrl => "insecure_url",
            Self::BlockedAddress => "blocked_address",
            Self::TooManyRedirects => "too_many_redirects",
            Self::NotFound => "not_found",
            Self::HttpStatus => "http_status",
            Self::Malformed => "malformed",
            Self::DigestMismatch => "digest_mismatch",
            Self::DecryptionFailed => "decryption_failed",
            Self::NotAnAttachment => "not_an_attachment",
            Self::Deleted => "deleted",
        }
    }
}

/// An attachment failure with a stable cause.
#[derive(Debug, thiserror::Error)]
#[error("attachment failure: {}", cause.as_str())]
pub struct AttachmentError {
    pub cause: AttachmentFailureCause,
    /// Final HTTP status from a host answer, when it is available.
    pub http_status: Option<u16>,
}

impl AttachmentError {
    pub const fn new(cause: AttachmentFailureCause) -> Self {
        Self {
            cause,
            http_status: None,
        }
    }

    pub(crate) const fn with_http_status(cause: AttachmentFailureCause, status: u16) -> Self {
        Self {
            cause,
            http_status: Some(status),
        }
    }
}
