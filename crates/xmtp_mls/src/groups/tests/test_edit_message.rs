use crate::groups::send_message_opts::SendMessageOpts;
use crate::tester;
use prost::Message as _;
use xmtp_content_types::{
    ContentCodec,
    edit_message::EditMessageCodec,
    reaction::ReactionCodec,
    reply::{Reply, ReplyCodec},
    text::TextCodec,
};
use xmtp_db::{
    encrypted_store::message_edit::QueryMessageEdit,
    group_message::{ContentType, Deletable, Editable, GroupMessageKind, QueryGroupMessage},
    message_deletion::QueryMessageDeletion,
};
use xmtp_proto::xmtp::mls::message_contents::{
    EncodedContent,
    content_types::{ReactionAction, ReactionSchema, ReactionV2},
};

/// Test basic message edit by the original sender
#[xmtp_common::test(unwrap_try = true)]
async fn test_edit_message_by_sender() {
    tester!(alix);
    tester!(bo);

    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;

    // Alix sends a message
    let text_content = TextCodec::encode("Hello, world!".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let message_id = alix_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    // Sync bo to get the group
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = &bo_groups[0];
    bo_group.sync().await?;

    // Verify the original message exists
    let original_message = alix.context.db().get_group_message(&message_id)?.unwrap();
    assert_eq!(original_message.kind, GroupMessageKind::Application);

    // Alix edits the message using the API
    let new_content = TextCodec::encode("Hello, edited world!".to_string())?;
    let new_content_bytes = xmtp_content_types::encoded_content_to_bytes(new_content);
    let edit_id = alix_group.edit_message(message_id.clone(), new_content_bytes)?;

    // Publish and sync
    alix_group.publish_messages().await?;
    bo_group.sync().await?;

    // Verify the edit message was stored
    let edit_message = alix.context.db().get_group_message(&edit_id)?.unwrap();
    assert_eq!(edit_message.kind, GroupMessageKind::Application);

    // Verify the edit record exists in the database
    let edit_record = alix
        .context
        .db()
        .get_latest_edit_by_original_message_id(&message_id)?;
    assert!(edit_record.is_some());

    let edit = edit_record.unwrap();
    assert_eq!(edit.original_message_id, message_id);
    assert_eq!(edit.edited_by_inbox_id, alix.inbox_id());
}

/// Test that non-sender edit fails with NotAuthorized error
#[xmtp_common::test(unwrap_try = true)]
async fn test_edit_message_non_sender_fails() {
    use crate::groups::GroupError;
    use crate::groups::error::EditMessageError;

    tester!(alix);
    tester!(bo);

    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;

    // Alix sends a message
    let text_content = TextCodec::encode("Hello, world!".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let message_id = alix_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    // Sync bo to get the group
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = &bo_groups[0];
    bo_group.sync().await?;

    // Bo tries to edit Alix's message - should fail
    let new_content = TextCodec::encode("Bo's unauthorized edit".to_string())?;
    let new_content_bytes = xmtp_content_types::encoded_content_to_bytes(new_content);

    let result = bo_group.edit_message(message_id.clone(), new_content_bytes);
    assert!(result.is_err());
    match result.unwrap_err() {
        GroupError::EditMessage(EditMessageError::NotAuthorized) => {}
        other => panic!("Expected NotAuthorized error, got: {:?}", other),
    }
}

/// Test edit of nonexistent message
#[xmtp_common::test(unwrap_try = true)]
async fn test_edit_nonexistent_message() {
    use crate::groups::GroupError;
    use crate::groups::error::EditMessageError;

    tester!(alix);

    let alix_group = alix.create_group(None, None)?;

    // Try to edit a message that doesn't exist
    let fake_message_id = vec![0u8; 32];
    let new_content = TextCodec::encode("new text".to_string())?;
    let new_content_bytes = xmtp_content_types::encoded_content_to_bytes(new_content);

    let result = alix_group.edit_message(fake_message_id, new_content_bytes);
    assert!(result.is_err());
    match result.unwrap_err() {
        GroupError::EditMessage(EditMessageError::MessageNotFound(_)) => {}
        other => panic!("Expected MessageNotFound error, got: {:?}", other),
    }
}

/// Test multiple edits - latest wins
#[xmtp_common::test(unwrap_try = true)]
async fn test_multiple_edits_latest_wins() {
    tester!(alix);

    let alix_group = alix.create_group(None, None)?;

    // Alix sends a message
    let text_content = TextCodec::encode("Original message".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let message_id = alix_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    alix_group.publish_messages().await?;

    // Alix edits the message multiple times using the API
    let edit1_text = "First edit";
    let edit1_content = TextCodec::encode(edit1_text.to_string())?;
    let edit1_bytes = xmtp_content_types::encoded_content_to_bytes(edit1_content);
    alix_group.edit_message(message_id.clone(), edit1_bytes)?;
    alix_group.publish_messages().await?;

    let edit2_text = "Second edit";
    let edit2_content = TextCodec::encode(edit2_text.to_string())?;
    let edit2_bytes = xmtp_content_types::encoded_content_to_bytes(edit2_content);
    alix_group.edit_message(message_id.clone(), edit2_bytes)?;
    alix_group.publish_messages().await?;

    let edit3_text = "Third and final edit";
    let edit3_content = TextCodec::encode(edit3_text.to_string())?;
    let edit3_bytes = xmtp_content_types::encoded_content_to_bytes(edit3_content);
    alix_group.edit_message(message_id.clone(), edit3_bytes)?;
    alix_group.publish_messages().await?;

    // Verify we have multiple edit records
    let all_edits = alix
        .context
        .db()
        .get_edits_by_original_message_id(&message_id)?;
    assert_eq!(all_edits.len(), 3);

    // The latest edit should be returned
    let latest_edit = alix
        .context
        .db()
        .get_latest_edit_by_original_message_id(&message_id)?
        .unwrap();

    // Decode the edited content to verify it's the latest
    let edited_content = EncodedContent::decode(&mut latest_edit.edited_content.as_slice())?;
    let edited_text = TextCodec::decode(edited_content)?;
    assert_eq!(edited_text, edit3_text);
}

/// Test edit message content type
#[xmtp_common::test(unwrap_try = true)]
async fn test_edit_content_type() {
    tester!(alix);

    let alix_group = alix.create_group(None, None)?;

    // Alix sends a message
    let text_content = TextCodec::encode("Original message".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let message_id = alix_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    alix_group.publish_messages().await?;

    // Alix edits the message using the API
    let new_content = TextCodec::encode("Edited message".to_string())?;
    let new_content_bytes = xmtp_content_types::encoded_content_to_bytes(new_content);
    let edit_id = alix_group.edit_message(message_id.clone(), new_content_bytes)?;

    alix_group.publish_messages().await?;

    // Verify the edit message has the correct content type
    let stored_edit = alix.context.db().get_group_message(&edit_id)?.unwrap();

    // Decode the content to check content type
    let encoded = EncodedContent::decode(&mut stored_edit.decrypted_message_bytes.as_slice())?;
    let content_type = encoded.r#type.unwrap();

    assert_eq!(content_type.type_id, EditMessageCodec::TYPE_ID);
    assert_eq!(content_type.authority_id, "xmtp.org");
    assert_eq!(content_type.version_major, EditMessageCodec::MAJOR_VERSION);
}

/// Test edit message across groups fails
#[xmtp_common::test(unwrap_try = true)]
async fn test_edit_message_across_groups_fails() {
    use crate::groups::GroupError;
    use crate::groups::error::EditMessageError;

    tester!(alix);
    tester!(bo);

    // Create two separate groups
    let group1 = alix.create_group(None, None)?;
    let group2 = alix.create_group(None, None)?;

    group1.add_members(&[bo.inbox_id()]).await?;
    group2.add_members(&[bo.inbox_id()]).await?;

    // Alix sends a message in group1
    let text_content = TextCodec::encode("Message in group 1".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let message_id = group1
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    group1.publish_messages().await?;

    // Try to edit the group1 message from group2 - should fail
    let new_content = TextCodec::encode("Cross-group edit".to_string())?;
    let new_content_bytes = xmtp_content_types::encoded_content_to_bytes(new_content);

    let result = group2.edit_message(message_id.clone(), new_content_bytes);
    assert!(result.is_err());
    match result.unwrap_err() {
        // Message won't be found in group2's context (different group_id check)
        GroupError::EditMessage(EditMessageError::NotAuthorized) => {}
        GroupError::EditMessage(EditMessageError::MessageNotFound(_)) => {}
        other => panic!(
            "Expected NotAuthorized or MessageNotFound error, got: {:?}",
            other
        ),
    }
}

/// Test that EditMessage content type is not deletable
#[xmtp_common::test(unwrap_try = true)]
async fn test_edit_message_is_not_deletable() {
    // Verify that EditMessage content type is not deletable
    assert!(!ContentType::EditMessage.is_deletable());
}

/// Test is_message_edited helper
#[xmtp_common::test(unwrap_try = true)]
async fn test_is_message_edited() {
    tester!(alix);

    let alix_group = alix.create_group(None, None)?;

    // Alix sends a message
    let text_content = TextCodec::encode("Original message".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let message_id = alix_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    alix_group.publish_messages().await?;

    // Check that the message is not edited yet
    assert!(!alix.context.db().is_message_edited(&message_id)?);

    // Alix edits the message using the API
    let new_content = TextCodec::encode("Edited message".to_string())?;
    let new_content_bytes = xmtp_content_types::encoded_content_to_bytes(new_content);
    alix_group.edit_message(message_id.clone(), new_content_bytes)?;

    alix_group.publish_messages().await?;

    // Check that the message is now edited
    assert!(alix.context.db().is_message_edited(&message_id)?);
}

/// Test get_group_edits helper
#[xmtp_common::test(unwrap_try = true)]
async fn test_get_group_edits() {
    tester!(alix);

    let alix_group = alix.create_group(None, None)?;

    // Alix sends multiple messages
    let text1 = TextCodec::encode("Message 1".to_string())?;
    let text1_bytes = xmtp_content_types::encoded_content_to_bytes(text1);
    let message1_id = alix_group
        .send_message(&text1_bytes, SendMessageOpts::default())
        .await?;
    alix_group.publish_messages().await?;

    let text2 = TextCodec::encode("Message 2".to_string())?;
    let text2_bytes = xmtp_content_types::encoded_content_to_bytes(text2);
    let message2_id = alix_group
        .send_message(&text2_bytes, SendMessageOpts::default())
        .await?;
    alix_group.publish_messages().await?;

    // Edit both messages using the API
    let edit1_content = TextCodec::encode("Edited message 1".to_string())?;
    let edit1_bytes = xmtp_content_types::encoded_content_to_bytes(edit1_content);
    alix_group.edit_message(message1_id.clone(), edit1_bytes)?;

    let edit2_content = TextCodec::encode("Edited message 2".to_string())?;
    let edit2_bytes = xmtp_content_types::encoded_content_to_bytes(edit2_content);
    alix_group.edit_message(message2_id.clone(), edit2_bytes)?;

    alix_group.publish_messages().await?;

    // Get all edits for the group
    let group_edits = alix.context.db().get_group_edits(&alix_group.group_id)?;

    assert_eq!(group_edits.len(), 2);

    // Verify both original messages have edits
    let original_ids: Vec<_> = group_edits
        .iter()
        .map(|e| e.original_message_id.clone())
        .collect();
    assert!(original_ids.contains(&message1_id));
    assert!(original_ids.contains(&message2_id));
}

/// Test get_edits_for_messages helper
#[xmtp_common::test(unwrap_try = true)]
async fn test_get_edits_for_messages() {
    tester!(alix);

    let alix_group = alix.create_group(None, None)?;

    // Alix sends multiple messages
    let text1 = TextCodec::encode("Message 1".to_string())?;
    let text1_bytes = xmtp_content_types::encoded_content_to_bytes(text1);
    let message1_id = alix_group
        .send_message(&text1_bytes, SendMessageOpts::default())
        .await?;
    alix_group.publish_messages().await?;

    let text2 = TextCodec::encode("Message 2".to_string())?;
    let text2_bytes = xmtp_content_types::encoded_content_to_bytes(text2);
    let message2_id = alix_group
        .send_message(&text2_bytes, SendMessageOpts::default())
        .await?;
    alix_group.publish_messages().await?;

    let text3 = TextCodec::encode("Message 3".to_string())?;
    let text3_bytes = xmtp_content_types::encoded_content_to_bytes(text3);
    let message3_id = alix_group
        .send_message(&text3_bytes, SendMessageOpts::default())
        .await?;
    alix_group.publish_messages().await?;

    // Edit only messages 1 and 3 using the API
    let edit1_content = TextCodec::encode("Edited message 1".to_string())?;
    let edit1_bytes = xmtp_content_types::encoded_content_to_bytes(edit1_content);
    alix_group.edit_message(message1_id.clone(), edit1_bytes)?;

    let edit3_content = TextCodec::encode("Edited message 3".to_string())?;
    let edit3_bytes = xmtp_content_types::encoded_content_to_bytes(edit3_content);
    alix_group.edit_message(message3_id.clone(), edit3_bytes)?;

    alix_group.publish_messages().await?;

    // Get edits for all three messages
    let edits = alix.context.db().get_edits_for_messages(vec![
        message1_id.clone(),
        message2_id.clone(),
        message3_id.clone(),
    ])?;

    // Should only return edits for messages 1 and 3
    assert_eq!(edits.len(), 2);

    let original_ids: Vec<_> = edits
        .iter()
        .map(|e| e.original_message_id.clone())
        .collect();
    assert!(original_ids.contains(&message1_id));
    assert!(!original_ids.contains(&message2_id));
    assert!(original_ids.contains(&message3_id));
}

/// Test that edits synced from network are processed correctly
#[xmtp_common::test(unwrap_try = true)]
async fn test_edit_message_sync_from_network() {
    tester!(alix);
    tester!(bo);

    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;

    // Sync bo to get the group
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = &bo_groups[0];
    bo_group.sync().await?;

    // Alix sends a message
    let text_content = TextCodec::encode("Hello from Alix".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let message_id = alix_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    // Publish and sync bo
    alix_group.publish_messages().await?;
    bo_group.sync().await?;

    // Alix edits the message using the API
    let new_content = TextCodec::encode("Hello from Alix (edited)".to_string())?;
    let new_content_bytes = xmtp_content_types::encoded_content_to_bytes(new_content);
    alix_group.edit_message(message_id.clone(), new_content_bytes)?;

    // Publish and sync bo
    alix_group.publish_messages().await?;
    bo_group.sync().await?;

    // Bo should see the edit in their database (via process_edit_message)
    let bo_edit_record = bo
        .context
        .db()
        .get_latest_edit_by_original_message_id(&message_id)?;
    assert!(bo_edit_record.is_some());
    assert_eq!(bo_edit_record.unwrap().edited_by_inbox_id, alix.inbox_id());
}

/// Test that Editable trait marks correct content types as editable
#[xmtp_common::test(unwrap_try = true)]
async fn test_editable_content_types() {
    // Editable content types
    assert!(ContentType::Text.is_editable());
    assert!(ContentType::Markdown.is_editable());
    assert!(ContentType::Reply.is_editable());
    assert!(ContentType::Attachment.is_editable());
    assert!(ContentType::RemoteAttachment.is_editable());
    assert!(ContentType::MultiRemoteAttachment.is_editable());
    assert!(ContentType::TransactionReference.is_editable());
    assert!(ContentType::WalletSendCalls.is_editable());

    // Non-editable content types
    assert!(!ContentType::GroupMembershipChange.is_editable());
    assert!(!ContentType::GroupUpdated.is_editable());
    assert!(!ContentType::Reaction.is_editable());
    assert!(!ContentType::ReadReceipt.is_editable());
    assert!(!ContentType::LeaveRequest.is_editable());
    assert!(!ContentType::Actions.is_editable());
    assert!(!ContentType::Intent.is_editable());
    assert!(!ContentType::DeleteMessage.is_editable());
    assert!(!ContentType::EditMessage.is_editable());
    assert!(!ContentType::Unknown.is_editable());
}

/// Test that GroupMessageKind is editable only for Application messages
#[xmtp_common::test(unwrap_try = true)]
async fn test_editable_message_kind() {
    assert!(GroupMessageKind::Application.is_editable());
    assert!(!GroupMessageKind::MembershipChange.is_editable());
}

/// Test that editing a text message with a reply content type fails (content type mismatch)
#[xmtp_common::test(unwrap_try = true)]
async fn test_edit_text_with_reply_fails() {
    use crate::groups::GroupError;
    use crate::groups::error::EditMessageError;

    tester!(alix);

    let alix_group = alix.create_group(None, None)?;

    // Alix sends a text message
    let text_content = TextCodec::encode("Original text message".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let message_id = alix_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    alix_group.publish_messages().await?;

    // Try to edit with a Reply content type - should fail
    let inner_text = TextCodec::encode("Edited as reply".to_string())?;
    let reply_content = ReplyCodec::encode(Reply {
        reference: "some_message_id".to_string(),
        reference_inbox_id: None,
        content: inner_text,
    })?;
    let reply_bytes = xmtp_content_types::encoded_content_to_bytes(reply_content);

    let result = alix_group.edit_message(message_id.clone(), reply_bytes);
    assert!(result.is_err());
    match result.unwrap_err() {
        GroupError::EditMessage(EditMessageError::ContentTypeMismatch { original, edited }) => {
            assert_eq!(original, "text");
            assert_eq!(edited, "reply");
        }
        other => panic!("Expected ContentTypeMismatch error, got: {:?}", other),
    }
}

/// Test that editing a reply message with text content type fails
#[xmtp_common::test(unwrap_try = true)]
async fn test_edit_reply_with_text_fails() {
    use crate::groups::GroupError;
    use crate::groups::error::EditMessageError;

    tester!(alix);

    let alix_group = alix.create_group(None, None)?;

    // First send a text message to reply to
    let original_text = TextCodec::encode("Original message to reply to".to_string())?;
    let original_bytes = xmtp_content_types::encoded_content_to_bytes(original_text);
    let original_message_id = alix_group
        .send_message(&original_bytes, SendMessageOpts::default())
        .await?;
    let original_message_id_hex = hex::encode(&original_message_id);

    alix_group.publish_messages().await?;

    // Now send a reply message
    let inner_text = TextCodec::encode("This is my reply".to_string())?;
    let reply_content = ReplyCodec::encode(Reply {
        reference: original_message_id_hex.clone(),
        reference_inbox_id: Some(alix.inbox_id().to_string()),
        content: inner_text,
    })?;
    let reply_bytes = xmtp_content_types::encoded_content_to_bytes(reply_content);
    let reply_message_id = alix_group
        .send_message(&reply_bytes, SendMessageOpts::default())
        .await?;

    alix_group.publish_messages().await?;

    // Try to edit the reply with a plain text content type - should fail
    let new_text = TextCodec::encode("Edited text (not a reply)".to_string())?;
    let new_text_bytes = xmtp_content_types::encoded_content_to_bytes(new_text);

    let result = alix_group.edit_message(reply_message_id.clone(), new_text_bytes);
    assert!(result.is_err());
    match result.unwrap_err() {
        GroupError::EditMessage(EditMessageError::ContentTypeMismatch { original, edited }) => {
            assert_eq!(original, "reply");
            assert_eq!(edited, "text");
        }
        other => panic!("Expected ContentTypeMismatch error, got: {:?}", other),
    }
}

/// Test that editing a reply message with matching content type succeeds
#[xmtp_common::test(unwrap_try = true)]
async fn test_edit_reply_with_reply_succeeds() {
    tester!(alix);

    let alix_group = alix.create_group(None, None)?;

    // First send a text message to reply to
    let original_text = TextCodec::encode("Original message to reply to".to_string())?;
    let original_bytes = xmtp_content_types::encoded_content_to_bytes(original_text);
    let original_message_id = alix_group
        .send_message(&original_bytes, SendMessageOpts::default())
        .await?;
    let original_message_id_hex = hex::encode(&original_message_id);

    alix_group.publish_messages().await?;

    // Now send a reply message
    let inner_text = TextCodec::encode("This is my reply".to_string())?;
    let reply_content = ReplyCodec::encode(Reply {
        reference: original_message_id_hex.clone(),
        reference_inbox_id: Some(alix.inbox_id().to_string()),
        content: inner_text,
    })?;
    let reply_bytes = xmtp_content_types::encoded_content_to_bytes(reply_content);
    let reply_message_id = alix_group
        .send_message(&reply_bytes, SendMessageOpts::default())
        .await?;

    alix_group.publish_messages().await?;

    // Edit the reply with another reply content type (preserving structure) - should succeed
    let new_inner_text = TextCodec::encode("This is my edited reply".to_string())?;
    let new_reply_content = ReplyCodec::encode(Reply {
        reference: original_message_id_hex.clone(),
        reference_inbox_id: Some(alix.inbox_id().to_string()),
        content: new_inner_text,
    })?;
    let new_reply_bytes = xmtp_content_types::encoded_content_to_bytes(new_reply_content);

    let result = alix_group.edit_message(reply_message_id.clone(), new_reply_bytes);
    assert!(result.is_ok());

    // Verify the edit was stored
    let edit_record = alix
        .context
        .db()
        .get_latest_edit_by_original_message_id(&reply_message_id)?;
    assert!(edit_record.is_some());

    // Verify the edited content is a reply with the new text
    let edit = edit_record.unwrap();
    let edited_content = EncodedContent::decode(&mut edit.edited_content.as_slice())?;
    let edited_reply = ReplyCodec::decode(edited_content)?;
    assert_eq!(edited_reply.reference, original_message_id_hex);
    let edited_text = TextCodec::decode(edited_reply.content)?;
    assert_eq!(edited_text, "This is my edited reply");
}

/// Test that transcript (membership change) messages cannot be edited
#[xmtp_common::test(unwrap_try = true)]
async fn test_cannot_edit_transcript_messages() {
    use crate::groups::GroupError;
    use crate::groups::error::EditMessageError;

    tester!(alix);
    tester!(bo);

    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;

    // Sync to get the membership change message
    alix_group.sync().await?;

    // Find the membership change message
    let messages = alix_group.find_messages(&xmtp_db::group_message::MsgQueryArgs {
        kind: Some(GroupMessageKind::MembershipChange),
        ..Default::default()
    })?;
    assert!(!messages.is_empty());

    let membership_message_id = messages[0].id.clone();

    // Try to edit the membership change message (should fail)
    let new_content = TextCodec::encode("Edited membership change".to_string())?;
    let new_content_bytes = xmtp_content_types::encoded_content_to_bytes(new_content);

    let result = alix_group.edit_message(membership_message_id, new_content_bytes);
    assert!(result.is_err());
    match result.unwrap_err() {
        GroupError::EditMessage(EditMessageError::NonEditableMessage) => {}
        other => panic!("Expected NonEditableMessage error, got: {:?}", other),
    }
}

/// Test that a message can be edited multiple times (re-edit succeeds)
#[xmtp_common::test(unwrap_try = true)]
async fn test_edit_already_edited_message_succeeds() {
    tester!(alix);

    let alix_group = alix.create_group(None, None)?;

    // Send a message
    let text_content = TextCodec::encode("Original message".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let message_id = alix_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    alix_group.publish_messages().await?;

    // First edit
    let edit1_content = TextCodec::encode("First edit".to_string())?;
    let edit1_bytes = xmtp_content_types::encoded_content_to_bytes(edit1_content);
    let edit1_id = alix_group.edit_message(message_id.clone(), edit1_bytes)?;
    alix_group.publish_messages().await?;

    // Verify first edit was stored
    assert!(alix.context.db().is_message_edited(&message_id)?);

    // Second edit (should succeed, unlike delete which fails on already deleted)
    let edit2_content = TextCodec::encode("Second edit".to_string())?;
    let edit2_bytes = xmtp_content_types::encoded_content_to_bytes(edit2_content);
    let edit2_id = alix_group.edit_message(message_id.clone(), edit2_bytes)?;
    alix_group.publish_messages().await?;

    // Verify both edits exist
    let all_edits = alix
        .context
        .db()
        .get_edits_by_original_message_id(&message_id)?;
    assert_eq!(all_edits.len(), 2);

    // Verify the edit IDs are different
    assert_ne!(edit1_id, edit2_id);
}

/// Test out-of-order edit (edit arrives before original message)
#[xmtp_common::test(unwrap_try = true)]
async fn test_out_of_order_edit() {
    tester!(alix);
    tester!(bo);

    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;

    // Alix sends a message
    let text_content = TextCodec::encode("Test message".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let message_id = alix_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    // Alix edits the message immediately (before bo syncs)
    let new_content = TextCodec::encode("Edited message".to_string())?;
    let new_content_bytes = xmtp_content_types::encoded_content_to_bytes(new_content);
    alix_group.edit_message(message_id.clone(), new_content_bytes)?;
    alix_group.publish_messages().await?;

    // Bola syncs and should receive both the message and edit
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = &bo_groups[0];
    bo_group.sync().await?;

    // Verify the message is marked as edited for Bola
    let bo_conn = bo.context.db();
    assert!(bo_conn.is_message_edited(&message_id)?);

    // Verify the edit record exists
    let edit = bo_conn.get_latest_edit_by_original_message_id(&message_id)?;
    assert!(edit.is_some());
}

/// Test edit record stored before the original message arrives (true out-of-order).
#[xmtp_common::test(unwrap_try = true)]
async fn test_true_out_of_order_edit_by_sender() {
    use crate::messages::decoded_message::MessageBody;
    use xmtp_db::Store;
    use xmtp_db::encrypted_store::message_edit::StoredMessageEdit;
    use xmtp_db::group_message::{DeliveryStatus, StoredGroupMessage};

    tester!(alix);
    let alix_group = alix.create_group(None, None)?;
    alix_group.sync().await?;

    let alix_conn = alix.context.db();
    let alix_inbox_id = alix.inbox_id().to_string();

    // Simulate a message ID that will be "sent" later
    let future_message_id = vec![0xED, 0x17, 0xBE, 0xEF];
    let edit_message_id = vec![0x01, 0x02, 0x03];

    // Create the edited content
    let edited_text = TextCodec::encode("Edited text".to_string())?;
    let edited_content_bytes = xmtp_content_types::encoded_content_to_bytes(edited_text);

    // Step 1: First store the EditMessage itself in group_messages
    let edit_msg = xmtp_proto::xmtp::mls::message_contents::content_types::EditMessage {
        message_id: hex::encode(&future_message_id),
        edited_content: Some(EncodedContent::decode(
            &mut edited_content_bytes.as_slice(),
        )?),
    };
    let edit_msg_content = EditMessageCodec::encode(edit_msg)?;
    let edit_msg_bytes = xmtp_content_types::encoded_content_to_bytes(edit_msg_content);

    let edit_message = StoredGroupMessage {
        id: edit_message_id.clone(),
        group_id: alix_group.group_id.clone(),
        decrypted_message_bytes: edit_msg_bytes,
        sent_at_ns: xmtp_common::time::now_ns(),
        kind: GroupMessageKind::Application,
        sender_installation_id: alix.context.installation_id().to_vec(),
        sender_inbox_id: alix_inbox_id.clone(),
        delivery_status: DeliveryStatus::Published,
        content_type: ContentType::EditMessage,
        version_major: 1,
        version_minor: 0,
        authority_id: "xmtp.org".to_string(),
        reference_id: None,
        expire_at_ns: None,
        sequence_id: 2,
        originator_id: 1,
        inserted_at_ns: 0,
        should_push: false,
    };
    edit_message.store(&alix_conn)?;

    // Step 2: Store the edit record (references the EditMessage above)
    // This simulates the edit arriving before the original message
    let edit = StoredMessageEdit {
        id: edit_message_id.clone(),
        group_id: alix_group.group_id.clone(),
        original_message_id: future_message_id.clone(),
        edited_by_inbox_id: alix_inbox_id.clone(),
        edited_content: edited_content_bytes.clone(),
        edited_at_ns: xmtp_common::time::now_ns(),
    };
    edit.store(&alix_conn)?;

    // Verify edit record exists but target message doesn't
    assert!(alix_conn.get_group_message(&future_message_id)?.is_none());
    assert!(
        alix_conn
            .get_latest_edit_by_original_message_id(&future_message_id)?
            .is_some()
    );

    // Step 3: Now store the original message (simulating it arriving later)
    let text_content = TextCodec::encode("Original text".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);

    let message = StoredGroupMessage {
        id: future_message_id.clone(),
        group_id: alix_group.group_id.clone(),
        decrypted_message_bytes: text_bytes,
        sent_at_ns: xmtp_common::time::now_ns() - 1000,
        kind: GroupMessageKind::Application,
        sender_installation_id: alix.context.installation_id().to_vec(),
        sender_inbox_id: alix_inbox_id.clone(),
        delivery_status: DeliveryStatus::Published,
        content_type: ContentType::Text,
        version_major: 1,
        version_minor: 0,
        authority_id: "xmtp.org".to_string(),
        reference_id: None,
        expire_at_ns: None,
        sequence_id: 1,
        originator_id: 1,
        inserted_at_ns: 0,
        should_push: false,
    };
    message.store(&alix_conn)?;

    // Step 4: Verify the message is now marked as edited
    assert!(alix_conn.is_message_edited(&future_message_id)?);

    // Step 5: Verify enrichment correctly shows the message with edit info
    let enriched =
        alix_group.find_enriched_messages(&xmtp_db::group_message::MsgQueryArgs::default())?;
    let edited_msg = enriched.iter().find(|m| m.metadata.id == future_message_id);
    assert!(
        edited_msg.is_some(),
        "Message should be in enriched results"
    );

    let edited_msg = edited_msg.unwrap();
    // Verify the message has edited metadata
    assert!(edited_msg.edited.is_some(), "Message should have edit info");

    // Content should still be Text (the enrichment layer stores edit info separately)
    let MessageBody::Text(_) = &edited_msg.content else {
        panic!("Expected Text message body, got {:?}", edited_msg.content);
    };
}

/// Test that unauthorized edit records are rejected at query time.
#[xmtp_common::test(unwrap_try = true)]
async fn test_out_of_order_unauthorized_edit_rejected() {
    use crate::messages::decoded_message::MessageBody;
    use xmtp_db::Store;
    use xmtp_db::encrypted_store::message_edit::StoredMessageEdit;
    use xmtp_db::group_message::{DeliveryStatus, StoredGroupMessage};

    tester!(alix);
    tester!(bo);
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;

    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = &bo_groups[0];
    bo_group.sync().await?;

    let bo_conn = bo.context.db();
    let alix_inbox_id = alix.inbox_id().to_string();
    let bo_inbox_id = bo.inbox_id().to_string();

    let future_message_id = vec![0xBA, 0xDC, 0x0D, 0xE1];
    let malicious_edit_msg_id = vec![0x0B, 0xAD, 0x02];

    // Create malicious edited content
    let edited_text = TextCodec::encode("Bo's malicious edit".to_string())?;
    let edited_content_bytes = xmtp_content_types::encoded_content_to_bytes(edited_text);

    // Step 1: First store the malicious EditMessage in group_messages (FK requirement)
    let edit_msg = xmtp_proto::xmtp::mls::message_contents::content_types::EditMessage {
        message_id: hex::encode(&future_message_id),
        edited_content: Some(EncodedContent::decode(
            &mut edited_content_bytes.as_slice(),
        )?),
    };
    let edit_msg_content = EditMessageCodec::encode(edit_msg)?;
    let edit_msg_bytes = xmtp_content_types::encoded_content_to_bytes(edit_msg_content);

    let malicious_edit_message = StoredGroupMessage {
        id: malicious_edit_msg_id.clone(),
        group_id: bo_group.group_id.clone(),
        decrypted_message_bytes: edit_msg_bytes,
        sent_at_ns: xmtp_common::time::now_ns(),
        kind: GroupMessageKind::Application,
        sender_installation_id: vec![1, 2, 3],
        sender_inbox_id: bo_inbox_id.clone(),
        delivery_status: DeliveryStatus::Published,
        content_type: ContentType::EditMessage,
        version_major: 1,
        version_minor: 0,
        authority_id: "xmtp.org".to_string(),
        reference_id: None,
        expire_at_ns: None,
        sequence_id: 3,
        originator_id: 1,
        inserted_at_ns: 0,
        should_push: false,
    };
    malicious_edit_message.store(&bo_conn)?;

    // Step 2: Bo (non-sender) tries to edit Alix's message by storing an edit record
    // This simulates a malicious edit arriving before the message
    let malicious_edit = StoredMessageEdit {
        id: malicious_edit_msg_id.clone(),
        group_id: bo_group.group_id.clone(),
        original_message_id: future_message_id.clone(),
        edited_by_inbox_id: bo_inbox_id.clone(),
        edited_content: edited_content_bytes.clone(),
        edited_at_ns: xmtp_common::time::now_ns(),
    };
    malicious_edit.store(&bo_conn)?;

    // Step 3: Store Alix's message (arriving after the malicious edit)
    let text_content = TextCodec::encode("Alix's message".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);

    let message = StoredGroupMessage {
        id: future_message_id.clone(),
        group_id: bo_group.group_id.clone(),
        decrypted_message_bytes: text_bytes,
        sent_at_ns: xmtp_common::time::now_ns() - 1000,
        kind: GroupMessageKind::Application,
        sender_installation_id: vec![1, 2, 3],
        sender_inbox_id: alix_inbox_id.clone(),
        delivery_status: DeliveryStatus::Published,
        content_type: ContentType::Text,
        version_major: 1,
        version_minor: 0,
        authority_id: "xmtp.org".to_string(),
        reference_id: None,
        expire_at_ns: None,
        sequence_id: 2,
        originator_id: 1,
        inserted_at_ns: 0,
        should_push: false,
    };
    message.store(&bo_conn)?;

    // Edit record exists but is unauthorized
    assert!(bo_conn.is_message_edited(&future_message_id)?);

    // Enrichment should show original message without edit since edit is unauthorized
    let enriched =
        bo_group.find_enriched_messages(&xmtp_db::group_message::MsgQueryArgs::default())?;
    let msg = enriched.iter().find(|m| m.metadata.id == future_message_id);
    assert!(msg.is_some(), "Message should be in enriched results");

    let msg = msg.unwrap();
    // The edit should NOT be applied because Bo is not the sender
    assert!(
        msg.edited.is_none(),
        "Unauthorized edit should not be applied"
    );
    match &msg.content {
        MessageBody::Text(text) => {
            assert_eq!(text.content, "Alix's message");
        }
        other => {
            panic!("Expected Text content, got {:?}", other);
        }
    }
}

/// Test that enrichment shows edited messages with the edit metadata
#[xmtp_common::test(unwrap_try = true)]
async fn test_enrichment_with_edited_messages() {
    use crate::messages::decoded_message::MessageBody;

    tester!(alix);
    tester!(bo);

    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;

    // Alix sends a message
    let text_content = TextCodec::encode("Original message".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let message_id = alix_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    // Sync bo's group
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = &bo_groups[0];
    bo_group.sync().await?;

    // Verify Bola can see the original message content
    let messages = bo_group.find_enriched_messages(&xmtp_db::group_message::MsgQueryArgs {
        content_types: Some(vec![ContentType::Text]),
        ..Default::default()
    })?;
    assert_eq!(messages.len(), 1);

    let MessageBody::Text(text) = &messages[0].content else {
        panic!("Expected Text message body");
    };
    assert_eq!(text.content, "Original message");
    assert!(messages[0].edited.is_none());

    // Alix edits the message
    let new_content = TextCodec::encode("Edited message".to_string())?;
    let new_content_bytes = xmtp_content_types::encoded_content_to_bytes(new_content);
    alix_group.edit_message(message_id.clone(), new_content_bytes)?;
    alix_group.publish_messages().await?;
    bo_group.sync().await?;

    // Verify the enriched message now has edit metadata
    let messages =
        bo_group.find_enriched_messages(&xmtp_db::group_message::MsgQueryArgs::default())?;

    // Find the edited message
    let edited_msg = messages.iter().find(|msg| msg.metadata.id == message_id);
    assert!(edited_msg.is_some());

    let edited_msg = edited_msg.unwrap();
    // Verify edit metadata is present
    assert!(edited_msg.edited.is_some());
    let edit_info = edited_msg.edited.as_ref().unwrap();
    assert!(edit_info.edited_at_ns > 0);

    // Content should still be Text (enrichment layer stores edit info in edited field)
    let MessageBody::Text(_) = &edited_msg.content else {
        panic!("Expected Text message body");
    };
}

/// Test that EditMessage content type is not shown in message lists when filtered
#[xmtp_common::test(unwrap_try = true)]
async fn test_edit_message_filtered_from_lists() {
    tester!(alix);
    let alix_group = alix.create_group(None, None)?;

    // Send a message
    let text_content = TextCodec::encode("Test message".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let message_id = alix_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    // Edit the message
    let new_content = TextCodec::encode("Edited message".to_string())?;
    let new_content_bytes = xmtp_content_types::encoded_content_to_bytes(new_content);
    alix_group.edit_message(message_id, new_content_bytes)?;
    alix_group.publish_messages().await?;
    alix_group.sync().await?;

    // Query messages excluding EditMessage content type
    let messages = alix_group.find_messages(&xmtp_db::group_message::MsgQueryArgs {
        exclude_content_types: Some(vec![ContentType::EditMessage]),
        ..Default::default()
    })?;

    // Should only see the original text message and membership change, not the EditMessage
    for msg in &messages {
        assert_ne!(msg.content_type, ContentType::EditMessage);
    }
}

/// Test that we cannot edit an edit message
#[xmtp_common::test(unwrap_try = true)]
async fn test_cannot_edit_edit_message() {
    use crate::groups::GroupError;
    use crate::groups::error::EditMessageError;

    tester!(alix);
    let alix_group = alix.create_group(None, None)?;

    // Alix sends a message
    let text_content = TextCodec::encode("Original message".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let original_message_id = alix_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    // Alix edits the message
    let edited_content = TextCodec::encode("Edited message".to_string())?;
    let edited_bytes = xmtp_content_types::encoded_content_to_bytes(edited_content);
    let edit_message_id = alix_group.edit_message(original_message_id.clone(), edited_bytes)?;

    // Publish the edit
    alix_group.publish_messages().await?;

    // Verify the original message is edited
    let alix_conn = alix.context.db();
    assert!(alix_conn.is_message_edited(&original_message_id)?);

    // Verify the edit message exists in the database
    let edit_msg = alix_conn.get_group_message(&edit_message_id)?;
    assert!(edit_msg.is_some());
    let edit_msg = edit_msg.unwrap();
    assert_eq!(edit_msg.content_type, ContentType::EditMessage);

    // Try to edit the edit message - should fail
    let new_content = TextCodec::encode("Editing an edit".to_string())?;
    let new_content_bytes = xmtp_content_types::encoded_content_to_bytes(new_content);
    let result = alix_group.edit_message(edit_message_id.clone(), new_content_bytes);
    assert!(result.is_err());
    match result.unwrap_err() {
        GroupError::EditMessage(EditMessageError::NonEditableMessage) => {}
        other => panic!("Expected NonEditableMessage error, got: {:?}", other),
    }

    // Verify the edit message is NOT edited
    assert!(!alix_conn.is_message_edited(&edit_message_id)?);
}

/// Test that replies to edited messages show the edit state
#[xmtp_common::test(unwrap_try = true)]
async fn test_reply_to_edited_message() {
    use crate::messages::decoded_message::MessageBody;

    tester!(alix);
    tester!(bo);

    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;

    // Alix sends an original message
    let text_content = TextCodec::encode("Original message".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let original_message_id = alix_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    // Bo syncs and replies to the message
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = &bo_groups[0];
    bo_group.sync().await?;

    // Bo replies to Alix's message
    let reply_text_content = TextCodec::encode("Reply to original".to_string())?;
    let reply_content = ReplyCodec::encode(Reply {
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

    // Verify the reply shows the original message correctly before edit
    let messages_before =
        alix_group.find_enriched_messages(&xmtp_db::group_message::MsgQueryArgs::default())?;
    let reply_msg_before = messages_before
        .iter()
        .find(|msg| msg.metadata.id == reply_message_id);
    assert!(reply_msg_before.is_some());

    let MessageBody::Reply(reply_body) = &reply_msg_before.unwrap().content else {
        panic!("Expected Reply message");
    };
    assert!(reply_body.in_reply_to.is_some());
    let in_reply_to = reply_body.in_reply_to.as_ref().unwrap();
    // Before edit, in_reply_to should not have edit info
    assert!(in_reply_to.edited.is_none());

    // Now Alix edits the original message
    let edited_content = TextCodec::encode("Edited original message".to_string())?;
    let edited_bytes = xmtp_content_types::encoded_content_to_bytes(edited_content);
    alix_group.edit_message(original_message_id.clone(), edited_bytes)?;
    alix_group.publish_messages().await?;
    bo_group.sync().await?;
    alix_group.sync().await?;

    // Verify the reply now shows the edited state for the referenced message
    let messages_after =
        alix_group.find_enriched_messages(&xmtp_db::group_message::MsgQueryArgs::default())?;
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

    // After edit, the referenced message should have edit info
    assert!(
        in_reply_to.edited.is_some(),
        "Referenced message should have edit info"
    );
    let edit_info = in_reply_to.edited.as_ref().unwrap();
    assert!(edit_info.edited_at_ns > 0);
}

/// Test that editing a non-editable content type (Reaction) fails
#[xmtp_common::test(unwrap_try = true)]
async fn test_edit_reaction_fails() {
    use crate::groups::GroupError;
    use crate::groups::error::EditMessageError;

    tester!(alix);

    let alix_group = alix.create_group(None, None)?;

    // First send a text message to react to
    let text_content = TextCodec::encode("Message to react to".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let message_id = alix_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;
    let message_id_hex = hex::encode(&message_id);

    alix_group.publish_messages().await?;

    // Send a reaction
    let reaction_content = ReactionCodec::encode(ReactionV2 {
        reference: message_id_hex.clone(),
        reference_inbox_id: String::new(),
        action: ReactionAction::Added as i32,
        content: "👍".to_string(),
        schema: ReactionSchema::Unicode as i32,
    })?;
    let reaction_bytes = xmtp_content_types::encoded_content_to_bytes(reaction_content);
    let reaction_message_id = alix_group
        .send_message(&reaction_bytes, SendMessageOpts::default())
        .await?;

    alix_group.publish_messages().await?;

    // Try to edit the reaction - should fail (reactions are not editable)
    let new_reaction = ReactionCodec::encode(ReactionV2 {
        reference: message_id_hex.clone(),
        reference_inbox_id: String::new(),
        action: ReactionAction::Added as i32,
        content: "❤️".to_string(),
        schema: ReactionSchema::Unicode as i32,
    })?;
    let new_reaction_bytes = xmtp_content_types::encoded_content_to_bytes(new_reaction);

    let result = alix_group.edit_message(reaction_message_id.clone(), new_reaction_bytes);
    assert!(result.is_err());
    match result.unwrap_err() {
        GroupError::EditMessage(EditMessageError::NonEditableMessage) => {}
        other => panic!("Expected NonEditableMessage error, got: {:?}", other),
    }
}

/// Test that editing a deleted message fails
#[xmtp_common::test(unwrap_try = true)]
async fn test_edit_deleted_message_fails() {
    use crate::groups::GroupError;
    use crate::groups::error::EditMessageError;

    tester!(alix);

    let alix_group = alix.create_group(None, None)?;

    // Alix sends a message
    let text_content = TextCodec::encode("Original message".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let message_id = alix_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    alix_group.publish_messages().await?;

    // Alix deletes the message
    alix_group.delete_message(message_id.clone())?;
    alix_group.publish_messages().await?;

    // Verify the message is deleted
    let alix_conn = alix.context.db();
    assert!(alix_conn.is_message_deleted(&message_id)?);

    // Try to edit the deleted message - should fail
    let new_content = TextCodec::encode("Trying to edit deleted message".to_string())?;
    let new_content_bytes = xmtp_content_types::encoded_content_to_bytes(new_content);

    let result = alix_group.edit_message(message_id.clone(), new_content_bytes);
    assert!(result.is_err());
    match result.unwrap_err() {
        GroupError::EditMessage(EditMessageError::MessageAlreadyDeleted) => {}
        other => panic!("Expected MessageAlreadyDeleted error, got: {:?}", other),
    }
}

/// Test that deleting an edited message works correctly
#[xmtp_common::test(unwrap_try = true)]
async fn test_delete_edited_message() {
    use crate::messages::decoded_message::{DeletedBy, MessageBody};

    tester!(alix);
    tester!(bo);

    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;

    // Alix sends a message
    let text_content = TextCodec::encode("Original message".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let message_id = alix_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    alix_group.publish_messages().await?;

    // Bo syncs to get the message
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = &bo_groups[0];
    bo_group.sync().await?;

    // Alix edits the message
    let edited_content = TextCodec::encode("Edited message".to_string())?;
    let edited_bytes = xmtp_content_types::encoded_content_to_bytes(edited_content);
    alix_group.edit_message(message_id.clone(), edited_bytes)?;
    alix_group.publish_messages().await?;

    // Verify the message is edited
    let alix_conn = alix.context.db();
    assert!(alix_conn.is_message_edited(&message_id)?);

    // Bo syncs to see the edit
    bo_group.sync().await?;

    // Now Alix deletes the edited message
    alix_group.delete_message(message_id.clone())?;
    alix_group.publish_messages().await?;

    // Verify the message is now deleted
    assert!(alix_conn.is_message_deleted(&message_id)?);

    // Both edit and deletion records should exist
    assert!(alix_conn.is_message_edited(&message_id)?);

    // Bo syncs to see the deletion
    bo_group.sync().await?;

    // Enrichment should show the message as deleted (deletion takes precedence over edit)
    let messages =
        bo_group.find_enriched_messages(&xmtp_db::group_message::MsgQueryArgs::default())?;
    let deleted_msg = messages.iter().find(|msg| msg.metadata.id == message_id);
    assert!(deleted_msg.is_some());

    let deleted_msg = deleted_msg.unwrap();
    let MessageBody::DeletedMessage { deleted_by } = &deleted_msg.content else {
        panic!(
            "Expected DeletedMessage placeholder, got {:?}",
            deleted_msg.content
        );
    };
    assert_eq!(*deleted_by, DeletedBy::Sender);

    // Edit info should be None for deleted messages
    assert!(deleted_msg.edited.is_none());
}

/// Test the full chain: edit → delete → try to edit again (should fail)
#[xmtp_common::test(unwrap_try = true)]
async fn test_edit_then_delete_then_edit_fails() {
    use crate::groups::GroupError;
    use crate::groups::error::EditMessageError;

    tester!(alix);

    let alix_group = alix.create_group(None, None)?;

    // Alix sends a message
    let text_content = TextCodec::encode("Original message".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let message_id = alix_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    alix_group.publish_messages().await?;

    // Step 1: Edit the message
    let edit1_content = TextCodec::encode("First edit".to_string())?;
    let edit1_bytes = xmtp_content_types::encoded_content_to_bytes(edit1_content);
    alix_group.edit_message(message_id.clone(), edit1_bytes)?;
    alix_group.publish_messages().await?;

    let alix_conn = alix.context.db();
    assert!(alix_conn.is_message_edited(&message_id)?);

    // Step 2: Delete the edited message
    alix_group.delete_message(message_id.clone())?;
    alix_group.publish_messages().await?;

    assert!(alix_conn.is_message_deleted(&message_id)?);

    // Step 3: Try to edit again - should fail because it's deleted
    let edit2_content = TextCodec::encode("Second edit after delete".to_string())?;
    let edit2_bytes = xmtp_content_types::encoded_content_to_bytes(edit2_content);

    let result = alix_group.edit_message(message_id.clone(), edit2_bytes);
    assert!(result.is_err());
    match result.unwrap_err() {
        GroupError::EditMessage(EditMessageError::MessageAlreadyDeleted) => {}
        other => panic!("Expected MessageAlreadyDeleted error, got: {:?}", other),
    }
}

/// Test complex reply chain with edits and deletes
#[xmtp_common::test(unwrap_try = true)]
async fn test_reply_chain_with_edits_and_deletes() {
    use crate::messages::decoded_message::{DeletedBy, MessageBody};

    tester!(alix);
    tester!(bo);

    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;

    // Bo syncs to join the group
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = &bo_groups[0];
    bo_group.sync().await?;

    // Step 1: Alix sends original message
    let original_content = TextCodec::encode("Original message from Alix".to_string())?;
    let original_bytes = xmtp_content_types::encoded_content_to_bytes(original_content);
    let original_id = alix_group
        .send_message(&original_bytes, SendMessageOpts::default())
        .await?;
    alix_group.publish_messages().await?;

    // Bo syncs to see the message
    bo_group.sync().await?;

    // Step 2: Bo replies to the original message
    let reply1_text = TextCodec::encode("Bo's reply to Alix".to_string())?;
    let reply1_content = ReplyCodec::encode(Reply {
        reference: hex::encode(&original_id),
        reference_inbox_id: Some(alix.inbox_id().to_string()),
        content: reply1_text,
    })?;
    let reply1_bytes = xmtp_content_types::encoded_content_to_bytes(reply1_content);
    let reply1_id = bo_group
        .send_message(&reply1_bytes, SendMessageOpts::default())
        .await?;
    bo_group.publish_messages().await?;

    // Alix syncs to see Bo's reply
    alix_group.sync().await?;

    // Step 3: Alix edits the original message
    let edited_original = TextCodec::encode("Edited original from Alix".to_string())?;
    let edited_original_bytes = xmtp_content_types::encoded_content_to_bytes(edited_original);
    alix_group.edit_message(original_id.clone(), edited_original_bytes)?;
    alix_group.publish_messages().await?;

    // Bo syncs to see the edit
    bo_group.sync().await?;

    // Step 4: Bo edits their reply
    let edited_reply1_text = TextCodec::encode("Bo's edited reply".to_string())?;
    let edited_reply1_content = ReplyCodec::encode(Reply {
        reference: hex::encode(&original_id),
        reference_inbox_id: Some(alix.inbox_id().to_string()),
        content: edited_reply1_text,
    })?;
    let edited_reply1_bytes = xmtp_content_types::encoded_content_to_bytes(edited_reply1_content);
    bo_group.edit_message(reply1_id.clone(), edited_reply1_bytes)?;
    bo_group.publish_messages().await?;

    // Alix syncs to see Bo's edit
    alix_group.sync().await?;

    // Step 5: Alix replies to Bo's (now edited) reply
    let reply2_text = TextCodec::encode("Alix's reply to Bo's edited reply".to_string())?;
    let reply2_content = ReplyCodec::encode(Reply {
        reference: hex::encode(&reply1_id),
        reference_inbox_id: Some(bo.inbox_id().to_string()),
        content: reply2_text,
    })?;
    let reply2_bytes = xmtp_content_types::encoded_content_to_bytes(reply2_content);
    let reply2_id = alix_group
        .send_message(&reply2_bytes, SendMessageOpts::default())
        .await?;
    alix_group.publish_messages().await?;

    // Bo syncs to see Alix's new reply
    bo_group.sync().await?;

    // Step 6: Alix deletes the original message
    alix_group.delete_message(original_id.clone())?;
    alix_group.publish_messages().await?;

    // Sync everyone
    bo_group.sync().await?;
    alix_group.sync().await?;

    // Verify final state from Bo's perspective
    let bo_messages =
        bo_group.find_enriched_messages(&xmtp_db::group_message::MsgQueryArgs::default())?;

    // Original message should be deleted
    let original_msg = bo_messages.iter().find(|m| m.metadata.id == original_id);
    assert!(original_msg.is_some());
    let original_msg = original_msg.unwrap();
    let MessageBody::DeletedMessage { deleted_by } = &original_msg.content else {
        panic!("Expected original message to be deleted");
    };
    assert_eq!(*deleted_by, DeletedBy::Sender);
    // Deleted messages should not show edit info
    assert!(original_msg.edited.is_none());

    // Bo's reply should be edited (not deleted)
    let reply1_msg = bo_messages.iter().find(|m| m.metadata.id == reply1_id);
    assert!(reply1_msg.is_some());
    let reply1_msg = reply1_msg.unwrap();
    let MessageBody::Reply(reply1_body) = &reply1_msg.content else {
        panic!("Expected Reply message");
    };
    // Reply should have edit info
    assert!(reply1_msg.edited.is_some());
    // The in_reply_to should point to a deleted message
    assert!(reply1_body.in_reply_to.is_some());
    let in_reply_to_original = reply1_body.in_reply_to.as_ref().unwrap();
    let MessageBody::DeletedMessage { .. } = &in_reply_to_original.content else {
        panic!("Expected in_reply_to to be DeletedMessage");
    };

    // Alix's reply to Bo's reply should exist and reference Bo's edited reply
    let reply2_msg = bo_messages.iter().find(|m| m.metadata.id == reply2_id);
    assert!(reply2_msg.is_some());
    let reply2_msg = reply2_msg.unwrap();
    let MessageBody::Reply(reply2_body) = &reply2_msg.content else {
        panic!("Expected Reply message");
    };
    // The in_reply_to should be Bo's edited reply
    assert!(reply2_body.in_reply_to.is_some());
    let in_reply_to_reply1 = reply2_body.in_reply_to.as_ref().unwrap();
    // Bo's reply should have edit info
    assert!(in_reply_to_reply1.edited.is_some());
}
