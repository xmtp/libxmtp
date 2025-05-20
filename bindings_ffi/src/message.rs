use std::sync::{Arc, Mutex};
use xmtp_cryptography::utils::LocalWallet;
use xmtp_mls::{
    client::MlsClient,
    api::ApiDebugWrapper,
    api::tonic::TonicApiClient,
    storage::InboxId,
    identity::FfiIdentifier,
    sync::FfiSyncWorkerMode,
    error::GenericError,
};
use xmtp_api_grpc::GrpcApiClient;
use xmtp_db::Storage;

pub struct FfiMessage {
    pub id: Vec<u8>,
    pub sent_at_ns: i64,
    pub conversation_id: Vec<u8>,
    pub sender_inbox_id: String,
    pub content: Vec<u8>,
    pub kind: FfiConversationMessageKind,
    pub delivery_status: FfiDeliveryStatus,
}

pub struct FfiMessageWithReactions {
    pub message: FfiMessage,
    pub reactions: Vec<FfiMessage>,
}

pub struct FfiRemoteAttachmentInfo {
    pub secret: Vec<u8>,
    pub content_digest: String,
    pub nonce: Vec<u8>,
    pub scheme: String,
    pub url: String,
    pub salt: Vec<u8>,
    pub content_length: Option<u32>,
    pub filename: Option<String>,
}

pub struct FfiMultiRemoteAttachment {
    pub attachments: Vec<FfiRemoteAttachmentInfo>,
}

pub struct FfiReaction {
    pub reference: String,
    pub reference_inbox_id: String,
    pub action: FfiReactionAction,
    pub content: String,
    pub schema: FfiReactionSchema,
}

pub enum FfiReactionAction {
    Unknown,
    #[default]
    Added,
    Removed,
}

pub enum FfiReactionSchema {
    Unknown,
    #[default]
    Unicode,
    Shortcode,
    Custom,
}

pub enum FfiDeliveryStatus {
    Unpublished,
    Published,
    Failed,
}

pub enum FfiContentType {
    Unknown,
    Text,
    GroupMembershipChange,
    GroupUpdated,
    Reaction,
    ReadReceipt,
    Reply,
    Attachment,
    RemoteAttachment,
    TransactionReference,
}

pub struct FfiListMessagesOptions {
    pub sent_before_ns: Option<i64>,
    pub sent_after_ns: Option<i64>,
    pub limit: Option<i64>,
    pub delivery_status: Option<FfiDeliveryStatus>,
    pub direction: Option<FfiDirection>,
    pub content_types: Option<Vec<FfiContentType>>,
}

pub enum FfiDirection {
    Ascending,
    Descending,
}

pub fn encode_reaction(reaction: FfiReaction) -> Result<Vec<u8>, GenericError> {
    // ... existing code ...
}

pub fn decode_reaction(bytes: Vec<u8>) -> Result<FfiReaction, GenericError> {
    // ... existing code ...
}

pub fn encode_multi_remote_attachment(
    ffi_multi_remote_attachment: FfiMultiRemoteAttachment,
) -> Result<Vec<u8>, GenericError> {
    // ... existing code ...
}

pub fn decode_multi_remote_attachment(
    bytes: Vec<u8>,
) -> Result<FfiMultiRemoteAttachment, GenericError> {
    // ... existing code ...
}

impl From<StoredGroupMessage> for FfiMessage {
    fn from(msg: StoredGroupMessage) -> Self {
        // ... existing code ...
    }
}

impl From<StoredGroupMessageWithReactions> for FfiMessageWithReactions {
    fn from(msg_with_reactions: StoredGroupMessageWithReactions) -> Self {
        // ... existing code ...
    }
}

impl From<FfiReaction> for ReactionV2 {
    fn from(reaction: FfiReaction) -> Self {
        // ... existing code ...
    }
}

impl From<ReactionV2> for FfiReaction {
    fn from(reaction: ReactionV2) -> Self {
        // ... existing code ...
    }
}

impl From<FfiRemoteAttachmentInfo> for RemoteAttachmentInfo {
    fn from(ffi_remote_attachment_info: FfiRemoteAttachmentInfo) -> Self {
        // ... existing code ...
    }
}

impl From<RemoteAttachmentInfo> for FfiRemoteAttachmentInfo {
    fn from(remote_attachment_info: RemoteAttachmentInfo) -> Self {
        // ... existing code ...
    }
}

