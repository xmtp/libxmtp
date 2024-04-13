use xmtp_proto::{
    api_client::XmtpMlsClient,
    xmtp::mls::message_contents::plaintext_envelope::v2::MessageType::{
        MessageHistoryRequest as HistoryRequest, MessageHistoryResponse as HistoryResponse,
    },
    xmtp::mls::message_contents::plaintext_envelope::{Content, V2},
    xmtp::mls::message_contents::PlaintextEnvelope,
    xmtp::mls::message_contents::{MessageHistoryRequest, MessageHistoryResponse},
};

use super::{GroupError, MlsGroup};

impl<'c, ApiClient> MlsGroup<'c, ApiClient>
where
    ApiClient: XmtpMlsClient,
{
    #[allow(dead_code)]
    pub(crate) async fn send_message_history_request(&self) -> Result<(), GroupError> {
        let pin_code = "1234".to_string();
        let request_id = "abc123".to_string();
        let _request = PlaintextEnvelope {
            content: Some(Content::V2(V2 {
                idempotency_key: String::from("unique"),
                message_type: Some(HistoryRequest(MessageHistoryRequest {
                    pin_code,
                    request_id,
                })),
            })),
        };
        // TODO: Implement sending request to network
        Ok(())
    }

    #[allow(dead_code)]
    pub(crate) async fn send_message_history_response(&self) -> Result<(), GroupError> {
        let backup_url = "https://example.com/uploads/long-id-123".to_string();
        let request_id = "abc123".to_string();
        let backup_file_hash = b"ABC123DEF456";
        let expiration_time_ns = 123;
        let _request = PlaintextEnvelope {
            content: Some(Content::V2(V2 {
                idempotency_key: String::from("unique"),
                message_type: Some(HistoryResponse(MessageHistoryResponse {
                    backup_url,
                    request_id,
                    backup_file_hash: backup_file_hash.into(),
                    expiration_time_ns,
                })),
            })),
        };
        // TODO: Implement sending (responding) request to network
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use crate::assert_ok;
    use crate::builder::ClientBuilder;
    use xmtp_cryptography::utils::generate_local_wallet;

    #[tokio::test]
    async fn test_send_mesage_history_request() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;
        let group = client.create_group(None).expect("create group");

        let result = group.send_message_history_request().await;
        assert_ok!(result);
    }

    #[tokio::test]
    async fn test_send_mesage_history_response() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;
        let group = client.create_group(None).expect("create group");

        let result = group.send_message_history_response().await;
        assert_ok!(result);
    }
}
