use flutter_rust_bridge::*;
use std::sync::Arc;
use thiserror::Error;
pub use xmtp_api_grpc::grpc_api_helper::Client as ApiClient;
pub use xmtp_mls::builder::ClientBuilderError;
use xmtp_mls::storage::group_message::GroupMessageKind::Application;
use xmtp_mls::storage::group_message::StoredGroupMessage;
pub use xmtp_mls::storage::StorageError;
use xmtp_mls::{
    builder::ClientBuilder, builder::IdentityStrategy, builder::LegacyIdentity,
    client::Client as MlsClient, storage::EncryptedMessageStore, storage::StorageOption,
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

pub type XmtpClient = MlsClient<ApiClient>;

#[frb(opaque)]
pub struct Client {
    pub inner: Arc<XmtpClient>,
}

pub struct Group {
    pub group_id: Vec<u8>,
    pub created_at_ns: i64,
}

pub struct Message {
    pub id: Vec<u8>,
    pub sent_at_ns: i64,
    pub group_id: Vec<u8>,
    pub sender_account_address: String,
    pub content_bytes: Vec<u8>,
}

impl From<StoredGroupMessage> for Message {
    fn from(msg: StoredGroupMessage) -> Self {
        Self {
            id: msg.id,
            sent_at_ns: msg.sent_at_ns,
            group_id: msg.group_id,
            sender_account_address: msg.sender_account_address,
            content_bytes: msg.decrypted_message_bytes,
        }
    }
}

pub struct GroupMember {
    pub account_address: String,
    pub installation_ids: Vec<Vec<u8>>,
}

impl Client {
    pub fn installation_public_key(&self) -> Vec<u8> {
        self.inner.installation_public_key()
    }

    pub async fn list_groups(
        &self,
        created_after_ns: Option<i64>,
        created_before_ns: Option<i64>,
        limit: Option<i64>,
    ) -> Result<Vec<Group>, XmtpError> {
        self.inner.sync_welcomes().await?;
        let groups: Vec<Group> = self
            .inner
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
        let group = self.inner.create_group(None)?;
        // TODO: consider filtering self address from the list
        if !account_addresses.is_empty() {
            group.add_members(account_addresses).await?;
        }
        self.inner.sync_welcomes().await?;
        return Ok(Group {
            group_id: group.group_id,
            created_at_ns: group.created_at_ns,
        });
    }

    pub async fn is_active_group(&self, group_id: Vec<u8>) -> Result<bool, XmtpError> {
        self.inner.sync_welcomes().await?;
        let group = self.inner.group(group_id)?;
        group.sync().await?; // TODO: consider an explicit sync method
        Ok(group.is_active()?)
    }

    pub async fn list_members(&self, group_id: Vec<u8>) -> Result<Vec<GroupMember>, XmtpError> {
        self.inner.sync_welcomes().await?;
        let group = self.inner.group(group_id)?;
        group.sync().await?; // TODO: consider an explicit sync method
        let members: Vec<GroupMember> = group
            .members()?
            .into_iter()
            .map(|member| GroupMember {
                account_address: member.account_address,
                installation_ids: member.installation_ids,
            })
            .collect();

        Ok(members)
    }

    pub async fn add_member(
        &self,
        group_id: Vec<u8>,
        account_address: String,
    ) -> Result<(), XmtpError> {
        self.inner.sync_welcomes().await?;
        let group = self.inner.group(group_id)?;
        group.add_members(vec![account_address]).await?;
        group.sync().await?; // TODO: consider an explicit sync method
        Ok(())
    }

    pub async fn remove_member(
        &self,
        group_id: Vec<u8>,
        account_address: String,
    ) -> Result<(), XmtpError> {
        self.inner.sync_welcomes().await?;
        let group = self.inner.group(group_id)?;
        group.remove_members(vec![account_address]).await?;
        group.sync().await?; // TODO: consider an explicit sync method
        Ok(())
    }

    pub async fn send_message(
        &self,
        group_id: Vec<u8>,
        content_bytes: Vec<u8>,
    ) -> Result<(), XmtpError> {
        self.inner.sync_welcomes().await?;
        // TODO: consider verifying content_bytes is a serialized EncodedContent proto
        let group = self.inner.group(group_id)?;
        group.send_message(content_bytes.as_slice()).await?;
        group.sync().await?; // TODO: consider an explicit sync method
        Ok(())
    }

    pub async fn list_messages(
        &self,
        group_id: Vec<u8>,
        sent_before_ns: Option<i64>,
        sent_after_ns: Option<i64>,
        limit: Option<i64>,
    ) -> Result<Vec<Message>, XmtpError> {
        self.inner.sync_welcomes().await?;
        let group = self.inner.group(group_id)?;
        group.sync().await?; // TODO: consider an explicit sync method
        let messages: Vec<Message> = group
            .find_messages(Some(Application), sent_before_ns, sent_after_ns, limit)?
            .into_iter()
            .map(|msg| msg.into())
            .collect();

        Ok(messages)
    }
}

pub enum CreatedClient {
    Ready(Client),
    RequiresSignature(SignatureRequiredClient),
}

pub struct SignatureRequiredClient {
    pub text_to_sign: String,
    pub inner: Arc<XmtpClient>,
}

impl SignatureRequiredClient {
    pub async fn sign(&self, signature: Vec<u8>) -> Result<Client, XmtpError> {
        self.inner.register_identity(Some(signature)).await?;
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
    let inner = Arc::new(xmtp_client);
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
    use prost::Message;
    use xmtp_cryptography::signature::RecoverableSignature;
    use xmtp_cryptography::utils::LocalWallet;
    use xmtp_mls::codecs::text::TextCodec;
    use xmtp_mls::codecs::ContentCodec;
    use xmtp_mls::InboxOwner;
    use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

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
        delay_to_propagate().await;

        // Now users A, B and C should all see the group
        for client in vec![&client_a, &client_b, &client_c] {
            let groups = client.list_groups(None, None, None).await.unwrap();
            assert_eq!(groups.len(), 1);
            assert_eq!(groups.first().unwrap().group_id, group.group_id);
        }
    }

    #[tokio::test]
    async fn test_group_membership() {
        let wallet_a = LocalWallet::new(&mut xmtp_cryptography::utils::rng());
        let wallet_b = LocalWallet::new(&mut xmtp_cryptography::utils::rng());
        let wallet_c = LocalWallet::new(&mut xmtp_cryptography::utils::rng());
        let client_a = create_client_for_wallet(&wallet_a).await;
        let client_b = create_client_for_wallet(&wallet_b).await;
        let client_c = create_client_for_wallet(&wallet_c).await;

        // At first, A creates a group with just B
        let group = client_a
            .create_group(vec![wallet_b.get_address()])
            .await
            .unwrap();
        delay_to_propagate().await;

        // Both A and B should all be able to list each other
        for client in vec![&client_a, &client_b] {
            let members = client.list_members(group.group_id.clone()).await.unwrap();
            assert_eq!(members.len(), 2);
            for member in members {
                assert!(vec![wallet_a.get_address(), wallet_b.get_address()]
                    .contains(&member.account_address));
            }
        }

        // And then when A adds C to the group...
        client_a
            .add_member(group.group_id.clone(), wallet_c.get_address())
            .await
            .unwrap();

        // ... then they should all see each other.
        for client in vec![&client_a, &client_b, &client_c] {
            let members = client.list_members(group.group_id.clone()).await.unwrap();
            assert_eq!(members.len(), 3);
            for member in members {
                assert!(vec![
                    wallet_a.get_address(),
                    wallet_b.get_address(),
                    wallet_c.get_address()
                ]
                .contains(&member.account_address));
            }
        }

        // And then when A removes B from the group...
        client_a
            .remove_member(group.group_id.clone(), wallet_b.get_address())
            .await
            .unwrap();

        // ... then only A and C should see each other.
        for client in vec![&client_a, &client_b] {
            let members = client.list_members(group.group_id.clone()).await.unwrap();
            assert_eq!(members.len(), 2);
            for member in members {
                assert!(vec![wallet_a.get_address(), wallet_c.get_address()]
                    .contains(&member.account_address));
            }
        }
    }

    #[tokio::test]
    async fn test_active_group_detection() {
        let wallet_a = LocalWallet::new(&mut xmtp_cryptography::utils::rng());
        let wallet_b = LocalWallet::new(&mut xmtp_cryptography::utils::rng());
        let wallet_c = LocalWallet::new(&mut xmtp_cryptography::utils::rng());
        let client_a = create_client_for_wallet(&wallet_a).await;
        let client_b = create_client_for_wallet(&wallet_b).await;
        let client_c = create_client_for_wallet(&wallet_c).await;
        let group = client_a
            .create_group(vec![wallet_b.get_address(), wallet_c.get_address()])
            .await
            .unwrap();
        delay_to_propagate().await;

        // At first the group should be active for all clients.
        for (client, should_be_active) in
            vec![(&client_a, true), (&client_b, true), (&client_c, true)]
        {
            let is_active = client
                .is_active_group(group.group_id.clone())
                .await
                .unwrap();
            assert_eq!(is_active, should_be_active);
        }

        // But when A removes B from the group...
        client_a
            .remove_member(group.group_id.clone(), wallet_b.get_address())
            .await
            .unwrap();

        // ... then the group should be inactive for B.
        for (client, should_be_active) in
            vec![(&client_a, true), (&client_b, false), (&client_c, true)]
        {
            let is_active = client
                .is_active_group(group.group_id.clone())
                .await
                .unwrap();
            assert_eq!(is_active, should_be_active);
        }
    }

    #[tokio::test]
    async fn test_message_sending_listing() {
        let wallet_a = LocalWallet::new(&mut xmtp_cryptography::utils::rng());
        let wallet_b = LocalWallet::new(&mut xmtp_cryptography::utils::rng());
        let wallet_c = LocalWallet::new(&mut xmtp_cryptography::utils::rng());
        let client_a = create_client_for_wallet(&wallet_a).await;
        let client_b = create_client_for_wallet(&wallet_b).await;
        let client_c = create_client_for_wallet(&wallet_c).await;
        let group = client_a
            .create_group(vec![wallet_b.get_address(), wallet_c.get_address()])
            .await
            .unwrap();
        delay_to_propagate().await;

        // When A sends a message to the group...
        let encoded: EncodedContent = TextCodec::encode("Hello, world!".to_string()).unwrap();
        client_a
            .send_message(group.group_id.clone(), encoded.encode_to_vec())
            .await
            .unwrap();

        // ... then they should all see the message.
        for client in vec![&client_a, &client_b, &client_c] {
            let groups = client.list_groups(None, None, None).await.unwrap();
            assert_eq!(groups.len(), 1);
            let group_id = groups.first().unwrap().group_id.clone();
            let messages = client
                .list_messages(group_id, None, None, None)
                .await
                .unwrap();
            assert_eq!(messages.len(), 1);
            let msg = messages.first().unwrap();
            assert_eq!(msg.sender_account_address, wallet_a.get_address());
            let encoded = EncodedContent::decode(msg.content_bytes.as_slice()).unwrap();
            let decoded = TextCodec::decode(encoded).unwrap();
            assert_eq!(decoded, "Hello, world!".to_string());
        }
    }

    // Helpers

    async fn delay_to_propagate() {
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await
    }

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
