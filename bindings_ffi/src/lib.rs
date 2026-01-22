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
use xmtp_common::time::Expired;
use xmtp_common::ErrorCode;
use xmtp_cryptography::signature::IdentifierValidationError;
use xmtp_mls::mls_common::group_metadata::GroupMetadataError;

extern crate tracing as log;

uniffi::setup_scaffolding!("xmtpv3");

#[derive(uniffi::Error, Debug, xmtp_macro::ErrorCode)]
#[uniffi(flat_error)]
pub enum GenericError {
    #[error_code(inherit)]
    Client(xmtp_mls::client::ClientError),
    #[error_code(inherit)]
    ClientBuilder(xmtp_mls::builder::ClientBuilderError),
    #[error_code(inherit)]
    Storage(xmtp_db::StorageError),
    #[error_code(inherit)]
    GroupError(xmtp_mls::groups::GroupError),
    #[error_code(inherit)]
    Signature(xmtp_cryptography::signature::SignatureError),
    #[error_code(inherit)]
    GroupMetadata(GroupMetadataError),
    #[error_code(inherit)]
    GroupMutablePermissions(xmtp_mls::groups::group_permissions::GroupMutablePermissionsError),
    Generic { err: String },
    #[error_code(inherit)]
    SignatureRequestError(xmtp_id::associations::builder::SignatureRequestError),
    #[error_code(inherit)]
    Erc1271SignatureError(xmtp_id::associations::signature::SignatureError),
    #[error_code(inherit)]
    Verifier(xmtp_id::scw_verifier::VerifierError),
    FailedToConvertToU32,
    #[error_code(inherit)]
    Association(xmtp_id::associations::AssociationError),
    #[error_code(inherit)]
    DeviceSync(xmtp_mls::groups::device_sync::DeviceSyncError),
    #[error_code(inherit)]
    Identity(xmtp_mls::identity::IdentityError),
    JoinError(tokio::task::JoinError),
    IoError(tokio::io::Error),
    #[error_code(inherit)]
    Subscription(xmtp_mls::subscriptions::SubscribeError),
    ApiClientBuild(xmtp_api_grpc::error::GrpcBuilderError),
    Grpc(Box<xmtp_api_grpc::error::GrpcError>),
    #[error_code(inherit)]
    AddressValidation(IdentifierValidationError),
    LogInit(tracing_appender::rolling::InitError),
    ReloadLog(tracing_subscriber::reload::Error),
    Log(String),
    Expired(Expired),
    #[error_code(inherit)]
    BackendBuilder(MessageBackendBuilderError),
}

impl std::fmt::Display for GenericError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let code = self.error_code();
        match self {
            Self::Client(e) => write!(f, "[{code}] Client error: {e}"),
            Self::ClientBuilder(e) => write!(f, "[{code}] Client builder error: {e}"),
            Self::Storage(e) => write!(f, "[{code}] Storage error: {e}"),
            Self::GroupError(e) => write!(f, "[{code}] Group error: {e}"),
            Self::Signature(e) => write!(f, "[{code}] Signature error: {e}"),
            Self::GroupMetadata(e) => write!(f, "[{code}] Group metadata error: {e}"),
            Self::GroupMutablePermissions(e) => write!(f, "[{code}] Group permissions error: {e}"),
            Self::Generic { err } => write!(f, "[{code}] {err}"),
            Self::SignatureRequestError(e) => write!(f, "[{code}] Signature request error: {e}"),
            Self::Erc1271SignatureError(e) => write!(f, "[{code}] ERC-1271 signature error: {e}"),
            Self::Verifier(e) => write!(f, "[{code}] Verifier error: {e}"),
            Self::FailedToConvertToU32 => write!(f, "[{code}] Failed to convert to u32"),
            Self::Association(e) => write!(f, "[{code}] Association error: {e}"),
            Self::DeviceSync(e) => write!(f, "[{code}] Device sync error: {e}"),
            Self::Identity(e) => write!(f, "[{code}] Identity error: {e}"),
            Self::JoinError(e) => write!(f, "[{code}] Join error: {e}"),
            Self::IoError(e) => write!(f, "[{code}] IO error: {e}"),
            Self::Subscription(e) => write!(f, "[{code}] Subscription error: {e}"),
            Self::ApiClientBuild(e) => write!(f, "[{code}] API client build error: {e}"),
            Self::Grpc(e) => write!(f, "[{code}] gRPC error: {e}"),
            Self::AddressValidation(e) => write!(f, "[{code}] Address validation error: {e}"),
            Self::LogInit(e) => write!(f, "[{code}] Log initialization error: {e}"),
            Self::ReloadLog(e) => write!(f, "[{code}] Log reload error: {e}"),
            Self::Log(s) => write!(f, "[{code}] Log error: {s}"),
            Self::Expired(e) => write!(f, "[{code}] Expired: {e}"),
            Self::BackendBuilder(e) => write!(f, "[{code}] Backend builder error: {e}"),
        }
    }
}

