use std::error::Error;
use xmtp::types::Address;
use xmtp_components::TonicApiClient;
use xmtp_cryptography::utils::rng;
use xmtp_cryptography::utils::LocalWallet;
use xmtp_networking::grpc_api_helper::Client as TonicApiClient;

pub type RustXmtpClient = xmtp::Client<TonicApiClient>;
uniffi::include_scaffolding!("xmtpv3");

#[derive(uniffi::Error, Debug)]
pub enum GenericError {
    Generic { err: String },
}

impl From<String> for GenericError {
    fn from(err: String) -> Self {
        Self::Generic { err }
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

#[uniffi::export(async_runtime = "tokio")]
pub async fn create_client(
    // TODO Plumb InboxOwner down from foreign language
    host: String,
    is_secure: bool,
    // TODO proper error handling
) -> Result<Arc<FfiXmtpClient>, GenericError> {
    let wallet = LocalWallet::new(&mut rng());
    let api_client = TonicApiClient::create(host, is_secure)
        .await
        .map_err(|e| stringify_error_chain(&e))?;

    let mut xmtp_client: RustXmtpClient = xmtp::ClientBuilder::new(wallet.into())
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
    use crate::create_client;

    // Try a query on a test topic, and make sure we get a response
    #[tokio::test]
    async fn test_client_creation() {
        let client = create_client(xmtp_networking::LOCALHOST_ADDRESS.to_string(), false)
            .await
            .unwrap();
        assert!(!client.wallet_address().is_empty());
    }
}
