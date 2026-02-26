#![recursion_limit = "256"]
#![warn(clippy::unwrap_used)]
pub mod crypto;
pub mod fork_recovery;
pub mod identity;
pub mod inbox_owner;
pub mod logger;
pub mod message;
pub mod mls;
pub mod worker;

pub use crate::inbox_owner::SigningError;
pub use logger::{enter_debug_writer, exit_debug_writer};
pub use message::*;
pub use mls::*;
use std::error::Error;
use xmtp_api_d14n::MessageBackendBuilderError;
use xmtp_common::ErrorCode;
use xmtp_cryptography::signature::IdentifierValidationError;
use xmtp_mls::{
    messages::enrichment::EnrichMessageError, mls_common::group_metadata::GroupMetadataError,
};

extern crate tracing as log;

uniffi::setup_scaffolding!("xmtpv3");

#[derive(thiserror::Error, Debug, ErrorCode)]
pub enum GenericError {
    #[error("Client error: {0}")]
    #[error_code(inherit)]
    Client(#[from] xmtp_mls::client::ClientError),
    #[error("Client builder error: {0}")]
    #[error_code(inherit)]
    ClientBuilder(#[from] xmtp_mls::builder::ClientBuilderError),
    #[error("Storage error: {0}")]
    #[error_code(inherit)]
    Storage(#[from] xmtp_db::StorageError),
    #[error("Group error: {0}")]
    #[error_code(inherit)]
    GroupError(#[from] xmtp_mls::groups::GroupError),
    #[error("Signature: {0}")]
    #[error_code(inherit)]
    Signature(#[from] xmtp_cryptography::signature::SignatureError),
    #[error("Group metadata: {0}")]
    #[error_code(inherit)]
    GroupMetadata(#[from] GroupMetadataError),
    #[error("Group permissions: {0}")]
    #[error_code(inherit)]
    GroupMutablePermissions(
        #[from] xmtp_mls::groups::group_permissions::GroupMutablePermissionsError,
    ),
    #[error("{err}")]
    Generic { err: String },
    #[error(transparent)]
    #[error_code(inherit)]
    SignatureRequestError(#[from] xmtp_id::associations::builder::SignatureRequestError),
    #[error(transparent)]
    #[error_code(inherit)]
    Erc1271SignatureError(#[from] xmtp_id::associations::signature::SignatureError),
    #[error(transparent)]
    #[error_code(inherit)]
    Verifier(#[from] xmtp_id::scw_verifier::VerifierError),
    #[error("Failed to convert to u32")]
    FailedToConvertToU32,
    #[error("Association error: {0}")]
    #[error_code(inherit)]
    Association(#[from] xmtp_id::associations::AssociationError),
    #[error(transparent)]
    #[error_code(inherit)]
    DeviceSync(#[from] xmtp_mls::groups::device_sync::DeviceSyncError),
    #[error(transparent)]
    #[error_code(inherit)]
    Identity(#[from] xmtp_mls::identity::IdentityError),
    #[error(transparent)]
    JoinError(#[from] tokio::task::JoinError),
    #[error(transparent)]
    IoError(#[from] tokio::io::Error),
    #[error(transparent)]
    #[error_code(inherit)]
    Subscription(#[from] xmtp_mls::subscriptions::SubscribeError),
    #[error(transparent)]
    #[error_code(inherit)]
    ApiClientBuild(#[from] xmtp_api_grpc::error::GrpcBuilderError),
    #[error(transparent)]
    #[error_code(inherit)]
    Grpc(#[from] Box<xmtp_api_grpc::error::GrpcError>),
    #[error(transparent)]
    #[error_code(inherit)]
    AddressValidation(#[from] IdentifierValidationError),
    #[error("Error initializing rolling log file")]
    LogInit(#[from] tracing_appender::rolling::InitError),
    #[error(transparent)]
    ReloadLog(#[from] tracing_subscriber::reload::Error),
    #[error("Error initializing debug log file")]
    Log(String),
    #[error("Timer duration expired")]
    Expired,
    #[error(transparent)]
    #[error_code(inherit)]
    BackendBuilder(#[from] MessageBackendBuilderError),
    #[error(transparent)]
    #[error_code(inherit)]
    Api(#[from] xmtp_api::ApiError),
    #[error(transparent)]
    #[error_code(inherit)]
    Enrich(#[from] EnrichMessageError),
}

// this impl allows us to gracefully handle unexpected errors from foreign code without panicking
impl From<uniffi::UnexpectedUniFFICallbackError> for GenericError {
    fn from(e: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Generic { err: e.to_string() }
    }
}

/// Wrapper that formats errors as `[error_code] message` for mobile SDKs.
/// UniFFI uses Display to convert errors to strings, so this wrapper
/// ensures mobile clients receive machine-readable error codes.
#[derive(Debug, uniffi::Error)]
#[uniffi(flat_error)]
pub enum FfiError {
    Error(GenericError),
}

impl std::fmt::Display for FfiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FfiError::Error(e) => write!(f, "[{}] {}", e.error_code(), e),
        }
    }
}

impl std::error::Error for FfiError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            FfiError::Error(e) => e.source(),
        }
    }
}

impl<T: Into<GenericError>> From<T> for FfiError {
    fn from(err: T) -> Self {
        Self::Error(err.into())
    }
}

impl FfiError {
    /// Create a generic error with a message
    pub fn generic(err: impl Into<String>) -> Self {
        FfiError::Error(GenericError::Generic { err: err.into() })
    }
}

impl From<xmtp_common::time::Expired> for FfiError {
    fn from(_: xmtp_common::time::Expired) -> Self {
        FfiError::Error(GenericError::Expired)
    }
}

impl From<String> for GenericError {
    fn from(err: String) -> Self {
        Self::Generic { err }
    }
}

impl GenericError {
    pub fn from_error<T: Error>(err: T) -> Self {
        Self::Generic {
            err: stringify_error_chain(&err),
        }
    }
}

// TODO Use non-string errors across Uniffi interface
fn stringify_error_chain<T: Error>(error: &T) -> String {
    let mut result = format!("Error: {}\n", error);

    let mut source = error.source();
    while let Some(src) = source {
        result += &format!("Caused by: {}\n", src);
        source = src.source();
    }

    result
}

#[uniffi::export]
pub fn get_version_info() -> String {
    include_str!("../libxmtp-version.txt").to_string()
}

#[cfg(test)]
mod lib_tests {
    use crate::{GenericError, get_version_info};
    use xmtp_common::ErrorCode;

    #[test]
    pub fn test_get_version_info() {
        print!("{}", get_version_info());
    }

    #[test]
    fn test_generic_error_code_unit_variant() {
        let err = GenericError::FailedToConvertToU32;
        assert_eq!(err.error_code(), "GenericError::FailedToConvertToU32");
    }

    #[test]
    fn test_generic_error_code_string_variant() {
        let err = GenericError::Generic {
            err: "test error".to_string(),
        };
        assert_eq!(err.error_code(), "GenericError::Generic");
    }

    #[test]
    fn test_generic_error_code_inherited_storage() {
        // GenericError::Storage inherits from StorageError
        // StorageError::NotFound returns "StorageError::NotFound" (doesn't inherit further)
        let storage_err =
            xmtp_db::StorageError::NotFound(xmtp_db::NotFound::MessageById(vec![1, 2, 3]));
        let err = GenericError::Storage(storage_err);
        assert_eq!(err.error_code(), "StorageError::NotFound");
    }

    #[test]
    fn test_generic_error_code_expired() {
        let err = GenericError::Expired;
        assert_eq!(err.error_code(), "GenericError::Expired");
    }

    #[test]
    fn test_ffi_error_display_format() {
        use crate::FfiError;

        // Test that FfiError Display includes the error code prefix
        let err = FfiError::generic("something went wrong");
        let display = err.to_string();
        assert!(
            display.starts_with("[GenericError::Generic]"),
            "Expected error to start with [GenericError::Generic], got: {}",
            display
        );
        assert!(
            display.contains("something went wrong"),
            "Expected error message in display"
        );
    }

    #[test]
    fn test_ffi_error_display_inherited_code() {
        use crate::FfiError;

        // Test that FfiError Display shows inherited error codes
        let storage_err =
            xmtp_db::StorageError::NotFound(xmtp_db::NotFound::MessageById(vec![1, 2, 3]));
        let err: FfiError = storage_err.into();
        let display = err.to_string();
        assert!(
            display.starts_with("[StorageError::NotFound]"),
            "Expected error to start with [StorageError::NotFound], got: {}",
            display
        );
    }

    #[test]
    fn test_ffi_error_source() {
        use crate::FfiError;
        use std::error::Error;

        let err = FfiError::generic("test error");
        // Generic errors don't have a source
        assert!(err.source().is_none());
    }

    #[test]
    fn test_generic_error_from_string() {
        let err: GenericError = "string error".to_string().into();
        assert!(matches!(err, GenericError::Generic { .. }));
        assert_eq!(err.error_code(), "GenericError::Generic");
    }

    #[test]
    fn test_generic_error_from_error() {
        use std::io;

        let io_err = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let err = GenericError::from_error(io_err);
        assert!(matches!(err, GenericError::Generic { .. }));
        let display = format!("{}", err);
        assert!(display.contains("file not found"));
    }

    #[test]
    fn test_ffi_error_from_expired() {
        use crate::FfiError;
        use xmtp_common::time::Expired;

        let expired = Expired;
        let ffi_err: FfiError = expired.into();
        let display = ffi_err.to_string();
        assert!(display.contains("[GenericError::Expired]"));
    }

    #[test]
    fn test_ffi_error_from_various_error_types() {
        use crate::FfiError;

        // Test From<String>
        let err: FfiError = "string error".to_string().into();
        assert!(err.to_string().contains("[GenericError::Generic]"));

        // Test FfiError::generic helper
        let err = FfiError::generic("helper error");
        assert!(err.to_string().contains("[GenericError::Generic]"));
        assert!(err.to_string().contains("helper error"));
    }

    // Execute once before any tests are run
    #[ctor::ctor]
    fn _setup() {
        let _ = fdlimit::raise_fd_limit();
    }
}
