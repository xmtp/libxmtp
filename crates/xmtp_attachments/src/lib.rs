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
pub use encoding::{
    AttachmentDecoder, ContentChunk, DecodedMeta, ciphertext_len, encoded_prefix,
    retained_fields_fit,
};
pub use http::{PutOutcome, Transfer, UploadRequest, download_cap};
pub use sanitize::{local_file_name, sanitize_path_component, sanitize_path_component_with_limit};
#[cfg(target_arch = "wasm32")]
pub use store::OpfsStore;
pub use store::{
    AttachmentOptions, DownloadSink, LocalStore, StagedFile, StoreFile, StoreWriter, staged_path,
    temporary_path,
};
#[cfg(not(target_arch = "wasm32"))]
pub use store::{NativeStore, create_private_directory};

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
    pub const ALL: [Self; 21] = [
        Self::NotOffered,
        Self::TooLarge,
        Self::SourceUnreadable,
        Self::LocalStorage,
        Self::StagedUnusable,
        Self::ConnectionBlocked,
        Self::Credential,
        Self::BackendRejected,
        Self::BackendUnavailable,
        Self::TargetRejected,
        Self::Network,
        Self::InsecureUrl,
        Self::BlockedAddress,
        Self::TooManyRedirects,
        Self::NotFound,
        Self::HttpStatus,
        Self::Malformed,
        Self::DigestMismatch,
        Self::DecryptionFailed,
        Self::NotAnAttachment,
        Self::Deleted,
    ];

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

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "not_offered" => Some(Self::NotOffered),
            "too_large" => Some(Self::TooLarge),
            "source_unreadable" => Some(Self::SourceUnreadable),
            "local_storage" => Some(Self::LocalStorage),
            "staged_unusable" => Some(Self::StagedUnusable),
            "connection_blocked" => Some(Self::ConnectionBlocked),
            "credential" => Some(Self::Credential),
            "backend_rejected" => Some(Self::BackendRejected),
            "backend_unavailable" => Some(Self::BackendUnavailable),
            "target_rejected" => Some(Self::TargetRejected),
            "network" => Some(Self::Network),
            "insecure_url" => Some(Self::InsecureUrl),
            "blocked_address" => Some(Self::BlockedAddress),
            "too_many_redirects" => Some(Self::TooManyRedirects),
            "not_found" => Some(Self::NotFound),
            "http_status" => Some(Self::HttpStatus),
            "malformed" => Some(Self::Malformed),
            "digest_mismatch" => Some(Self::DigestMismatch),
            "decryption_failed" => Some(Self::DecryptionFailed),
            "not_an_attachment" => Some(Self::NotAnAttachment),
            "deleted" => Some(Self::Deleted),
            _ => None,
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

#[cfg(test)]
mod tests {
    use super::AttachmentFailureCause as Cause;

    // verifies: ATCH-060
    #[xmtp_common::test(unwrap_try = true)]
    fn failure_cause_strings_round_trip() {
        fn index(cause: Cause) -> usize {
            match cause {
                Cause::NotOffered => 0,
                Cause::TooLarge => 1,
                Cause::SourceUnreadable => 2,
                Cause::LocalStorage => 3,
                Cause::StagedUnusable => 4,
                Cause::ConnectionBlocked => 5,
                Cause::Credential => 6,
                Cause::BackendRejected => 7,
                Cause::BackendUnavailable => 8,
                Cause::TargetRejected => 9,
                Cause::Network => 10,
                Cause::InsecureUrl => 11,
                Cause::BlockedAddress => 12,
                Cause::TooManyRedirects => 13,
                Cause::NotFound => 14,
                Cause::HttpStatus => 15,
                Cause::Malformed => 16,
                Cause::DigestMismatch => 17,
                Cause::DecryptionFailed => 18,
                Cause::NotAnAttachment => 19,
                Cause::Deleted => 20,
            }
        }
        for (expected, cause) in Cause::ALL.into_iter().enumerate() {
            assert_eq!(index(cause), expected);
            assert_eq!(Cause::parse(cause.as_str()), Some(cause));
        }
        assert_eq!(index(Cause::Deleted) + 1, Cause::ALL.len());
        assert_eq!(Cause::parse("unknown"), None);
    }
}
