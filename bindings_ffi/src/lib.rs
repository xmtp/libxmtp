mod tonic_api_client;

use std::sync::Arc;

use tokio::sync::Mutex;
use tonic_api_client::TonicApiClient;
use xmtp::contact::Contact;
use xmtp::types::Address;
use xmtp_cryptography::utils::rng;
use xmtp_cryptography::utils::LocalWallet;

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

#[uniffi::export(async_runtime = "tokio")]
pub async fn create_client(
    // TODO Plumb InboxOwner down from foreign language
    host: String,
    is_secure: bool,
    // TODO proper error handling
) -> Result<Arc<FfiXmtpClient>, GenericError> {
    let wallet = LocalWallet::new(&mut rng());
    let api_client = TonicApiClient::new(&host, is_secure).await?;

    let mut xmtp_client: RustXmtpClient = xmtp::ClientBuilder::new(wallet.into())
        .api_client(api_client)
        .build()
        .map_err(|e| format!("{:?}", e))?;
    xmtp_client.init().await.map_err(|e| e.to_string())?;
    Ok(Arc::new(FfiXmtpClient {
        inner_client: Mutex::new(xmtp_client),
    }))
}

#[derive(uniffi::Object)]
pub struct FfiXmtpClient {
    inner_client: Mutex<RustXmtpClient>,
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiXmtpClient {
    pub async fn wallet_address(&self) -> Address {
        let client = self.inner_client.lock().await;
        client.wallet_address()
    }

    pub async fn get_contacts(&self, wallet_address: String) -> Result<Vec<Contact>, GenericError> {
        let client = self.inner_client.lock().await;
        client
            .get_contacts(&wallet_address)
            .await
            .map_err(|e| e.into())
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
