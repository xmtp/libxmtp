// TODO proper error handling
#[derive(Debug, thiserror::Error)]
pub enum SigningError {
    #[error("This is a generic error")]
    Generic,
}

// impl From<uniffi::UnexpectedUniFFICallbackError> for SigningError {
//     fn from(_: uniffi::UnexpectedUniFFICallbackError) -> Self {
//         Self::Generic
//     }
// }

// A simplified InboxOwner passed to Rust across the FFI boundary
pub trait WasmInboxOwner: Send + Sync {
    fn get_address(&self) -> String;
    fn sign(&self, text: String) -> Result<Vec<u8>, SigningError>;
}

pub struct RustInboxOwner {
    wasm_inbox_owner: Box<dyn WasmInboxOwner>,
}

impl RustInboxOwner {
    pub fn new(wasm_inbox_owner: Box<dyn WasmInboxOwner>) -> Self {
        Self { wasm_inbox_owner }
    }
}

impl xmtp_mls::InboxOwner for RustInboxOwner {
    fn get_address(&self) -> String {
        self.wasm_inbox_owner.get_address().to_lowercase()
    }

    fn sign(
        &self,
        text: &str,
    ) -> Result<
        xmtp_cryptography::signature::RecoverableSignature,
        xmtp_cryptography::signature::SignatureError,
    > {
        let bytes = self
            .wasm_inbox_owner
            .sign(text.to_string())
            .map_err(|_flat_err| xmtp_cryptography::signature::SignatureError::Unknown)?;
        Ok(xmtp_cryptography::signature::RecoverableSignature::Eip191Signature(bytes))
    }
}
