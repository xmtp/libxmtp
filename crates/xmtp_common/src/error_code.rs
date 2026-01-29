//! Unique error codes for cross-binding error identification.
//!
//! This module provides the `ErrorCode` trait which gives errors a stable,
//! machine-readable identifier that can be used across language bindings.
//!
//! # Example
//!
//! ```ignore
//! use xmtp_common::ErrorCode;
//! use thiserror::Error;
//!
//! #[derive(Debug, Error, ErrorCode)]
//! pub enum GroupError {
//!     #[error("Group not found")]
//!     NotFound,  // Returns "GroupError::NotFound"
//!
//!     #[error("Storage error: {0}")]
//!     #[error_code(inherit)]  // Delegates to StorageError::error_code()
//!     Storage(#[from] StorageError),
//! }
//! ```

/// A trait for errors that have a unique, stable error code.
///
/// Error codes are formatted as `"TypeName::VariantName"` for enum variants
/// or `"TypeName"` for struct errors.
///
/// Use `#[derive(ErrorCode)]` from `xmtp_macro` to automatically implement this trait.
pub trait ErrorCode: std::error::Error {
    /// Returns the unique error code for this error.
    ///
    /// The code is a static string in the format `"TypeName::VariantName"`.
    fn error_code(&self) -> &'static str;
}

impl<E: ErrorCode> ErrorCode for Box<E> {
    fn error_code(&self) -> &'static str {
        (**self).error_code()
    }
}

impl<E: ErrorCode> ErrorCode for &E {
    fn error_code(&self) -> &'static str {
        (*self).error_code()
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

// Manual implementation for external hex crate error
impl ErrorCode for hex::FromHexError {
    fn error_code(&self) -> &'static str {
        "hex::FromHexError"
    }
}

#[cfg(test)]
mod tests {
    use super::ErrorCode;
    use thiserror::Error;
    use xmtp_macro::ErrorCode;

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

    // Tests for manual implementations of external types

    #[test]
    fn test_signature_error_codes() {
        use xmtp_cryptography::signature::SignatureError;

        // BadAddressFormat wraps hex::FromHexError
        let err = SignatureError::BadAddressFormat(hex::FromHexError::OddLength);
        assert_eq!(err.error_code(), "SignatureError::BadAddressFormat");

        // BadSignature has an addr field
        let err = SignatureError::BadSignature {
            addr: "0x123".to_string(),
        };
        assert_eq!(err.error_code(), "SignatureError::BadSignature");

        let err = SignatureError::Unknown;
        assert_eq!(err.error_code(), "SignatureError::Unknown");
    }

    #[test]
    fn test_identifier_validation_error_codes() {
        use xmtp_cryptography::signature::IdentifierValidationError;

        let err = IdentifierValidationError::InvalidAddresses(vec!["bad".to_string()]);
        assert_eq!(
            err.error_code(),
            "IdentifierValidationError::InvalidAddresses"
        );

        let err = IdentifierValidationError::HexDecode(hex::FromHexError::OddLength);
        assert_eq!(err.error_code(), "IdentifierValidationError::HexDecode");

        let err = IdentifierValidationError::Generic("generic error".to_string());
        assert_eq!(err.error_code(), "IdentifierValidationError::Generic");
    }

    #[test]
    fn test_ethereum_crypto_error_codes() {
        use xmtp_cryptography::ethereum::EthereumCryptoError;

        let err = EthereumCryptoError::InvalidLength;
        assert_eq!(err.error_code(), "EthereumCryptoError::InvalidLength");

        let err = EthereumCryptoError::InvalidKey;
        assert_eq!(err.error_code(), "EthereumCryptoError::InvalidKey");

        let err = EthereumCryptoError::SignFailure;
        assert_eq!(err.error_code(), "EthereumCryptoError::SignFailure");

        let err = EthereumCryptoError::DecompressFailure;
        assert_eq!(err.error_code(), "EthereumCryptoError::DecompressFailure");
    }

    #[test]
    fn test_hex_from_hex_error_code() {
        let err = hex::FromHexError::OddLength;
        assert_eq!(err.error_code(), "hex::FromHexError");

        let err = hex::FromHexError::InvalidHexCharacter { c: 'Z', index: 0 };
        assert_eq!(err.error_code(), "hex::FromHexError");

        let err = hex::FromHexError::InvalidStringLength;
        assert_eq!(err.error_code(), "hex::FromHexError");
    }
}
