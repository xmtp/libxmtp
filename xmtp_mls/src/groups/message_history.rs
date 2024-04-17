use rand::{
    distributions::{Alphanumeric, DistString},
    Rng,
};

use xmtp_proto::{
    api_client::XmtpMlsClient,
    xmtp::mls::message_contents::plaintext_envelope::v2::MessageType::{Reply, Request},
    xmtp::mls::message_contents::plaintext_envelope::{Content, V2},
    xmtp::mls::message_contents::PlaintextEnvelope,
    xmtp::mls::message_contents::{MessageHistoryReply, MessageHistoryRequest},
};

use super::{GroupError, MlsGroup};

impl<'c, ApiClient> MlsGroup<'c, ApiClient>
where
    ApiClient: XmtpMlsClient,
{
    #[allow(dead_code)]
    pub(crate) async fn send_message_history_request(&self) -> Result<(), GroupError> {
        let contents = new_message_history_request();
        let _request = PlaintextEnvelope {
            content: Some(Content::V2(V2 {
                idempotency_key: new_request_id(),
                message_type: Some(Request(contents)),
            })),
        };
        // TODO: Implement sending this request to the network
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
        // TODO: Implement sending this reply to the network
        Ok(())
    }
}

pub(crate) fn new_message_history_request() -> MessageHistoryRequest {
    MessageHistoryRequest {
        pin_code: new_pin(),
        request_id: new_request_id(),
    }
}

#[allow(dead_code)]
pub(crate) fn new_message_history_reply(
    id: &str,
    url: &str,
    hash: Vec<u8>,
    exp: i64,
) -> MessageHistoryReply {
    MessageHistoryReply {
        request_id: id.into(),
        backup_url: url.into(),
        backup_file_hash: hash,
        expiration_time_ns: exp,
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
    use crate::utils::time::now_ns;

    #[tokio::test]
    async fn test_send_mesage_history_request() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;
        let group = client.create_group(None).expect("create group");

        let result = group.send_message_history_request().await;
        assert_ok!(result);
    }

    #[tokio::test]
    async fn test_send_mesage_history_reply() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;

        let request_id = new_request_id();
        let url = "https://test.com/abc-123";
        let backup_hash = b"ABC123".into();
        let expiry = now_ns() + 10_000;
        let reply = new_message_history_reply(&request_id, url, backup_hash, expiry);
        let result = group.send_message_history_reply(reply).await;
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

        let _ = group.send_message_history_request().await;
    }
}
