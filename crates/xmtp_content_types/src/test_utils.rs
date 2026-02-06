use crate::{
    ContentCodec,
    attachment::{Attachment, AttachmentCodec},
    delete_message::DeleteMessageCodec,
    edit_message::EditMessageCodec,
    group_updated::GroupUpdatedCodec,
    membership_change::GroupMembershipChangeCodec,
    multi_remote_attachment::MultiRemoteAttachmentCodec,
    reaction::ReactionCodec,
    read_receipt::{ReadReceipt, ReadReceiptCodec},
    remote_attachment::{RemoteAttachment, RemoteAttachmentCodec},
    reply::{Reply, ReplyCodec},
    text::TextCodec,
    transaction_reference::{TransactionReference, TransactionReferenceCodec},
};
use xmtp_proto::xmtp::mls::message_contents::{
    ContentTypeId, EncodedContent, GroupMembershipChanges, GroupUpdated,
    content_types::{
        DeleteMessage, EditMessage, MultiRemoteAttachment, ReactionAction, ReactionSchema,
        ReactionV2,
    },
};

pub struct TestContentGenerator;

impl TestContentGenerator {
    pub fn text_content(text: &str) -> EncodedContent {
        TextCodec::encode(text.to_string()).expect("Failed to encode text")
    }

    pub fn attachment_content(filename: &str, data: Vec<u8>) -> EncodedContent {
        let attachment = Attachment {
            filename: Some(filename.to_string()),
            mime_type: "application/octet-stream".to_string(),
            content: data,
        };
        AttachmentCodec::encode(attachment).expect("Failed to encode attachment")
    }

    pub fn remote_attachment_content(url: &str, filename: &str) -> EncodedContent {
        let remote_attachment = RemoteAttachment {
            url: url.to_string(),
            filename: Some(filename.to_string()),
            content_digest: "test-digest".to_string(),
            secret: b"test-secret".to_vec(),
            salt: b"test-salt".to_vec(),
            nonce: b"test-nonce".to_vec(),
            scheme: "https".to_string(),
            content_length: Some(100),
        };
        RemoteAttachmentCodec::encode(remote_attachment)
            .expect("Failed to encode remote attachment")
    }

    pub fn multi_remote_attachment_content(urls: Vec<String>) -> EncodedContent {
        let attachments = urls
            .into_iter()
            .map(|url| {
                xmtp_proto::xmtp::mls::message_contents::content_types::RemoteAttachmentInfo {
                    url,
                    filename: Some("test.txt".to_string()),
                    content_digest: "test-digest".to_string(),
                    secret: b"test-secret".to_vec(),
                    salt: b"test-salt".to_vec(),
                    nonce: b"test-nonce".to_vec(),
                    scheme: "https".to_string(),
                    content_length: Some(100),
                }
            })
            .collect();

        let multi_attachment = MultiRemoteAttachment { attachments };
        MultiRemoteAttachmentCodec::encode(multi_attachment)
            .expect("Failed to encode multi remote attachment")
    }

    pub fn reaction_content(
        reference_id: &str,
        content: &str,
        action: ReactionAction,
    ) -> EncodedContent {
        let reaction = ReactionV2 {
            reference: reference_id.to_string(),
            reference_inbox_id: "test-inbox-id".to_string(),
            action: action.into(),
            content: content.to_string(),
            schema: ReactionSchema::Unicode.into(),
        };
        ReactionCodec::encode(reaction).expect("Failed to encode reaction")
    }

    pub fn reply_content(
        reference_id: &str,
        content_type_id: ContentTypeId,
        content: Vec<u8>,
    ) -> EncodedContent {
        let reply = Reply {
            reference: reference_id.to_string(),
            reference_inbox_id: Some("test-inbox-id".to_string()),
            content: EncodedContent {
                r#type: Some(content_type_id),
                parameters: Default::default(),
                compression: None,
                content,
                fallback: None,
            },
        };
        ReplyCodec::encode(reply).expect("Failed to encode reply")
    }

