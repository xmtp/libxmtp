use xmtp_db::group_message::{ContentType as DbContentType, MsgQueryArgs};

use crate::context::XmtpSharedContext;
use crate::groups::{GroupError, MlsGroup};
use crate::messages::decoded_message::DecodedMessage;
use crate::messages::enrichment::enrich_messages;
use xmtp_db::prelude::QueryGroupMessage;

impl<Context> MlsGroup<Context>
where
    Context: XmtpSharedContext,
{
    pub fn find_messages_v2(
        &self,
        query: &MsgQueryArgs,
    ) -> Result<Vec<DecodedMessage>, GroupError> {
        let conn = self.context.db();

        let initial_messages = conn.get_group_messages(
            &self.group_id,
            &filter_out_hidden_message_types_from_query(query),
        )?;

        Ok(enrich_messages(conn, &self.group_id, initial_messages)?)
    }
}

fn filter_out_hidden_message_types_from_query(query: &MsgQueryArgs) -> MsgQueryArgs {
    let mut new_query = query.clone();
    // Get the list of all content types, or use the provided one
    let mut content_types = match new_query.content_types {
        Some(types) => types,
        None => DbContentType::all(),
    };

    let hidden_message_types = [DbContentType::Reaction, DbContentType::ReadReceipt];

    // Remove reaction content types
    content_types.retain(|ct| !hidden_message_types.contains(ct));

    new_query.content_types = Some(content_types);
    new_query
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::ClientBuilder;
    use crate::groups::QueryableContentFields;
    use crate::messages::decoded_message::MessageBody;
    use hex::ToHexExt;
    use xmtp_common::time::now_ns;
    use xmtp_content_types::ContentCodec;
    use xmtp_content_types::test_utils::TestContentGenerator;
    use xmtp_content_types::text::TextCodec;
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_db::Store;
    use xmtp_db::group_message::{
        ContentType as DbContentType, DeliveryStatus, GroupMessageKind, StoredGroupMessage,
    };
    use xmtp_proto::xmtp::mls::message_contents::content_types::ReactionAction;
    use xmtp_proto::xmtp::mls::message_contents::{ContentTypeId, EncodedContent};

    async fn setup_test_group() -> (MlsGroup<impl XmtpSharedContext>, impl XmtpSharedContext) {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let group = client.create_group(None, Default::default()).unwrap();

        (group, client.context)
    }

    fn create_test_message(
        group_id: &[u8],
        message_id: Vec<u8>,
        encoded_content: EncodedContent,
        sent_at_ns: i64,
        sender_inbox_id: String,
    ) -> StoredGroupMessage {
        let content_bytes = xmtp_content_types::encoded_content_to_bytes(encoded_content.clone());
        let queryable_fields =
            QueryableContentFields::try_from(encoded_content).unwrap_or_default();

        StoredGroupMessage {
            id: message_id,
            group_id: group_id.to_vec(),
            decrypted_message_bytes: content_bytes,
            sent_at_ns,
            kind: GroupMessageKind::Application,
            sender_installation_id: vec![1, 2, 3],
            sender_inbox_id,
            delivery_status: DeliveryStatus::Published,
            content_type: queryable_fields.content_type,
            version_major: queryable_fields.version_major,
            version_minor: queryable_fields.version_minor,
            authority_id: queryable_fields.authority_id,
            reference_id: queryable_fields.reference_id,
            sequence_id: None,
            originator_id: None,
            expire_at_ns: None,
        }
    }

    // For tests that need malformed content
    fn create_test_message_raw(
        group_id: &[u8],
        message_id: Vec<u8>,
        content: Vec<u8>,
        sent_at_ns: i64,
        sender_inbox_id: String,
        content_type_id: Option<ContentTypeId>,
        reference_id: Option<Vec<u8>>,
    ) -> StoredGroupMessage {
        let queryable_fields = content_type_id
            .as_ref()
            .map(|ct| {
                (
                    DbContentType::from(ct.type_id.clone()),
                    ct.version_major as i32,
                    ct.version_minor as i32,
                    ct.authority_id.clone(),
                )
            })
            .unwrap_or((DbContentType::Text, 1, 0, "xmtp.org".to_string()));

        StoredGroupMessage {
            id: message_id,
            group_id: group_id.to_vec(),
            decrypted_message_bytes: content,
            sent_at_ns,
            kind: GroupMessageKind::Application,
            sender_installation_id: vec![1, 2, 3],
            sender_inbox_id,
            delivery_status: DeliveryStatus::Published,
            content_type: queryable_fields.0,
            version_major: queryable_fields.1,
            version_minor: queryable_fields.2,
            authority_id: queryable_fields.3,
            reference_id,
            sequence_id: None,
            originator_id: None,
            expire_at_ns: None,
        }
    }

    // ===== Assertion Helpers =====

    fn assert_message_count(messages: &[DecodedMessage], expected: usize) {
        assert_eq!(
            messages.len(),
            expected,
            "Expected {} messages, got {}",
            expected,
            messages.len()
        );
    }

    fn assert_text_content(message: &DecodedMessage, expected: &str) {
        match &message.content {
            MessageBody::Text(text) => assert_eq!(text.content, expected),
            _ => panic!("Expected text message, got {:?}", message.content),
        }
    }

    fn assert_reaction_count(message: &DecodedMessage, expected: usize) {
        assert_eq!(
            message.reactions.len(),
            expected,
            "Expected {} reactions, got {}",
            expected,
            message.reactions.len()
        );
    }

    fn assert_has_reaction(message: &DecodedMessage, emoji: &str) {
        assert!(
            message.reactions.iter().any(|r| {
                if let MessageBody::Reaction(reaction) = &r.content {
                    reaction.content == emoji
                } else {
                    false
                }
            }),
            "Expected reaction '{}' not found",
            emoji
        );
    }

    fn assert_reply_references(reply: &DecodedMessage, expected_ref_id: &[u8]) {
        match &reply.content {
            MessageBody::Reply(reply_body) => {
                assert!(
                    reply_body.in_reply_to.is_some(),
                    "Reply should have in_reply_to populated"
                );
                let referenced = reply_body.in_reply_to.as_ref().unwrap();
                assert_eq!(referenced.metadata.id, expected_ref_id);
            }
            _ => panic!("Expected reply message, got {:?}", reply.content),
        }
    }

    fn assert_reply_has_no_reference(reply: &DecodedMessage) {
        match &reply.content {
            MessageBody::Reply(reply_body) => {
                assert!(
                    reply_body.in_reply_to.is_none(),
                    "Reply should not have in_reply_to populated"
                );
            }
            _ => panic!("Expected reply message, got {:?}", reply.content),
        }
    }

    fn find_message_by_id<'a>(messages: &'a [DecodedMessage], id: &[u8]) -> &'a DecodedMessage {
        messages
            .iter()
            .find(|m| m.metadata.id == id)
            .unwrap_or_else(|| panic!("Message with id {:?} not found", id))
    }

    fn create_and_store_message<S>(
        conn: &S,
        group_id: &[u8],
        message_id: Vec<u8>,
        content: EncodedContent,
        timestamp_offset: i64,
        sender: &str,
    ) -> Vec<u8>
    where
        StoredGroupMessage: Store<S>,
    {
        let msg = create_test_message(
            group_id,
            message_id.clone(),
            content,
            now_ns() + timestamp_offset,
            sender.to_string(),
        );
        msg.store(conn).unwrap();
        message_id
    }

    // ===== Tests =====

    #[tokio::test]
    async fn test_find_messages_no_reactions_or_replies() {
        let (group, context) = setup_test_group().await;
        let conn = context.db();

        // Store two simple text messages
        create_and_store_message(
            &conn,
            &group.group_id,
            vec![1],
            TestContentGenerator::text_content("Hello World"),
            0,
            "sender1",
        );

        create_and_store_message(
            &conn,
            &group.group_id,
            vec![2],
            TestContentGenerator::text_content("Another message"),
            1000,
            "sender2",
        );

        // Query and verify
        let messages = group.find_messages_v2(&MsgQueryArgs::default()).unwrap();
        assert_message_count(&messages, 2);
        assert_text_content(&messages[0], "Hello World");
        assert_text_content(&messages[1], "Another message");
        assert_reaction_count(&messages[0], 0);
        assert_reaction_count(&messages[1], 0);
    }

    #[tokio::test]
    async fn test_find_messages_with_reactions() {
        let (group, context) = setup_test_group().await;
        let conn = context.db();

        // Store a text message
        let msg_id = vec![1, 2, 3];
        let msg_id_hex = msg_id.encode_hex();
        create_and_store_message(
            &conn,
            &group.group_id,
            msg_id.clone(),
            TestContentGenerator::text_content("Hello World"),
            0,
            "sender1",
        );

        // Store reactions to the message
        create_and_store_message(
            &conn,
            &group.group_id,
            vec![4],
            TestContentGenerator::reaction_content(&msg_id_hex, "👍", ReactionAction::Added),
            1000,
            "reactor1",
        );

        create_and_store_message(
            &conn,
            &group.group_id,
            vec![5],
            TestContentGenerator::reaction_content(&msg_id_hex, "❤️", ReactionAction::Added),
            2000,
            "reactor2",
        );

        // Query messages
        let messages = group.find_messages_v2(&MsgQueryArgs::default()).unwrap();

        // Should have 1 message (reactions are attached to the message, not returned separately)
        assert_message_count(&messages, 1);

        // Find the original message (should be first based on timestamp)
        let original_msg = find_message_by_id(&messages, &msg_id);
        assert_reaction_count(original_msg, 2);
        assert_has_reaction(original_msg, "👍");
        assert_has_reaction(original_msg, "❤️");
    }

    #[tokio::test]
    async fn test_find_messages_with_replies() {
        let (group, context) = setup_test_group().await;
        let conn = context.db();

        // Store original message
        let msg_id = vec![1, 2, 3];
        let msg_id_hex = msg_id.encode_hex();
        create_and_store_message(
            &conn,
            &group.group_id,
            msg_id.clone(),
            TestContentGenerator::text_content("Original message"),
            0,
            "sender1",
        );

        // Store a reply to the message
        let reply_id = create_and_store_message(
            &conn,
            &group.group_id,
            vec![4, 5, 6],
            TestContentGenerator::reply_content(
                &msg_id_hex,
                TextCodec::content_type(),
                b"This is a reply".to_vec(),
            ),
            1000,
            "replier1",
        );

        // Query messages
        let messages = group.find_messages_v2(&MsgQueryArgs::default()).unwrap();

        assert_message_count(&messages, 2);

        // Find the reply message and verify it references the original
        let reply_msg = find_message_by_id(&messages, &reply_id);
        assert_reply_references(reply_msg, &msg_id);

        // Verify the referenced message content
        if let MessageBody::Reply(reply) = &reply_msg.content {
            let referenced_msg = reply.in_reply_to.as_ref().unwrap();
            assert_text_content(referenced_msg, "Original message");
        }
    }

    #[tokio::test]
    async fn test_find_messages_invalid_reply_reference() {
        let (group, context) = setup_test_group().await;
        let conn = context.db();

        // Store a reply with invalid reference ID (not valid hex)
        create_and_store_message(
            &conn,
            &group.group_id,
            vec![1],
            TestContentGenerator::reply_content(
                "not-valid-hex!@#",
                TextCodec::content_type(),
                b"This is a reply".to_vec(),
            ),
            0,
            "replier1",
        );

        // Query messages - should still return the reply but with None in_reply_to
        let messages = group.find_messages_v2(&MsgQueryArgs::default()).unwrap();
        assert_message_count(&messages, 1);

        assert_reply_has_no_reference(&messages[0]);
        if let MessageBody::Reply(reply) = &messages[0].content {
            assert_eq!(reply.reference_id, "not-valid-hex!@#");
        }
    }

    #[tokio::test]
    async fn test_find_messages_missing_reply_reference() {
        let (group, context) = setup_test_group().await;
        let conn = context.db();

        // Store a reply with valid hex reference but non-existent message
        let non_existent_id = vec![99, 99, 99];
        create_and_store_message(
            &conn,
            &group.group_id,
            vec![1],
            TestContentGenerator::reply_content(
                &non_existent_id.encode_hex(),
                TextCodec::content_type(),
                b"This is a reply".to_vec(),
            ),
            0,
            "replier1",
        );

        // Query messages - should still return the reply but with None in_reply_to
        let messages = group.find_messages_v2(&MsgQueryArgs::default()).unwrap();
        assert_message_count(&messages, 1);
        assert_reply_has_no_reference(&messages[0]);

        if let MessageBody::Reply(reply) = &messages[0].content {
            assert_eq!(reply.reference_id, non_existent_id.encode_hex());
        }
    }

    #[tokio::test]
    async fn test_find_messages_undecodable_messages() {
        let (group, context) = setup_test_group().await;
        let conn = context.db();

        // Store a valid text message
        create_and_store_message(
            &conn,
            &group.group_id,
            vec![1],
            TestContentGenerator::text_content("Valid message"),
            0,
            "sender1",
        );

        // Store a message with invalid/malformed content (use Text type to avoid filtering)
        create_and_store_message(
            &conn,
            &group.group_id,
            vec![2],
            TestContentGenerator::malformed_content_with_type(TextCodec::content_type()),
            1000,
            "sender2",
        );

        // Store a message with unknown content type
        create_and_store_message(
            &conn,
            &group.group_id,
            vec![3],
            TestContentGenerator::invalid_content(),
            2000,
            "sender3",
        );

        // Query messages - malformed content still creates a message
        let messages = group.find_messages_v2(&MsgQueryArgs::default()).unwrap();

        // We should get all 3 messages even though some have malformed content
        assert_message_count(&messages, 3);

        // First message should be valid text
        assert_text_content(&messages[0], "Valid message");

        // Second message - since we're using EncodedContent, it gets decoded as text
        assert_text_content(&messages[1], "malformed content for a known type");

        if let MessageBody::Custom(content) = &messages[2].content {
            assert_eq!(content.fallback, Some("Invalid message".to_string()));
        } else {
            panic!("Expected custom content for unknown type message");
        }
    }

    #[tokio::test]
    async fn test_find_messages_invalid_reactions() {
        let (group, context) = setup_test_group().await;
        let conn = context.db();

        // Store a text message
        let msg_id = vec![1, 2, 3];
        let msg_id_hex = msg_id.encode_hex();
        create_and_store_message(
            &conn,
            &group.group_id,
            msg_id.clone(),
            TestContentGenerator::text_content("Hello World"),
            0,
            "sender1",
        );

        // Store a valid reaction
        create_and_store_message(
            &conn,
            &group.group_id,
            vec![4],
            TestContentGenerator::reaction_content(&msg_id_hex, "👍", ReactionAction::Added),
            1000,
            "reactor1",
        );

        // Store an invalid reaction (malformed content)
        let reaction_type = xmtp_content_types::reaction::ReactionCodec::content_type();
        let invalid_reaction = create_test_message_raw(
            &group.group_id,
            vec![5],
            b"invalid reaction bytes".to_vec(),
            now_ns() + 2000,
            "reactor2".to_string(),
            Some(reaction_type),
            Some(msg_id.clone()),
        );
        invalid_reaction.store(&conn).unwrap();

        // Query messages
        let messages = group.find_messages_v2(&MsgQueryArgs::default()).unwrap();

        // Should have 1 message total
        assert_message_count(&messages, 1);

        // Find the original message
        let original_msg = find_message_by_id(&messages, &msg_id);

        // Should only have the valid reaction, invalid one should be filtered out
        assert_reaction_count(original_msg, 1);
        assert_has_reaction(original_msg, "👍");
    }

    #[tokio::test]
    async fn test_find_messages_chain_of_replies() {
        let (group, context) = setup_test_group().await;
        let conn = context.db();

        // Store original message
        let msg1_id = vec![1];
        let msg1_id_hex = msg1_id.encode_hex();
        create_and_store_message(
            &conn,
            &group.group_id,
            msg1_id.clone(),
            TestContentGenerator::text_content("Original message"),
            0,
            "sender1",
        );

        // Store first reply (reply to original)
        let msg2_id = vec![2];
        let msg2_id_hex = msg2_id.encode_hex();
        create_and_store_message(
            &conn,
            &group.group_id,
            msg2_id.clone(),
            TestContentGenerator::reply_content(
                &msg1_id_hex,
                TextCodec::content_type(),
                b"First reply".to_vec(),
            ),
            1000,
            "replier1",
        );

        // Store second reply (reply to first reply)
        let msg3_id = vec![3];
        create_and_store_message(
            &conn,
            &group.group_id,
            msg3_id.clone(),
            TestContentGenerator::reply_content(
                &msg2_id_hex,
                TextCodec::content_type(),
                b"Second reply - reply to reply".to_vec(),
            ),
            2000,
            "replier2",
        );

        // Query messages
        let messages = group.find_messages_v2(&MsgQueryArgs::default()).unwrap();

        assert_message_count(&messages, 3);

        // Find the first reply
        let first_reply = find_message_by_id(&messages, &msg2_id);
        if let MessageBody::Reply(reply) = &first_reply.content {
            // Should have reference to original message
            assert!(reply.in_reply_to.is_some());
            let referenced = reply.in_reply_to.as_ref().unwrap();
            assert_eq!(referenced.metadata.id, msg1_id);

            // The referenced message should NOT have its own in_reply_to (no recursive resolution)
            if let MessageBody::Text(_) = &referenced.content {
                // Expected: original message is just text
            } else {
                panic!("Expected text in first layer reference");
            }
        } else {
            panic!("Expected reply message");
        }

        // Find the second reply (reply to reply)
        let second_reply = find_message_by_id(&messages, &msg3_id);
        if let MessageBody::Reply(reply) = &second_reply.content {
            // Should have reference to first reply
            assert!(reply.in_reply_to.is_some());
            let referenced = reply.in_reply_to.as_ref().unwrap();
            assert_eq!(referenced.metadata.id, msg2_id);

            // The referenced message should be a Reply but its in_reply_to should be None (not resolved recursively)
            if let MessageBody::Reply(inner_reply) = &referenced.content {
                // We don't recursively resolve, so the inner reply's in_reply_to should be None
                assert!(
                    inner_reply.in_reply_to.is_none(),
                    "Should not recursively resolve replies - got {:?}",
                    inner_reply.in_reply_to
                );
            } else {
                panic!("Expected reply in second layer reference");
            }
        } else {
            panic!("Expected reply message");
        }
    }

    #[test]
    fn test_filter_out_hidden_message_types_from_query() {
        // Test with no content_types specified (should use all types minus hidden)
        let query = MsgQueryArgs::default();
        let filtered = filter_out_hidden_message_types_from_query(&query);

        assert!(filtered.content_types.is_some());
        let types = filtered.content_types.unwrap();
        assert!(!types.contains(&DbContentType::Reaction));
        assert!(!types.contains(&DbContentType::ReadReceipt));
        assert!(types.contains(&DbContentType::Text));
        assert!(types.contains(&DbContentType::Attachment));
        assert!(types.contains(&DbContentType::Reply));

        // Test with specific content_types including hidden ones
        let query_with_types = MsgQueryArgs::builder()
            .content_types(Some(vec![
                DbContentType::Text,
                DbContentType::Reaction,
                DbContentType::Attachment,
                DbContentType::ReadReceipt,
                DbContentType::Reply,
            ]))
            .build()
            .unwrap();
        let filtered = filter_out_hidden_message_types_from_query(&query_with_types);

        assert!(filtered.content_types.is_some());
        let types = filtered.content_types.unwrap();
        assert_eq!(types.len(), 3);
        assert!(types.contains(&DbContentType::Text));
        assert!(types.contains(&DbContentType::Attachment));
        assert!(types.contains(&DbContentType::Reply));
        assert!(!types.contains(&DbContentType::Reaction));
        assert!(!types.contains(&DbContentType::ReadReceipt));

        // Test with only non-hidden types (should remain unchanged)
        let query_no_hidden = MsgQueryArgs::builder()
            .content_types(Some(vec![
                DbContentType::Text,
                DbContentType::Attachment,
                DbContentType::RemoteAttachment,
            ]))
            .build()
            .unwrap();
        let filtered = filter_out_hidden_message_types_from_query(&query_no_hidden);

        assert!(filtered.content_types.is_some());
        let types = filtered.content_types.unwrap();
        assert_eq!(types.len(), 3);
        assert!(types.contains(&DbContentType::Text));
        assert!(types.contains(&DbContentType::Attachment));
        assert!(types.contains(&DbContentType::RemoteAttachment));
    }
}
