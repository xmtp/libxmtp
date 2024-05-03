use rand::distributions::{Alphanumeric, DistString};
use rand::Rng;
use rand::RngCore;

use xmtp_cryptography::utils as crypto_utils;
use xmtp_proto::{
    api_client::{XmtpIdentityClient, XmtpMlsClient},
    xmtp::mls::message_contents::plaintext_envelope::v2::MessageType::{Reply, Request},
    xmtp::mls::message_contents::plaintext_envelope::{Content, V2},
    xmtp::mls::message_contents::PlaintextEnvelope,
    xmtp::mls::message_contents::{
        message_history_key_type::Key, MessageHistoryKeyType, MessageHistoryReply,
        MessageHistoryRequest,
    },
};

use super::GroupError;

use crate::{
    client::ClientError,
    groups::StoredGroupMessage,
    storage::{group::StoredGroup, StorageError},
    Client,
};

impl<'c, ApiClient> Client<ApiClient>
where
    ApiClient: XmtpMlsClient + XmtpIdentityClient,
{
    pub async fn allow_history_sync(&self) -> Result<(), ClientError> {
        let history_sync_group = self.create_sync_group()?;
        history_sync_group
            .sync()
            .await
            .map_err(|e| ClientError::Generic(e.to_string()))?;
        Ok(())
    }

    #[allow(dead_code)]
    pub(crate) async fn send_message_history_request(&self) -> Result<(), GroupError> {
        let contents = HistoryRequest::new();
        let _request = PlaintextEnvelope {
            content: Some(Content::V2(V2 {
                idempotency_key: new_request_id(),
                message_type: Some(Request(contents.into())),
            })),
        };

        Ok(())
    }

    #[allow(dead_code)]
    pub(crate) async fn send_message_history_reply(
        &self,
        contents: MessageHistoryReply,
    ) -> Result<(), GroupError> {
        let _request = PlaintextEnvelope {
            content: Some(Content::V2(V2 {
                idempotency_key: new_request_id(),
                message_type: Some(Reply(contents)),
            })),
        };
        Ok(())
    }

    #[allow(dead_code)]
    pub(crate) async fn prepare_messages_to_sync(
        &self,
    ) -> Result<Vec<StoredGroupMessage>, StorageError> {
        let conn = self.store.conn()?;
        let groups = conn.find_groups(None, None, None, None)?;
        let mut all_messages: Vec<StoredGroupMessage> = vec![];

        for StoredGroup { id, .. } in groups.clone() {
            let messages = conn.get_group_messages(id, None, None, None, None, None)?;
            all_messages.extend(messages);
        }

        Ok(all_messages)
    }
}

struct HistoryRequest {
    pin_code: String,
    request_id: String,
}

impl HistoryRequest {
    pub(crate) fn new() -> Self {
        Self {
            pin_code: new_pin(),
            request_id: new_request_id(),
        }
    }
}

impl From<HistoryRequest> for MessageHistoryRequest {
    fn from(req: HistoryRequest) -> Self {
        MessageHistoryRequest {
            pin_code: req.pin_code,
            request_id: req.request_id,
        }
    }
}

struct HistoryReply {
    request_id: String,
    url: String,
    bundle_hash: Vec<u8>,
    signing_key: HistoryKeyType,
    encryption_key: HistoryKeyType,
}

impl HistoryReply {
    #[allow(dead_code)]
    pub(crate) fn new(
        id: &str,
        url: &str,
        bundle_hash: Vec<u8>,
        signing_key: HistoryKeyType,
        encryption_key: HistoryKeyType,
    ) -> Self {
        Self {
            request_id: id.into(),
            url: url.into(),
            bundle_hash,
            signing_key,
            encryption_key,
        }
    }
}

