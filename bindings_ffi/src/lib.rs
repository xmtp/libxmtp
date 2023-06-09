mod tonic_api_client;

use std::sync::Arc;

use tonic_api_client::TonicApiClient;
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
