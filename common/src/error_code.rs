//! Unique error codes for cross-binding error identification.

use thiserror::Error;
use xmtp_macro::ErrorCode;

// Wrapper error types for external crate errors.
//
// We cannot implement `ErrorCode` for external types like `hex::FromHexError`
// due to Rust's orphan rule, so we wrap them in types we own.

/// Wrapper for `hex::FromHexError`.
#[cfg(feature = "error-code")]
#[derive(Debug, Error, ErrorCode)]
pub enum HexError {
    #[error(transparent)]
    Decode(#[from] hex::FromHexError),
}

/// Error during logging initialization.
#[cfg(feature = "error-code")]
#[derive(Debug, Error, ErrorCode)]
pub enum LoggingError {
    #[error("logging initialization failed: {0}")]
    Init(String),
}

/// A trait for errors that have a unique, stable error code.
///
/// Use `#[derive(ErrorCode)]` to automatically implement this trait.
pub trait ErrorCode: std::error::Error {
    /// Returns the unique error code for this error.
    fn error_code(&self) -> &'static str;
}

impl<E: ErrorCode> ErrorCode for Box<E> {
    fn error_code(&self) -> &'static str {
        (**self).error_code()
    }
}

// Manual implementations for xmtp_cryptography errors.
// These cannot use the derive macro due to circular dependency issues.

impl ErrorCode for xmtp_cryptography::signature::SignatureError {
    fn error_code(&self) -> &'static str {
        use xmtp_cryptography::signature::SignatureError;
        match self {
            SignatureError::BadAddressFormat(_) => "SignatureError::BadAddressFormat",
            SignatureError::BadSignatureFormat(_) => "SignatureError::BadSignatureFormat",
            SignatureError::BadSignature { .. } => "SignatureError::BadSignature",
            SignatureError::Signer(_) => "SignatureError::Signer",
            SignatureError::Unknown => "SignatureError::Unknown",
        }
    }
}

impl ErrorCode for xmtp_cryptography::signature::IdentifierValidationError {
    fn error_code(&self) -> &'static str {
        use xmtp_cryptography::signature::IdentifierValidationError;
        match self {
            IdentifierValidationError::InvalidAddresses(_) => {
                "IdentifierValidationError::InvalidAddresses"
            }
            IdentifierValidationError::HexDecode(_) => "IdentifierValidationError::HexDecode",
            IdentifierValidationError::Generic(_) => "IdentifierValidationError::Generic",
        }
    }
}

impl ErrorCode for xmtp_cryptography::ethereum::EthereumCryptoError {
    fn error_code(&self) -> &'static str {
        use xmtp_cryptography::ethereum::EthereumCryptoError;
        match self {
            EthereumCryptoError::InvalidLength => "EthereumCryptoError::InvalidLength",
            EthereumCryptoError::InvalidKey => "EthereumCryptoError::InvalidKey",
            EthereumCryptoError::SignFailure => "EthereumCryptoError::SignFailure",
            EthereumCryptoError::DecompressFailure => "EthereumCryptoError::DecompressFailure",
        }
    }
}

// Manual implementations for prost errors.
// These cannot use the derive macro because they are external types.

#[cfg(feature = "error-code")]
impl ErrorCode for prost::EncodeError {
    fn error_code(&self) -> &'static str {
        "prost::EncodeError"
    }
}

#[cfg(feature = "error-code")]
impl ErrorCode for prost::DecodeError {
    fn error_code(&self) -> &'static str {
        "prost::DecodeError"
    }
}

#[cfg(test)]
mod tests {
    use crate::ErrorCode;
    use thiserror::Error;

    #[derive(Debug, Error, ErrorCode)]
    #[error("inner error")]
    struct InnerError;

    #[derive(Debug, Error, ErrorCode)]
    enum StorageError {
        #[error("connection failed")]
        Connection,
        #[error("not found")]
        NotFound,
    }

    #[derive(Debug, Error, ErrorCode)]
    enum GroupError {
        #[error("group not found")]
        NotFound,
        #[error("invalid membership")]
        InvalidMembership,
        #[error("storage: {0}")]
        #[error_code(inherit)]
        Storage(#[from] StorageError),
        #[error("inner: {0}")]
        #[error_code(inherit)]
        Inner(#[from] InnerError),
    }

    #[test]
    fn test_struct_error_code() {
        let err = InnerError;
        assert_eq!(err.error_code(), "InnerError");
    }

    #[test]
    fn test_enum_error_code() {
        let err = StorageError::Connection;
        assert_eq!(err.error_code(), "StorageError::Connection");

        let err = StorageError::NotFound;
        assert_eq!(err.error_code(), "StorageError::NotFound");
    }

    #[test]
    fn test_inherited_error_code() {
        let err = GroupError::NotFound;
        assert_eq!(err.error_code(), "GroupError::NotFound");

        let err = GroupError::InvalidMembership;
        assert_eq!(err.error_code(), "GroupError::InvalidMembership");

        // Inherited from StorageError
        let err = GroupError::Storage(StorageError::Connection);
        assert_eq!(err.error_code(), "StorageError::Connection");

        // Inherited from InnerError (struct)
        let err = GroupError::Inner(InnerError);
        assert_eq!(err.error_code(), "InnerError");
    }

    #[test]
    fn test_boxed_error_code() {
        let err = Box::new(StorageError::Connection);
        assert_eq!(err.error_code(), "StorageError::Connection");
    }

    #[test]
    fn test_ref_error_code() {
        let err = StorageError::Connection;
        let err_ref = &err;
        assert_eq!(err_ref.error_code(), "StorageError::Connection");
    }

    // Test custom code override for backwards compatibility
    #[derive(Debug, Error, ErrorCode)]
    enum RenamedError {
        #[error("new name for the variant")]
        #[error_code("RenamedError::OldVariantName")]
        NewVariantName,
        #[error("another variant")]
        AnotherVariant,
    }

    #[test]
    fn test_custom_error_code() {
        // Custom code preserves backwards compatibility
        let err = RenamedError::NewVariantName;
        assert_eq!(err.error_code(), "RenamedError::OldVariantName");

        // Default code generation still works
        let err = RenamedError::AnotherVariant;
        assert_eq!(err.error_code(), "RenamedError::AnotherVariant");
    }
}