impl From<HistoryReply> for MessageHistoryReply {
    fn from(reply: HistoryReply) -> Self {
        MessageHistoryReply {
            request_id: reply.request_id,
            url: reply.url,
            bundle_hash: reply.bundle_hash,
            signing_key: Some(reply.signing_key.into()),
            encryption_key: Some(reply.encryption_key.into()),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
enum HistoryKeyType {
    Chacha20Poly1305([u8; 32]),
}

impl HistoryKeyType {
    #[allow(dead_code)]
    fn new_chacha20_poly1305_key() -> Self {
        let mut key = [0u8; 32];
        crypto_utils::rng().fill_bytes(&mut key[..]);
        HistoryKeyType::Chacha20Poly1305(key)
    }

    #[allow(dead_code)]
    fn len(&self) -> usize {
        match self {
            HistoryKeyType::Chacha20Poly1305(key) => key.len(),
        }
    }
}

impl From<HistoryKeyType> for MessageHistoryKeyType {
    fn from(key: HistoryKeyType) -> Self {
        match key {
            HistoryKeyType::Chacha20Poly1305(key) => MessageHistoryKeyType {
                key: Some(Key::Chacha20Poly1305(key.to_vec())),
            },
        }
    }
}

fn new_request_id() -> String {
    Alphanumeric.sample_string(&mut rand::thread_rng(), 24)
}

fn new_pin() -> String {
    let mut rng = rand::thread_rng();
    let pin: u32 = rng.gen_range(0..10000);
    format!("{:04}", pin)
}

#[allow(dead_code)]
fn verify_pin(expected: &str, actual: &str) -> bool {
    expected == actual
}

#[cfg(test)]
mod tests {

    use super::*;
    use xmtp_cryptography::utils::generate_local_wallet;

    use crate::assert_ok;
    use crate::builder::ClientBuilder;

    #[tokio::test]
    async fn test_allow_history_sync() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;
        assert!(client.allow_history_sync().await.is_ok());
    }

    #[tokio::test]
    async fn test_installations_are_added_to_sync_group() {
        let wallet = generate_local_wallet();
        let amal_a = ClientBuilder::new_test_client(&wallet).await;
        let amal_b = ClientBuilder::new_test_client(&wallet).await;
        let amal_c = ClientBuilder::new_test_client(&wallet).await;
        assert!(amal_c.allow_history_sync().await.is_ok());

        amal_a.sync_welcomes().await.expect("sync_welcomes");
        amal_b.sync_welcomes().await.expect("sync_welcomes");

        let conn_a = amal_a.store.conn().unwrap();
        let amal_a_sync_groups = conn_a.find_sync_groups().unwrap();

        let conn_b = amal_b.store.conn().unwrap();
        let amal_b_sync_groups = conn_b.find_sync_groups().unwrap();

        let conn_c = amal_c.store.conn().unwrap();
        let amal_c_sync_groups = conn_c.find_sync_groups().unwrap();

        assert_eq!(amal_a_sync_groups.len(), 1);
        assert_eq!(amal_b_sync_groups.len(), 1);
        assert_eq!(amal_c_sync_groups.len(), 1);
        // make sure all installations are in the same sync group
        assert_eq!(amal_a_sync_groups[0].id, amal_b_sync_groups[0].id);
        assert_eq!(amal_b_sync_groups[0].id, amal_c_sync_groups[0].id);
    }

    #[tokio::test]
    async fn test_send_mesage_history_request() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;
        client
            .allow_history_sync()
            .await
            .expect("allow history sync | create sync group");
        let result = client.send_message_history_request().await;
        assert_ok!(result);
    }

    #[tokio::test]
    async fn test_send_mesage_history_reply() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;
        client
            .allow_history_sync()
            .await
            .expect("allow history sync | create sync group");
        let request_id = new_request_id();
        let url = "https://test.com/abc-123";
        let backup_hash = b"ABC123".into();
        let signing_key = HistoryKeyType::new_chacha20_poly1305_key();
        let encryption_key = HistoryKeyType::new_chacha20_poly1305_key();
        let reply = HistoryReply::new(&request_id, url, backup_hash, signing_key, encryption_key);
        let result = client.send_message_history_reply(reply.into()).await;
        assert_ok!(result);
    }

    #[tokio::test]
    async fn test_request_reply_roundtrip() {
        let wallet = generate_local_wallet();
        let amal_a = ClientBuilder::new_test_client(&wallet).await;
        let amal_b = ClientBuilder::new_test_client(&wallet).await;
        let group = amal_a.create_group(None).expect("create group");
        let add_members_result = group
            .add_members_by_installation_id(vec![amal_b.installation_public_key()])
            .await;
        assert_ok!(add_members_result);

        assert!(amal_b.allow_history_sync().await.is_ok());

        group.send_message(b"hello").await.expect("send message");
        let result = amal_b.send_message_history_request().await;
        assert_ok!(result);
    }

    #[tokio::test]
    async fn test_prepare_messages_to_sync() {
        let wallet = generate_local_wallet();
        let amal_a = ClientBuilder::new_test_client(&wallet).await;
        let group = amal_a.create_group(None).expect("create group");

        group.send_message(b"hello").await.expect("send message");
        group.send_message(b"hello x2").await.expect("send message");
        let messages_result = amal_a.prepare_messages_to_sync().await;
        assert_ok!(messages_result);
    }

    #[test]
    fn test_new_pin() {
        let pin = new_pin();
        assert_eq!(pin.len(), 4);
    }

    #[test]
    fn test_new_key() {
        let sig_key = HistoryKeyType::new_chacha20_poly1305_key();
        let enc_key = HistoryKeyType::new_chacha20_poly1305_key();
        assert_eq!(sig_key.len(), 32);
        // ensure keys are different (seed isn't reused)
        assert_ne!(sig_key, enc_key);
    }
}