    pub fn read_receipt_content() -> EncodedContent {
        let read_receipt = ReadReceipt {};
        ReadReceiptCodec::encode(read_receipt).expect("Failed to encode read receipt")
    }

    pub fn delete_message_content(message_id: &str) -> EncodedContent {
        let delete_message = DeleteMessage {
            message_id: message_id.to_string(),
        };
        DeleteMessageCodec::encode(delete_message).expect("Failed to encode delete message")
    }

    pub fn edit_message_content(message_id: &str, new_content: EncodedContent) -> EncodedContent {
        let edit_message = EditMessage {
            message_id: message_id.to_string(),
            edited_content: Some(new_content),
        };
        EditMessageCodec::encode(edit_message).expect("Failed to encode edit message")
    }

    pub fn transaction_reference_content(
        reference: &str,
        network_id: i32,
        _symbol: &str,
    ) -> EncodedContent {
        let transaction_ref = TransactionReference {
            namespace: Some("eip155".to_string()),
            network_id: network_id.to_string(),
            reference: reference.to_string(),
            metadata: None,
        };
        TransactionReferenceCodec::encode(transaction_ref)
            .expect("Failed to encode transaction reference")
    }

    pub fn group_updated_content(added_inbox_ids: Vec<String>) -> EncodedContent {
        let group_updated = GroupUpdated {
            initiated_by_inbox_id: "initiator-inbox-id".to_string(),
            added_inboxes: added_inbox_ids
                .into_iter()
                .map(
                    |id| xmtp_proto::xmtp::mls::message_contents::group_updated::Inbox {
                        inbox_id: id,
                    },
                )
                .collect(),
            removed_inboxes: vec![],
            metadata_field_changes: vec![],
            left_inboxes: vec![],
            added_admin_inboxes: vec![],
            removed_admin_inboxes: vec![],
            added_super_admin_inboxes: vec![],
            removed_super_admin_inboxes: vec![],
        };
        GroupUpdatedCodec::encode(group_updated).expect("Failed to encode group updated")
    }

    pub fn membership_change_content(
        members_added: Vec<String>,
        members_removed: Vec<String>,
    ) -> EncodedContent {
        let membership_change = GroupMembershipChanges {
            members_added: members_added
                .into_iter()
                .map(
                    |account_address| xmtp_proto::xmtp::mls::message_contents::MembershipChange {
                        account_address: account_address.clone(),
                        initiated_by_account_address: "initiator".to_string(),
                        installation_ids: vec![b"installation-1".to_vec()],
                    },
                )
                .collect(),
            members_removed: members_removed
                .into_iter()
                .map(
                    |account_address| xmtp_proto::xmtp::mls::message_contents::MembershipChange {
                        account_address: account_address.clone(),
                        initiated_by_account_address: "initiator".to_string(),
                        installation_ids: vec![b"installation-2".to_vec()],
                    },
                )
                .collect(),
            installations_added: vec![],
            installations_removed: vec![],
        };
        GroupMembershipChangeCodec::encode(membership_change)
            .expect("Failed to encode membership change")
    }

    pub fn invalid_content() -> EncodedContent {
        EncodedContent {
            r#type: Some(ContentTypeId {
                authority_id: "invalid".to_string(),
                type_id: "invalid/type".to_string(),
                version_major: 1,
                version_minor: 0,
            }),
            parameters: Default::default(),
            compression: None,
            content: b"invalid content that cannot be decoded".to_vec(),
            fallback: Some("Invalid message".to_string()),
        }
    }

    pub fn malformed_content_with_type(content_type_id: ContentTypeId) -> EncodedContent {
        EncodedContent {
            r#type: Some(content_type_id),
            parameters: Default::default(),
            compression: None,
            content: b"malformed content for a known type".to_vec(),
            fallback: Some("Malformed message".to_string()),
        }
    }
}
