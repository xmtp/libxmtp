use flutter_rust_bridge::*;
use std::sync::Arc;
use thiserror::Error;
use xmtp_api_grpc::grpc_api_helper::Client as ApiClient;
use xmtp_cryptography::utils::LocalWallet;
pub use xmtp_mls::builder::ClientBuilderError;
pub use xmtp_mls::storage::StorageError;
use xmtp_mls::{
    builder::ClientBuilder, builder::IdentityStrategy, builder::LegacyIdentity, client::Client as MlsClient,
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
    #[error("GroupError: {0}")]
    GroupError(#[from] xmtp_mls::groups::GroupError),
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

#[frb(opaque)]
pub struct Client {
    // We use this second wrapper to keep the inner client opaque.
    pub inner: Arc<InnerClient>,
}

pub struct Group {
    pub group_id: Vec<u8>,
    pub created_at_ns: i64,
}

#[frb(opaque)]
pub struct InnerClient {
    pub client: MlsClient<ApiClient>,
}

impl Client {
    pub fn installation_public_key(&self) -> Vec<u8> {
        self.inner.client.installation_public_key()
    }

    pub async fn list_groups(
        &self,
        created_after_ns: Option<i64>,
        created_before_ns: Option<i64>,
        limit: Option<i64>,
    ) -> Result<Vec<Group>, XmtpError> {
        self.inner.client.sync_welcomes().await?;
        let groups: Vec<Group> = self
            .inner
            .client
            .find_groups(None, created_after_ns, created_before_ns, limit)?
            .into_iter()
            .map(|group| Group {
                group_id: group.group_id,
                created_at_ns: group.created_at_ns,
            })
            .collect();
        return Ok(groups);
    }

    pub async fn create_group(&self, account_addresses: Vec<String>) -> Result<Group, XmtpError> {
        let group = self.inner.client.create_group(None)?;
        // TODO: consider filtering self address from the list
        if !account_addresses.is_empty() {
            group.add_members(account_addresses).await?;
        }
        self.inner.client.sync_welcomes().await?;
        return Ok(Group {
            group_id: group.group_id,
            created_at_ns: group.created_at_ns,
        });
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
            .register_identity(Some(signature))
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
    let identity_strategy: IdentityStrategy =
        IdentityStrategy::CreateIfNotFound(account_address, LegacyIdentity::None); // TODO plumb legacy identity here
    let xmtp_client = ClientBuilder::new(identity_strategy)
        .api_client(api_client)
        .store(store)
        .build()
        .await?;

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

    #[tokio::test]
    async fn test_listing_groups() {
        let wallet_a = LocalWallet::new(&mut xmtp_cryptography::utils::rng());
        let wallet_b = LocalWallet::new(&mut xmtp_cryptography::utils::rng());
        let wallet_c = LocalWallet::new(&mut xmtp_cryptography::utils::rng());
        let client_a = create_client_for_wallet(&wallet_a).await;
        let client_b = create_client_for_wallet(&wallet_b).await;
        let client_c = create_client_for_wallet(&wallet_c).await;
        for client in vec![&client_a, &client_b, &client_c] {
            assert_eq!(client.list_groups(None, None, None).await.unwrap().len(), 0);
        }

        // When user A creates a group with B and C
        let group = client_a
            .create_group(vec![wallet_b.get_address(), wallet_c.get_address()])
            .await
            .unwrap();

        // Wait a minute for the group to propagate
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

        // Now users A, B and C should all see the group
        for client in vec![&client_a, &client_b, &client_c] {
            let groups = client.list_groups(None, None, None).await.unwrap();
            assert_eq!(groups.len(), 1);
            assert_eq!(groups.first().unwrap().group_id, group.group_id);
        }
    }

    // Helpers

    async fn create_client_for_wallet(wallet: &LocalWallet) -> Client {
        let db_path = format!(
            "{}/{}.db3",
            std::env::temp_dir().to_str().unwrap(),
            wallet.get_address()
        );
        let account_address = wallet.get_address();

        // When we first create the client it should require a signature.
        let created = create_client(
            "http://localhost:5556".to_string(),
            false,
            db_path.clone(),
            [2u8; 32],
            account_address.clone(),
        )
        .await
        .unwrap();
        let req = match created {
            CreatedClient::RequiresSignature(req) => req,
            _ => panic!("it should require a signature"),
        };
        let sig = wallet.sign(req.text_to_sign.as_str()).unwrap();
        let RecoverableSignature::Eip191Signature(sig_bytes) = sig;
        return req.sign(sig_bytes).await.unwrap();
    }
}
