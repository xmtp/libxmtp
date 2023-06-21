use std::error::Error;
use std::sync::Arc;
use xmtp::types::Address;
use xmtp_networking::grpc_api_helper::Client as TonicApiClient;

pub type RustXmtpClient = xmtp::Client<TonicApiClient>;
uniffi::include_scaffolding!("xmtpv3");

#[derive(uniffi::Error, Debug)]
#[uniffi(handle_unknown_callback_error)]
pub enum GenericError {
    Generic { err: String },
}

impl From<String> for GenericError {
    fn from(err: String) -> Self {
        Self::Generic { err }
    }
}

impl From<uniffi::UnexpectedUniFFICallbackError> for GenericError {
    fn from(e: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Generic { err: e.reason }
    }
}

// TODO Use non-string errors across Uniffi interface
fn stringify_error_chain(error: &(dyn Error + 'static)) -> String {
    let mut result = format!("Error: {}\n", error);

    let mut source = error.source();
    while let Some(src) = source {
        result += &format!("Caused by: {}\n", src);
        source = src.source();
    }

    result
}

// A simplified InboxOwner passed to Rust across the FFI boundary
#[uniffi::export(callback_interface)]
pub trait FfiInboxOwner: Send + Sync {
    fn get_address(&self) -> String;
    fn sign(&self, text: String) -> Result<Vec<u8>, GenericError>;
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

#[uniffi::export(async_runtime = "tokio")]
pub async fn create_client(
    ffi_inbox_owner: Box<dyn FfiInboxOwner>,
    host: String,
    is_secure: bool,
    // TODO proper error handling
) -> Result<Arc<FfiXmtpClient>, GenericError> {
    let inbox_owner = RustInboxOwner::new(ffi_inbox_owner);
    let api_client = TonicApiClient::create(host, is_secure)
        .await
        .map_err(|e| stringify_error_chain(&e))?;

    let mut xmtp_client: RustXmtpClient = xmtp::ClientBuilder::new(inbox_owner.into())
        .api_client(api_client)
        .build()
        .map_err(|e| stringify_error_chain(&e))?;
    xmtp_client
        .init()
        .await
        .map_err(|e| stringify_error_chain(&e))?;
    Ok(Arc::new(FfiXmtpClient {
        inner_client: xmtp_client,
    }))
}

#[derive(uniffi::Object)]
pub struct FfiXmtpClient {
    inner_client: RustXmtpClient,
}

#[uniffi::export]
impl FfiXmtpClient {
    pub fn wallet_address(&self) -> Address {
        self.inner_client.wallet_address()
    }
}

#[cfg(test)]
mod tests {
    use crate::{create_client, FfiInboxOwner, GenericError};
    use xmtp::InboxOwner;
    use xmtp_cryptography::{signature::RecoverableSignature, utils::rng};

    pub struct LocalWalletInboxOwner {
        wallet: xmtp_cryptography::utils::LocalWallet,
    }

    impl LocalWalletInboxOwner {
        pub fn new() -> Self {
            Self {
                wallet: xmtp_cryptography::utils::LocalWallet::new(&mut rng()),
            }
        }
    }

    impl FfiInboxOwner for LocalWalletInboxOwner {
        fn get_address(&self) -> String {
            self.wallet.get_address()
        }

        fn sign(&self, text: String) -> Result<Vec<u8>, GenericError> {
            let recoverable_signature = self.wallet.sign(&text).map_err(|err| err.to_string())?;
            match recoverable_signature {
                RecoverableSignature::Eip191Signature(signature_bytes) => Ok(signature_bytes),
            }
        }
    }

    // Try a query on a test topic, and make sure we get a response
    #[tokio::test]
    async fn test_client_creation() {
        let ffi_inbox_owner = LocalWalletInboxOwner::new();
        let client = create_client(
            Box::new(ffi_inbox_owner),
            xmtp_networking::LOCALHOST_ADDRESS.to_string(),
            false,
        )
        .await
        .unwrap();
        assert!(!client.wallet_address().is_empty());
    }
}
