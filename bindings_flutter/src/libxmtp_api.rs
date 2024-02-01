use flutter_rust_bridge::*;
use std::sync::Arc;
use thiserror::Error;
use xmtp_api_grpc::grpc_api_helper::Client as ApiClient;
use xmtp_cryptography::utils::LocalWallet;
pub use xmtp_mls::builder::ClientBuilderError;
pub use xmtp_mls::storage::StorageError;
use xmtp_mls::{
    builder::ClientBuilder, builder::IdentityStrategy, client::Client as MlsClient,
    storage::EncryptedMessageStore, storage::StorageOption,
};
pub use xmtp_proto::api_client::Error as ApiError;

#[derive(Error, Debug)]
pub enum XmtpError {
    #[error("ApiError: {0}")]
    ApiError(#[from] ApiError),
    #[error("ClientBuildError: {0}")]
    ClientBuilderError(#[from] ClientBuilderError),
    #[error("ClientError: {0}")]
    ClientError(#[from] xmtp_mls::client::ClientError),
    #[error("StorageError: {0}")]
    StorageError(#[from] StorageError),
    #[error("GenericError: {0}")]
    Generic(#[from] anyhow::Error),
}

pub fn generate_private_preferences_topic_identifier(
    private_key_bytes: Vec<u8>,
) -> Result<String, XmtpError> {
    xmtp_user_preferences::topic::generate_private_preferences_topic_identifier(
        private_key_bytes.as_slice(),
    )
    .map_err(|e| XmtpError::Generic(anyhow::Error::msg(e)))
}

pub fn user_preferences_encrypt(
    public_key: Vec<u8>,
    private_key: Vec<u8>,
    message: Vec<u8>,
) -> Result<Vec<u8>, XmtpError> {
    xmtp_user_preferences::encrypt_message(
        public_key.as_slice(),
        private_key.as_slice(),
        message.as_slice(),
    )
    .map_err(|e| XmtpError::Generic(anyhow::Error::msg(e)))
}

pub fn user_preferences_decrypt(
    public_key: Vec<u8>,
    private_key: Vec<u8>,
    encrypted_message: Vec<u8>,
) -> Result<Vec<u8>, XmtpError> {
    xmtp_user_preferences::decrypt_message(
        public_key.as_slice(),
        private_key.as_slice(),
        encrypted_message.as_slice(),
    )
    .map_err(|e| XmtpError::Generic(anyhow::Error::msg(e)))
}

pub struct Client {
    // We use this second wrapper to keep the inner client opaque.
    pub inner: Arc<InnerClient>,
}

#[frb(opaque)]
pub struct InnerClient {
    pub client: MlsClient<ApiClient>,
}

impl Client {
    pub fn installation_public_key(&self) -> Vec<u8> {
        self.inner.client.installation_public_key()
    }
}

pub enum CreatedClient {
    Ready(Client),
    RequiresSignature(SignatureRequiredClient),
}

pub struct SignatureRequiredClient {
    pub text_to_sign: String,
    pub inner: Arc<InnerClient>,
}

impl SignatureRequiredClient {
    pub async fn sign(&self, signature: Vec<u8>) -> Result<Client, XmtpError> {
        self.inner
            .client
            .register_identity_with_external_signature(Some(signature))
            .await?;
        return Ok(Client {
            inner: self.inner.clone(),
        });
    }
}

pub async fn create_client(
    // logger_fn: impl Fn(u32, String, String) -> DartFnFuture<()>,
    host: String,
    is_secure: bool,
    db_path: String,
    encryption_key: [u8; 32],
    account_address: String,
    // legacy_identity_source: LegacyIdentitySource,
    // legacy_signed_private_key_proto: Option<Vec<u8>>,
) -> Result<CreatedClient, XmtpError> {
    let api_client = ApiClient::create(host.clone(), is_secure).await?;
    let store = EncryptedMessageStore::new(StorageOption::Persistent(db_path), encryption_key)?;
    // log::info!("Creating XMTP client");
    let identity_strategy: IdentityStrategy<LocalWallet> =
        IdentityStrategy::CreateUnsignedIfNotFound(account_address);
    let xmtp_client = ClientBuilder::new(identity_strategy)
        .api_client(api_client)
        .store(store)
        .build()?;

    // log::info!(
    //     "Created XMTP client for address: {}",
    //     xmtp_client.account_address()
    // );
    let text_to_sign = xmtp_client.text_to_sign();
    let inner = Arc::new(InnerClient {
        client: xmtp_client,
    });
    if text_to_sign.is_none() {
        return Ok(CreatedClient::Ready(Client { inner }));
    }
    return Ok(CreatedClient::RequiresSignature(SignatureRequiredClient {
        text_to_sign: text_to_sign.unwrap(),
        inner,
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use xmtp_cryptography::signature::RecoverableSignature;
    use xmtp_cryptography::utils::LocalWallet;
    use xmtp_mls::InboxOwner;

    #[tokio::test]
    async fn test_create_client() {
        let wallet = LocalWallet::new(&mut xmtp_cryptography::utils::rng());
        let db_path = format!(
            "{}/{}.db3",
            std::env::temp_dir().to_str().unwrap(),
            wallet.get_address()
        );
        let account_address = wallet.get_address();

        // When we first create the client it should require a signature.
        let created_a = create_client(
            "http://localhost:5556".to_string(),
            false,
            db_path.clone(),
            [2u8; 32],
            account_address.clone(),
        )
        .await
        .unwrap();
        let req_a = match created_a {
            CreatedClient::RequiresSignature(req) => req,
            _ => panic!("it should require a signature"),
        };
        let sig_a = wallet.sign(req_a.text_to_sign.as_str()).unwrap();
        let RecoverableSignature::Eip191Signature(sig_a_bytes) = sig_a;
        let client_a = req_a.sign(sig_a_bytes).await.unwrap();

        // But when we re-created the same client it should not require a signature.
        let created_b = create_client(
            "http://localhost:5556".to_string(),
            false,
            db_path.clone(),
            [2u8; 32],
            account_address.clone(),
        )
        .await
        .unwrap();
        let client_b = match created_b {
            CreatedClient::Ready(client) => client,
            _ => panic!("it should already be ready without requiring a signature"),
        };

        assert_eq!(
            client_a.installation_public_key(),
            client_b.installation_public_key(),
            "both created clients should have the same installation key"
        );
    }
}
