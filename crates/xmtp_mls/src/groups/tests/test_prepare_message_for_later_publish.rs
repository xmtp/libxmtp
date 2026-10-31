use crate::context::XmtpSharedContext;
use crate::groups::message_id::calculate_message_id;
use crate::groups::send_message_opts::SendMessageOpts;
use crate::groups::{DeliveryStatus, GroupError};
use crate::tester;
use xmtp_common::RetryableError;
use xmtp_db::group_intent::{IntentKind, IntentState, QueryGroupIntent};
use xmtp_db::group_message::{GroupMessageKind, MsgQueryArgs};

/// Prepared messages stay local until an explicit publish. Repeated publish is idempotent.
#[xmtp_common::test(unwrap_try = true)]
async fn test_prepared_message_requires_explicit_idempotent_publish() {
    tester!(alix);
    let group = alix.create_group(None, None)?;
    let message_id = group.prepare_message_for_later_publish(b"test message", true, None)?;
    let query = MsgQueryArgs {
        kind: Some(GroupMessageKind::Application),
        ..Default::default()
    };

    let messages = group.find_messages(&query)?;
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].id, message_id);
    assert_eq!(messages[0].delivery_status, DeliveryStatus::Unpublished);
    assert_eq!(
        group.find_messages(&MsgQueryArgs::default())?[0].delivery_status,
        DeliveryStatus::Unpublished
    );

    group.publish_messages().await?;
    let messages = group.find_messages(&query)?;
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].id, message_id);
    assert_eq!(messages[0].delivery_status, DeliveryStatus::Unpublished);

    group.publish_stored_message(&message_id).await?;
    group.publish_stored_message(&message_id).await?;
    group.publish_stored_message(&message_id).await?;
    group.sync().await?;
    let messages = group.find_messages(&query)?;
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].id, message_id);
    assert_eq!(messages[0].delivery_status, DeliveryStatus::Published);

    group.publish_stored_message(&message_id).await?;
    group.publish_stored_message(&message_id).await?;
    group.sync().await?;
    let messages = group.find_messages(&query)?;
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].id, message_id);
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

/// Without an explicit key, two identical-content sends still get distinct ids
/// (the random default), preserving the historical always-unique behavior.
#[xmtp_common::test(unwrap_try = true)]
async fn test_default_idempotency_key_is_unique_per_send() {
    tester!(alix);
    let group = alix.create_group(None, None)?;

    let id_1 = group.prepare_message_for_later_publish(b"same content", true, None)?;
    let id_2 = group.prepare_message_for_later_publish(b"same content", true, None)?;

    assert_ne!(
        id_1, id_2,
        "default (random) keys must yield unique message ids"
    );
}

/// Re-preparing the same content with the same explicit key is idempotent: it
/// returns the existing message id instead of erroring on the PK conflict, and
/// does not create a duplicate row. This is the sender side of
/// at-least-once-with-dedup (e.g. a crash-recovery retry).
// verifies: SEND-002, SEND-003
#[xmtp_common::test(unwrap_try = true)]
async fn test_duplicate_idempotency_key_is_idempotent() {
    tester!(alix);
    let group = alix.create_group(None, None)?;

    let key = "retry-key".to_string();
    let content = b"retry me";
    let id_1 = group.prepare_message_for_later_publish(content, true, Some(key.clone()))?;
    assert_eq!(id_1, calculate_message_id(group.group_id, content, &key));
    // Second identical prepare must not error and must return the same id.
    let id_2 = group.prepare_message_for_later_publish(content, true, Some(key.clone()))?;

    assert_eq!(id_1, id_2);

    // Both query paths return the one row with its supplied key.
    for kind in [None, Some(GroupMessageKind::Application)] {
        let messages = group.find_messages(&MsgQueryArgs {
            kind,
            ..Default::default()
        })?;
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].id, id_1);
        assert_eq!(messages[0].idempotency_key, key);
    }
}

/// End-to-end: the idempotency key rides the wire so the receiver recomputes the
/// exact same message id. This is the foundation of at-least-once-with-dedup:
/// a retry of identical content with the same key collapses to one message id.
// verifies: SEND-002, SEND-003
#[rstest::rstest]
#[case::prepare("prepare")]
#[case::optimistic("optimistic")]
#[case::normal("normal")]
#[xmtp_common::test(unwrap_try = true)]
async fn test_idempotency_key_crosses_the_wire(#[case] mode: &str) -> Result<(), GroupError> {
    tester!(alix, disable_workers);
    tester!(alix2, from: alix, disable_workers);
    tester!(bo, disable_workers);
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
    let bo_groups = bo.wait_for_welcomes().await?;
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
    assert_eq!(received.sender_inbox_id, alix.inbox_id());
    assert_eq!(received.delivery_status, DeliveryStatus::Published);

    let alix2_groups = alix2.wait_for_welcomes().await?;
    let alix2_group = &alix2_groups[0];
    alix2_group.sync().await?;
    assert_ne!(alix.installation_id, alix2.installation_id);
    let opts = || SendMessageOpts {
        should_push: true,
        idempotency_key: Some(key.clone()),
    };
    for peer in [alix2_group, bo_group] {
        let pending = || {
            peer.context
                .db()
                .find_group_intents(
                    peer.group_id,
                    Some(vec![
                        IntentState::ToPublish,
                        IntentState::Published,
                        IntentState::Committed,
                    ]),
                    Some(vec![IntentKind::SendMessage]),
                )
                .unwrap()
        };
        let before = pending();
        assert!(before.is_empty());
        let result = match mode {
            "prepare" => peer.prepare_message_for_later_publish(content, true, Some(key.clone())),
            "optimistic" => peer.send_message_optimistic(content, opts()),
            "normal" => peer.send_message(content, opts()).await,
            _ => unreachable!(),
        };
        if peer.context.inbox_id() == alix.inbox_id() {
            assert_eq!(result?, sent_id);
        } else {
            let error = result.expect_err("another inbox must not reuse a stored send");
            assert!(
                matches!(error, GroupError::MessageSenderConflict),
                "{error:?}"
            );
            assert!(!error.is_retryable());
        }
        assert!(pending() == before, "send intents must remain unchanged");
        let messages = peer.find_messages(&MsgQueryArgs {
            kind: Some(GroupMessageKind::Application),
            ..Default::default()
        })?;
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].id, sent_id);
        assert_eq!(messages[0].sender_inbox_id, received.sender_inbox_id);
        assert_eq!(messages[0].decrypted_message_bytes, content);
        assert_eq!(messages[0].delivery_status, received.delivery_status);
    }
    Ok(())
}
