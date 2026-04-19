use crate::groups::GroupError;
use crate::groups::error::EditMessageError;
use crate::groups::send_message_opts::SendMessageOpts;
use crate::tester;
use xmtp_content_types::{ContentCodec, text::TextCodec};
use xmtp_db::group_message::{ContentType, GroupMessageKind, MsgQueryArgs};
use xmtp_db::message_edit::QueryMessageEdit;

#[xmtp_common::test(unwrap_try = true)]
async fn test_edit_message_by_sender() {
    tester!(alix);
    tester!(bo);
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;

    let original = TextCodec::encode("Hello original".to_string())?;
    let msg_bytes = xmtp_content_types::encoded_content_to_bytes(original);
    let message_id = alix_group
        .send_message(&msg_bytes, SendMessageOpts::default())
        .await?;

    let edited = TextCodec::encode("Hello edited".to_string())?;
    let edit_id = alix_group.edit_message(message_id.clone(), edited)?;

    assert!(!edit_id.is_empty());

    let conn = alix.context.db();
    assert!(conn.is_message_edited(&message_id)?);

    let edit = conn.get_latest_edit_by_message_id(&message_id)?.unwrap();
    assert_eq!(edit.edited_by_inbox_id, alix.inbox_id());
    assert_eq!(edit.edited_message_id, message_id);
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_edit_message_authorization_failure() {
    tester!(alix);
    tester!(bo);
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = &bo_groups[0];

    let text = TextCodec::encode("Alix's message".to_string())?;
    let msg_bytes = xmtp_content_types::encoded_content_to_bytes(text);
    let message_id = alix_group
        .send_message(&msg_bytes, SendMessageOpts::default())
        .await?;

    bo_group.sync().await?;

    let edited = TextCodec::encode("Bo tries to edit".to_string())?;
    let result = bo_group.edit_message(message_id, edited);

    assert!(matches!(
        result,
        Err(GroupError::EditMessage(EditMessageError::NotAuthorized))
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_edit_nonexistent_message() {
    tester!(alix);
    let alix_group = alix.create_group(None, None)?;

    let edited = TextCodec::encode("nope".to_string())?;
    let result = alix_group.edit_message(vec![1, 2, 3, 4, 5], edited);

    assert!(matches!(
        result,
        Err(GroupError::EditMessage(EditMessageError::MessageNotFound(
            _
        )))
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_admin_cannot_edit_others_message() {
    // Alix is group creator / super admin; Bo is a regular member.
    tester!(alix);
    tester!(bo);
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = &bo_groups[0];

    // Bo sends a message.
    let text = TextCodec::encode("Bo's message".to_string())?;
    let msg_bytes = xmtp_content_types::encoded_content_to_bytes(text);
    let message_id = bo_group
        .send_message(&msg_bytes, SendMessageOpts::default())
        .await?;

    alix_group.sync().await?;

    // Admin Alix attempts to edit Bo's message — must fail per XIP-77.
    let edited = TextCodec::encode("Alix tries admin edit".to_string())?;
    let result = alix_group.edit_message(message_id, edited);

    assert!(matches!(
        result,
        Err(GroupError::EditMessage(EditMessageError::NotAuthorized))
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_cannot_edit_transcript_messages() {
    tester!(alix);
    tester!(bo);
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;

    let messages = alix_group.find_messages(&MsgQueryArgs {
        kind: Some(GroupMessageKind::MembershipChange),
        ..Default::default()
    })?;
    assert!(!messages.is_empty());

    let edited = TextCodec::encode("nope".to_string())?;
    let result = alix_group.edit_message(messages[0].id.clone(), edited);

    assert!(matches!(
        result,
        Err(GroupError::EditMessage(
            EditMessageError::NonEditableMessage
        ))
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_cannot_edit_edit_message() {
    tester!(alix);
    let alix_group = alix.create_group(None, None)?;

    let text = TextCodec::encode("original".to_string())?;
    let msg_bytes = xmtp_content_types::encoded_content_to_bytes(text);
    let message_id = alix_group
        .send_message(&msg_bytes, SendMessageOpts::default())
        .await?;

    let edited = TextCodec::encode("v2".to_string())?;
    let edit_message_id = alix_group.edit_message(message_id, edited)?;

    let edited_again = TextCodec::encode("v3".to_string())?;
    let result = alix_group.edit_message(edit_message_id, edited_again);

    assert!(matches!(
        result,
        Err(GroupError::EditMessage(
            EditMessageError::NonEditableMessage
        ))
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_cannot_edit_deleted_message() {
    tester!(alix);
    let alix_group = alix.create_group(None, None)?;

    let text = TextCodec::encode("soon deleted".to_string())?;
    let msg_bytes = xmtp_content_types::encoded_content_to_bytes(text);
    let message_id = alix_group
        .send_message(&msg_bytes, SendMessageOpts::default())
        .await?;

    alix_group.delete_message(message_id.clone())?;
    alix_group.publish_messages().await?;

    let edited = TextCodec::encode("too late".to_string())?;
    let result = alix_group.edit_message(message_id, edited);

    assert!(matches!(
        result,
        Err(GroupError::EditMessage(EditMessageError::MessageDeleted))
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_cannot_edit_message_from_different_group() {
    tester!(alix);
    tester!(bo);
    let group1 = alix.create_group(None, None)?;
    let group2 = alix.create_group(None, None)?;
    group1.add_members(&[bo.inbox_id()]).await?;
    group2.add_members(&[bo.inbox_id()]).await?;

    let text = TextCodec::encode("in group1".to_string())?;
    let msg_bytes = xmtp_content_types::encoded_content_to_bytes(text);
    let message_id = group1
        .send_message(&msg_bytes, SendMessageOpts::default())
        .await?;

    let edited = TextCodec::encode("from group2".to_string())?;
    let result = group2.edit_message(message_id, edited);

    assert!(matches!(
        result,
        Err(GroupError::EditMessage(EditMessageError::NotAuthorized))
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_enrichment_shows_edited_content() {
    tester!(alix);
    let alix_group = alix.create_group(None, None)?;

    let text = TextCodec::encode("before edit".to_string())?;
    let msg_bytes = xmtp_content_types::encoded_content_to_bytes(text);
    let message_id = alix_group
        .send_message(&msg_bytes, SendMessageOpts::default())
        .await?;

    let edited = TextCodec::encode("after edit".to_string())?;
    alix_group.edit_message(message_id.clone(), edited)?;
    alix_group.publish_messages().await?;
    alix_group.sync().await?;

    let enriched = alix_group.find_enriched_messages(&MsgQueryArgs::default())?;
    let msg = enriched
        .iter()
        .find(|m| m.metadata.id == message_id)
        .unwrap();

    // Content should be the edited version
    match &msg.content {
        crate::messages::decoded_message::MessageBody::Text(t) => {
            assert_eq!(t.content, "after edit");
        }
        other => panic!("Expected Text body with edited content, got {:?}", other),
    }

    // edited field should be set so consumers can show "(edited)"
    assert_eq!(
        msg.edited,
        Some(crate::messages::decoded_message::EditedBy::Sender)
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_multiple_edits_latest_wins() {
    tester!(alix);
    let alix_group = alix.create_group(None, None)?;

    let text = TextCodec::encode("v1".to_string())?;
    let msg_bytes = xmtp_content_types::encoded_content_to_bytes(text);
    let message_id = alix_group
        .send_message(&msg_bytes, SendMessageOpts::default())
        .await?;

    let v2 = TextCodec::encode("v2".to_string())?;
    alix_group.edit_message(message_id.clone(), v2)?;

    let v3 = TextCodec::encode("v3".to_string())?;
    alix_group.edit_message(message_id.clone(), v3)?;

    let conn = alix.context.db();
    let latest = conn.get_latest_edit_by_message_id(&message_id)?.unwrap();
    let content = xmtp_content_types::bytes_to_encoded_content(latest.edited_content_bytes);
    let decoded_text = TextCodec::decode(content)?;
    assert_eq!(decoded_text, "v3");
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_enrichment_preserves_reactions_after_edit() {
    use xmtp_content_types::reaction::ReactionCodec;
    use xmtp_proto::xmtp::mls::message_contents::content_types::ReactionV2;

    tester!(alix);
    tester!(bo);
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = &bo_groups[0];

    let text = TextCodec::encode("react to me".to_string())?;
    let msg_bytes = xmtp_content_types::encoded_content_to_bytes(text);
    let message_id = alix_group
        .send_message(&msg_bytes, SendMessageOpts::default())
        .await?;
    alix_group.publish_messages().await?;
    bo_group.sync().await?;

    let reaction = ReactionV2 {
        reference: hex::encode(&message_id),
        reference_inbox_id: bo.inbox_id().to_string(),
        action: 1, // ReactionAction::Added
        content: "👍".to_string(),
        schema: 1, // ReactionSchema::Unicode
    };
    let reaction_bytes =
        xmtp_content_types::encoded_content_to_bytes(ReactionCodec::encode(reaction)?);
    bo_group
        .send_message(&reaction_bytes, SendMessageOpts::default())
        .await?;
    bo_group.publish_messages().await?;
    alix_group.sync().await?;

    let edited = TextCodec::encode("edited with reaction".to_string())?;
    alix_group.edit_message(message_id.clone(), edited)?;
    alix_group.publish_messages().await?;
    alix_group.sync().await?;

    let enriched = alix_group.find_enriched_messages(&MsgQueryArgs::default())?;
    let msg = enriched
        .iter()
        .find(|m| m.metadata.id == message_id)
        .unwrap();

    assert!(
        !msg.reactions.is_empty(),
        "Reactions must be preserved after edit"
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_enrichment_edit_then_delete() {
    tester!(alix);
    let alix_group = alix.create_group(None, None)?;

    let text = TextCodec::encode("original".to_string())?;
    let msg_bytes = xmtp_content_types::encoded_content_to_bytes(text);
    let message_id = alix_group
        .send_message(&msg_bytes, SendMessageOpts::default())
        .await?;

    let edited = TextCodec::encode("edited".to_string())?;
    alix_group.edit_message(message_id.clone(), edited)?;

    alix_group.delete_message(message_id.clone())?;
    alix_group.publish_messages().await?;
    alix_group.sync().await?;

    let enriched = alix_group.find_enriched_messages(&MsgQueryArgs::default())?;
    let msg = enriched
        .iter()
        .find(|m| m.metadata.id == message_id)
        .unwrap();

    assert!(matches!(
        msg.content,
        crate::messages::decoded_message::MessageBody::DeletedMessage { .. }
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_out_of_order_edit() {
    tester!(alix);
    tester!(bo);
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = &bo_groups[0];

    let text = TextCodec::encode("original".to_string())?;
    let msg_bytes = xmtp_content_types::encoded_content_to_bytes(text);
    let message_id = alix_group
        .send_message(&msg_bytes, SendMessageOpts::default())
        .await?;

    let edited = TextCodec::encode("edited".to_string())?;
    alix_group.edit_message(message_id.clone(), edited)?;
    alix_group.publish_messages().await?;

    bo_group.sync().await?;

    let enriched = bo_group.find_enriched_messages(&MsgQueryArgs::default())?;
    let msg = enriched
        .iter()
        .find(|m| m.metadata.id == message_id)
        .unwrap();

    match &msg.content {
        crate::messages::decoded_message::MessageBody::Text(t) => {
            assert_eq!(t.content, "edited");
        }
        other => panic!("Expected edited Text body, got {:?}", other),
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_true_out_of_order_edit() {
    use xmtp_db::Store;
    use xmtp_db::group_message::{DeliveryStatus, StoredGroupMessage};
    use xmtp_db::message_edit::StoredMessageEdit;

    tester!(alix);
    let alix_group = alix.create_group(None, None)?;
    let conn = alix.context.db();

    let original_msg_id = vec![10, 20, 30];
    let edit_msg_id = vec![40, 50, 60];

    // Store the EditMessage envelope in group_messages first (FK target for the
    // edit record), even though the target message hasn't arrived yet.
    let edit_gm = StoredGroupMessage {
        id: edit_msg_id.clone(),
        group_id: alix_group.group_id.clone(),
        decrypted_message_bytes: vec![],
        sent_at_ns: xmtp_common::time::now_ns(),
        kind: GroupMessageKind::Application,
        sender_installation_id: vec![],
        sender_inbox_id: alix.inbox_id().to_string(),
        delivery_status: DeliveryStatus::Published,
        content_type: ContentType::EditMessage,
        version_major: 1,
        version_minor: 0,
        authority_id: "xmtp.org".to_string(),
        reference_id: Some(original_msg_id.clone()),
        originator_id: 0,
        sequence_id: 100,
        inserted_at_ns: 0,
        expire_at_ns: None,
        should_push: false,
    };
    edit_gm.store(&conn)?;

    // Store the edit record before the original message exists.
    let edited_text = TextCodec::encode("edited content".to_string())?;
    let edit_record = StoredMessageEdit {
        id: edit_msg_id,
        group_id: alix_group.group_id.clone(),
        edited_message_id: original_msg_id.clone(),
        edited_by_inbox_id: alix.inbox_id().to_string(),
        edited_content_bytes: xmtp_content_types::encoded_content_to_bytes(edited_text),
        edited_at_ns: xmtp_common::time::now_ns(),
    };
    edit_record.store(&conn)?;

    // Now the original message arrives.
    let original_text = TextCodec::encode("original content".to_string())?;
    let original_gm = StoredGroupMessage {
        id: original_msg_id.clone(),
        group_id: alix_group.group_id.clone(),
        decrypted_message_bytes: xmtp_content_types::encoded_content_to_bytes(original_text),
        sent_at_ns: xmtp_common::time::now_ns() - 1000,
        kind: GroupMessageKind::Application,
        sender_installation_id: vec![],
        sender_inbox_id: alix.inbox_id().to_string(),
        delivery_status: DeliveryStatus::Published,
        content_type: ContentType::Text,
        version_major: 1,
        version_minor: 0,
        authority_id: "xmtp.org".to_string(),
        reference_id: None,
        originator_id: 0,
        sequence_id: 99,
        inserted_at_ns: 0,
        expire_at_ns: None,
        should_push: false,
    };
    original_gm.store(&conn)?;

    let enriched = alix_group.find_enriched_messages(&MsgQueryArgs::default())?;
    let msg = enriched
        .iter()
        .find(|m| m.metadata.id == original_msg_id)
        .unwrap();

    match &msg.content {
        crate::messages::decoded_message::MessageBody::Text(t) => {
            assert_eq!(t.content, "edited content");
        }
        other => panic!("Expected edited Text body, got {:?}", other),
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_out_of_order_unauthorized_edit_rejected() {
    use xmtp_db::Store;
    use xmtp_db::group_message::{DeliveryStatus, StoredGroupMessage};
    use xmtp_db::message_edit::StoredMessageEdit;

    tester!(alix);
    let alix_group = alix.create_group(None, None)?;

    let text = TextCodec::encode("Alix's message".to_string())?;
    let msg_bytes = xmtp_content_types::encoded_content_to_bytes(text);
    let message_id = alix_group
        .send_message(&msg_bytes, SendMessageOpts::default())
        .await?;

    let conn = alix.context.db();

    // Simulate a malicious actor inserting a fraudulent edit from "fake_inbox".
    let fake_edit_id = vec![99, 99, 99];
    let fake_msg = StoredGroupMessage {
        id: fake_edit_id.clone(),
        group_id: alix_group.group_id.clone(),
        decrypted_message_bytes: vec![],
        sent_at_ns: xmtp_common::time::now_ns(),
        kind: GroupMessageKind::Application,
        sender_installation_id: vec![],
        sender_inbox_id: "fake_inbox".to_string(),
        delivery_status: DeliveryStatus::Published,
        content_type: ContentType::EditMessage,
        version_major: 1,
        version_minor: 0,
        authority_id: "xmtp.org".to_string(),
        reference_id: Some(message_id.clone()),
        originator_id: 0,
        sequence_id: 999,
        inserted_at_ns: 0,
        expire_at_ns: None,
        should_push: false,
    };
    fake_msg.store(&conn)?;

    let fake_content = TextCodec::encode("hacked".to_string())?;
    let fake_edit = StoredMessageEdit {
        id: fake_edit_id,
        group_id: alix_group.group_id.clone(),
        edited_message_id: message_id.clone(),
        edited_by_inbox_id: "fake_inbox".to_string(),
        edited_content_bytes: xmtp_content_types::encoded_content_to_bytes(fake_content),
        edited_at_ns: xmtp_common::time::now_ns(),
    };
    fake_edit.store(&conn)?;

    // Enrichment must reject the unauthorized edit and show the original content.
    let enriched = alix_group.find_enriched_messages(&MsgQueryArgs::default())?;
    let msg = enriched
        .iter()
        .find(|m| m.metadata.id == message_id)
        .unwrap();

    match &msg.content {
        crate::messages::decoded_message::MessageBody::Text(t) => {
            assert_eq!(
                t.content, "Alix's message",
                "Unauthorized edit must be rejected"
            );
        }
        other => panic!("Expected original Text body, got {:?}", other),
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_edit_database_queries() {
    tester!(alix);
    let alix_group = alix.create_group(None, None)?;

    let mk = |s: &str| {
        xmtp_content_types::encoded_content_to_bytes(TextCodec::encode(s.to_string()).unwrap())
    };

    let msg1 = alix_group
        .send_message(&mk("one"), SendMessageOpts::default())
        .await?;
    let msg2 = alix_group
        .send_message(&mk("two"), SendMessageOpts::default())
        .await?;
    let msg3 = alix_group
        .send_message(&mk("three"), SendMessageOpts::default())
        .await?;

    let edit1_id =
        alix_group.edit_message(msg1.clone(), TextCodec::encode("one edited".to_string())?)?;
    alix_group.edit_message(msg3.clone(), TextCodec::encode("three edited".to_string())?)?;

    let conn = alix.context.db();

    // Lookup by edit's own ID
    let edit = conn.get_message_edit(&edit1_id)?.unwrap();
    assert_eq!(edit.edited_message_id, msg1);

    // Batch query for latest per target
    let edits =
        conn.get_latest_edits_for_messages(vec![msg1.clone(), msg2.clone(), msg3.clone()])?;
    assert_eq!(edits.len(), 2);

    // Boolean check
    assert!(conn.is_message_edited(&msg1)?);
    assert!(!conn.is_message_edited(&msg2)?);
    assert!(conn.is_message_edited(&msg3)?);

    // Group-scoped query
    let group_edits = conn.get_group_edits(&alix_group.group_id)?;
    assert_eq!(group_edits.len(), 2);
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_edit_message_filtered_from_lists() {
    tester!(alix);
    let alix_group = alix.create_group(None, None)?;

    let text = TextCodec::encode("original".to_string())?;
    let msg_bytes = xmtp_content_types::encoded_content_to_bytes(text);
    let message_id = alix_group
        .send_message(&msg_bytes, SendMessageOpts::default())
        .await?;

    alix_group.edit_message(message_id, TextCodec::encode("edited".to_string())?)?;

    let messages = alix_group.find_messages(&MsgQueryArgs {
        exclude_content_types: Some(vec![ContentType::EditMessage]),
        ..Default::default()
    })?;

    assert!(
        !messages
            .iter()
            .any(|m| m.content_type == ContentType::EditMessage),
        "EditMessage should be filtered out"
    );
}

/// Task 20: a recipient watching `stream_message_edits` receives the original
/// (now-edited) message when another member edits + publishes.
#[xmtp_common::test(unwrap_try = true)]
async fn test_stream_message_edits_from_other_client() {
    use crate::utils::FullXmtpClient;
    use parking_lot::Mutex;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::Notify;
    use xmtp_common::StreamHandle;

    tester!(alix);
    tester!(bo);
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = &bo_groups[0];

    let text = TextCodec::encode("original".to_string())?;
    let msg_bytes = xmtp_content_types::encoded_content_to_bytes(text);
    let message_id = alix_group
        .send_message(&msg_bytes, SendMessageOpts::default())
        .await?;
    alix_group.publish_messages().await?;
    bo_group.sync().await?;

    let received: Arc<Mutex<Option<crate::messages::decoded_message::DecodedMessage>>> =
        Arc::new(Mutex::new(None));
    let notify = Arc::new(Notify::new());
    let received_clone = received.clone();
    let notify_clone = notify.clone();

    let mut handle = FullXmtpClient::stream_message_edits_with_callback(
        Arc::new(bo.client.clone()),
        move |msg| {
            if let Ok(message) = msg {
                *received_clone.lock() = Some(message);
                notify_clone.notify_one();
            }
        },
    );
    handle.wait_for_ready().await;

    let edited = TextCodec::encode("edited".to_string())?;
    alix_group.edit_message(message_id.clone(), edited)?;
    alix_group.publish_messages().await?;
    bo_group.sync().await?;

    xmtp_common::time::timeout(Duration::from_secs(5), notify.notified())
        .await
        .expect("Edit stream should fire within 5s");

    let received = received.lock();
    assert!(received.is_some(), "Edit event should be received");
    let received_msg = received.as_ref().unwrap();
    assert_eq!(received_msg.metadata.id, message_id);
    assert_eq!(
        received_msg.edited,
        Some(crate::messages::decoded_message::EditedBy::Sender),
        "Stream event must expose `edited` metadata so consumers can display \"(edited)\""
    );
    match &received_msg.content {
        crate::messages::decoded_message::MessageBody::Text(t) => assert_eq!(
            t.content, "edited",
            "Stream event body must carry the post-edit text, not the original"
        ),
        other => panic!("Expected Text body in stream event, got {:?}", other),
    }
}

/// Task 21: a sender watching `stream_message_edits` receives their own edit
/// after publish + sync completes the local round-trip.
#[xmtp_common::test(unwrap_try = true)]
async fn test_stream_message_edits_fires_for_self_after_publish() {
    use crate::utils::FullXmtpClient;
    use parking_lot::Mutex;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::Notify;
    use xmtp_common::StreamHandle;

    tester!(alix);
    let alix_group = alix.create_group(None, None)?;

    let text = TextCodec::encode("original".to_string())?;
    let msg_bytes = xmtp_content_types::encoded_content_to_bytes(text);
    let message_id = alix_group
        .send_message(&msg_bytes, SendMessageOpts::default())
        .await?;

    let received: Arc<Mutex<Option<crate::messages::decoded_message::DecodedMessage>>> =
        Arc::new(Mutex::new(None));
    let notify = Arc::new(Notify::new());
    let received_clone = received.clone();
    let notify_clone = notify.clone();

    let mut handle = FullXmtpClient::stream_message_edits_with_callback(
        Arc::new(alix.client.clone()),
        move |msg| {
            if let Ok(message) = msg {
                *received_clone.lock() = Some(message);
                notify_clone.notify_one();
            }
        },
    );
    handle.wait_for_ready().await;

    let edited = TextCodec::encode("self-edited".to_string())?;
    alix_group.edit_message(message_id.clone(), edited)?;
    alix_group.publish_messages().await?;
    alix_group.sync().await?;

    xmtp_common::time::timeout(Duration::from_secs(5), notify.notified())
        .await
        .expect("Self-edit stream should fire within 5s");

    let received_msg = received.lock().clone().unwrap();
    assert_eq!(received_msg.metadata.id, message_id);
    assert_eq!(
        received_msg.edited,
        Some(crate::messages::decoded_message::EditedBy::Sender),
        "Self-edit stream must expose `edited` metadata"
    );
    match &received_msg.content {
        crate::messages::decoded_message::MessageBody::Text(t) => assert_eq!(
            t.content, "self-edited",
            "Self-edit stream event body must carry the post-edit text"
        ),
        other => panic!("Expected Text body in stream event, got {:?}", other),
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_sync_rejects_unauthorized_edit_over_wire() {
    use xmtp_content_types::edit_message::EditMessageCodec;
    use xmtp_proto::xmtp::mls::message_contents::content_types::EditMessage as EditMessageProto;

    tester!(alix);
    tester!(bo);
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = &bo_groups[0];

    let text = TextCodec::encode("Alix's message".to_string())?;
    let msg_bytes = xmtp_content_types::encoded_content_to_bytes(text);
    let message_id = alix_group
        .send_message(&msg_bytes, SendMessageOpts::default())
        .await?;
    alix_group.publish_messages().await?;
    bo_group.sync().await?;

    // Bo bypasses edit_message()'s sender check by hand-rolling an EditMessage
    // proto and sending it through send_message directly. MLS still signs it
    // as Bo, so the recipient sees sender_inbox_id = bo but the payload claims
    // to edit Alix's message.
    let fraudulent = EditMessageProto {
        message_id: hex::encode(&message_id),
        edited_content: Some(TextCodec::encode("hacked".to_string())?),
    };
    let wire_bytes =
        xmtp_content_types::encoded_content_to_bytes(EditMessageCodec::encode(fraudulent)?);
    bo_group
        .send_message(&wire_bytes, SendMessageOpts::default())
        .await?;
    bo_group.publish_messages().await?;

    alix_group.sync().await?;

    // Storage-layer assertion: process_edit_message must have refused to persist.
    let conn = alix.context.db();
    assert!(
        !conn.is_message_edited(&message_id)?,
        "Unauthorized edit from a non-sender must not persist a StoredMessageEdit row"
    );

    // Read-path assertion: enrichment still shows the original content.
    let enriched = alix_group.find_enriched_messages(&MsgQueryArgs::default())?;
    let msg = enriched
        .iter()
        .find(|m| m.metadata.id == message_id)
        .unwrap();
    match &msg.content {
        crate::messages::decoded_message::MessageBody::Text(t) => {
            assert_eq!(t.content, "Alix's message");
        }
        other => panic!("Expected original Text body, got {:?}", other),
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_cannot_edit_across_content_types() {
    use xmtp_content_types::markdown::MarkdownCodec;

    tester!(alix);
    let alix_group = alix.create_group(None, None)?;

    let text = TextCodec::encode("hello".to_string())?;
    let msg_bytes = xmtp_content_types::encoded_content_to_bytes(text);
    let message_id = alix_group
        .send_message(&msg_bytes, SendMessageOpts::default())
        .await?;

    // Edit a Text message with Markdown content — must be rejected per XIP-77.
    let markdown = MarkdownCodec::encode("**bold**".to_string())?;
    let result = alix_group.edit_message(message_id, markdown);

    assert!(matches!(
        result,
        Err(GroupError::EditMessage(
            EditMessageError::ContentTypeMismatch
        ))
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_reply_edit_preserves_text_succeeds() {
    use xmtp_content_types::reply::{Reply, ReplyCodec};

    tester!(alix);
    let alix_group = alix.create_group(None, None)?;

    // Send a target message to reply to.
    let target_text = TextCodec::encode("original target".to_string())?;
    let target_bytes = xmtp_content_types::encoded_content_to_bytes(target_text);
    let target_id = alix_group
        .send_message(&target_bytes, SendMessageOpts::default())
        .await?;

    // Send a Reply to that target.
    let reply = Reply {
        reference: hex::encode(&target_id),
        reference_inbox_id: Some(alix.inbox_id().to_string()),
        content: TextCodec::encode("first reply".to_string())?,
    };
    let reply_bytes = xmtp_content_types::encoded_content_to_bytes(ReplyCodec::encode(reply)?);
    let reply_id = alix_group
        .send_message(&reply_bytes, SendMessageOpts::default())
        .await?;

    // Edit the Reply with new text but the same reference — must succeed.
    let edited_reply = Reply {
        reference: hex::encode(&target_id),
        reference_inbox_id: Some(alix.inbox_id().to_string()),
        content: TextCodec::encode("edited reply text".to_string())?,
    };
    let edit_id = alix_group.edit_message(reply_id.clone(), ReplyCodec::encode(edited_reply)?)?;
    assert!(!edit_id.is_empty());

    let conn = alix.context.db();
    assert!(conn.is_message_edited(&reply_id)?);
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_reply_edit_cannot_change_reference() {
    use xmtp_content_types::reply::{Reply, ReplyCodec};

    tester!(alix);
    let alix_group = alix.create_group(None, None)?;

    // Two target messages.
    let first_target = alix_group
        .send_message(
            &xmtp_content_types::encoded_content_to_bytes(TextCodec::encode("first".to_string())?),
            SendMessageOpts::default(),
        )
        .await?;
    let other_target = alix_group
        .send_message(
            &xmtp_content_types::encoded_content_to_bytes(TextCodec::encode("other".to_string())?),
            SendMessageOpts::default(),
        )
        .await?;

    // Reply to the first target.
    let reply = Reply {
        reference: hex::encode(&first_target),
        reference_inbox_id: Some(alix.inbox_id().to_string()),
        content: TextCodec::encode("my reply".to_string())?,
    };
    let reply_id = alix_group
        .send_message(
            &xmtp_content_types::encoded_content_to_bytes(ReplyCodec::encode(reply)?),
            SendMessageOpts::default(),
        )
        .await?;

    // Attempt to edit the Reply to point at a different target — must be rejected.
    let repointed = Reply {
        reference: hex::encode(&other_target),
        reference_inbox_id: Some(alix.inbox_id().to_string()),
        content: TextCodec::encode("my reply".to_string())?,
    };
    let result = alix_group.edit_message(reply_id, ReplyCodec::encode(repointed)?);

    assert!(matches!(
        result,
        Err(GroupError::EditMessage(
            EditMessageError::ReplyReferenceChanged
        ))
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_sync_rejects_cross_type_edit_over_wire() {
    use xmtp_content_types::edit_message::EditMessageCodec;
    use xmtp_content_types::markdown::MarkdownCodec;
    use xmtp_proto::xmtp::mls::message_contents::content_types::EditMessage as EditMessageProto;

    tester!(alix);
    let alix_group = alix.create_group(None, None)?;

    let text = TextCodec::encode("alix's text".to_string())?;
    let msg_bytes = xmtp_content_types::encoded_content_to_bytes(text);
    let message_id = alix_group
        .send_message(&msg_bytes, SendMessageOpts::default())
        .await?;

    // Hand-roll a fraudulent cross-type EditMessage: claim to edit a Text
    // message with Markdown content. Bypasses edit_message()'s API-level check.
    let cross_type = EditMessageProto {
        message_id: hex::encode(&message_id),
        edited_content: Some(MarkdownCodec::encode("**hacked**".to_string())?),
    };
    let wire_bytes =
        xmtp_content_types::encoded_content_to_bytes(EditMessageCodec::encode(cross_type)?);
    alix_group
        .send_message(&wire_bytes, SendMessageOpts::default())
        .await?;
    alix_group.publish_messages().await?;
    alix_group.sync().await?;

    // process_edit_message must refuse to persist.
    let conn = alix.context.db();
    assert!(
        !conn.is_message_edited(&message_id)?,
        "Cross-type edit must not persist a StoredMessageEdit row"
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_latest_edit_tie_breaks_by_smallest_id() {
    use xmtp_db::Store;
    use xmtp_db::group_message::{DeliveryStatus, StoredGroupMessage};
    use xmtp_db::message_edit::StoredMessageEdit;

    tester!(alix);
    let alix_group = alix.create_group(None, None)?;
    let conn = alix.context.db();

    let target_id = vec![77, 77, 77];
    let edit_a_id = vec![1, 1, 1];
    let edit_b_id = vec![2, 2, 2];

    // Target message.
    StoredGroupMessage {
        id: target_id.clone(),
        group_id: alix_group.group_id.clone(),
        decrypted_message_bytes: xmtp_content_types::encoded_content_to_bytes(TextCodec::encode(
            "target".to_string(),
        )?),
        sent_at_ns: 1_000,
        kind: GroupMessageKind::Application,
        sender_installation_id: vec![],
        sender_inbox_id: alix.inbox_id().to_string(),
        delivery_status: DeliveryStatus::Published,
        content_type: ContentType::Text,
        version_major: 1,
        version_minor: 0,
        authority_id: "xmtp.org".to_string(),
        reference_id: None,
        originator_id: 0,
        sequence_id: 1,
        inserted_at_ns: 0,
        expire_at_ns: None,
        should_push: false,
    }
    .store(&conn)?;

    // Two edit-carrier group_messages rows with the same sent time (FK target
    // for message_edits.id).
    for id in [&edit_a_id, &edit_b_id] {
        StoredGroupMessage {
            id: id.clone(),
            group_id: alix_group.group_id.clone(),
            decrypted_message_bytes: vec![],
            sent_at_ns: 2_000,
            kind: GroupMessageKind::Application,
            sender_installation_id: vec![],
            sender_inbox_id: alix.inbox_id().to_string(),
            delivery_status: DeliveryStatus::Published,
            content_type: ContentType::EditMessage,
            version_major: 1,
            version_minor: 0,
            authority_id: "xmtp.org".to_string(),
            reference_id: Some(target_id.clone()),
            originator_id: 0,
            sequence_id: 1,
            inserted_at_ns: 0,
            expire_at_ns: None,
            should_push: false,
        }
        .store(&conn)?;
    }

    // Two edit records with IDENTICAL edited_at_ns — tie-break must pick the
    // row with the smaller id (edit_a_id < edit_b_id lexicographically).
    let common_ts = 2_000i64;
    StoredMessageEdit {
        id: edit_b_id.clone(),
        group_id: alix_group.group_id.clone(),
        edited_message_id: target_id.clone(),
        edited_by_inbox_id: alix.inbox_id().to_string(),
        edited_content_bytes: xmtp_content_types::encoded_content_to_bytes(TextCodec::encode(
            "edit B".to_string(),
        )?),
        edited_at_ns: common_ts,
    }
    .store(&conn)?;
    StoredMessageEdit {
        id: edit_a_id.clone(),
        group_id: alix_group.group_id.clone(),
        edited_message_id: target_id.clone(),
        edited_by_inbox_id: alix.inbox_id().to_string(),
        edited_content_bytes: xmtp_content_types::encoded_content_to_bytes(TextCodec::encode(
            "edit A".to_string(),
        )?),
        edited_at_ns: common_ts,
    }
    .store(&conn)?;

    let latest = conn
        .get_latest_edit_by_message_id(&target_id)?
        .expect("expected an edit to resolve");
    assert_eq!(
        latest.id, edit_a_id,
        "tie-break must prefer the smallest id"
    );

    let batch = conn.get_latest_edits_for_messages(vec![target_id.clone()])?;
    assert_eq!(batch.len(), 1, "one target → one latest edit");
    assert_eq!(
        batch[0].id, edit_a_id,
        "batch query must match single-lookup tie-break"
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_concurrent_edits_from_two_installations() {
    // Alix has two installations (e.g., mobile + desktop). Both edit the same
    // message. Edits must converge: both installations see the latest edit
    // (by edited_at_ns DESC, id ASC) after sync. This exercises the tie-break
    // path live rather than via manual DB inserts.
    tester!(alix_mobile);
    let alix_desktop = alix_mobile.new_installation().await;

    let alix_group = alix_mobile.create_group(None, None)?;

    let text = TextCodec::encode("v0".to_string())?;
    let message_id = alix_group
        .send_message(
            &xmtp_content_types::encoded_content_to_bytes(text),
            SendMessageOpts::default(),
        )
        .await?;
    alix_group.publish_messages().await?;

    // Desktop syncs and sees the group + message.
    let desktop_groups = alix_desktop.sync_welcomes().await?;
    let desktop_group = &desktop_groups[0];
    desktop_group.sync().await?;

    // Both installations edit concurrently.
    let mobile_edit = TextCodec::encode("from mobile".to_string())?;
    alix_group.edit_message(message_id.clone(), mobile_edit)?;
    let desktop_edit = TextCodec::encode("from desktop".to_string())?;
    desktop_group.edit_message(message_id.clone(), desktop_edit)?;

    alix_group.publish_messages().await?;
    desktop_group.publish_messages().await?;

    // Each side syncs and should converge on the same latest edit.
    alix_group.sync().await?;
    desktop_group.sync().await?;

    let mobile_enriched = alix_group.find_enriched_messages(&MsgQueryArgs::default())?;
    let mobile_msg = mobile_enriched
        .iter()
        .find(|m| m.metadata.id == message_id)
        .expect("mobile sees original");

    let desktop_enriched = desktop_group.find_enriched_messages(&MsgQueryArgs::default())?;
    let desktop_msg = desktop_enriched
        .iter()
        .find(|m| m.metadata.id == message_id)
        .expect("desktop sees original");

    // Both sides must agree on the latest edited content.
    let mobile_text = match &mobile_msg.content {
        crate::messages::decoded_message::MessageBody::Text(t) => t.content.clone(),
        other => panic!("Expected Text body on mobile, got {:?}", other),
    };
    let desktop_text = match &desktop_msg.content {
        crate::messages::decoded_message::MessageBody::Text(t) => t.content.clone(),
        other => panic!("Expected Text body on desktop, got {:?}", other),
    };

    assert_eq!(
        mobile_text, desktop_text,
        "concurrent-edit convergence: both installations must display the same winner"
    );
    assert!(
        mobile_text == "from mobile" || mobile_text == "from desktop",
        "winner must be one of the two edits, got {:?}",
        mobile_text
    );

    // Both sides should expose the edited metadata so UIs can render "(edited)".
    assert_eq!(
        mobile_msg.edited,
        Some(crate::messages::decoded_message::EditedBy::Sender)
    );
    assert_eq!(
        desktop_msg.edited,
        Some(crate::messages::decoded_message::EditedBy::Sender)
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_sync_rejects_edit_of_deleted_message_over_wire() {
    use xmtp_content_types::edit_message::EditMessageCodec;
    use xmtp_proto::xmtp::mls::message_contents::content_types::EditMessage as EditMessageProto;

    tester!(alix);
    let alix_group = alix.create_group(None, None)?;

    let text = TextCodec::encode("alix says hi".to_string())?;
    let msg_bytes = xmtp_content_types::encoded_content_to_bytes(text);
    let message_id = alix_group
        .send_message(&msg_bytes, SendMessageOpts::default())
        .await?;
    alix_group.publish_messages().await?;

    // Delete locally before the edit arrives.
    alix_group.delete_message(message_id.clone())?;
    alix_group.publish_messages().await?;
    alix_group.sync().await?;

    // Hand-roll an EditMessage payload and send it as a raw message so it
    // bypasses `edit_message()`'s `is_message_deleted` guard.
    let fraudulent = EditMessageProto {
        message_id: hex::encode(&message_id),
        edited_content: Some(TextCodec::encode("reanimated".to_string())?),
    };
    let wire_bytes =
        xmtp_content_types::encoded_content_to_bytes(EditMessageCodec::encode(fraudulent)?);
    alix_group
        .send_message(&wire_bytes, SendMessageOpts::default())
        .await?;
    alix_group.publish_messages().await?;
    alix_group.sync().await?;

    // Storage-layer assertion: process_edit_message must have skipped the write.
    let conn = alix.context.db();
    assert!(
        !conn.is_message_edited(&message_id)?,
        "Edit targeting a deleted message must not persist a StoredMessageEdit row"
    );

    // Read-path assertion: the target still presents as deleted, not edited.
    let enriched = alix_group.find_enriched_messages(&MsgQueryArgs::default())?;
    let msg = enriched
        .iter()
        .find(|m| m.metadata.id == message_id)
        .unwrap();
    assert!(matches!(
        msg.content,
        crate::messages::decoded_message::MessageBody::DeletedMessage { .. }
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_enrichment_in_reply_to_reflects_target_edit() {
    use xmtp_content_types::reply::{Reply, ReplyCodec};

    tester!(alix);
    tester!(bo);
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = &bo_groups[0];

    // Alix sends the target.
    let target_text = TextCodec::encode("original target".to_string())?;
    let target_bytes = xmtp_content_types::encoded_content_to_bytes(target_text);
    let target_id = alix_group
        .send_message(&target_bytes, SendMessageOpts::default())
        .await?;
    alix_group.publish_messages().await?;
    bo_group.sync().await?;

    // Bo replies to the target.
    let reply = Reply {
        reference: hex::encode(&target_id),
        reference_inbox_id: Some(alix.inbox_id().to_string()),
        content: TextCodec::encode("nice point".to_string())?,
    };
    let reply_bytes = xmtp_content_types::encoded_content_to_bytes(ReplyCodec::encode(reply)?);
    let reply_id = bo_group
        .send_message(&reply_bytes, SendMessageOpts::default())
        .await?;
    bo_group.publish_messages().await?;
    alix_group.sync().await?;

    // Alix edits the target.
    let edited = TextCodec::encode("edited target".to_string())?;
    alix_group.edit_message(target_id.clone(), edited)?;
    alix_group.publish_messages().await?;
    alix_group.sync().await?;
    bo_group.sync().await?;

    // From either side, Bo's reply's in_reply_to must show the edited text.
    for (label, group) in [("alix", &alix_group), ("bo", bo_group)] {
        let enriched = group.find_enriched_messages(&MsgQueryArgs::default())?;
        let reply_msg = enriched
            .iter()
            .find(|m| m.metadata.id == reply_id)
            .unwrap_or_else(|| panic!("{label}: expected reply in enriched list"));
        let reply_body = match &reply_msg.content {
            crate::messages::decoded_message::MessageBody::Reply(r) => r,
            other => panic!("{label}: expected Reply body, got {:?}", other),
        };
        let in_reply_to = reply_body
            .in_reply_to
            .as_ref()
            .unwrap_or_else(|| panic!("{label}: in_reply_to must be populated"));
        match &in_reply_to.content {
            crate::messages::decoded_message::MessageBody::Text(t) => {
                assert_eq!(
                    t.content, "edited target",
                    "{label}: in_reply_to should mirror the main-list enrichment"
                );
            }
            other => panic!(
                "{label}: expected Text body in in_reply_to, got {:?}",
                other
            ),
        }
        assert_eq!(
            in_reply_to.edited,
            Some(crate::messages::decoded_message::EditedBy::Sender),
            "{label}: in_reply_to must carry the `edited` marker"
        );
    }
}
