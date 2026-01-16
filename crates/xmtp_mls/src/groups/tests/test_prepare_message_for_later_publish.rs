use crate::groups::DeliveryStatus;
use crate::tester;
use xmtp_db::group_message::{GroupMessageKind, MsgQueryArgs};

/// Test that `prepare_message_for_later_publish` stores messages locally with Unpublished status
/// and does NOT create an intent to publish.
#[xmtp_common::test(unwrap_try = true)]
async fn test_prepare_message_stores_unpublished() {
    tester!(alix);
    let group = alix.create_group(None, None)?;

    let message_id = group.prepare_message_for_later_publish(b"test message", true)?;

    let messages = group.find_messages(&MsgQueryArgs {
        kind: Some(GroupMessageKind::Application),
        ..Default::default()
    })?;

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].id, message_id);
    assert_eq!(messages[0].delivery_status, DeliveryStatus::Unpublished);
}

/// Test that calling `publish_messages` does NOT publish messages created with
/// `prepare_message_for_later_publish`. Only `publish_stored_message` should publish them.
#[xmtp_common::test(unwrap_try = true)]
async fn test_publish_messages_does_not_publish_prepared_messages() {
    tester!(alix);
    let group = alix.create_group(None, None)?;

    let message_id = group.prepare_message_for_later_publish(b"prepared message", true)?;

    // publish_messages should be a no-op for prepared messages (no intent was created)
    group.publish_messages().await?;

    let messages = group.find_messages(&MsgQueryArgs {
        kind: Some(GroupMessageKind::Application),
        ..Default::default()
    })?;

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].id, message_id);
    assert_eq!(messages[0].delivery_status, DeliveryStatus::Unpublished);
}

/// Test that `publish_stored_message` correctly publishes a prepared message.
#[xmtp_common::test(unwrap_try = true)]
async fn test_publish_stored_message_publishes_prepared_message() {
    tester!(alix);
    let group = alix.create_group(None, None)?;

    let message_id = group.prepare_message_for_later_publish(b"test message", true)?;
    assert_eq!(
        group.find_messages(&MsgQueryArgs::default())?[0].delivery_status,
        DeliveryStatus::Unpublished
    );

    group.publish_stored_message(&message_id).await?;

    // After sync, the message should be published
    group.sync().await?;
    let messages = group.find_messages(&MsgQueryArgs {
        kind: Some(GroupMessageKind::Application),
        ..Default::default()
    })?;

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].delivery_status, DeliveryStatus::Published);
}

/// Test that `publish_stored_message` is idempotent - calling it multiple times
/// on an already published message should be a no-op.
#[xmtp_common::test(unwrap_try = true)]
async fn test_publish_stored_message_is_idempotent() {
    tester!(alix);
    let group = alix.create_group(None, None)?;

    let message_id = group.prepare_message_for_later_publish(b"idempotent test", true)?;

    // Publish three times - should not error or create duplicates
    group.publish_stored_message(&message_id).await?;
    group.publish_stored_message(&message_id).await?;
    group.publish_stored_message(&message_id).await?;

    group.sync().await?;
    let messages = group.find_messages(&MsgQueryArgs {
        kind: Some(GroupMessageKind::Application),
        ..Default::default()
    })?;

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].delivery_status, DeliveryStatus::Published);
}

/// Test that multiple messages can be prepared and then selectively published.
#[xmtp_common::test(unwrap_try = true)]
async fn test_selective_publish_of_prepared_messages() {
    tester!(alix);
    let group = alix.create_group(None, None)?;

    let id_1 = group.prepare_message_for_later_publish(b"message one", true)?;
    let id_2 = group.prepare_message_for_later_publish(b"message two", true)?;
    let _id_3 = group.prepare_message_for_later_publish(b"message three", true)?;

    // Only publish messages 1 and 2
    group.publish_stored_message(&id_1).await?;
    group.publish_stored_message(&id_2).await?;

    group.sync().await?;
    let messages = group.find_messages(&MsgQueryArgs {
        kind: Some(GroupMessageKind::Application),
        ..Default::default()
    })?;

    assert_eq!(messages.len(), 3);

    let published: Vec<_> = messages
        .iter()
        .filter(|m| m.delivery_status == DeliveryStatus::Published)
        .collect();
    let unpublished: Vec<_> = messages
        .iter()
        .filter(|m| m.delivery_status == DeliveryStatus::Unpublished)
        .collect();

    assert_eq!(published.len(), 2);
    assert_eq!(unpublished.len(), 1);
    assert_eq!(unpublished[0].decrypted_message_bytes, b"message three");
}
