//! Message deletion tests: deleted message views.

use super::*;

/// Test that enrichment replaces deleted messages with placeholders
// verifies: PROC-060
#[xmtp_common::test(unwrap_try = true)]
async fn test_enrichment_with_deleted_messages() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;

    // Alix sends a message
    let text_content = TextCodec::encode("Secret message".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let message_id = alix_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    // Sync bo's group
    let bo_groups = bo.wait_for_welcomes().await?;
    let bo_group = &bo_groups[0];
    bo_group.sync().await?;

    // Verify Bola can see the original message content
    let messages = bo_group.find_messages_v2(&MsgQueryArgs {
        content_types: Some(vec![ContentType::Text]),
        ..Default::default()
    })?;
    assert_eq!(messages.len(), 1);

    let MessageBody::Text(text) = &messages[0].content else {
        panic!("Expected Text message body");
    };
    assert_eq!(text.content, "Secret message");

    // Bo reacts and replies, so the placeholder has relations to drop.
    use xmtp_content_types::{reaction::ReactionCodec, reply::ReplyCodec};
    use xmtp_proto::xmtp::mls::message_contents::content_types::{
        ReactionAction, ReactionSchema, ReactionV2,
    };
    let reaction = ReactionCodec::encode(ReactionV2 {
        reference: hex::encode(&message_id),
        reference_inbox_id: alix.inbox_id().into(),
        action: ReactionAction::Added.into(),
        content: "👍".into(),
        schema: ReactionSchema::Unicode.into(),
    })?;
    bo_group
        .send_message(
            &xmtp_content_types::encoded_content_to_bytes(reaction),
            SendMessageOpts::default(),
        )
        .await?;
    let reply = ReplyCodec::encode(xmtp_content_types::reply::Reply {
        reference: hex::encode(&message_id),
        reference_inbox_id: None,
        content: TextCodec::encode("reply".into())?,
    })?;
    bo_group
        .send_message(
            &xmtp_content_types::encoded_content_to_bytes(reply),
            SendMessageOpts::default(),
        )
        .await?;
    let before = bo_group
        .find_messages_v2(&MsgQueryArgs::default())?
        .into_iter()
        .find(|msg| msg.metadata.id == message_id)
        .unwrap();
    assert_eq!(before.reactions.len(), 1);
    assert_eq!(before.num_replies, 1);
    assert_eq!(before.num_reaction_messages, 1);

    // Alix deletes the message
    alix_group.sync().await?;
    alix_group.delete_message(message_id.clone())?;
    alix_group.publish_messages().await?;
    bo_group.sync().await?;

    // Verify the enriched message is now a DeletedMessage placeholder
    let messages = bo_group.find_messages_v2(&MsgQueryArgs::default())?;

    // Find the deleted message (skip membership changes)
    let deleted_msg = messages.iter().find(|msg| msg.metadata.id == message_id);
    assert!(deleted_msg.is_some());

    let deleted_msg = deleted_msg.unwrap();
    let MessageBody::DeletedMessage { deleted_by } = &deleted_msg.content else {
        panic!("Expected DeletedMessage placeholder");
    };
    assert_eq!(*deleted_by, DeletedBy::Sender);

    // Verify reactions and replies are cleared
    assert_eq!(deleted_msg.reactions.len(), 0);
    assert_eq!(deleted_msg.num_replies, 0);
    assert_eq!(deleted_msg.num_reaction_messages, 0);
}