impl std::error::Error for GenericError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Client(e) => Some(e),
            Self::ClientBuilder(e) => Some(e),
            Self::Storage(e) => Some(e),
            Self::GroupError(e) => Some(e),
            Self::Signature(e) => Some(e),
            Self::GroupMetadata(e) => Some(e),
            Self::GroupMutablePermissions(e) => Some(e),
            Self::Generic { .. } => None,
            Self::SignatureRequestError(e) => Some(e),
            Self::Erc1271SignatureError(e) => Some(e),
            Self::Verifier(e) => Some(e),
            Self::FailedToConvertToU32 => None,
            Self::Association(e) => Some(e),
            Self::DeviceSync(e) => Some(e),
            Self::Identity(e) => Some(e),
            Self::JoinError(e) => Some(e),
            Self::IoError(e) => Some(e),
            Self::Subscription(e) => Some(e),
            Self::ApiClientBuild(e) => Some(e),
            Self::Grpc(e) => Some(e.as_ref()),
            Self::AddressValidation(e) => Some(e),
            Self::LogInit(e) => Some(e),
            Self::ReloadLog(e) => Some(e),
            Self::Log(_) => None,
            Self::Expired(e) => Some(e),
            Self::BackendBuilder(e) => Some(e),
        }
    }
}

// From implementations for each wrapped error type
impl From<xmtp_mls::client::ClientError> for GenericError {
    fn from(e: xmtp_mls::client::ClientError) -> Self {
        Self::Client(e)
    }
}

impl From<xmtp_mls::builder::ClientBuilderError> for GenericError {
    fn from(e: xmtp_mls::builder::ClientBuilderError) -> Self {
        Self::ClientBuilder(e)
    }
}

impl From<xmtp_db::StorageError> for GenericError {
    fn from(e: xmtp_db::StorageError) -> Self {
        Self::Storage(e)
    }
}

impl From<xmtp_mls::groups::GroupError> for GenericError {
    fn from(e: xmtp_mls::groups::GroupError) -> Self {
        Self::GroupError(e)
    }
}

impl From<xmtp_cryptography::signature::SignatureError> for GenericError {
    fn from(e: xmtp_cryptography::signature::SignatureError) -> Self {
        Self::Signature(e)
    }
}

impl From<GroupMetadataError> for GenericError {
    fn from(e: GroupMetadataError) -> Self {
        Self::GroupMetadata(e)
    }
}

impl From<xmtp_mls::groups::group_permissions::GroupMutablePermissionsError> for GenericError {
    fn from(e: xmtp_mls::groups::group_permissions::GroupMutablePermissionsError) -> Self {
        Self::GroupMutablePermissions(e)
    }
}

impl From<xmtp_id::associations::builder::SignatureRequestError> for GenericError {
    fn from(e: xmtp_id::associations::builder::SignatureRequestError) -> Self {
        Self::SignatureRequestError(e)
    }
}

impl From<xmtp_id::associations::signature::SignatureError> for GenericError {
    fn from(e: xmtp_id::associations::signature::SignatureError) -> Self {
        Self::Erc1271SignatureError(e)
    }
}

impl From<xmtp_id::scw_verifier::VerifierError> for GenericError {
    fn from(e: xmtp_id::scw_verifier::VerifierError) -> Self {
        Self::Verifier(e)
    }
}

impl From<xmtp_id::associations::AssociationError> for GenericError {
    fn from(e: xmtp_id::associations::AssociationError) -> Self {
        Self::Association(e)
    }
}

impl From<xmtp_mls::groups::device_sync::DeviceSyncError> for GenericError {
    fn from(e: xmtp_mls::groups::device_sync::DeviceSyncError) -> Self {
        Self::DeviceSync(e)
    }
}

impl From<xmtp_mls::identity::IdentityError> for GenericError {
    fn from(e: xmtp_mls::identity::IdentityError) -> Self {
        Self::Identity(e)
    }
}

impl From<tokio::task::JoinError> for GenericError {
    fn from(e: tokio::task::JoinError) -> Self {
        Self::JoinError(e)
    }
}

impl From<tokio::io::Error> for GenericError {
    fn from(e: tokio::io::Error) -> Self {
        Self::IoError(e)
    }
}

impl From<xmtp_mls::subscriptions::SubscribeError> for GenericError {
    fn from(e: xmtp_mls::subscriptions::SubscribeError) -> Self {
        Self::Subscription(e)
    }
}

impl From<xmtp_api_grpc::error::GrpcBuilderError> for GenericError {
    fn from(e: xmtp_api_grpc::error::GrpcBuilderError) -> Self {
        Self::ApiClientBuild(e)
    }
}

