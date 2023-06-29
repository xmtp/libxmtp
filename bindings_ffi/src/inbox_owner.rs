// TODO proper error handling
#[derive(Debug, thiserror::Error)]
pub enum SigningError {
    #[error("This is a generic error")]
    Generic,
}

impl From<uniffi::UnexpectedUniFFICallbackError> for SigningError {
    fn from(_: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Generic
    }
}

// A simplified InboxOwner passed to Rust across the FFI boundary
pub trait FfiInboxOwner: Send + Sync {
    fn get_address(&self) -> String;
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

impl xmtp::InboxOwner for RustInboxOwner {
    fn get_address(&self) -> String {
        self.ffi_inbox_owner.get_address()
    }

    fn sign(
        &self,
        text: &str,
    ) -> Result<
        xmtp_cryptography::signature::RecoverableSignature,
        xmtp_cryptography::signature::SignatureError,
    > {
        let bytes = self
            .ffi_inbox_owner
            .sign(text.to_string())
            .map_err(|_flat_err| xmtp_cryptography::signature::SignatureError::Unknown)?;
        Ok(xmtp_cryptography::signature::RecoverableSignature::Eip191Signature(bytes))
    }
}
