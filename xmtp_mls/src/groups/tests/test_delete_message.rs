use crate::groups::GroupError;
use crate::groups::error::DeleteMessageError;
use crate::groups::send_message_opts::SendMessageOpts;
use crate::messages::decoded_message::{DeletedBy, MessageBody};
use crate::tester;
use xmtp_content_types::{ContentCodec, text::TextCodec};
use xmtp_db::group_message::{ContentType, GroupMessageKind, MsgQueryArgs, QueryGroupMessage};
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
    assert!(matches!(
        result,
        Err(GroupError::DeleteMessage(DeleteMessageError::NotAuthorized))
    ));
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
    assert!(matches!(
        result,
        Err(GroupError::DeleteMessage(
            DeleteMessageError::NonDeletableMessage
        ))
    ));
}

/// Test deleting a message that doesn't exist
#[xmtp_common::test(unwrap_try = true)]
async fn test_delete_nonexistent_message() {
    tester!(alix);
    let alix_group = alix.create_group(None, None)?;

    // Try to delete a message that doesn't exist
    let fake_message_id = vec![1, 2, 3, 4, 5];
    let result = alix_group.delete_message(fake_message_id);
    assert!(matches!(
        result,
        Err(GroupError::DeleteMessage(
            DeleteMessageError::MessageNotFound(_)
        ))
    ));
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
    assert!(matches!(
        result,
        Err(GroupError::DeleteMessage(
            DeleteMessageError::MessageAlreadyDeleted
        ))
    ));
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

/// Test deletion record stored before the original message arrives.
#[xmtp_common::test(unwrap_try = true)]
async fn test_true_out_of_order_deletion_by_sender() {
    use xmtp_db::Store;
    use xmtp_db::group_message::{DeliveryStatus, StoredGroupMessage};
    use xmtp_db::message_deletion::StoredMessageDeletion;

    tester!(alix);
    let alix_group = alix.create_group(None, None)?;
    alix_group.sync().await?;

    let alix_conn = alix.context.db();
    let alix_inbox_id = alix.inbox_id().to_string();

    // Simulate a message ID that will be "sent" later
    let future_message_id = vec![0xDE, 0xAD, 0xBE, 0xEF];
    let delete_message_id = vec![0x01, 0x02, 0x03];

    // Step 1: First store the DeleteMessage itself in group_messages
    let delete_msg = xmtp_proto::xmtp::mls::message_contents::content_types::DeleteMessage {
        message_id: hex::encode(&future_message_id),
    };
    let delete_msg_content =
        xmtp_content_types::delete_message::DeleteMessageCodec::encode(delete_msg)?;
    let delete_msg_bytes = xmtp_content_types::encoded_content_to_bytes(delete_msg_content);

    let delete_message = StoredGroupMessage {
        id: delete_message_id.clone(),
        group_id: alix_group.group_id.clone(),
        decrypted_message_bytes: delete_msg_bytes,
        sent_at_ns: xmtp_common::time::now_ns(),
        kind: GroupMessageKind::Application,
        sender_installation_id: alix.context.installation_id().to_vec(),
        sender_inbox_id: alix_inbox_id.clone(),
        delivery_status: DeliveryStatus::Published,
        content_type: ContentType::DeleteMessage,
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
    delete_message.store(&alix_conn)?;

    // Step 2: Store the deletion record (references the DeleteMessage above)
    // This simulates the deletion arriving before the original message
    let deletion = StoredMessageDeletion {
        id: delete_message_id.clone(),
        group_id: alix_group.group_id.clone(),
        deleted_message_id: future_message_id.clone(),
        deleted_by_inbox_id: alix_inbox_id.clone(), // Sender deleting their own message
        is_super_admin_deletion: false,             // Regular user deletion
        deleted_at_ns: xmtp_common::time::now_ns(),
    };
    deletion.store(&alix_conn)?;

    // Verify deletion record exists but target message doesn't
    assert!(alix_conn.get_group_message(&future_message_id)?.is_none());
    assert!(
        alix_conn
            .get_deletion_by_deleted_message_id(&future_message_id)?
            .is_some()
    );

    // Step 3: Now store the original message (simulating it arriving later)
    let text_content = TextCodec::encode("Message to be deleted".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);

    let message = StoredGroupMessage {
        id: future_message_id.clone(),
        group_id: alix_group.group_id.clone(),
        decrypted_message_bytes: text_bytes,
        sent_at_ns: xmtp_common::time::now_ns() - 1000, // Sent before deletion
        kind: GroupMessageKind::Application,
        sender_installation_id: alix.context.installation_id().to_vec(),
        sender_inbox_id: alix_inbox_id.clone(), // Same sender as deleter
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

    // Step 4: Verify the message is now marked as deleted via is_message_deleted
    assert!(alix_conn.is_message_deleted(&future_message_id)?);

    // Step 5: Verify enrichment correctly shows the message as deleted
    let enriched = alix_group.find_enriched_messages(&MsgQueryArgs::default())?;
    let deleted_msg = enriched.iter().find(|m| m.metadata.id == future_message_id);
    assert!(
        deleted_msg.is_some(),
        "Message should be in enriched results"
    );

    let deleted_msg = deleted_msg.unwrap();
    let MessageBody::DeletedMessage { deleted_by } = &deleted_msg.content else {
        panic!(
            "Expected DeletedMessage placeholder, got {:?}",
            deleted_msg.content
        );
    };
    assert_eq!(*deleted_by, DeletedBy::Sender);
}

/// Test that unauthorized deletion records are rejected at query time.
#[xmtp_common::test(unwrap_try = true)]
async fn test_out_of_order_unauthorized_deletion_rejected() {
    use xmtp_db::Store;
    use xmtp_db::group_message::{DeliveryStatus, StoredGroupMessage};
    use xmtp_db::message_deletion::StoredMessageDeletion;

    tester!(alix);
    tester!(bo);
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members_by_inbox_id(&[bo.inbox_id()]).await?;

    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = &bo_groups[0];
    bo_group.sync().await?;

    let bo_conn = bo.context.db();
    let alix_inbox_id = alix.inbox_id().to_string();
    let bo_inbox_id = bo.inbox_id().to_string();

    let future_message_id = vec![0xBA, 0xDC, 0x0D, 0xE0];
    let malicious_delete_msg_id = vec![0x0B, 0xAD, 0x01];

    // Step 1: First store the malicious DeleteMessage in group_messages (FK requirement)
    let delete_msg = xmtp_proto::xmtp::mls::message_contents::content_types::DeleteMessage {
        message_id: hex::encode(&future_message_id),
    };
    let delete_msg_content =
        xmtp_content_types::delete_message::DeleteMessageCodec::encode(delete_msg)?;
    let delete_msg_bytes = xmtp_content_types::encoded_content_to_bytes(delete_msg_content);

    let malicious_delete_message = StoredGroupMessage {
        id: malicious_delete_msg_id.clone(),
        group_id: bo_group.group_id.clone(),
        decrypted_message_bytes: delete_msg_bytes,
        sent_at_ns: xmtp_common::time::now_ns(),
        kind: GroupMessageKind::Application,
        sender_installation_id: vec![1, 2, 3],
        sender_inbox_id: bo_inbox_id.clone(), // Bo sending the delete
        delivery_status: DeliveryStatus::Published,
        content_type: ContentType::DeleteMessage,
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
    malicious_delete_message.store(&bo_conn)?;

    // Step 2: Bo (non-admin) tries to delete Alix's message by storing a deletion record
    // This simulates a malicious deletion arriving before the message
    let malicious_deletion = StoredMessageDeletion {
        id: malicious_delete_msg_id.clone(),
        group_id: bo_group.group_id.clone(),
        deleted_message_id: future_message_id.clone(),
        deleted_by_inbox_id: bo_inbox_id.clone(), // Bo trying to delete
        is_super_admin_deletion: false,           // Bo is not super admin
        deleted_at_ns: xmtp_common::time::now_ns(),
    };
    malicious_deletion.store(&bo_conn)?;

    // Step 3: Store Alix's message (arriving after the malicious deletion)
    let text_content = TextCodec::encode("Alix's message".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);

    let message = StoredGroupMessage {
        id: future_message_id.clone(),
        group_id: bo_group.group_id.clone(),
        decrypted_message_bytes: text_bytes,
        sent_at_ns: xmtp_common::time::now_ns() - 1000,
        kind: GroupMessageKind::Application,
        sender_installation_id: vec![1, 2, 3],
        sender_inbox_id: alix_inbox_id.clone(), // Message from Alix
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

    // Deletion record exists but is unauthorized
    assert!(bo_conn.is_message_deleted(&future_message_id)?);

    // Enrichment should show original message since deletion is unauthorized
    let enriched = bo_group.find_enriched_messages(&MsgQueryArgs::default())?;
    let msg = enriched.iter().find(|m| m.metadata.id == future_message_id);
    assert!(msg.is_some(), "Message should be in enriched results");

    let msg = msg.unwrap();
    match &msg.content {
        MessageBody::Text(text) => {
            assert_eq!(text.content, "Alix's message");
        }
        MessageBody::DeletedMessage { .. } => {
            panic!("Message should NOT be deleted - unauthorized deletion should be rejected");
        }
        other => {
            panic!("Unexpected message body: {:?}", other);
        }
    }
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

    let MessageBody::Text(text) = &messages[0].content else {
        panic!("Expected Text message body");
    };
    assert_eq!(text.content, "Secret message");

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
    let MessageBody::DeletedMessage { deleted_by } = &deleted_msg.content else {
        panic!("Expected DeletedMessage placeholder");
    };
    assert_eq!(*deleted_by, DeletedBy::Sender);

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

    let MessageBody::DeletedMessage { deleted_by } = &deleted_msg.unwrap().content else {
        panic!("Expected DeletedMessage placeholder");
    };
    let DeletedBy::Admin(admin_inbox_id) = deleted_by else {
        panic!("Expected Admin deletion");
    };
    assert_eq!(admin_inbox_id, alix.inbox_id());
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
    let messages_after = alix_group.find_enriched_messages(&MsgQueryArgs::default())?;
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
    assert!(matches!(
        result,
        Err(GroupError::DeleteMessage(DeleteMessageError::NotAuthorized))
    ));

    // Verify the message in group1 is NOT deleted
    let alix_conn = alix.context.db();
    assert!(!alix_conn.is_message_deleted(&group1_message_id)?);

    // Verify we can still delete it from the correct group
    group1.delete_message(group1_message_id.clone())?;
    assert!(alix_conn.is_message_deleted(&group1_message_id)?);
}

/// Test that we cannot delete a delete message
#[xmtp_common::test(unwrap_try = true)]
async fn test_cannot_delete_delete_message() {
    tester!(alix);
    tester!(bo);
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members_by_inbox_id(&[bo.inbox_id()]).await?;

    // Alix sends a message
    let text_content = TextCodec::encode("Original message".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let original_message_id = alix_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    // Alix deletes the message
    let delete_message_id = alix_group.delete_message(original_message_id.clone())?;

    // Publish the deletion
    alix_group.publish_messages().await?;

    // Verify the original message is deleted
    let alix_conn = alix.context.db();
    assert!(alix_conn.is_message_deleted(&original_message_id)?);

    // Verify the delete message exists in the database
    let delete_msg = alix_conn.get_group_message(&delete_message_id)?;
    assert!(delete_msg.is_some());
    let delete_msg = delete_msg.unwrap();
    assert_eq!(delete_msg.content_type, ContentType::DeleteMessage);

    // Try to delete the delete message - should fail
    let result = alix_group.delete_message(delete_message_id.clone());
    assert!(matches!(
        result,
        Err(GroupError::DeleteMessage(
            DeleteMessageError::NonDeletableMessage
        ))
    ));

    // Verify the delete message is NOT deleted
    assert!(!alix_conn.is_message_deleted(&delete_message_id)?);
}

/// Test concurrent deletions - multiple people trying to delete the same message
#[xmtp_common::test(unwrap_try = true)]
async fn test_concurrent_deletions() {
    tester!(alix);
    tester!(bo);
    tester!(caro);

    // Alix creates a group with Bo and Caro, making Bo also a super admin
    let alix_group = alix.create_group(None, None)?;
    alix_group
        .add_members_by_inbox_id(&[bo.inbox_id(), caro.inbox_id()])
        .await?;

    // Bo syncs and gets the group
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = &bo_groups[0];
    bo_group.sync().await?;

    // Caro syncs and gets the group
    let caro_groups = caro.sync_welcomes().await?;
    let caro_group = &caro_groups[0];
    caro_group.sync().await?;

    // Make Bo a super admin
    alix_group
        .update_admin_list(
            crate::groups::UpdateAdminListType::AddSuper,
            bo.inbox_id().to_string(),
        )
        .await?;
    alix_group.publish_messages().await?;

    // Sync everyone
    bo_group.sync().await?;
    caro_group.sync().await?;

    // Caro sends a message
    let text_content = TextCodec::encode("Message to be deleted".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let message_id = caro_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    // Sync Alix and Bo to receive the message
    alix_group.sync().await?;
    bo_group.sync().await?;

    // Both Alix (super admin) and Bo (super admin) try to delete the message "concurrently"
    // In practice, one will succeed and one will get MessageAlreadyDeleted
    let alix_deletion = alix_group.delete_message(message_id.clone());
    alix_group.publish_messages().await?;

    // Bo syncs to get Alix's deletion, then tries to delete
    bo_group.sync().await?;
    let bo_deletion = bo_group.delete_message(message_id.clone());

    // At least one should succeed, the other should get MessageAlreadyDeleted
    let alix_succeeded = alix_deletion.is_ok();
    let bo_succeeded = bo_deletion.is_ok();

    // At least one must succeed
    assert!(
        alix_succeeded || bo_succeeded,
        "At least one deletion should succeed"
    );

    // If both succeeded locally (before sync), both should publish
    // After sync, both should see the message as deleted
    if bo_succeeded {
        bo_group.publish_messages().await?;
    }

    alix_group.sync().await?;
    bo_group.sync().await?;
    caro_group.sync().await?;

    // Verify the message is deleted for everyone
    let alix_conn = alix.context.db();
    let bo_conn = bo.context.db();
    let caro_conn = caro.context.db();

    assert!(alix_conn.is_message_deleted(&message_id)?);
    assert!(bo_conn.is_message_deleted(&message_id)?);
    assert!(caro_conn.is_message_deleted(&message_id)?);

    // Verify enriched messages show the deleted state
    let caro_messages = caro_group.find_enriched_messages(&MsgQueryArgs::default())?;
    let deleted_msg = caro_messages
        .iter()
        .find(|msg| msg.metadata.id == message_id);
    assert!(deleted_msg.is_some());

    let deleted_msg = deleted_msg.unwrap();
    let MessageBody::DeletedMessage { deleted_by } = &deleted_msg.content else {
        panic!("Expected DeletedMessage placeholder");
    };
    // The message was deleted by a super admin (either Alix or Bo)
    let DeletedBy::Admin(admin_inbox_id) = deleted_by else {
        panic!("Expected Admin deletion");
    };
    assert!(
        admin_inbox_id == alix.inbox_id() || admin_inbox_id == bo.inbox_id(),
        "Deletion should be by Alix or Bo, got: {}",
        admin_inbox_id
    );
}

/// Test that deletion works correctly when both sender and admin try to delete
#[xmtp_common::test(unwrap_try = true)]
async fn test_sender_and_admin_both_delete() {
    tester!(alix);
    tester!(bo);

    // Alix creates a group with Bo
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members_by_inbox_id(&[bo.inbox_id()]).await?;

    // Bo syncs and gets the group
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = &bo_groups[0];
    bo_group.sync().await?;

    // Bo sends a message
    let text_content = TextCodec::encode("Bo's message".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let message_id = bo_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    // Alix syncs to receive the message
    alix_group.sync().await?;

    // Bo (sender) deletes their own message first
    bo_group.delete_message(message_id.clone())?;
    bo_group.publish_messages().await?;

    // Alix syncs to see the deletion
    alix_group.sync().await?;

    // Alix (super admin) tries to delete the same message - should fail
    let result = alix_group.delete_message(message_id.clone());
    assert!(matches!(
        result,
        Err(GroupError::DeleteMessage(
            DeleteMessageError::MessageAlreadyDeleted
        ))
    ));

    // Verify the message is deleted and shows as deleted by sender
    let bo_messages = bo_group.find_enriched_messages(&MsgQueryArgs::default())?;
    let deleted_msg = bo_messages.iter().find(|msg| msg.metadata.id == message_id);
    assert!(deleted_msg.is_some());

    let deleted_msg = deleted_msg.unwrap();
    let MessageBody::DeletedMessage { deleted_by } = &deleted_msg.content else {
        panic!("Expected DeletedMessage placeholder");
    };
    assert_eq!(*deleted_by, DeletedBy::Sender);
}

/// Test that sender deletion shows DeletedBy::Sender even with is_super_admin_deletion=true.
#[xmtp_common::test(unwrap_try = true)]
async fn test_out_of_order_sender_deletion_shows_correct_deleted_by() {
    use xmtp_db::Store;
    use xmtp_db::group_message::{DeliveryStatus, StoredGroupMessage};
    use xmtp_db::message_deletion::StoredMessageDeletion;

    tester!(alix);
    let alix_group = alix.create_group(None, None)?;
    let alix_conn = alix.context.db();
    let alix_inbox_id = alix.inbox_id().to_string();

    let future_message_id = vec![0xF1, 0xA6, 0xC0, 0xDE];
    let delete_message_id = vec![0xD3, 0x1E, 0x7E];

    // Step 1: Store the DeleteMessage in group_messages first
    let delete_msg = xmtp_proto::xmtp::mls::message_contents::content_types::DeleteMessage {
        message_id: hex::encode(&future_message_id),
    };
    let delete_msg_content =
        xmtp_content_types::delete_message::DeleteMessageCodec::encode(delete_msg)?;
    let delete_msg_bytes = xmtp_content_types::encoded_content_to_bytes(delete_msg_content);

    let delete_message = StoredGroupMessage {
        id: delete_message_id.clone(),
        group_id: alix_group.group_id.clone(),
        decrypted_message_bytes: delete_msg_bytes,
        sent_at_ns: xmtp_common::time::now_ns(),
        kind: GroupMessageKind::Application,
        sender_installation_id: vec![1, 2, 3],
        sender_inbox_id: alix_inbox_id.clone(), // Alix sends the delete
        delivery_status: DeliveryStatus::Published,
        content_type: ContentType::DeleteMessage,
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
    delete_message.store(&alix_conn)?;

    // Store deletion with is_super_admin_deletion=true (out-of-order scenario)
    let deletion = StoredMessageDeletion {
        id: delete_message_id.clone(),
        group_id: alix_group.group_id.clone(),
        deleted_message_id: future_message_id.clone(),
        deleted_by_inbox_id: alix_inbox_id.clone(),
        is_super_admin_deletion: true,
        deleted_at_ns: xmtp_common::time::now_ns(),
    };
    deletion.store(&alix_conn)?;

    // Store the original message (same sender as deleter)
    let text_content = TextCodec::encode("Alix's message".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);

    let original_message = StoredGroupMessage {
        id: future_message_id.clone(),
        group_id: alix_group.group_id.clone(),
        decrypted_message_bytes: text_bytes,
        sent_at_ns: xmtp_common::time::now_ns() - 1000,
        kind: GroupMessageKind::Application,
        sender_installation_id: vec![1, 2, 3],
        sender_inbox_id: alix_inbox_id.clone(), // Same as deleter
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
    original_message.store(&alix_conn)?;

    // Verify enrichment shows DeletedBy::Sender since deleter == sender
    let enriched = alix_group.find_enriched_messages(&MsgQueryArgs::default())?;
    let deleted_msg = enriched
        .iter()
        .find(|m| m.metadata.id == future_message_id)
        .expect("Message should be in enriched results");

    let MessageBody::DeletedMessage { deleted_by } = &deleted_msg.content else {
        panic!("Expected DeletedMessage placeholder");
    };
    assert_eq!(*deleted_by, DeletedBy::Sender);
}

/// Test that stream_message_deletions receives a callback when another client deletes a message
#[xmtp_common::test(unwrap_try = true)]
async fn test_stream_message_deletions_from_other_client() {
    use crate::utils::FullXmtpClient;
    use parking_lot::Mutex;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::Notify;
    use xmtp_common::StreamHandle;

    tester!(alix);
    tester!(bo);

    // Create a group and add bo
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;

    // Bo syncs to join the group
    let bo_groups = bo.sync_welcomes().await?;
    assert_eq!(bo_groups.len(), 1);
    let bo_group = &bo_groups[0];

    // Alix sends a message
    let text_content = TextCodec::encode("Message to be deleted".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let message_id = alix_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    // Bo syncs to receive the message (needed so the original message is in Bo's DB)
    bo_group.sync().await?;

    // Set up shared state for Bo's callback
    let deleted_message: Arc<Mutex<Option<crate::messages::decoded_message::DecodedMessage>>> =
        Arc::new(Mutex::new(None));
    let notify = Arc::new(Notify::new());

    let deleted_message_clone = deleted_message.clone();
    let notify_clone = notify.clone();

    // Bo sets up the deletion stream with callback
    // (Bo will receive the deletion event when syncing, since alix sent it)
    let mut handle = FullXmtpClient::stream_message_deletions_with_callback(
        Arc::new(bo.client.clone()),
        move |msg| {
            if let Ok(message) = msg {
                *deleted_message_clone.lock() = Some(message);
                notify_clone.notify_one();
            }
        },
    );

    // Wait for stream to be ready
    handle.wait_for_ready().await;

    // Alix deletes the message and publishes
    alix_group.delete_message(message_id.clone())?;
    alix_group.publish_messages().await?;

    // Bo syncs to receive the deletion (this triggers the MessageDeleted event)
    bo_group.sync().await?;

    // Wait for callback to be called (5s timeout provides buffer for async processing)
    xmtp_common::time::timeout(Duration::from_secs(5), notify.notified())
        .await
        .expect("Timed out waiting for deletion callback");

    // Verify the stream received the correct deleted message
    let deleted_message = deleted_message
        .lock()
        .take()
        .expect("No deleted message received");
    assert_eq!(deleted_message.metadata.id, message_id);
    assert_eq!(deleted_message.metadata.sender_inbox_id, alix.inbox_id());
}

/// Test that stream_message_deletions fires for self-deletions after publishing.
/// When the same client deletes a message and publishes it, the local event
/// should be emitted once the deletion is confirmed on the network.
#[xmtp_common::test(unwrap_try = true)]
async fn test_stream_message_deletions_fires_for_self_after_publish() {
    use crate::utils::FullXmtpClient;
    use parking_lot::Mutex;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::Notify;
    use xmtp_common::StreamHandle;

    tester!(alix);

    // Create a group
    let alix_group = alix.create_group(None, None)?;

    // Alix sends a message
    let text_content = TextCodec::encode("Message to be deleted".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let message_id = alix_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    // Set up shared state for Alix's callback
    let deleted_message: Arc<Mutex<Option<crate::messages::decoded_message::DecodedMessage>>> =
        Arc::new(Mutex::new(None));
    let notify = Arc::new(Notify::new());

    let deleted_message_clone = deleted_message.clone();
    let notify_clone = notify.clone();

    // Alix sets up the deletion stream with callback
    let mut handle = FullXmtpClient::stream_message_deletions_with_callback(
        Arc::new(alix.client.clone()),
        move |msg| {
            if let Ok(message) = msg {
                *deleted_message_clone.lock() = Some(message);
                notify_clone.notify_one();
            }
        },
    );

    // Wait for stream to be ready
    handle.wait_for_ready().await;

    // Alix deletes the message and publishes
    alix_group.delete_message(message_id.clone())?;
    alix_group.publish_messages().await?;

    // Alix syncs (the deletion message is skipped because it was already processed locally)
    alix_group.sync().await?;

    // Wait for the deletion event callback (5s timeout provides buffer for async processing)
    let result = xmtp_common::time::timeout(Duration::from_secs(5), notify.notified()).await;

    // Verify the callback was called (self-deletions fire local events after network confirmation)
    assert!(
        result.is_ok(),
        "stream_message_deletions should fire for self-deletions after publish"
    );

    // Verify the correct message was received
    let received = deleted_message.lock();
    assert!(
        received.is_some(),
        "Deletion event should be received for self-deletions"
    );
    let received_msg = received.as_ref().unwrap();
    assert_eq!(
        received_msg.metadata.id, message_id,
        "Deleted message ID should match"
    );
}