impl From<Box<xmtp_api_grpc::error::GrpcError>> for GenericError {
    fn from(e: Box<xmtp_api_grpc::error::GrpcError>) -> Self {
        Self::Grpc(e)
    }
}

impl From<IdentifierValidationError> for GenericError {
    fn from(e: IdentifierValidationError) -> Self {
        Self::AddressValidation(e)
    }
}

impl From<tracing_appender::rolling::InitError> for GenericError {
    fn from(e: tracing_appender::rolling::InitError) -> Self {
        Self::LogInit(e)
    }
}

impl From<tracing_subscriber::reload::Error> for GenericError {
    fn from(e: tracing_subscriber::reload::Error) -> Self {
        Self::ReloadLog(e)
    }
}

impl From<Expired> for GenericError {
    fn from(e: Expired) -> Self {
        Self::Expired(e)
    }
}

impl From<MessageBackendBuilderError> for GenericError {
    fn from(e: MessageBackendBuilderError) -> Self {
        Self::BackendBuilder(e)
    }
}

// this impl allows us to gracefully handle unexpected errors from foreign code without panicking
impl From<uniffi::UnexpectedUniFFICallbackError> for GenericError {
    fn from(e: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Generic { err: e.to_string() }
    }
}

#[derive(uniffi::Error, Debug, xmtp_macro::ErrorCode)]
#[uniffi(flat_error)]
pub enum FfiSubscribeError {
    #[error_code(inherit)]
    Subscribe(xmtp_mls::subscriptions::SubscribeError),
    #[error_code(inherit)]
    Storage(xmtp_db::StorageError),
}

impl std::fmt::Display for FfiSubscribeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let code = self.error_code();
        match self {
            Self::Subscribe(e) => write!(f, "[{code}] Subscribe error: {e}"),
            Self::Storage(e) => write!(f, "[{code}] Storage error: {e}"),
        }
    }
}

impl std::error::Error for FfiSubscribeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Subscribe(e) => Some(e),
            Self::Storage(e) => Some(e),
        }
    }
}

impl From<xmtp_mls::subscriptions::SubscribeError> for FfiSubscribeError {
    fn from(e: xmtp_mls::subscriptions::SubscribeError) -> Self {
        Self::Subscribe(e)
    }
}

impl From<xmtp_db::StorageError> for FfiSubscribeError {
    fn from(e: xmtp_db::StorageError) -> Self {
        Self::Storage(e)
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

    pub fn from_error_with_code<T: Error + ErrorCode>(err: T) -> Self {
        Self::Generic {
            err: stringify_error_chain_with_code(&err),
        }
    }
}

fn stringify_error_chain<T: Error>(error: &T) -> String {
    let mut result = format!("Error: {}\n", error);

    let mut source = error.source();
    while let Some(src) = source {
        result += &format!("Caused by: {}\n", src);
        source = src.source();
    }

    result
}

fn stringify_error_chain_with_code<T: Error + ErrorCode>(error: &T) -> String {
    let mut result = format!("[{}] Error: {}\n", error.error_code(), error);

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
    use crate::{get_version_info, GenericError};
    use xmtp_common::ErrorCode;

    #[test]
    pub fn test_get_version_info() {
        print!("{}", get_version_info());
    }

    #[test]
    pub fn test_error_code_format() {
        // Test that GenericError has [ErrorCode] prefix in Display
        let error = GenericError::FailedToConvertToU32;
        let error_string = error.to_string();

        // Verify the format is [ErrorType::Variant] message
        assert!(
            error_string.starts_with("[GenericError::FailedToConvertToU32]"),
            "Error should start with error code prefix, got: {}",
            error_string
        );
        assert!(
            error_string.contains("Failed to convert to u32"),
            "Error should contain message"
        );
    }

    #[test]
    pub fn test_error_code_inherited() {
        // Test that inherited error codes are properly propagated
        let inner = xmtp_cryptography::signature::IdentifierValidationError::InvalidAddresses(
            vec!["invalid".to_string()],
        );
        let error = GenericError::AddressValidation(inner);
        let error_string = error.to_string();

        // With #[error_code(inherit)], the error code comes from the inner error
        assert!(
            error_string.starts_with("[IdentifierValidationError::InvalidAddresses]"),
            "Error should inherit error code from inner error, got: {}",
            error_string
        );
    }

    #[test]
    pub fn test_error_code_trait() {
        // Test that error_code() returns the expected value
        let error = GenericError::Generic {
            err: "test error".to_string(),
        };
        assert_eq!(error.error_code(), "GenericError::Generic");
    }

    // Execute once before any tests are run
    #[ctor::ctor]
    fn _setup() {
        let _ = fdlimit::raise_fd_limit();
    }
}