/// Test that DeleteMessage content type is not shown in message lists
#[xmtp_common::test(unwrap_try = true)]
async fn test_delete_message_filtered_from_lists() {
    let alix = EngineTestPeer::new().await;
    let alix_group = alix.create_group(None, None)?;

    // Send a message
    let text_content = TextCodec::encode("Test message".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let message_id = alix_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    // Delete the message
    alix_group.delete_message(message_id)?;
    alix_group.publish_messages().await?;
    alix_group.sync().await?;

    // Query messages excluding DeleteMessage content type
    let messages = alix_group.find_messages(&MsgQueryArgs {
        exclude_content_types: Some(vec![ContentType::DeleteMessage]),
        ..Default::default()
    })?;

    // Should only see the original text message and membership change, not the DeleteMessage
    for msg in &messages {
        assert_ne!(msg.content_type, ContentType::DeleteMessage);
    }
}

/// Test that replies to deleted messages show the deleted state
#[xmtp_common::test(unwrap_try = true)]
async fn test_reply_to_deleted_message() {
    use xmtp_content_types::reply::ReplyCodec;

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;

    // Alix sends an original message
    let text_content = TextCodec::encode("Original message".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let original_message_id = alix_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    // Bo syncs and replies to the message
    let bo_groups = bo.wait_for_welcomes().await?;
    let bo_group = &bo_groups[0];
    bo_group.sync().await?;

    // Bo replies to Alix's message
    let reply_text_content = TextCodec::encode("Reply to original".to_string())?;
    let reply_content = ReplyCodec::encode(xmtp_content_types::reply::Reply {
        reference: hex::encode(&original_message_id),
        reference_inbox_id: None,
        content: reply_text_content,
    })?;
    let reply_bytes = xmtp_content_types::encoded_content_to_bytes(reply_content);
    let reply_message_id = bo_group
        .send_message(&reply_bytes, SendMessageOpts::default())
        .await?;

    // Alix syncs to see the reply
    alix_group.sync().await?;

    // Verify the reply shows the original message correctly before deletion
    let messages_before = alix_group.find_messages_v2(&MsgQueryArgs::default())?;
    let reply_msg_before = messages_before
        .iter()
        .find(|msg| msg.metadata.id == reply_message_id);
    assert!(reply_msg_before.is_some());

    let MessageBody::Reply(reply_body) = &reply_msg_before.unwrap().content else {
        panic!("Expected Reply message");
    };
    assert!(reply_body.in_reply_to.is_some());
    let MessageBody::Text(_) = &reply_body.in_reply_to.as_ref().unwrap().content else {
        panic!("Expected Text in in_reply_to");
    };

    // Now Alix deletes the original message
    alix_group.delete_message(original_message_id.clone())?;
    alix_group.publish_messages().await?;
    bo_group.sync().await?;
    alix_group.sync().await?;

    // Verify the reply now shows the deleted state for the referenced message
    let messages_after = alix_group.find_messages_v2(&MsgQueryArgs::default())?;
    let reply_msg_after = messages_after
        .iter()
        .find(|msg| msg.metadata.id == reply_message_id);
    assert!(reply_msg_after.is_some());

    let MessageBody::Reply(reply_body) = &reply_msg_after.unwrap().content else {
        panic!("Expected Reply message");
    };
    assert!(
        reply_body.in_reply_to.is_some(),
        "Expected in_reply_to to be set"
    );
    let in_reply_to = reply_body.in_reply_to.as_ref().unwrap();
    // After deletion, the referenced message should be a DeletedMessage
    let MessageBody::DeletedMessage { deleted_by } = &in_reply_to.content else {
        panic!("Expected DeletedMessage in reply's in_reply_to");
    };
    assert_eq!(*deleted_by, DeletedBy::Sender);
    // Reactions and replies should be cleared on the deleted referenced message
    assert_eq!(in_reply_to.reactions.len(), 0);
    assert_eq!(in_reply_to.num_replies, 0);
}

/// An authorized deletion leaves no original evidence on the placeholder
/// or on a reply's parent: no body, fallback, or original type.
// verifies: PROC-037
#[xmtp_common::test(unwrap_try = true)]
async fn deletion_clears_original_evidence_on_message_and_parent() {
    use xmtp_content_types::reply::ReplyCodec;
    use xmtp_mls_messages::enrichment::deleted_message_content_type;

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;

    let mut text_content = TextCodec::encode("secret-body".to_string())?;
    text_content.fallback = Some("secret-fallback".to_string());
    text_content
        .parameters
        .insert("note".to_string(), "secret-parameter".to_string());
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let original_id = alix_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    let bo_group = &bo.wait_for_welcomes().await?[0];
    bo_group.sync().await?;
    let reply_content = ReplyCodec::encode(xmtp_content_types::reply::Reply {
        reference: hex::encode(&original_id),
        reference_inbox_id: None,
        content: TextCodec::encode("reply".to_string())?,
    })?;
    let reply_bytes = xmtp_content_types::encoded_content_to_bytes(reply_content);
    let reply_id = bo_group
        .send_message(&reply_bytes, SendMessageOpts::default())
        .await?;
    alix_group.sync().await?;

    let before = alix_group.find_messages_v2(&MsgQueryArgs::default())?;
    let original = before
        .iter()
        .find(|m| m.metadata.id == original_id)
        .unwrap();
    assert_eq!(original.fallback_text.as_deref(), Some("secret-fallback"));

    alix_group.delete_message(original_id.clone())?;
    alix_group.publish_messages().await?;
    bo_group.sync().await?;
    alix_group.sync().await?;

    let after = alix_group.find_messages_v2(&MsgQueryArgs::default())?;
    let placeholder = after.iter().find(|m| m.metadata.id == original_id).unwrap();
    let MessageBody::Reply(reply) = &after
        .iter()
        .find(|m| m.metadata.id == reply_id)
        .unwrap()
        .content
    else {
        panic!("expected reply");
    };
    let parent = reply.in_reply_to.as_deref().expect("reply parent");
    for (path, message) in [("message", placeholder), ("parent", parent)] {
        assert!(
            matches!(message.content, MessageBody::DeletedMessage { .. }),
            "{path}"
        );
        assert_eq!(
            message.metadata.content_type,
            Some(deleted_message_content_type()),
            "{path} kept the original type"
        );
        assert_eq!(message.fallback_text, None, "{path} kept the fallback");
        assert!(
            !format!("{message:?}").contains("secret"),
            "{path} kept original evidence: {message:?}"
        );
    }
}

/// A row with a deletable type whose payload does not decode is kept as
/// undecodable content; an authorized deletion clears its bytes and fallback
/// on the placeholder and on a reply's parent.
// verifies: CTYPE-018, PROC-037
#[xmtp_common::test(unwrap_try = true)]
async fn deletion_clears_an_undecodable_deletable_row_and_parent() {
    use xmtp_content_types::reply::ReplyCodec;
    use xmtp_mls_messages::decoded_message::ContentDecodeFailureKind;

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;

    let mut text_content = TextCodec::encode("placeholder".to_string())?;
    // Ids print as byte lists, so a short marker can match them by chance.
    // No UTF-8 text holds these bytes.
    text_content.content = vec![0xff, 0xfe, 0xfd, 0xfc, 0xfb, 0xfa, 0xf9, 0xf8];
    text_content.fallback = Some("secret-fallback".to_string());
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let original_id = alix_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    let bo_group = &bo.wait_for_welcomes().await?[0];
    bo_group.sync().await?;
    let reply_content = ReplyCodec::encode(xmtp_content_types::reply::Reply {
        reference: hex::encode(&original_id),
        reference_inbox_id: None,
        content: TextCodec::encode("reply".to_string())?,
    })?;
    let reply_id = bo_group
        .send_message(
            &xmtp_content_types::encoded_content_to_bytes(reply_content),
            SendMessageOpts::default(),
        )
        .await?;
    alix_group.sync().await?;

    let before = alix_group.find_messages_v2(&MsgQueryArgs::default())?;
    let original = before
        .iter()
        .find(|m| m.metadata.id == original_id)
        .unwrap();
    assert!(
        matches!(&original.content, MessageBody::Undecodable(content)
        if content.raw_bytes == text_bytes
            && content.failure.kind == ContentDecodeFailureKind::CodecDecodeFailed)
    );

    alix_group.delete_message(original_id.clone())?;
    alix_group.publish_messages().await?;
    bo_group.sync().await?;
    alix_group.sync().await?;

    let after = alix_group.find_messages_v2(&MsgQueryArgs::default())?;
    let placeholder = after.iter().find(|m| m.metadata.id == original_id).unwrap();
    let MessageBody::Reply(reply) = &after
        .iter()
        .find(|m| m.metadata.id == reply_id)
        .unwrap()
        .content
    else {
        panic!("expected reply");
    };
    let parent = reply.in_reply_to.as_deref().expect("reply parent");
    for (path, message) in [("message", placeholder), ("parent", parent)] {
        assert!(
            matches!(message.content, MessageBody::DeletedMessage { .. }),
            "{path}: {:?}",
            message.content
        );
        assert_eq!(message.fallback_text, None, "{path} kept the fallback");
        let debug = format!("{message:?}");
        assert!(!debug.contains("secret"), "{path} kept the fallback text");
        assert!(
            !debug.contains("255, 254, 253, 252, 251, 250, 249, 248"),
            "{path} kept the raw bytes"
        );
    }
}
