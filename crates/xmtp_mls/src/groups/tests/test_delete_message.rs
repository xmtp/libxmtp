use crate::groups::GroupError;
use crate::groups::error::DeleteMessageError;
use crate::groups::send_message_opts::SendMessageOpts;
use crate::messages::decoded_message::{DeletedBy, MessageBody};
use crate::tester;
use xmtp_content_types::{ContentCodec, compression::compress, text::TextCodec};
use xmtp_db::Store;
use xmtp_db::group_message::{
    ContentType, DeliveryStatus, GroupMessageKind, MsgQueryArgs, QueryGroupMessage,
    StoredGroupMessage,
};
use xmtp_db::message_deletion::QueryMessageDeletion;
use xmtp_proto::types::GroupId;
use xmtp_proto::xmtp::mls::message_contents::Compression;

/// The inbox that marked `id` deleted, or `None` while it is unmarked.
fn mark_of(db: &impl QueryGroupMessage, id: &[u8]) -> Option<String> {
    db.get_group_message(id)
        .expect("message read")
        .and_then(|message| message.deleted_by_inbox_id)
}

fn cached_text_row(
    group_id: GroupId,
    id: Vec<u8>,
    sender_inbox_id: &str,
    raw_authority: &str,
) -> Result<StoredGroupMessage, xmtp_content_types::CodecError> {
    let mut encoded = TextCodec::encode("stored text".into())?;
    encoded
        .r#type
        .as_mut()
        .expect("text has a type")
        .authority_id = raw_authority.into();
    let idempotency_key = hex::encode(&id);
    Ok(StoredGroupMessage {
        id,
        group_id,
        decrypted_message_bytes: xmtp_content_types::encoded_content_to_bytes(encoded),
        sent_at_ns: xmtp_common::time::now_ns(),
        kind: GroupMessageKind::Application,
        sender_installation_id: vec![1, 2, 3],
        sender_inbox_id: sender_inbox_id.into(),
        delivery_status: DeliveryStatus::Published,
        content_type: ContentType::Text,
        version_major: 1,
        version_minor: 0,
        authority_id: "xmtp.org".into(),
        reference_id: None,
        expiry_unknown: false,
        deleted_by_inbox_id: None,
        expire_at_ns: None,
        sequence_id: 900_001,
        envelope_hash: None,
        expiry_ns: None,
        inserted_at_ns: 0,
        should_push: false,
        idempotency_key,
    })
}

// verifies: CTYPE-018, PROC-037
#[xmtp_common::test(unwrap_try = true)]
async fn old_cached_text_cannot_be_deleted_locally() {
    tester!(alix);
    let group = alix.create_group(None, None)?;
    let db = alix.context.db();
    for (id, sender) in [
        (vec![0xC7, 0x01], alix.inbox_id()),
        (vec![0xC7, 0x02], "another-inbox"),
    ] {
        let target = cached_text_row(group.group_id, id, sender, "custom.example")?;
        target.store(&db)?;
        let loaded = db.get_group_message(&target.id)?.expect("stored target");
        assert_eq!(loaded.content_type, ContentType::Text);
        assert_eq!(loaded.authority_id, "xmtp.org");
        assert!(matches!(
            group.delete_message(target.id),
            Err(GroupError::DeleteMessage(
                DeleteMessageError::NonDeletableMessage
            ))
        ));
    }

    let standard = cached_text_row(
        group.group_id,
        vec![0xC7, 0x03],
        alix.inbox_id(),
        "xmtp.org",
    )?;
    standard.store(&db)?;
    assert!(!group.delete_message(standard.id)?.is_empty());
}

