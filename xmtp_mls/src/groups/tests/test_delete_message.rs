use crate::groups::GroupError;
use crate::groups::error::DeleteMessageError;
use crate::groups::send_message_opts::SendMessageOpts;
use crate::messages::decoded_message::{DeletedBy, MessageBody};
use crate::tester;
use xmtp_content_types::{ContentCodec, text::TextCodec};
use xmtp_db::group_message::{ContentType, GroupMessageKind, MsgQueryArgs};
use xmtp_db::message_deletion::QueryMessageDeletion;

/// Test basic message deletion by the original sender
#[xmtp_common::test(unwrap_try = true)]
async fn test_delete_message_by_sender() {
    tester!(alix);
    tester!(bo);
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members_by_inbox_id(&[bo.inbox_id()]).await?;

    // Alix sends a message
    let text_content = TextCodec::encode("Hello, world!".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let message_id = alix_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    // Sync bo's group to receive the message
    let bo_groups = bo.sync_welcomes().await?;
    assert_eq!(bo_groups.len(), 1);
    let bo_group = &bo_groups[0];
    bo_group.sync().await?;

    // Verify the message exists for both
    let alix_messages = alix_group.find_messages(&MsgQueryArgs::default())?;
    assert_eq!(alix_messages.len(), 2); // 1 text message + 1 membership change

    let bo_messages = bo_group.find_messages(&MsgQueryArgs::default())?;
    assert_eq!(bo_messages.len(), 2);

    // Alix deletes the message
    let deletion_id = alix_group.delete_message(message_id.clone())?;
    assert!(!deletion_id.is_empty());

    // Publish and sync
    alix_group.publish_messages().await?;
    bo_group.sync().await?;

    // Verify the message is marked as deleted in the database
    let alix_conn = alix.context.db();
    assert!(alix_conn.is_message_deleted(&message_id)?);

    let bo_conn = bo.context.db();
    assert!(bo_conn.is_message_deleted(&message_id)?);

    // Verify deletion record exists
    let deletion = alix_conn.get_deletion_by_deleted_message_id(&message_id)?;
    assert!(deletion.is_some());
    let deletion = deletion.unwrap();
    assert_eq!(deletion.deleted_by_inbox_id, alix.inbox_id());
    assert!(!deletion.is_super_admin_deletion);
}

/// Test message deletion by super admin
#[xmtp_common::test(unwrap_try = true)]
async fn test_delete_message_by_super_admin() {
    tester!(alix);
    tester!(bo);
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members_by_inbox_id(&[bo.inbox_id()]).await?;

    // Bola sends a message
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = &bo_groups[0];

    let text_content = TextCodec::encode("Message from Bola".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let message_id = bo_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    // Sync alix's group to receive the message
    alix_group.sync().await?;

    // Alix (super admin) deletes Bola's message
    let deletion_id = alix_group.delete_message(message_id.clone())?;
    assert!(!deletion_id.is_empty());

    // Publish and sync
    alix_group.publish_messages().await?;
    bo_group.sync().await?;

    // Verify the message is marked as deleted
    let alix_conn = alix.context.db();
    assert!(alix_conn.is_message_deleted(&message_id)?);

    // Verify deletion was done by super admin
    let deletion = alix_conn.get_deletion_by_deleted_message_id(&message_id)?;
    assert!(deletion.is_some());
    let deletion = deletion.unwrap();
    assert_eq!(deletion.deleted_by_inbox_id, alix.inbox_id());
    assert!(deletion.is_super_admin_deletion);
}

/// Test that non-authorized members cannot delete messages
#[xmtp_common::test(unwrap_try = true)]
async fn test_delete_message_authorization_failure() {
    tester!(alix);
    tester!(bo);
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members_by_inbox_id(&[bo.inbox_id()]).await?;

    // Alix sends a message
    let text_content = TextCodec::encode("Alix's message".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let message_id = alix_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    // Sync bo's group
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = &bo_groups[0];
    bo_group.sync().await?;

    // Bola tries to delete Alix's message (should fail - not authorized)
    let result = bo_group.delete_message(message_id.clone());
    assert!(result.is_err());

    if let Err(GroupError::DeleteMessage(DeleteMessageError::NotAuthorized)) = result {
        // Expected error
    } else {
        panic!("Expected NotAuthorized error, got: {:?}", result);
    }
}

/// Test that transcript messages cannot be deleted
#[xmtp_common::test(unwrap_try = true)]
async fn test_cannot_delete_transcript_messages() {
    tester!(alix);
    tester!(bo);
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members_by_inbox_id(&[bo.inbox_id()]).await?;

    // Sync to get the membership change message
    alix_group.sync().await?;

    // Find the membership change message
    let messages = alix_group.find_messages(&MsgQueryArgs {
        kind: Some(GroupMessageKind::MembershipChange),
        ..Default::default()
    })?;
    assert!(!messages.is_empty());

    let membership_message_id = messages[0].id.clone();

    // Try to delete the membership change message (should fail)
    let result = alix_group.delete_message(membership_message_id);
    assert!(result.is_err());

    if let Err(GroupError::DeleteMessage(DeleteMessageError::CannotDeleteTranscript)) = result {
        // Expected error
    } else {
        panic!("Expected CannotDeleteTranscript error, got: {:?}", result);
    }
}

/// Test deleting a message that doesn't exist
#[xmtp_common::test(unwrap_try = true)]
async fn test_delete_nonexistent_message() {
    tester!(alix);
    let alix_group = alix.create_group(None, None)?;

    // Try to delete a message that doesn't exist
    let fake_message_id = vec![1, 2, 3, 4, 5];
    let result = alix_group.delete_message(fake_message_id);
    assert!(result.is_err());

    if let Err(GroupError::DeleteMessage(DeleteMessageError::MessageNotFound(_))) = result {
        // Expected error
    } else {
        panic!("Expected MessageNotFound error, got: {:?}", result);
    }
}

/// Test deleting an already deleted message
#[xmtp_common::test(unwrap_try = true)]
async fn test_delete_already_deleted_message() {
    tester!(alix);
    let alix_group = alix.create_group(None, None)?;

    // Send a message
    let text_content = TextCodec::encode("Test message".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let message_id = alix_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    // Delete the message
    alix_group.delete_message(message_id.clone())?;
    alix_group.publish_messages().await?;
    alix_group.sync().await?;

    // Try to delete again (should fail)
    let result = alix_group.delete_message(message_id);
    assert!(result.is_err());

    if let Err(GroupError::DeleteMessage(DeleteMessageError::MessageAlreadyDeleted)) = result {
        // Expected error
    } else {
        panic!("Expected MessageAlreadyDeleted error, got: {:?}", result);
    }
}

/// Test out-of-order deletion (deletion arrives before original message)
#[xmtp_common::test(unwrap_try = true)]
async fn test_out_of_order_deletion() {
    tester!(alix);
    tester!(bo);
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members_by_inbox_id(&[bo.inbox_id()]).await?;

    // Alix sends a message
    let text_content = TextCodec::encode("Test message".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let message_id = alix_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    // Alix deletes the message immediately (before bo syncs)
    alix_group.delete_message(message_id.clone())?;
    alix_group.publish_messages().await?;

    // Bola syncs and should receive both the message and deletion
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = &bo_groups[0];
    bo_group.sync().await?;

    // Verify the message is marked as deleted for Bola
    let bo_conn = bo.context.db();
    assert!(bo_conn.is_message_deleted(&message_id)?);

    // Verify the deletion record exists
    let deletion = bo_conn.get_deletion_by_deleted_message_id(&message_id)?;
    assert!(deletion.is_some());
}

/// Test that enrichment replaces deleted messages with placeholders
#[xmtp_common::test(unwrap_try = true)]
async fn test_enrichment_with_deleted_messages() {
    tester!(alix);
    tester!(bo);
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members_by_inbox_id(&[bo.inbox_id()]).await?;

    // Alix sends a message
    let text_content = TextCodec::encode("Secret message".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let message_id = alix_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    // Sync bo's group
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = &bo_groups[0];
    bo_group.sync().await?;

    // Verify Bola can see the original message content
    let messages = bo_group.find_enriched_messages(&MsgQueryArgs {
        content_types: Some(vec![ContentType::Text]),
        ..Default::default()
    })?;
    assert_eq!(messages.len(), 1);

    if let MessageBody::Text(text) = &messages[0].content {
        assert_eq!(text.content, "Secret message");
    } else {
        panic!("Expected Text message body");
    }

    // Alix deletes the message
    alix_group.delete_message(message_id.clone())?;
    alix_group.publish_messages().await?;
    bo_group.sync().await?;

    // Verify the enriched message is now a DeletedMessage placeholder
    let messages = bo_group.find_enriched_messages(&MsgQueryArgs::default())?;

    // Find the deleted message (skip membership changes)
    let deleted_msg = messages.iter().find(|msg| msg.metadata.id == message_id);
    assert!(deleted_msg.is_some());

    let deleted_msg = deleted_msg.unwrap();
    if let MessageBody::DeletedMessage { deleted_by } = &deleted_msg.content {
        assert_eq!(deleted_by, &DeletedBy::Sender);
    } else {
        panic!(
            "Expected DeletedMessage placeholder, got: {:?}",
            deleted_msg.content
        );
    }

    // Verify reactions and replies are cleared
    assert_eq!(deleted_msg.reactions.len(), 0);
    assert_eq!(deleted_msg.num_replies, 0);
}

/// Test that DeleteMessage content type is not shown in message lists
#[xmtp_common::test(unwrap_try = true)]
async fn test_delete_message_filtered_from_lists() {
    tester!(alix);
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

/// Test database query methods for deletions
#[xmtp_common::test(unwrap_try = true)]
async fn test_deletion_database_queries() {
    tester!(alix);
    let alix_group = alix.create_group(None, None)?;

    // Send multiple messages
    let mut message_ids = vec![];
    for i in 0..3 {
        let text_content = TextCodec::encode(format!("Message {}", i))?;
        let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
        let message_id = alix_group
            .send_message(&text_bytes, SendMessageOpts::default())
            .await?;
        message_ids.push(message_id);
    }

    // Delete two of them
    alix_group.delete_message(message_ids[0].clone())?;
    alix_group.delete_message(message_ids[2].clone())?;
    alix_group.publish_messages().await?;
    alix_group.sync().await?;

    let conn = alix.context.db();

    // Test get_deletions_for_messages
    let deletions = conn.get_deletions_for_messages(message_ids.clone())?;
    assert_eq!(deletions.len(), 2);

    // Test is_message_deleted
    assert!(conn.is_message_deleted(&message_ids[0])?);
    assert!(!conn.is_message_deleted(&message_ids[1])?);
    assert!(conn.is_message_deleted(&message_ids[2])?);

    // Test get_group_deletions
    let group_deletions = conn.get_group_deletions(&alix_group.group_id)?;
    assert_eq!(group_deletions.len(), 2);
}

/// Test that super admin deletion is marked correctly
#[xmtp_common::test(unwrap_try = true)]
async fn test_admin_deletion_flag() {
    tester!(alix);
    tester!(bo);
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members_by_inbox_id(&[bo.inbox_id()]).await?;

    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = &bo_groups[0];

    // Bola sends a message
    let text_content = TextCodec::encode("Bola's message".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let bo_message_id = bo_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    alix_group.sync().await?;

    // Alix (super admin) deletes Bola's message
    alix_group.delete_message(bo_message_id.clone())?;
    alix_group.publish_messages().await?;
    bo_group.sync().await?;

    // Verify deletion is marked as super admin deletion
    let bo_conn = bo.context.db();
    let deletion = bo_conn.get_deletion_by_deleted_message_id(&bo_message_id)?;
    assert!(deletion.is_some());

    let deletion = deletion.unwrap();
    assert!(deletion.is_super_admin_deletion);
    assert_eq!(deletion.deleted_by_inbox_id, alix.inbox_id());

    // Verify enriched message shows admin deletion
    let messages = bo_group.find_enriched_messages(&MsgQueryArgs::default())?;
    let deleted_msg = messages.iter().find(|msg| msg.metadata.id == bo_message_id);
    assert!(deleted_msg.is_some());

    if let MessageBody::DeletedMessage { deleted_by } = &deleted_msg.unwrap().content {
        if let DeletedBy::Admin(inbox_id) = deleted_by {
            assert_eq!(inbox_id, alix.inbox_id());
        } else {
            panic!("Expected Admin deletion");
        }
    } else {
        panic!("Expected DeletedMessage placeholder");
    }
}

/// Test that replies to deleted messages show the deleted state
#[xmtp_common::test(unwrap_try = true)]
async fn test_reply_to_deleted_message() {
    use xmtp_content_types::reply::ReplyCodec;

    tester!(alix);
    tester!(bo);
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members_by_inbox_id(&[bo.inbox_id()]).await?;

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
    let messages_before = alix_group.find_enriched_messages(&MsgQueryArgs::default())?;
    let reply_msg_before = messages_before
        .iter()
        .find(|msg| msg.metadata.id == reply_message_id);
    assert!(reply_msg_before.is_some());

    if let MessageBody::Reply(reply_body) = &reply_msg_before.unwrap().content {
        assert!(reply_body.in_reply_to.is_some());
        let in_reply_to = reply_body.in_reply_to.as_ref().unwrap();
        // Before deletion, the referenced message should be a Text message
        assert!(matches!(in_reply_to.content, MessageBody::Text(_)));
    } else {
        panic!("Expected Reply message");
    }

    // Now Alix deletes the original message
    alix_group.delete_message(original_message_id.clone())?;
    alix_group.publish_messages().await?;
    bo_group.sync().await?;
    alix_group.sync().await?;

    // Verify the reply now shows the deleted state for the referenced message
    let messages_after = alix_group.find_enriched_messages(&MsgQueryArgs::default())?;
    let reply_msg_after = messages_after
        .iter()
        .find(|msg| msg.metadata.id == reply_message_id);
    assert!(reply_msg_after.is_some());

    if let MessageBody::Reply(reply_body) = &reply_msg_after.unwrap().content {
        assert!(reply_body.in_reply_to.is_some());
        let in_reply_to = reply_body.in_reply_to.as_ref().unwrap();
        // After deletion, the referenced message should be a DeletedMessage
        if let MessageBody::DeletedMessage { deleted_by } = &in_reply_to.content {
            assert_eq!(deleted_by, &DeletedBy::Sender);
            // Reactions and replies should be cleared on the deleted referenced message
            assert_eq!(in_reply_to.reactions.len(), 0);
            assert_eq!(in_reply_to.num_replies, 0);
        } else {
            panic!(
                "Expected DeletedMessage in reply's in_reply_to, got: {:?}",
                in_reply_to.content
            );
        }
    } else {
        panic!("Expected Reply message");
    }
}

/// Test that cross-group deletion attempts are rejected
#[xmtp_common::test(unwrap_try = true)]
async fn test_cannot_delete_message_from_different_group() {
    tester!(alix);
    tester!(bo);

    // Create two separate groups
    let group1 = alix.create_group(None, None)?;
    group1.add_members_by_inbox_id(&[bo.inbox_id()]).await?;

    let group2 = alix.create_group(None, None)?;
    group2.add_members_by_inbox_id(&[bo.inbox_id()]).await?;

    // Alix sends a message in group1
    let text_content = TextCodec::encode("Message in group 1".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let group1_message_id = group1
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    // Bo syncs both groups
    let bo_groups = bo.sync_welcomes().await?;
    assert_eq!(bo_groups.len(), 2);
    bo_groups[0].sync().await?;
    bo_groups[1].sync().await?;

    // Attempt to delete group1's message from group2 (should fail)
    let result = group2.delete_message(group1_message_id.clone());
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        GroupError::DeleteMessage(DeleteMessageError::NotAuthorized)
    ));

    // Verify the message in group1 is NOT deleted
    let alix_conn = alix.context.db();
    assert!(!alix_conn.is_message_deleted(&group1_message_id)?);

    // Verify we can still delete it from the correct group
    group1.delete_message(group1_message_id.clone())?;
    assert!(alix_conn.is_message_deleted(&group1_message_id)?);
}
