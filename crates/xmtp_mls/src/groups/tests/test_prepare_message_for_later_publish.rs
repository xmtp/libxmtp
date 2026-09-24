use crate::groups::DeliveryStatus;
use crate::groups::send_message_opts::SendMessageOpts;
use crate::tester;
use crate::utils::id::calculate_message_id;
use xmtp_db::group_message::{GroupMessageKind, MsgQueryArgs};

/// Test that `prepare_message_for_later_publish` stores messages locally with Unpublished status
/// and does NOT create an intent to publish.
#[xmtp_common::test(unwrap_try = true)]
async fn test_prepare_message_stores_unpublished() {
    tester!(alix);
    let group = alix.create_group(None, None)?;

    let message_id = group.prepare_message_for_later_publish(b"test message", true, None)?;

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

    let message_id = group.prepare_message_for_later_publish(b"prepared message", true, None)?;

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

    let message_id = group.prepare_message_for_later_publish(b"test message", true, None)?;
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

    let message_id = group.prepare_message_for_later_publish(b"idempotent test", true, None)?;

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

    let id_1 = group.prepare_message_for_later_publish(b"message one", true, None)?;
    let id_2 = group.prepare_message_for_later_publish(b"message two", true, None)?;
    let _id_3 = group.prepare_message_for_later_publish(b"message three", true, None)?;

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

/// A caller-supplied idempotency key fully determines the message id (instead of
/// a timestamp) and is persisted on the stored message.
#[xmtp_common::test(unwrap_try = true)]
async fn test_explicit_idempotency_key_produces_deterministic_id() {
    tester!(alix);
    let group = alix.create_group(None, None)?;

    let key = "stable-key-123".to_string();
    let content = b"hello idempotent";
    let id = group.prepare_message_for_later_publish(content, true, Some(key.clone()))?;

    // The id is derived from the supplied key, not from a timestamp.
    assert_eq!(id, calculate_message_id(group.group_id, content, &key));

    // The resolved key is persisted alongside the message.
    let stored = group.find_messages(&MsgQueryArgs::default())?;
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].id, id);
    assert_eq!(stored[0].idempotency_key, key);
}

/// Without an explicit key, two identical-content sends still get distinct ids
/// (the timestamp default), preserving the historical always-unique behavior.
#[xmtp_common::test(unwrap_try = true)]
async fn test_default_idempotency_key_is_unique_per_send() {
    tester!(alix);
    let group = alix.create_group(None, None)?;

    let id_1 = group.prepare_message_for_later_publish(b"same content", true, None)?;
    let id_2 = group.prepare_message_for_later_publish(b"same content", true, None)?;

    assert_ne!(
        id_1, id_2,
        "default (timestamp) keys must yield unique message ids"
    );
}

/// Re-preparing the same content with the same explicit key is idempotent: it
/// returns the existing message id instead of erroring on the PK conflict, and
/// does not create a duplicate row. This is the sender side of
/// at-least-once-with-dedup (e.g. a crash-recovery retry).
#[xmtp_common::test(unwrap_try = true)]
async fn test_duplicate_idempotency_key_is_idempotent() {
    tester!(alix);
    let group = alix.create_group(None, None)?;

    let key = "retry-key".to_string();
    let id_1 = group.prepare_message_for_later_publish(b"retry me", true, Some(key.clone()))?;
    // Second identical prepare must not error and must return the same id.
    let id_2 = group.prepare_message_for_later_publish(b"retry me", true, Some(key))?;

    assert_eq!(id_1, id_2);

    // Only one row was stored.
    let messages = group.find_messages(&MsgQueryArgs {
        kind: Some(GroupMessageKind::Application),
        ..Default::default()
    })?;
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].id, id_1);
}

/// End-to-end: the idempotency key rides the wire so the receiver recomputes the
/// exact same message id. This is the foundation of at-least-once-with-dedup:
/// a retry of identical content with the same key collapses to one message id.
#[xmtp_common::test(unwrap_try = true)]
async fn test_idempotency_key_crosses_the_wire() {
    tester!(alix);
    tester!(bo);
    let group = alix.create_group(None, None)?;
    group.add_members(&[bo.inbox_id()]).await?;

    let key = "wire-key-xyz".to_string();
    let content = b"crash-safe message";
    let sent_id = group
        .send_message(
            content,
            SendMessageOpts {
                should_push: true,
                idempotency_key: Some(key.clone()),
            },
        )
        .await?;

    // bo receives the message and must derive the same id from the shared key.
    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = &bo_groups[0];
    bo_group.sync().await?;

    let received = bo_group
        .find_messages(&MsgQueryArgs {
            kind: Some(GroupMessageKind::Application),
            ..Default::default()
        })?
        .into_iter()
        .find(|m| m.decrypted_message_bytes == content)
        .expect("bo should receive the message");

    assert_eq!(received.id, sent_id);
    assert_eq!(
        received.id,
        calculate_message_id(bo_group.group_id, content, &key)
    );
    assert_eq!(received.idempotency_key, key);
}
