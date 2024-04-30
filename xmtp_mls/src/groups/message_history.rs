use rand::{
    distributions::{Alphanumeric, DistString},
    Rng,
};

use xmtp_proto::{
    api_client::{XmtpIdentityClient, XmtpMlsClient},
    xmtp::mls::message_contents::plaintext_envelope::v2::MessageType::{Reply, Request},
    xmtp::mls::message_contents::plaintext_envelope::{Content, V2},
    xmtp::mls::message_contents::PlaintextEnvelope,
    xmtp::mls::message_contents::{MessageHistoryReply, MessageHistoryRequest},
};

use super::GroupError;

use crate::client::ClientError;
use crate::groups::StoredGroupMessage;
use crate::storage::group::StoredGroup;
use crate::storage::StorageError;
use crate::Client;

impl<'c, ApiClient> Client<ApiClient>
where
    ApiClient: XmtpMlsClient + XmtpIdentityClient,
{
    pub async fn allow_history_sync(&self) -> Result<(), ClientError> {
        let sync_group = self.create_sync_group()?;
        let conn = self.store.conn()?;
        let provider = sync_group.client.mls_provider(&conn);
        sync_group
            .add_missing_installations(provider)
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
        // println!("prepreparepare_messages_to_sync called");
        let conn = self.store.conn()?;
        let groups = conn.find_groups(None, None, None, None)?;
        let mut all_messages: Vec<StoredGroupMessage> = vec![];

        for StoredGroup { id, .. } in groups.clone() {
            let messages = conn.get_group_messages(id, None, None, None, None, None)?;
            // println!("{:#?}", messages);
            all_messages.extend(messages);
        }

        // println!("groups: {:#?}", groups);
        // println!("# of grprepareoup messages: {:?}", all_messages.len());
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
    bundle_signing_key: String,
    encryption_key: String,
}

impl HistoryReply {
    #[allow(dead_code)]
    pub(crate) fn new(
        id: &str,
        url: &str,
        hash: Vec<u8>,
        signing_key: &str,
        encryption_key: &str,
    ) -> Self {
        Self {
            request_id: id.into(),
            url: url.into(),
            bundle_hash: hash,
            bundle_signing_key: signing_key.into(),
            encryption_key: encryption_key.into(),
        }
    }
}

impl From<HistoryReply> for MessageHistoryReply {
    fn from(reply: HistoryReply) -> Self {
        MessageHistoryReply {
            request_id: reply.request_id,
            url: reply.url,
            bundle_hash: reply.bundle_hash,
            bundle_signing_key: reply.bundle_signing_key,
            encryption_key: reply.encryption_key,
        }
    }
}

fn new_pin() -> String {
    let mut rng = rand::thread_rng();
    let pin: u32 = rng.gen_range(0..10000);
    format!("{:04}", pin)
}

fn new_request_id() -> String {
    Alphanumeric.sample_string(&mut rand::thread_rng(), 24)
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
    async fn test_send_mesage_history_request() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;
        // calls create_sync_group() internally.
        client
            .allow_history_sync()
            .await
            .expect("create sync group");
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
            .expect("create sync group");
        let request_id = new_request_id();
        let url = "https://test.com/abc-123";
        let backup_hash = b"ABC123".into();
        let aes_key = "1234567890";
        let signing_key = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
        let reply = HistoryReply::new(&request_id, url, backup_hash, aes_key, signing_key);
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

        let _ = amal_b.send_message_history_request().await;
    }

    #[tokio::test]
    async fn test_prepare_messages_to_sync() {
        let wallet = generate_local_wallet();
        let amal_a = ClientBuilder::new_test_client(&wallet).await;
        // let group_a = amal_a.create_group(None);
        let _messages_result = amal_a.prepare_messages_to_sync().await;
        // println!("{:?}", messages_result);
    }
}
