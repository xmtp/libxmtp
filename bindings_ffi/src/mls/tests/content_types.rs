//! Tests for different message content types (reactions, attachments, replies, receipts)

use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_can_send_and_receive_reaction() {
    // Create two test clients
    let alix = new_test_client().await;
    let bo = new_test_client().await;

    // Create a conversation between them
    let alix_conversation = alix
        .conversations()
        .create_group(
            vec![bo.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();
    // Send initial message to react to
    let mut buf = Vec::new();
    TextCodec::encode("Hello world".to_string())
        .unwrap()
        .encode(&mut buf)
        .unwrap();
    alix_conversation
        .send(buf, FfiSendMessageOpts::default())
        .await
        .unwrap();

    // Have Bo sync to get the conversation and message
    bo.conversations().sync().await.unwrap();
    let bo_conversation = bo.conversation(alix_conversation.id()).unwrap();
    bo_conversation.sync().await.unwrap();

    // Get the message to react to
    let messages = bo_conversation
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();
    let message_to_react_to = &messages[1];

    // Create and send reaction
    let ffi_reaction = FfiReactionPayload {
        reference: hex::encode(message_to_react_to.id.clone()),
        reference_inbox_id: alix.inbox_id(),
        action: FfiReactionAction::Added,
        content: "👍".to_string(),
        schema: FfiReactionSchema::Unicode,
    };
    let bytes_to_send = encode_reaction(ffi_reaction).unwrap();
    bo_conversation
        .send(bytes_to_send, FfiSendMessageOpts::default())
        .await
        .unwrap();

    // Have Alix sync to get the reaction
    alix_conversation.sync().await.unwrap();

    // Get reactions for the original message
    let messages = alix_conversation
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();

    // Verify reaction details
    assert_eq!(messages.len(), 3);
    let received_reaction = &messages[2];
    let message_content = received_reaction.content.clone();
    let reaction = decode_reaction(message_content).unwrap();
    assert_eq!(reaction.content, "👍");
    assert_eq!(reaction.action, FfiReactionAction::Added);
    assert_eq!(reaction.reference_inbox_id, alix.inbox_id());
    assert_eq!(
        reaction.reference,
        hex::encode(message_to_react_to.id.clone())
    );
    assert_eq!(reaction.schema, FfiReactionSchema::Unicode);

    // Test find_messages_with_reactions query
    let messages_with_reactions: Vec<FfiMessageWithReactions> = alix_conversation
        .find_messages_with_reactions(FfiListMessagesOptions::default())
        .unwrap();
    assert_eq!(messages_with_reactions.len(), 2);
    let message_with_reactions = &messages_with_reactions[1];
    assert_eq!(message_with_reactions.reactions.len(), 1);
    let message_content = message_with_reactions.reactions[0].content.clone();
    let slice: &[u8] = message_content.as_slice();
    let encoded_content = EncodedContent::decode(slice).unwrap();
    let reaction = ReactionV2::decode(encoded_content.content.as_slice()).unwrap();
    assert_eq!(reaction.content, "👍");
    assert_eq!(reaction.action, ReactionAction::Added as i32);
    assert_eq!(reaction.reference_inbox_id, alix.inbox_id());
    assert_eq!(
        reaction.reference,
        hex::encode(message_to_react_to.id.clone())
    );
    assert_eq!(reaction.schema, ReactionSchema::Unicode as i32);
}

#[tokio::test]
async fn test_reaction_encode_decode() {
    // Create a test reaction
    let original_reaction = FfiReactionPayload {
        reference: "123abc".to_string(),
        reference_inbox_id: "test_inbox_id".to_string(),
        action: FfiReactionAction::Added,
        content: "👍".to_string(),
        schema: FfiReactionSchema::Unicode,
    };

    // Encode the reaction
    let encoded_bytes =
        encode_reaction(original_reaction.clone()).expect("Should encode reaction successfully");

    // Decode the reaction
    let decoded_reaction =
        decode_reaction(encoded_bytes).expect("Should decode reaction successfully");

    // Verify the decoded reaction matches the original
    assert_eq!(decoded_reaction.reference, original_reaction.reference);
    assert_eq!(
        decoded_reaction.reference_inbox_id,
        original_reaction.reference_inbox_id
    );
    assert!(matches!(decoded_reaction.action, FfiReactionAction::Added));
    assert_eq!(decoded_reaction.content, original_reaction.content);
    assert!(matches!(
        decoded_reaction.schema,
        FfiReactionSchema::Unicode
    ));
}

#[tokio::test]
async fn test_multi_remote_attachment_encode_decode() {
    // Create a test attachment
    let original_attachment = FfiMultiRemoteAttachment {
        attachments: vec![
            FfiRemoteAttachmentInfo {
                filename: Some("test1.jpg".to_string()),
                content_length: Some(1000),
                secret: vec![1, 2, 3],
                content_digest: "123".to_string(),
                nonce: vec![7, 8, 9],
                salt: vec![1, 2, 3],
                scheme: "https".to_string(),
                url: "https://example.com/test1.jpg".to_string(),
            },
            FfiRemoteAttachmentInfo {
                filename: Some("test2.pdf".to_string()),
                content_length: Some(2000),
                secret: vec![4, 5, 6],
                content_digest: "456".to_string(),
                nonce: vec![10, 11, 12],
                salt: vec![1, 2, 3],
                scheme: "https".to_string(),
                url: "https://example.com/test2.pdf".to_string(),
            },
        ],
    };

    // Encode the attachment
    let encoded_bytes = encode_multi_remote_attachment(original_attachment.clone())
        .expect("Should encode multi remote attachment successfully");

    // Decode the attachment
    let decoded_attachment = decode_multi_remote_attachment(encoded_bytes)
        .expect("Should decode multi remote attachment successfully");

    assert_eq!(
        decoded_attachment.attachments.len(),
        original_attachment.attachments.len()
    );

    for (decoded, original) in decoded_attachment
        .attachments
        .iter()
        .zip(original_attachment.attachments.iter())
    {
        assert_eq!(decoded.filename, original.filename);
        assert_eq!(decoded.content_digest, original.content_digest);
        assert_eq!(decoded.nonce, original.nonce);
        assert_eq!(decoded.scheme, original.scheme);
        assert_eq!(decoded.url, original.url);
    }
}

#[tokio::test]
async fn test_transaction_reference_roundtrip() {
    let original = FfiTransactionReference {
        namespace: Some("eip155".to_string()),
        network_id: "1".to_string(),
        reference: "0xabc123".to_string(),
        metadata: Some(FfiTransactionMetadata {
            transaction_type: "transfer".to_string(),
            currency: "ETH".to_string(),
            amount: 0.42,
            decimals: 18,
            from_address: "0xfrom".to_string(),
            to_address: "0xto".to_string(),
        }),
    };

    let encoded = encode_transaction_reference(original.clone()).unwrap();
    let decoded = decode_transaction_reference(encoded).unwrap();

    assert_eq!(original.reference, decoded.reference);
    assert_eq!(
        original.metadata.as_ref().unwrap().currency,
        decoded.metadata.as_ref().unwrap().currency
    );
}

#[tokio::test]
async fn test_attachment_roundtrip() {
    let original = FfiAttachment {
        filename: Some("test.txt".to_string()),
        mime_type: "text/plain".to_string(),
        content: "Hello, World!".as_bytes().to_vec(),
    };

    let encoded = encode_attachment(original.clone()).unwrap();
    let decoded = decode_attachment(encoded).unwrap();

    assert_eq!(original.filename, decoded.filename);
    assert_eq!(original.mime_type, decoded.mime_type);
    assert_eq!(original.content, decoded.content);
}

#[tokio::test]
async fn test_reply_roundtrip() {
    let original = FfiReply {
        reference: "0x1234567890abcdef".to_string(),
        reference_inbox_id: Some("test_inbox_id".to_string()),
        content: FfiEncodedContent {
            type_id: None,
            parameters: HashMap::new(),
            fallback: Some("This is a reply".to_string()),
            compression: None,
            content: b"This is a reply".to_vec(),
        },
    };

    let encoded = encode_reply(original.clone()).unwrap();
    let decoded = decode_reply(encoded).unwrap();

    assert_eq!(original.reference, decoded.reference);
    assert_eq!(original.reference_inbox_id, decoded.reference_inbox_id);
    assert_eq!(original.content, decoded.content);
}

#[tokio::test]
async fn test_read_receipt_roundtrip() {
    let original = FfiReadReceipt {};

    let encoded = encode_read_receipt(original.clone()).unwrap();
    decode_read_receipt(encoded).unwrap();
}

#[tokio::test]
async fn test_remote_attachment_roundtrip() {
    let original = FfiRemoteAttachment {
        filename: Some("remote_file.txt".to_string()),
        content_length: 2048,
        url: "https://example.com/file.txt".to_string(),
        content_digest: "sha256:abc123def456".to_string(),
        scheme: "https".to_string(),
        secret: vec![1, 2, 3, 4, 5],
        nonce: vec![6, 7, 8, 9, 10],
        salt: vec![11, 12, 13, 14, 15],
    };

    let encoded = encode_remote_attachment(original.clone()).unwrap();
    let decoded = decode_remote_attachment(encoded).unwrap();

    assert_eq!(original.filename, decoded.filename);
    assert_eq!(original.content_length, decoded.content_length);
    assert_eq!(original.url, decoded.url);
    assert_eq!(original.content_digest, decoded.content_digest);
    assert_eq!(original.secret, decoded.secret);
    assert_eq!(original.nonce, decoded.nonce);
    assert_eq!(original.salt, decoded.salt);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_long_messages() {
    let alix = Tester::new().await;
    let bo = Tester::new().await;

    let dm = alix
        .conversations()
        .find_or_create_dm_by_inbox_id(bo.inbox_id(), FfiCreateDMOptions::default())
        .await
        .unwrap();

    bo.conversations()
        .sync_all_conversations(None)
        .await
        .unwrap();

    let mut data = String::new();
    let mut i = 0;
    while data.len() < 100_000 {
        data.push_str(&format!("{i:4}: This is a test message that is really long for testing purposes and should be truncated if ever logged in tests\n"));
        i += 1;
    }
    dm.send(data.as_bytes().to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();

    let bo_dm = bo
        .conversations()
        .find_or_create_dm_by_inbox_id(alix.inbox_id(), FfiCreateDMOptions::default())
        .await
        .unwrap();

    bo_dm.sync().await.unwrap();
    let bo_msgs = bo_dm
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();
    assert!(bo_msgs.iter().any(|msg| msg.content.eq(data.as_bytes())));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_find_enriched_messages_with_reactions() {
    let alix = Tester::new().await;
    let bo = Tester::new().await;

    // Create a group with both participants
    let alix_group = alix
        .client
        .conversations()
        .create_group(
            vec![bo.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    // Bo accepts the invitation
    bo.client.conversations().sync().await.unwrap();
    let bo_group = bo.client.conversation(alix_group.id()).unwrap();

    // Send a few initial messages using proper text encoding
    let text1 = TextCodec::encode("Message 1".to_string()).unwrap();
    alix_group
        .send(
            encoded_content_to_bytes(text1),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();

    let text2 = TextCodec::encode("Message 2".to_string()).unwrap();
    alix_group
        .send(
            encoded_content_to_bytes(text2),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();

    let text3 = TextCodec::encode("Message 3".to_string()).unwrap();
    bo_group
        .send(
            encoded_content_to_bytes(text3),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();

    // Sync both clients
    alix_group.sync().await.unwrap();
    bo_group.sync().await.unwrap();

    // Get messages to react to
    let all_messages = bo_group
        .find_enriched_messages(FfiListMessagesOptions::default())
        .unwrap();

    // Filter for just text messages to react to
    let text_messages: Vec<_> = all_messages
        .into_iter()
        .filter(|m| {
            m.kind() == FfiGroupMessageKind::Application && m.content_type_id().type_id == "text"
        })
        .collect();

    assert_eq!(text_messages.len(), 3);
    let messages = text_messages;

    // Add reactions to different messages
    let reaction1 = FfiReactionPayload {
        reference: hex::encode(messages[0].id()),
        reference_inbox_id: alix.client.inbox_id(),
        action: FfiReactionAction::Added,
        content: "👍".to_string(),
        schema: FfiReactionSchema::Unicode,
    };
    bo_group
        .send(
            encode_reaction(reaction1).unwrap(),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();

    let reaction2 = FfiReactionPayload {
        reference: hex::encode(messages[1].id()),
        reference_inbox_id: alix.client.inbox_id(),
        action: FfiReactionAction::Added,
        content: "❤️".to_string(),
        schema: FfiReactionSchema::Unicode,
    };
    alix_group
        .send(
            encode_reaction(reaction2).unwrap(),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();

    // Remove a reaction
    let reaction3 = FfiReactionPayload {
        reference: hex::encode(messages[0].id()),
        reference_inbox_id: alix.client.inbox_id(),
        action: FfiReactionAction::Removed,
        content: "👍".to_string(),
        schema: FfiReactionSchema::Unicode,
    };
    bo_group
        .send(
            encode_reaction(reaction3).unwrap(),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();

    // Sync and verify messages with reactions
    alix_group.sync().await.unwrap();
    bo_group.sync().await.unwrap();

    // Test find_enriched_messages returns all messages including reactions
    let all_messages = alix_group
        .find_enriched_messages(FfiListMessagesOptions::default())
        .unwrap();

    // Should have 1 membership change + 3 text messages
    assert_eq!(all_messages.len(), 4);

    let message_0 = all_messages
        .iter()
        .find(|m| m.id() == messages[0].id())
        .unwrap();

    // Verify reaction content
    for reaction in message_0.reactions() {
        if let FfiDecodedMessageContent::Reaction(reaction) = reaction.content() {
            assert!(reaction.content == "👍" || reaction.content == "❤️");
        } else {
            panic!("Expected reaction content type");
        }
    }

    assert_eq!(message_0.reactions().len(), 2);
    assert_eq!(message_0.reaction_count(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_find_enriched_messages_with_replies() {
    let alix = Tester::new().await;
    let bo = Tester::new().await;

    // Create a DM conversation
    let alix_dm = alix
        .client
        .conversations()
        .find_or_create_dm(bo.account_identifier.clone(), FfiCreateDMOptions::default())
        .await
        .unwrap();

    // Bo finds the DM
    bo.client.conversations().sync().await.unwrap();
    let bo_dm = bo.client.dm_conversation(alix.client.inbox_id()).unwrap();

    // Send initial messages using proper text encoding
    let text1 = TextCodec::encode("Hello!".to_string()).unwrap();
    let msg1_id = alix_dm
        .send(
            encoded_content_to_bytes(text1),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();

    let text2 = TextCodec::encode("Hi there!".to_string()).unwrap();
    let msg2_id = bo_dm
        .send(
            encoded_content_to_bytes(text2),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();

    let text3 = TextCodec::encode("How are you?".to_string()).unwrap();
    alix_dm
        .send(
            encoded_content_to_bytes(text3),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();

    // Sync both clients
    alix_dm.sync().await.unwrap();
    bo_dm.sync().await.unwrap();

    // Get messages to reply to
    let messages = alix_dm
        .find_enriched_messages(FfiListMessagesOptions::default())
        .unwrap();
    // 3 messages sent + group membership change
    assert_eq!(messages.len(), 4);

    // Create replies to different messages
    let reply1 = FfiReply {
        reference: hex::encode(msg1_id),
        reference_inbox_id: Some(alix.client.inbox_id()),
        content: TextCodec::encode("Replying to Hello".to_string())
            .unwrap()
            .into(),
    };
    bo_dm
        .send(encode_reply(reply1).unwrap(), FfiSendMessageOpts::default())
        .await
        .unwrap();

    let reply2 = FfiReply {
        reference: hex::encode(msg2_id),
        reference_inbox_id: Some(bo.client.inbox_id()),
        content: TextCodec::encode("Replying to Hi there".to_string())
            .unwrap()
            .into(),
    };
    alix_dm
        .send(encode_reply(reply2).unwrap(), FfiSendMessageOpts::default())
        .await
        .unwrap();

    // Add a reaction to a reply
    alix_dm.sync().await.unwrap();
    let updated_messages = alix_dm
        .find_enriched_messages(FfiListMessagesOptions::default())
        .unwrap();

    // Find the first reply message
    updated_messages
        .iter()
        .find(|m| {
            if let FfiDecodedMessageContent::Reply(reply) = m.content() {
                // Check if the content matches
                if let Some(FfiDecodedMessageBody::Text(text)) = &reply.content {
                    text.content == "Replying to Hello"
                } else {
                    false
                }
            } else {
                false
            }
        })
        .unwrap();
}

#[tokio::test]
async fn test_intent_codec() {
    use prost::Message;
    use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

    // Test with complex metadata
    let intent_with_metadata = FfiIntent {
        id: "intent1".to_string(),
        action_id: "action1".to_string(),
        metadata: Some(r#"{"nested":{"value":"test"},"array":[1,true,null]}"#.to_string()),
    };
    let encoded = encode_intent(intent_with_metadata.clone()).unwrap();
    let decoded = decode_intent(encoded).unwrap();
    assert_eq!(decoded.id, intent_with_metadata.id);
    assert_eq!(decoded.action_id, intent_with_metadata.action_id);
    let original: serde_json::Value =
        serde_json::from_str(intent_with_metadata.metadata.as_ref().unwrap()).unwrap();
    let decoded_meta: serde_json::Value =
        serde_json::from_str(decoded.metadata.as_ref().unwrap()).unwrap();
    assert_eq!(decoded_meta, original);

    // Test without metadata
    let intent_no_metadata = FfiIntent {
        id: "intent2".to_string(),
        action_id: "action2".to_string(),
        metadata: None,
    };
    let encoded = encode_intent(intent_no_metadata.clone()).unwrap();
    let decoded = decode_intent(encoded).unwrap();
    assert_eq!(decoded.metadata, None);

    // Test metadata size limit (>10KB)
    let large_metadata = format!(r#"{{"data":"{}"}}"#, "x".repeat(11 * 1024));
    let intent_large = FfiIntent {
        id: "intent3".to_string(),
        action_id: "action3".to_string(),
        metadata: Some(large_metadata),
    };
    assert!(
        encode_intent(intent_large)
            .unwrap_err()
            .to_string()
            .contains("too large")
    );

    // Test malformed JSON
    let intent_invalid = FfiIntent {
        id: "intent4".to_string(),
        action_id: "action4".to_string(),
        metadata: Some(r#"{"unclosed"#.to_string()),
    };
    assert!(encode_intent(intent_invalid).is_err());

    // Simulate decoding malformed metadata in encoded content

    // metadata field is a string instead of an object
    let malformed_json_wrong_type = r#"{
        "id": "intent1",
        "action_id": "action1",
        "metadata": "this should be an object not a string"
    }"#;

    let encoded_content = EncodedContent {
        content: malformed_json_wrong_type.as_bytes().to_vec(),
        ..Default::default()
    };

    let mut buf = Vec::new();
    encoded_content.encode(&mut buf).unwrap();

    // Decoding should fail gracefully
    let result = decode_intent(buf);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Unable to deserialize")
    );

    // metadata is an array instead of an object
    let malformed_json_array = r#"{
        "id": "intent2",
        "action_id": "action2",
        "metadata": ["array", "instead", "of", "object"]
    }"#;

    let encoded_content = EncodedContent {
        content: malformed_json_array.as_bytes().to_vec(),
        ..Default::default()
    };

    let mut buf = Vec::new();
    encoded_content.encode(&mut buf).unwrap();

    let result = decode_intent(buf);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Unable to deserialize")
    );

    // metadata has invalid JSON syntax
    let malformed_json_invalid = r#"{
        "id": "intent3",
        "action_id": "action3",
        "metadata": {"foo": bar
    }"#;

    let encoded_content = EncodedContent {
        content: malformed_json_invalid.as_bytes().to_vec(),
        ..Default::default()
    };

    let mut buf = Vec::new();
    encoded_content.encode(&mut buf).unwrap();

    // Decoding should fail due to invalid JSON syntax
    let result = decode_intent(buf);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Unable to deserialize")
    );
}

#[tokio::test]
async fn test_actions_codec() {
    use chrono::NaiveDate;
    use prost::Message;
    use xmtp_content_types::actions::{Action, Actions};
    use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

    // Test basic encode/decode
    let actions = FfiActions {
        id: "actions1".to_string(),
        description: "Test".to_string(),
        actions: vec![
            FfiAction {
                id: "a1".to_string(),
                label: "Action 1".to_string(),
                image_url: Some("https://example.com/image.png".to_string()),
                style: Some(FfiActionStyle::Primary),
                expires_at_ns: Some(1_234_567_890_000_000_000),
            },
            FfiAction {
                id: "a2".to_string(),
                label: "Action 2".to_string(),
                image_url: None,
                style: None,
                expires_at_ns: None,
            },
        ],
        expires_at_ns: Some(1_700_000_000_000_000_000),
    };
    let encoded = encode_actions(actions.clone()).unwrap();
    let decoded = decode_actions(encoded).unwrap();
    assert_eq!(decoded.id, actions.id);
    assert_eq!(decoded.actions.len(), 2);
    assert_eq!(
        decoded.actions[0].expires_at_ns,
        actions.actions[0].expires_at_ns
    );

    // Test min/max timestamps
    let min_ns = i64::MIN;
    let max_ns = i64::MAX;
    let actions_minmax = FfiActions {
        id: "actions2".to_string(),
        description: "MinMax".to_string(),
        actions: vec![FfiAction {
            id: "a1".to_string(),
            label: "A1".to_string(),
            image_url: None,
            style: None,
            expires_at_ns: Some(min_ns),
        }],
        expires_at_ns: Some(max_ns),
    };
    let encoded = encode_actions(actions_minmax).unwrap();
    let decoded = decode_actions(encoded).unwrap();
    assert_eq!(decoded.expires_at_ns, Some(max_ns));
    assert_eq!(decoded.actions[0].expires_at_ns, Some(min_ns));

    // Test timestamp 0 (Unix epoch is valid)
    let actions_zero = FfiActions {
        id: "actions3".to_string(),
        description: "Zero".to_string(),
        actions: vec![FfiAction {
            id: "a1".to_string(),
            label: "A1".to_string(),
            image_url: None,
            style: None,
            expires_at_ns: Some(0),
        }],
        expires_at_ns: Some(0),
    };
    let encoded = encode_actions(actions_zero).unwrap();
    let decoded = decode_actions(encoded).unwrap();
    assert_eq!(decoded.expires_at_ns, Some(0));
    assert_eq!(decoded.actions[0].expires_at_ns, Some(0));

    // empty actions
    let empty = FfiActions {
        id: "empty".to_string(),
        description: "Empty".to_string(),
        actions: vec![],
        expires_at_ns: None,
    };
    assert!(
        encode_actions(empty)
            .unwrap_err()
            .to_string()
            .contains("at least one")
    );

    // too many actions (>10)
    let too_many = FfiActions {
        id: "many".to_string(),
        description: "Many".to_string(),
        actions: (0..11)
            .map(|i| FfiAction {
                id: format!("a{}", i),
                label: format!("A{}", i),
                image_url: None,
                style: None,
                expires_at_ns: None,
            })
            .collect(),
        expires_at_ns: None,
    };
    assert!(
        encode_actions(too_many)
            .unwrap_err()
            .to_string()
            .contains("exceed 10")
    );

    // duplicate IDs
    let duplicates = FfiActions {
        id: "dup".to_string(),
        description: "Dup".to_string(),
        actions: vec![
            FfiAction {
                id: "same".to_string(),
                label: "A1".to_string(),
                image_url: None,
                style: None,
                expires_at_ns: None,
            },
            FfiAction {
                id: "same".to_string(),
                label: "A2".to_string(),
                image_url: None,
                style: None,
                expires_at_ns: None,
            },
        ],
        expires_at_ns: None,
    };
    assert!(
        encode_actions(duplicates)
            .unwrap_err()
            .to_string()
            .contains("unique")
    );

    // invalid timestamp format - string instead of proper datetime object
    let malformed_timestamp_string = r#"{
        "id": "actions1",
        "description": "Test",
        "actions": [
            {
                "id": "a1",
                "label": "Action 1",
                "image_url": null,
                "style": null,
                "expires_at": "not a valid datetime"
            }
        ],
        "expires_at": "also not valid"
    }"#;

    let encoded_content = EncodedContent {
        content: malformed_timestamp_string.as_bytes().to_vec(),
        ..Default::default()
    };

    let mut buf = Vec::new();
    encoded_content.encode(&mut buf).unwrap();

    let result = decode_actions(buf);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Unable to deserialize")
    );

    // invalid datetime value - number instead of datetime object
    let malformed_timestamp_number = r#"{
        "id": "actions2",
        "description": "Test",
        "actions": [
            {
                "id": "a1",
                "label": "Action 1",
                "image_url": null,
                "style": null,
                "expires_at": 12345
            }
        ],
        "expires_at": 67890
    }"#;

    let encoded_content = EncodedContent {
        content: malformed_timestamp_number.as_bytes().to_vec(),
        ..Default::default()
    };

    let mut buf = Vec::new();
    encoded_content.encode(&mut buf).unwrap();

    let result = decode_actions(buf);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Unable to deserialize")
    );

    // malformed JSON in timestamp field
    let malformed_json_timestamp = r#"{
        "id": "actions3",
        "description": "Test",
        "actions": [
            {
                "id": "a1",
                "label": "Action 1",
                "image_url": null,
                "style": null,
                "expires_at": {invalid json}
            }
        ],
        "expires_at": null
    }"#;

    let encoded_content = EncodedContent {
        content: malformed_json_timestamp.as_bytes().to_vec(),
        ..Default::default()
    };

    let mut buf = Vec::new();
    encoded_content.encode(&mut buf).unwrap();

    let result = decode_actions(buf);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Unable to deserialize")
    );

    // Year 2800 is > 584 years from now and will overflow i64 nanoseconds
    let far_future_date = NaiveDate::from_ymd_opt(2800, 1, 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();

    let actions_far_future = Actions {
        id: "far_future_actions".to_string(),
        description: "Test far future".to_string(),
        actions: vec![Action {
            id: "a1".to_string(),
            label: "Action 1".to_string(),
            image_url: None,
            style: None,
            expires_at: None,
        }],
        expires_at: Some(far_future_date),
    };

    // Conversion should fail because the timestamp is out of range
    let result: Result<FfiActions, _> = actions_far_future.try_into();
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("out of valid range")
    );

    // Test egress: action (not actions) with expiration > 584 years in the future
    let actions_with_far_future_action = Actions {
        id: "far_future_action".to_string(),
        description: "Test far future action".to_string(),
        actions: vec![Action {
            id: "a1".to_string(),
            label: "Action 1".to_string(),
            image_url: None,
            style: None,
            expires_at: Some(far_future_date),
        }],
        expires_at: None,
    };

    // Conversion should fail because one of the actions has an out-of-range timestamp
    let result: Result<FfiActions, _> = actions_with_far_future_action.try_into();
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("out of valid range")
    );
}

#[tokio::test]
async fn test_group_updated_codec() {
    fn encode_group_updated(group_updated: FfiGroupUpdated) -> Result<Vec<u8>, GenericError> {
        let encoded = GroupUpdatedCodec::encode(group_updated.into())
            .map_err(|e| GenericError::Generic { err: e.to_string() })?;

        let mut buf = Vec::new();
        encoded
            .encode(&mut buf)
            .map_err(|e| GenericError::Generic { err: e.to_string() })?;

        Ok(buf)
    }

    // Test basic roundtrip with typical data
    let basic = FfiGroupUpdated {
        initiated_by_inbox_id: "inbox_123".to_string(),
        added_inboxes: vec![
            FfiInbox {
                inbox_id: "inbox_456".to_string(),
            },
            FfiInbox {
                inbox_id: "inbox_789".to_string(),
            },
        ],
        removed_inboxes: vec![FfiInbox {
            inbox_id: "inbox_000".to_string(),
        }],
        left_inboxes: vec![],
        metadata_field_changes: vec![
            FfiMetadataFieldChange {
                field_name: "group_name".to_string(),
                old_value: Some("Old Name".to_string()),
                new_value: Some("New Name".to_string()),
            },
            FfiMetadataFieldChange {
                field_name: "description".to_string(),
                old_value: None,
                new_value: Some("Added description".to_string()),
            },
        ],
        added_admin_inboxes: vec![FfiInbox {
            inbox_id: "inbox_admin_1".to_string(),
        }],
        removed_admin_inboxes: vec![FfiInbox {
            inbox_id: "inbox_old_admin".to_string(),
        }],
        added_super_admin_inboxes: vec![FfiInbox {
            inbox_id: "inbox_super_admin_1".to_string(),
        }],
        removed_super_admin_inboxes: vec![FfiInbox {
            inbox_id: "inbox_old_super_admin".to_string(),
        }],
    };
    let encoded = encode_group_updated(basic.clone()).unwrap();
    let decoded = decode_group_updated(encoded).unwrap();
    assert_eq!(decoded.initiated_by_inbox_id, basic.initiated_by_inbox_id);
    assert_eq!(decoded.added_inboxes.len(), 2);
    assert_eq!(
        decoded.added_inboxes[0].inbox_id,
        basic.added_inboxes[0].inbox_id
    );
    assert_eq!(decoded.removed_inboxes.len(), 1);
    assert_eq!(decoded.metadata_field_changes.len(), 2);
    assert_eq!(decoded.added_admin_inboxes.len(), 1);
    assert_eq!(decoded.added_admin_inboxes[0].inbox_id, "inbox_admin_1");
    assert_eq!(decoded.removed_admin_inboxes.len(), 1);
    assert_eq!(decoded.removed_admin_inboxes[0].inbox_id, "inbox_old_admin");
    assert_eq!(decoded.added_super_admin_inboxes.len(), 1);
    assert_eq!(
        decoded.added_super_admin_inboxes[0].inbox_id,
        "inbox_super_admin_1"
    );
    assert_eq!(decoded.removed_super_admin_inboxes.len(), 1);
    assert_eq!(
        decoded.removed_super_admin_inboxes[0].inbox_id,
        "inbox_old_super_admin"
    );

    // Test with minimal data - all lists empty
    let minimal = FfiGroupUpdated {
        initiated_by_inbox_id: "initiator_inbox".to_string(),
        added_inboxes: vec![],
        removed_inboxes: vec![],
        left_inboxes: vec![],
        metadata_field_changes: vec![],
        added_admin_inboxes: vec![],
        removed_admin_inboxes: vec![],
        added_super_admin_inboxes: vec![],
        removed_super_admin_inboxes: vec![],
    };
    let encoded = encode_group_updated(minimal.clone()).unwrap();
    let decoded = decode_group_updated(encoded).unwrap();
    assert_eq!(decoded.initiated_by_inbox_id, minimal.initiated_by_inbox_id);
    assert_eq!(decoded.added_inboxes.len(), 0);
    assert_eq!(decoded.removed_inboxes.len(), 0);
    assert_eq!(decoded.left_inboxes.len(), 0);
    assert_eq!(decoded.metadata_field_changes.len(), 0);

    // Test with members leaving
    let with_left = FfiGroupUpdated {
        initiated_by_inbox_id: "inbox_admin".to_string(),
        added_inboxes: vec![],
        removed_inboxes: vec![],
        left_inboxes: vec![
            FfiInbox {
                inbox_id: "inbox_left_1".to_string(),
            },
            FfiInbox {
                inbox_id: "inbox_left_2".to_string(),
            },
        ],
        metadata_field_changes: vec![],
        added_admin_inboxes: vec![],
        removed_admin_inboxes: vec![],
        added_super_admin_inboxes: vec![],
        removed_super_admin_inboxes: vec![],
    };
    let encoded = encode_group_updated(with_left.clone()).unwrap();
    let decoded = decode_group_updated(encoded).unwrap();
    assert_eq!(decoded.left_inboxes.len(), 2);
    assert_eq!(
        decoded.left_inboxes[0].inbox_id,
        with_left.left_inboxes[0].inbox_id
    );
    assert_eq!(
        decoded.left_inboxes[1].inbox_id,
        with_left.left_inboxes[1].inbox_id
    );

    // Test metadata changes with various null value combinations
    let with_metadata = FfiGroupUpdated {
        initiated_by_inbox_id: "inbox_initiator".to_string(),
        added_inboxes: vec![],
        removed_inboxes: vec![],
        left_inboxes: vec![],
        metadata_field_changes: vec![
            // Field removed (had value, now null)
            FfiMetadataFieldChange {
                field_name: "removed_field".to_string(),
                old_value: Some("old value".to_string()),
                new_value: None,
            },
            // Field added (was null, now has value)
            FfiMetadataFieldChange {
                field_name: "added_field".to_string(),
                old_value: None,
                new_value: Some("new value".to_string()),
            },
            // Field changed (both values present)
            FfiMetadataFieldChange {
                field_name: "changed_field".to_string(),
                old_value: Some("before".to_string()),
                new_value: Some("after".to_string()),
            },
            // Both null (edge case)
            FfiMetadataFieldChange {
                field_name: "null_field".to_string(),
                old_value: None,
                new_value: None,
            },
        ],
        added_admin_inboxes: vec![],
        removed_admin_inboxes: vec![],
        added_super_admin_inboxes: vec![],
        removed_super_admin_inboxes: vec![],
    };
    let encoded = encode_group_updated(with_metadata.clone()).unwrap();
    let decoded = decode_group_updated(encoded).unwrap();
    assert_eq!(decoded.metadata_field_changes.len(), 4);
    assert_eq!(
        decoded.metadata_field_changes[0].old_value,
        Some("old value".to_string())
    );
    assert_eq!(decoded.metadata_field_changes[0].new_value, None);
    assert_eq!(decoded.metadata_field_changes[1].old_value, None);
    assert_eq!(
        decoded.metadata_field_changes[1].new_value,
        Some("new value".to_string())
    );
    assert_eq!(
        decoded.metadata_field_changes[2].old_value,
        Some("before".to_string())
    );
    assert_eq!(
        decoded.metadata_field_changes[2].new_value,
        Some("after".to_string())
    );
    assert_eq!(decoded.metadata_field_changes[3].old_value, None);
    assert_eq!(decoded.metadata_field_changes[3].new_value, None);

    // Test with all fields populated
    let complex = FfiGroupUpdated {
        initiated_by_inbox_id: "admin_inbox_id".to_string(),
        added_inboxes: vec![
            FfiInbox {
                inbox_id: "new_member_1".to_string(),
            },
            FfiInbox {
                inbox_id: "new_member_2".to_string(),
            },
            FfiInbox {
                inbox_id: "new_member_3".to_string(),
            },
        ],
        removed_inboxes: vec![FfiInbox {
            inbox_id: "removed_member_1".to_string(),
        }],
        left_inboxes: vec![
            FfiInbox {
                inbox_id: "left_member_1".to_string(),
            },
            FfiInbox {
                inbox_id: "left_member_2".to_string(),
            },
        ],
        metadata_field_changes: vec![
            FfiMetadataFieldChange {
                field_name: "group_name".to_string(),
                old_value: Some("Old Group Name".to_string()),
                new_value: Some("New Group Name".to_string()),
            },
            FfiMetadataFieldChange {
                field_name: "group_image_url".to_string(),
                old_value: Some("https://old-image.com/image.png".to_string()),
                new_value: Some("https://new-image.com/image.png".to_string()),
            },
            FfiMetadataFieldChange {
                field_name: "description".to_string(),
                old_value: None,
                new_value: Some("A new description was added".to_string()),
            },
        ],
        added_admin_inboxes: vec![],
        removed_admin_inboxes: vec![],
        added_super_admin_inboxes: vec![],
        removed_super_admin_inboxes: vec![],
    };
    let encoded = encode_group_updated(complex.clone()).unwrap();
    let decoded = decode_group_updated(encoded).unwrap();
    assert_eq!(decoded.initiated_by_inbox_id, complex.initiated_by_inbox_id);
    assert_eq!(decoded.added_inboxes.len(), 3);
    assert_eq!(decoded.removed_inboxes.len(), 1);
    assert_eq!(decoded.left_inboxes.len(), 2);
    assert_eq!(decoded.metadata_field_changes.len(), 3);
    for (i, inbox) in decoded.added_inboxes.iter().enumerate() {
        assert_eq!(inbox.inbox_id, complex.added_inboxes[i].inbox_id);
    }
    for (i, change) in decoded.metadata_field_changes.iter().enumerate() {
        assert_eq!(
            change.field_name,
            complex.metadata_field_changes[i].field_name
        );
        assert_eq!(
            change.old_value,
            complex.metadata_field_changes[i].old_value
        );
        assert_eq!(
            change.new_value,
            complex.metadata_field_changes[i].new_value
        );
    }

    // Test decoding invalid bytes
    let invalid_bytes = vec![0xFF, 0xFF, 0xFF, 0xFF];
    let result = decode_group_updated(invalid_bytes);
    assert!(result.is_err());
}

#[tokio::test]
async fn test_leave_request_decode() {
    use prost::Message;
    use xmtp_content_types::ContentCodec;
    use xmtp_content_types::leave_request::LeaveRequestCodec;
    use xmtp_proto::xmtp::mls::message_contents::content_types::LeaveRequest;

    // Test decoding leave request with no authenticated note
    let leave_request_no_note = LeaveRequest {
        authenticated_note: None,
    };
    let encoded = LeaveRequestCodec::encode(leave_request_no_note).unwrap();
    let mut buf = Vec::new();
    encoded.encode(&mut buf).unwrap();

    let decoded = decode_leave_request(buf).unwrap();
    assert!(decoded.authenticated_note.is_none());

    // Test decoding leave request with authenticated note
    let note_data = b"I am leaving because of reasons".to_vec();
    let leave_request_with_note = LeaveRequest {
        authenticated_note: Some(note_data.clone()),
    };
    let encoded = LeaveRequestCodec::encode(leave_request_with_note).unwrap();
    let mut buf = Vec::new();
    encoded.encode(&mut buf).unwrap();

    let decoded = decode_leave_request(buf).unwrap();
    assert_eq!(decoded.authenticated_note, Some(note_data));

    // Test decoding invalid bytes
    let invalid_bytes = vec![0xFF, 0xFF, 0xFF, 0xFF];
    let result = decode_leave_request(invalid_bytes);
    assert!(result.is_err());
}

#[tokio::test]
async fn test_text_codec() {
    // Test basic text encoding/decoding
    let basic_text = "Hello, World!".to_string();
    let encoded = encode_text(basic_text.clone()).unwrap();
    let decoded = decode_text(encoded).unwrap();
    assert_eq!(decoded, basic_text);

    // Test empty string
    let empty_text = "".to_string();
    let encoded = encode_text(empty_text.clone()).unwrap();
    let decoded = decode_text(encoded).unwrap();
    assert_eq!(decoded, empty_text);

    // Test text with unicode characters
    let unicode_text = "Hello 👋 World 🌍! こんにちは 🎉".to_string();
    let encoded = encode_text(unicode_text.clone()).unwrap();
    let decoded = decode_text(encoded).unwrap();
    assert_eq!(decoded, unicode_text);

    // Test text with newlines and special characters
    let special_text = "Line 1\nLine 2\tTabbed\r\nWindows newline".to_string();
    let encoded = encode_text(special_text.clone()).unwrap();
    let decoded = decode_text(encoded).unwrap();
    assert_eq!(decoded, special_text);

    // Test long text
    let long_text = "a".repeat(10000);
    let encoded = encode_text(long_text.clone()).unwrap();
    let decoded = decode_text(encoded).unwrap();
    assert_eq!(decoded, long_text);

    // Test text with various emoji combinations
    let emoji_text = "😀😃😄😁🥰😍🤩😎🤓🧐".to_string();
    let encoded = encode_text(emoji_text.clone()).unwrap();
    let decoded = decode_text(encoded).unwrap();
    assert_eq!(decoded, emoji_text);

    // Test text with mixed scripts
    let mixed_script = "English, العربية, 中文, Русский, हिन्दी".to_string();
    let encoded = encode_text(mixed_script.clone()).unwrap();
    let decoded = decode_text(encoded).unwrap();
    assert_eq!(decoded, mixed_script);

    // Test text with control characters
    let control_chars = "Text with \0 null \x01 and \x1F control chars".to_string();
    let encoded = encode_text(control_chars.clone()).unwrap();
    let decoded = decode_text(encoded).unwrap();
    assert_eq!(decoded, control_chars);

    // Test decoding invalid bytes
    let invalid_bytes = vec![0xFF, 0xFF, 0xFF, 0xFF];
    let result = decode_text(invalid_bytes);
    assert!(result.is_err());
}
