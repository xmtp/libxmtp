use crate::identity::FfiIdentifier;
use xmtp_cryptography::signature::{IdentifierValidationError, SignatureError};
use xmtp_id::associations::{
    Identifier,
    unverified::{UnverifiedRecoverableEcdsaSignature, UnverifiedSignature},
};

// TODO proper error handling
#[derive(uniffi::Error, Debug, thiserror::Error)]
pub enum SigningError {
    #[error("This is a generic error")]
    Generic,
}

#[derive(uniffi::Error, Debug, thiserror::Error)]
pub enum IdentityValidationError {
    #[error("Error: {0:?}")]
    Generic(String),
}

impl From<uniffi::UnexpectedUniFFICallbackError> for SigningError {
    fn from(_: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Generic
    }
}

// A simplified InboxOwner passed to Rust across the FFI boundary
#[uniffi::export(with_foreign)]
pub trait FfiInboxOwner: Send + Sync {
    fn get_identifier(&self) -> Result<FfiIdentifier, IdentityValidationError>;
    fn sign(&self, text: String) -> Result<Vec<u8>, SigningError>;
}

pub struct RustInboxOwner {
    ffi_inbox_owner: Box<dyn FfiInboxOwner>,
}

impl RustInboxOwner {
    pub fn new(ffi_inbox_owner: Box<dyn FfiInboxOwner>) -> Self {
        Self { ffi_inbox_owner }
    }
}

impl xmtp_mls::InboxOwner for RustInboxOwner {
    fn get_identifier(&self) -> Result<Identifier, IdentifierValidationError> {
        let ident = self
            .ffi_inbox_owner
            .get_identifier()
            .map_err(|err| IdentifierValidationError::Generic(err.to_string()))?;
        ident.try_into()
    }

    fn sign(&self, text: &str) -> Result<UnverifiedSignature, SignatureError> {
        let bytes = self
            .ffi_inbox_owner
            .sign(text.to_string())
            .map_err(|_flat_err| SignatureError::Unknown)?;
        Ok(UnverifiedSignature::RecoverableEcdsa(
            UnverifiedRecoverableEcdsaSignature::new(bytes),
        ))
    }
}
