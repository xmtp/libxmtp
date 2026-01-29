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

#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct FfiErrorInfo {
    pub code: String,
    pub message: String,
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

impl From<GenericError> for FfiError {
    fn from(err: GenericError) -> Self {
        FfiError::Error(err)
    }
}

impl FfiError {
    /// Create a generic error with a message
    pub fn generic(err: impl Into<String>) -> Self {
        FfiError::Error(GenericError::Generic { err: err.into() })
    }
}

fn parse_error_message(message: &str) -> FfiErrorInfo {
    if let Some(rest) = message.strip_prefix('[')
        && let Some(end) = rest.find(']')
    {
        let code = rest[..end].to_string();
        return FfiErrorInfo {
            code,
            message: message.to_string(),
        };
    }

    FfiErrorInfo {
        code: "Unknown".to_string(),
        message: message.to_string(),
    }
}

/// Parse an error string like `[ErrorCode] message` into a structured error info.
#[uniffi::export]
pub fn parse_xmtp_error(message: String) -> FfiErrorInfo {
    parse_error_message(&message)
}

// Implement From for all error types that GenericError supports.
// NOTE: When adding a new error type with #[from] to GenericError,
// also add it here to enable the ? operator with FfiError return types.
macro_rules! impl_ffi_error_from {
    ($($error_ty:ty),* $(,)?) => {
        $(
            impl From<$error_ty> for FfiError {
                fn from(err: $error_ty) -> Self {
                    FfiError::Error(GenericError::from(err))
                }
            }
        )*
    };
}

impl_ffi_error_from! {
    xmtp_mls::client::ClientError,
    xmtp_mls::builder::ClientBuilderError,
    xmtp_db::StorageError,
    xmtp_mls::groups::GroupError,
    xmtp_cryptography::signature::SignatureError,
    GroupMetadataError,
    xmtp_mls::groups::group_permissions::GroupMutablePermissionsError,
    xmtp_id::associations::builder::SignatureRequestError,
    xmtp_id::associations::signature::SignatureError,
    xmtp_id::scw_verifier::VerifierError,
    xmtp_id::associations::AssociationError,
    xmtp_mls::groups::device_sync::DeviceSyncError,
    xmtp_mls::identity::IdentityError,
    tokio::task::JoinError,
    tokio::io::Error,
    xmtp_mls::subscriptions::SubscribeError,
    xmtp_api_grpc::error::GrpcBuilderError,
    Box<xmtp_api_grpc::error::GrpcError>,
    IdentifierValidationError,
    tracing_appender::rolling::InitError,
    tracing_subscriber::reload::Error,
    MessageBackendBuilderError,
    xmtp_api::ApiError,
    EnrichMessageError,
    uniffi::UnexpectedUniFFICallbackError,
    String,
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
    fn test_parse_xmtp_error_with_code() {
        let parsed = crate::parse_xmtp_error("[GroupError::NotFound] Group not found".to_string());
        assert_eq!(parsed.code, "GroupError::NotFound");
        assert_eq!(parsed.message, "[GroupError::NotFound] Group not found");
    }

    #[test]
    fn test_parse_xmtp_error_without_code() {
        let parsed = crate::parse_xmtp_error("Some error".to_string());
        assert_eq!(parsed.code, "Unknown");
        assert_eq!(parsed.message, "Some error");
    }

    // Execute once before any tests are run
    #[ctor::ctor]
    fn _setup() {
        let _ = fdlimit::raise_fd_limit();
    }
}
