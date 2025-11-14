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

    let data = xmtp_common::rand_vec::<100000>();
    dm.send(data.clone(), FfiSendMessageOpts::default())
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
    assert!(bo_msgs.iter().any(|msg| msg.content.eq(&data)));
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