// verifies: CTYPE-018, PROC-037
#[xmtp_common::test(unwrap_try = true)]
async fn received_deletion_rejects_old_cached_text_target() {
    use xmtp_content_types::delete_message::DeleteMessageCodec;
    use xmtp_proto::xmtp::mls::message_contents::content_types::DeleteMessage;

    tester!(alix);
    tester!(bo);
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;
    let bo_group = bo.sync_welcomes().await?.remove(0);
    bo_group.sync().await?;
    let db = bo.context.db();

    for (id, sender, raw_authority) in [
        (vec![0x11; 32], alix.inbox_id(), "custom.example"),
        (vec![0x12; 32], bo.inbox_id(), "custom.example"),
        (vec![0x13; 32], alix.inbox_id(), "xmtp.org"),
    ] {
        cached_text_row(bo_group.group_id, id.clone(), sender, raw_authority)?.store(&db)?;
        let deletion = DeleteMessageCodec::encode(DeleteMessage {
            message_id: hex::encode(id),
        })?;
        alix_group
            .send_message(
                &xmtp_content_types::encoded_content_to_bytes(deletion),
                SendMessageOpts::default(),
            )
            .await?;
    }
    bo_group.sync().await?;
    // A wire type that CTYPE-018 refuses keeps its content and no mark.
    for id in [vec![0x11; 32], vec![0x12; 32]] {
        assert_eq!(mark_of(&db, &id), None);
        assert!(
            !db.get_group_message(&id)?
                .unwrap()
                .decrypted_message_bytes
                .is_empty()
        );
    }
    assert_eq!(mark_of(&db, &[0x13; 32]).as_deref(), Some(alix.inbox_id()));
    assert!(
        db.get_group_message([0x13; 32])?
            .unwrap()
            .decrypted_message_bytes
            .is_empty()
    );
}

/// Test basic message deletion by the original sender
#[xmtp_common::test(unwrap_try = true)]
async fn test_delete_message_by_sender() {
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

    // Sync bo's group to receive the message
    let bo_groups = bo.wait_for_welcomes().await?;
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
    assert_eq!(
        mark_of(&alix_conn, &message_id).as_deref(),
        Some(alix.inbox_id())
    );

    let bo_conn = bo.context.db();
    assert_eq!(
        mark_of(&bo_conn, &message_id).as_deref(),
        Some(alix.inbox_id())
    );

    // Verify deletion record exists
    for conn in [&alix_conn, &bo_conn] {
        let deletion = conn.get_message_deletion(&deletion_id)?.unwrap();
        assert_eq!(deletion.deleted_message_id, message_id);
        assert_eq!(deletion.deleted_by_inbox_id, alix.inbox_id());
        // The creator is a super admin; its own message still reads as Sender.
        assert!(deletion.is_super_admin_deletion);
        assert!(deletion.authenticated);
    }
}

// verifies: CTYPE-024, PROC-037
#[xmtp_common::test(unwrap_try = true)]
async fn compressed_delete_message_is_applied() {
    use xmtp_content_types::delete_message::DeleteMessageCodec;
    use xmtp_proto::xmtp::mls::message_contents::content_types::DeleteMessage;

    tester!(alix);
    tester!(bo);
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;
    let bo_group = bo.sync_welcomes().await?.remove(0);

    let text = TextCodec::encode("delete me".into())?;
    let message_id = alix_group
        .send_message(
            &xmtp_content_types::encoded_content_to_bytes(text),
            SendMessageOpts::default(),
        )
        .await?;
    bo_group.sync().await?;
    assert_eq!(mark_of(&bo.context.db(), &message_id), None);

    let delete = DeleteMessageCodec::encode(DeleteMessage {
        message_id: hex::encode(&message_id),
    })?;
    let delete = compress(delete, Compression::Deflate)?;
    alix_group
        .send_message(
            &xmtp_content_types::encoded_content_to_bytes(delete),
            SendMessageOpts::default(),
        )
        .await?;
    bo_group.sync().await?;

    assert_eq!(
        mark_of(&bo.context.db(), &message_id).as_deref(),
        Some(alix.inbox_id())
    );
}

/// Test message deletion by super admin
// verifies: PROC-037
#[xmtp_common::test(unwrap_try = true)]
async fn test_delete_message_by_super_admin() {
    tester!(alix);
    tester!(bo);
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;

    // Bola sends a message
    let bo_groups = bo.wait_for_welcomes().await?;
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

    // Verify the message is marked as deleted on both installations
    let alix_conn = alix.context.db();
    assert_eq!(
        mark_of(&alix_conn, &message_id).as_deref(),
        Some(alix.inbox_id())
    );
    assert_eq!(
        mark_of(&bo.context.db(), &message_id).as_deref(),
        Some(alix.inbox_id())
    );

    // Verify deletion was done by super admin
    let deletion = alix_conn.get_message_deletion(&deletion_id)?.unwrap();
    assert_eq!(deletion.deleted_by_inbox_id, alix.inbox_id());
    assert!(deletion.is_super_admin_deletion);
    let MessageBody::DeletedMessage { deleted_by } = &bo_group
        .find_messages_v2(&MsgQueryArgs::default())?
        .into_iter()
        .find(|message| message.metadata.id == message_id)
        .unwrap()
        .content
    else {
        panic!("expected a placeholder");
    };
    assert_eq!(*deleted_by, DeletedBy::Admin(alix.inbox_id().to_string()));
}