impl From<FfiMultiRemoteAttachment> for MultiRemoteAttachment {
    fn from(ffi_multi_remote_attachment: FfiMultiRemoteAttachment) -> Self {
        // ... existing code ...
    }
}

impl From<MultiRemoteAttachment> for FfiMultiRemoteAttachment {
    fn from(multi_remote_attachment: MultiRemoteAttachment) -> Self {
        // ... existing code ...
    }
}

impl From<FfiContentType> for ContentType {
    fn from(value: FfiContentType) -> Self {
        // ... existing code ...
    }
}

impl From<FfiDeliveryStatus> for DeliveryStatus {
    fn from(status: FfiDeliveryStatus) -> Self {
        // ... existing code ...
    }
}

impl From<DeliveryStatus> for FfiDeliveryStatus {
    fn from(status: DeliveryStatus) -> Self {
        // ... existing code ...
    }
}

impl From<FfiDirection> for SortDirection {
    fn from(direction: FfiDirection) -> Self {
        // ... existing code ...
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::*;
    use xmtp_cryptography::utils::LocalWallet;

    #[tokio::test]
    async fn test_send_message() {
        let client_a = new_test_client().await;
        let client_b = new_test_client().await;

        let conversation = client_a
            .create_conversation(vec![client_b.account_identifier.clone()])
            .await
            .unwrap();

        let message = conversation
            .send_message("Hello, world!".to_string())
            .await
            .unwrap();

        assert_eq!(message.content(), "Hello, world!");
    }

    #[tokio::test]
    async fn test_send_message_with_metadata() {
        let client_a = new_test_client().await;
        let client_b = new_test_client().await;

        let conversation = client_a
            .create_conversation(vec![client_b.account_identifier.clone()])
            .await
            .unwrap();

        let metadata = vec![("key".to_string(), "value".to_string())];
        let message = conversation
            .send_message_with_metadata("Hello, world!".to_string(), metadata)
            .await
            .unwrap();

        assert_eq!(message.content(), "Hello, world!");
        assert_eq!(message.metadata().get("key").unwrap(), "value");
    }

    #[tokio::test]
    async fn test_list_messages() {
        let client_a = new_test_client().await;
        let client_b = new_test_client().await;

        let conversation = client_a
            .create_conversation(vec![client_b.account_identifier.clone()])
            .await
            .unwrap();

        conversation
            .send_message("Hello, world!".to_string())
            .await
            .unwrap();

        let messages = conversation.list_messages().await.unwrap();

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content(), "Hello, world!");
    }

    #[tokio::test]
    async fn test_list_messages_with_limit() {
        let client_a = new_test_client().await;
        let client_b = new_test_client().await;

        let conversation = client_a
            .create_conversation(vec![client_b.account_identifier.clone()])
            .await
            .unwrap();

        conversation
            .send_message("Message 1".to_string())
            .await
            .unwrap();
        conversation
            .send_message("Message 2".to_string())
            .await
            .unwrap();
        conversation
            .send_message("Message 3".to_string())
            .await
            .unwrap();

        let messages = conversation.list_messages_with_limit(2).await.unwrap();

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].content(), "Message 3");
        assert_eq!(messages[1].content(), "Message 2");
    }

    #[tokio::test]
    async fn test_list_messages_with_cursor() {
        let client_a = new_test_client().await;
        let client_b = new_test_client().await;

        let conversation = client_a
            .create_conversation(vec![client_b.account_identifier.clone()])
            .await
            .unwrap();

        conversation
            .send_message("Message 1".to_string())
            .await
            .unwrap();
        conversation
            .send_message("Message 2".to_string())
            .await
            .unwrap();
        conversation
            .send_message("Message 3".to_string())
            .await
            .unwrap();

        let first_page = conversation.list_messages_with_limit(2).await.unwrap();
        let cursor = first_page[1].id();

        let second_page = conversation
            .list_messages_with_cursor(cursor)
            .await
            .unwrap();

        assert_eq!(second_page.len(), 1);
        assert_eq!(second_page[0].content(), "Message 1");
    }

    #[tokio::test]
    async fn test_stream_messages() {
        let client_a = new_test_client().await;
        let client_b = new_test_client().await;

        let conversation = client_a
            .create_conversation(vec![client_b.account_identifier.clone()])
            .await
            .unwrap();

        let mut stream = conversation.stream_messages().await.unwrap();

        conversation
            .send_message("Hello, world!".to_string())
            .await
            .unwrap();

        let message = stream.next().await.unwrap();
        assert_eq!(message.content(), "Hello, world!");

        stream.close().await.unwrap();
    }
} 