/// Test that non-authorized members cannot delete messages
#[xmtp_common::test(unwrap_try = true)]
async fn test_delete_message_authorization_failure() {
    tester!(alix);
    tester!(bo);
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;

    // Alix sends a message
    let text_content = TextCodec::encode("Alix's message".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let message_id = alix_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    // Sync bo's group
    let bo_groups = bo.wait_for_welcomes().await?;
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
    alix_group.add_members(&[bo.inbox_id()]).await?;

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

/// A member who joins after a message and its deletion receives both in topic order
#[xmtp_common::test(unwrap_try = true)]
async fn test_out_of_order_deletion() {
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

    // Alix deletes the message immediately (before bo syncs)
    alix_group.delete_message(message_id.clone())?;
    alix_group.publish_messages().await?;

    // Bola syncs and should receive both the message and deletion
    let bo_groups = bo.wait_for_welcomes().await?;
    let bo_group = &bo_groups[0];
    bo_group.sync().await?;

    // The topic orders the message before its deletion.
    let bo_conn = bo.context.db();
    assert_eq!(
        mark_of(&bo_conn, &message_id).as_deref(),
        Some(alix.inbox_id())
    );
}

/// An admin delete of a stored message that has already expired names the
/// admin and carries no body.
// verifies: META-051
#[xmtp_common::test(unwrap_try = true)]
async fn admin_delete_of_expired_message_names_the_admin() {
    use crate::context::CoreContext;
    use crate::subscriptions::{StreamMessages, internal::InternalEvent};
    use futures::StreamExt;
    use xmtp_content_types::delete_message::DeleteMessageCodec;
    use xmtp_proto::xmtp::mls::message_contents::content_types::DeleteMessage;

    tester!(alix);
    let alix_group = alix.create_group(None, None)?;
    let target_id = vec![0xAD, 0x01];
    let target = StoredGroupMessage {
        expire_at_ns: Some(xmtp_common::time::now_ns() - 1),
        ..cached_text_row(alix_group.group_id, target_id.clone(), "sender", "xmtp.org")?
    };
    target.store(&alix.context.db())?;
    let delete = DeleteMessageCodec::encode(DeleteMessage {
        message_id: hex::encode(&target_id),
    })?;
    let deletion = StoredGroupMessage {
        content_type: ContentType::DeleteMessage,
        reference_id: Some(target_id.clone()),
        decrypted_message_bytes: xmtp_content_types::encoded_content_to_bytes(delete),
        ..cached_text_row(
            alix_group.group_id,
            vec![0xAD, 0x02],
            alix.inbox_id(),
            "xmtp.org",
        )?
    };
    let deletions = alix
        .context
        .events()
        .subscribe(
            xmtp_events::EventFilter::default().with_internal(InternalEvent::is_message_deletion),
            Some(8),
        )
        .stream_message_deletions();
    futures::pin_mut!(deletions);

    crate::state_tx::state_write_with_events(
        alix.context.mls_storage(),
        alix.context.events(),
        |tx, buffer| {
            let storage = tx.storage();
            alix_group.store_external_application_message(&storage, &deletion, true, buffer)?;
            Ok::<_, crate::groups::mls_sync::GroupMessageProcessingError>(
                xmtp_db::TransactionOutcome::Continue(()),
            )
        },
    )?;

    let deleted = xmtp_common::time::timeout(std::time::Duration::from_secs(5), deletions.next())
        .await?
        .expect("a deletion item")?;
    assert_eq!(deleted.metadata.id, target_id);
    assert_eq!(deleted.fallback_text, None);
    let MessageBody::DeletedMessage { deleted_by } = &deleted.content else {
        panic!("the deletion item carried a body: {:?}", deleted.content);
    };
    assert_eq!(*deleted_by, DeletedBy::Admin(alix.inbox_id().to_string()));
}

// verifies: PROC-056
#[xmtp_common::test(unwrap_try = true)]
async fn unauthorized_pending_delete_does_not_block_super_admin_delete() {
    use crate::context::CoreContext;
    use xmtp_content_types::delete_message::DeleteMessageCodec;
    use xmtp_proto::xmtp::mls::message_contents::content_types::DeleteMessage;

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(caro, disable_workers);
    let alix_group = alix.create_group(None, None)?;
    alix_group
        .add_members(&[bo.inbox_id(), caro.inbox_id()])
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.remove(0);
    let caro_group = caro.wait_for_welcomes().await?.remove(0);
    bo_group.sync().await?;
    caro_group.sync().await?;

    let original = TextCodec::encode("message for moderation".into())?;
    let target_id = bo_group
        .send_message(
            &xmtp_content_types::encoded_content_to_bytes(original),
            SendMessageOpts::default(),
        )
        .await?;
    caro_group.sync().await?;
    assert!(
        alix.context.db().get_group_message(&target_id)?.is_none(),
        "the super admin has not synced the target"
    );

    // Caro is an ordinary member. Publish a real MLS application message with
    // the target ID, then deliver its decoded row before the target.
    let delete = DeleteMessageCodec::encode(DeleteMessage {
        message_id: hex::encode(&target_id),
    })?;
    let malicious_delete_id = caro_group
        .send_message(
            &xmtp_content_types::encoded_content_to_bytes(delete),
            SendMessageOpts::default(),
        )
        .await?;
    let malicious_delete = caro
        .context
        .db()
        .get_group_message(&malicious_delete_id)?
        .expect("the attacker's deletion message is stored");
    crate::state_tx::state_write_with_events(
        alix.context.mls_storage(),
        alix.context.events(),
        |tx, buffer| {
            let storage = tx.storage();
            alix_group.store_external_application_message(
                &storage,
                &malicious_delete,
                false,
                buffer,
            )?;
            Ok::<_, crate::groups::mls_sync::GroupMessageProcessingError>(
                xmtp_db::TransactionOutcome::Continue(()),
            )
        },
    )?;
    // The record is kept, but it marks nothing.
    assert!(
        alix.context
            .db()
            .get_message_deletion(&malicious_delete_id)?
            .is_some()
    );

    // The ordered sync now stores the target and processes the invalid delete
    // again. It marks nothing, and the local delete gate does not refuse.
    alix_group.sync().await?;
    let visible = alix_group
        .find_messages_v2(&MsgQueryArgs::default())?
        .into_iter()
        .find(|message| message.metadata.id == target_id)
        .expect("the target message is visible");
    assert!(matches!(visible.content, MessageBody::Text(_)));
    assert_eq!(mark_of(&alix.context.db(), &target_id), None);
    let deletion_id = alix_group.delete_message(target_id.clone())?;
    assert!(!deletion_id.is_empty());
    alix_group.publish_messages().await?;
    let visible = alix_group
        .find_messages_v2(&MsgQueryArgs::default())?
        .into_iter()
        .find(|message| message.metadata.id == target_id)
        .expect("the target has a deletion placeholder");
    assert!(matches!(
        visible.content,
        MessageBody::DeletedMessage { .. }
    ));
}

// verifies: PROC-056
#[xmtp_common::test(unwrap_try = true)]
async fn cross_group_pending_delete_does_not_block_super_admin_delete() {
    use crate::context::CoreContext;
    use xmtp_content_types::delete_message::DeleteMessageCodec;
    use xmtp_proto::xmtp::mls::message_contents::content_types::DeleteMessage;

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(caro, disable_workers);
    let target_group = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await?;
    let side_group = alix
        .create_group_with_members(&[caro.inbox_id()], None, None)
        .await?;
    bo.sync_all_welcomes_and_groups(None).await?;
    caro.sync_all_welcomes_and_groups(None).await?;
    let bo_target = bo.group(&target_group.group_id)?;
    let caro_target = caro.group(&target_group.group_id)?;
    let caro_side = caro.group(&side_group.group_id)?;

    let content = TextCodec::encode("message for moderation".into())?;
    let target_id = bo_target
        .send_message(
            &xmtp_content_types::encoded_content_to_bytes(content),
            SendMessageOpts::default(),
        )
        .await?;
    caro_target.sync().await?;
    assert!(alix.context.db().get_group_message(&target_id)?.is_none());

    let deletion = DeleteMessageCodec::encode(DeleteMessage {
        message_id: hex::encode(&target_id),
    })?;
    let side_deletion_id = caro_side
        .send_message(
            &xmtp_content_types::encoded_content_to_bytes(deletion),
            SendMessageOpts::default(),
        )
        .await?;
    // Each topic remains ordered. The target topic simply is not synced yet.
    side_group.sync().await?;
    assert!(alix.context.db().get_group_message(&target_id)?.is_none());
    let pending = alix
        .context
        .db()
        .get_message_deletion(&side_deletion_id)?
        .expect("cross-group record stored before target");
    assert_eq!(pending.group_id, side_group.group_id);
    assert_eq!(pending.deleted_message_id, target_id);

    target_group.sync().await?;
    let visible = target_group
        .find_messages_v2(&MsgQueryArgs::default())?
        .into_iter()
        .find(|message| message.metadata.id == target_id)
        .expect("target remains visible in its true group");
    assert!(matches!(visible.content, MessageBody::Text(_)));
    let deletion_id = target_group.delete_message(target_id.clone())?;
    assert!(!deletion_id.is_empty());
    target_group.publish_messages().await?;
    let visible = target_group
        .find_messages_v2(&MsgQueryArgs::default())?
        .into_iter()
        .find(|message| message.metadata.id == target_id)
        .expect("the target has a deletion placeholder");
    assert!(matches!(
        visible.content,
        MessageBody::DeletedMessage { .. }
    ));
}

/// Test that enrichment replaces deleted messages with placeholders
#[xmtp_common::test(unwrap_try = true)]
async fn test_enrichment_with_deleted_messages() {
    tester!(alix);
    tester!(bo);
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

    // Alix deletes the message
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

/// Test that replies to deleted messages show the deleted state
#[xmtp_common::test(unwrap_try = true)]
async fn test_reply_to_deleted_message() {
    use xmtp_content_types::reply::ReplyCodec;

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
    use crate::messages::enrichment::deleted_message_content_type;
    use xmtp_content_types::reply::ReplyCodec;

    tester!(alix);
    tester!(bo);
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
    use crate::messages::decoded_message::ContentDecodeFailureKind;
    use xmtp_content_types::reply::ReplyCodec;

    tester!(alix);
    tester!(bo);
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

/// Test that cross-group deletion attempts are rejected
#[xmtp_common::test(unwrap_try = true)]
async fn test_cannot_delete_message_from_different_group() {
    tester!(alix);
    tester!(bo);

    // Create two separate groups
    let group1 = alix.create_group(None, None)?;
    group1.add_members(&[bo.inbox_id()]).await?;

    let group2 = alix.create_group(None, None)?;
    group2.add_members(&[bo.inbox_id()]).await?;

    // Alix sends a message in group1
    let text_content = TextCodec::encode("Message in group 1".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let group1_message_id = group1
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    // Bo syncs both groups
    let bo_groups = bo.wait_for_welcomes().await?;
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
    assert_eq!(mark_of(&alix_conn, &group1_message_id), None);

    // Verify we can still delete it from the correct group
    group1.delete_message(group1_message_id.clone())?;
    group1.publish_messages().await?;
    assert_eq!(
        mark_of(&alix_conn, &group1_message_id).as_deref(),
        Some(alix.inbox_id())
    );
}

/// Test that we cannot delete a delete message
#[xmtp_common::test(unwrap_try = true)]
async fn test_cannot_delete_delete_message() {
    tester!(alix);
    tester!(bo);
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;

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
    assert_eq!(
        mark_of(&alix_conn, &original_message_id).as_deref(),
        Some(alix.inbox_id())
    );

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
    assert_eq!(mark_of(&alix_conn, &delete_message_id), None);
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
        .add_members(&[bo.inbox_id(), caro.inbox_id()])
        .await?;

    // Bo syncs and gets the group
    let bo_groups = bo.wait_for_welcomes().await?;
    let bo_group = &bo_groups[0];
    bo_group.sync().await?;

    // Caro syncs and gets the group
    let caro_groups = caro.wait_for_welcomes().await?;
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

    // Alix (super admin) deletes first; once Bo (super admin) has synced
    // its echo, Bo's request finds the message marked.
    let alix_deletion = alix_group.delete_message(message_id.clone());
    alix_group.publish_messages().await?;

    // Bo syncs to get Alix's deletion, then tries to delete
    bo_group.sync().await?;
    let bo_deletion = bo_group.delete_message(message_id.clone());

    assert!(alix_deletion.is_ok());
    assert!(matches!(
        bo_deletion,
        Err(GroupError::DeleteMessage(
            DeleteMessageError::MessageAlreadyDeleted
        ))
    ));

    alix_group.sync().await?;
    bo_group.sync().await?;
    caro_group.sync().await?;

    // Verify the message is deleted for everyone
    let alix_conn = alix.context.db();
    let bo_conn = bo.context.db();
    let caro_conn = caro.context.db();

    // The first deletion applied marks the message on every installation.
    let mark = mark_of(&alix_conn, &message_id);
    assert_eq!(mark.as_deref(), Some(alix.inbox_id()));
    assert_eq!(mark_of(&bo_conn, &message_id), mark);
    assert_eq!(mark_of(&caro_conn, &message_id), mark);

    // Verify enriched messages show the deleted state
    let caro_messages = caro_group.find_messages_v2(&MsgQueryArgs::default())?;
    let deleted_msg = caro_messages
        .iter()
        .find(|msg| msg.metadata.id == message_id);
    assert!(deleted_msg.is_some());

    let deleted_msg = deleted_msg.unwrap();
    let MessageBody::DeletedMessage { deleted_by } = &deleted_msg.content else {
        panic!("Expected DeletedMessage placeholder");
    };
    assert_eq!(*deleted_by, DeletedBy::Admin(alix.inbox_id().to_string()));
}

/// Test that deletion works correctly when both sender and admin try to delete
#[xmtp_common::test(unwrap_try = true)]
async fn test_sender_and_admin_both_delete() {
    tester!(alix);
    tester!(bo);

    // Alix creates a group with Bo
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;

    // Bo syncs and gets the group
    let bo_groups = bo.wait_for_welcomes().await?;
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
    let bo_messages = bo_group.find_messages_v2(&MsgQueryArgs::default())?;
    let deleted_msg = bo_messages.iter().find(|msg| msg.metadata.id == message_id);
    assert!(deleted_msg.is_some());

    let deleted_msg = deleted_msg.unwrap();
    let MessageBody::DeletedMessage { deleted_by } = &deleted_msg.content else {
        panic!("Expected DeletedMessage placeholder");
    };
    assert_eq!(*deleted_by, DeletedBy::Sender);
    assert_eq!(
        mark_of(&alix.context.db(), &message_id).as_deref(),
        Some(bo.inbox_id())
    );
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
    let bo_groups = bo.wait_for_welcomes().await?;
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
        || {},
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

/// A request queues the deletion and changes nothing; the echo marks the
/// target and emits one `message.deleted`. A second request before publish is
/// allowed, and both publish to one mark.
// verifies: PROC-037, PROC-056, EVENT-001, EVENT-007
#[xmtp_common::test(unwrap_try = true)]
async fn request_queues_and_applies_at_echo() {
    use crate::context::CoreContext;

    tester!(alix);
    tester!(bo);
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;
    let bo_group = bo.sync_welcomes().await?.remove(0);
    let text = TextCodec::encode("delete me".into())?;
    let message_id = alix_group
        .send_message(
            &xmtp_content_types::encoded_content_to_bytes(text),
            SendMessageOpts::default(),
        )
        .await?;
    let events = alix.context.events().subscribe(
        xmtp_events::EventFilter::new([xmtp_events::EventKind::MessageDeleted]),
        Some(10),
    );

    let first = alix_group.delete_message(message_id.clone())?;
    assert_ne!(first, message_id);
    assert_eq!(mark_of(&alix.context.db(), &message_id), None);
    assert!(events.drain().is_empty(), "the request emits nothing");
    let readable = alix_group
        .find_messages_v2(&MsgQueryArgs::default())?
        .into_iter()
        .find(|message| message.metadata.id == message_id)
        .unwrap();
    assert!(matches!(readable.content, MessageBody::Text(_)));
    // implements: PROC-056
    let second = alix_group.delete_message(message_id.clone())?;
    assert_ne!(second, first);

    alix_group.publish_messages().await?;
    assert_eq!(
        mark_of(&alix.context.db(), &message_id).as_deref(),
        Some(alix.inbox_id())
    );
    assert!(matches!(
        events.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(xmtp_events::ClientEvent::MessageDeleted(deleted)), ..
        }] if deleted.message_id == message_id
            && deleted.cause == xmtp_events::DeletionCause::DeletedLocally
    ));
    for id in [&first, &second] {
        assert_eq!(
            alix.context
                .db()
                .get_group_message(id)?
                .unwrap()
                .delivery_status,
            DeliveryStatus::Published
        );
    }
    bo_group.sync().await?;
    assert_eq!(
        mark_of(&bo.context.db(), &message_id).as_deref(),
        Some(alix.inbox_id())
    );
}
