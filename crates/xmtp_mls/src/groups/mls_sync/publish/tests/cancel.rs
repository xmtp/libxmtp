//! Deleting an own message that was never sent cancels it.

use super::faults::refusing_group;
use super::send_state_transitions::{UNRESOLVED, intents_in, status};
use super::*;
use crate::groups::DeleteMessageError;
use crate::groups::send_message_opts::SendMessageOpts;
use xmtp_content_types::{ContentCodec, encoded_content_to_bytes, text::TextCodec};
use xmtp_db::group_message::{ContentType, DeliveryStatus, MsgQueryArgs};

fn text(body: &str) -> Vec<u8> {
    encoded_content_to_bytes(TextCodec::encode(body.into()).unwrap())
}

fn deletion_rows<C: XmtpSharedContext>(group: &MlsGroup<C>) -> usize {
    group
        .find_messages(&MsgQueryArgs {
            content_types: Some(vec![ContentType::DeleteMessage]),
            ..Default::default()
        })
        .unwrap()
        .len()
}

fn deleted_events(
    events: &xmtp_events::Subscription<crate::subscriptions::internal::InternalEvent>,
    id: &[u8],
) -> Vec<xmtp_events::DeletionCause> {
    events
        .drain()
        .into_iter()
        .filter_map(|event| match event.client {
            Some(xmtp_events::ClientEvent::MessageDeleted(deleted)) if deleted.message_id == id => {
                Some(deleted.cause)
            }
            _ => None,
        })
        .collect()
}

/// How the unsent message was left before the delete.
#[derive(Clone, Copy, Debug)]
enum Unsent {
    /// `Unpublished`, with a `ToPublish` intent and no attempt.
    Queued,
    /// `Failed` after a definite backend refusal.
    Failed,
    /// Stored for later publish, with no intent.
    Prepared,
}

/// The row and every queued intent go in one write, nothing is published,
/// one local `message.deleted` fires, and the call returns the target's id.
// verifies: SEND-026
#[rstest::rstest]
#[case::queued(Unsent::Queued)]
#[case::failed(Unsent::Failed)]
#[case::prepared(Unsent::Prepared)]
#[xmtp_common::test(unwrap_try = true)]
async fn cancel_unsent(#[case] unsent: Unsent) -> Result<(), Box<dyn std::error::Error>> {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    bo.wait_for_welcomes().await?;
    let bo_group = bo.group(&group.group_id)?;
    let message = match unsent {
        Unsent::Queued => group.send_message_optimistic(&text("unsent"), Default::default())?,
        Unsent::Failed => {
            let id = group.send_message_optimistic(&text("refused"), Default::default())?;
            let refusing =
                refusing_group(&alix, &group.group_id, [tonic::Code::InvalidArgument]).await;
            assert!(refusing.publish_intents().await.is_err());
            assert_eq!(status(&group, &id), DeliveryStatus::Failed);
            id
        }
        Unsent::Prepared => group.prepare_message_for_later_publish(&text("later"), true, None)?,
    };
    let events = alix.context.events().subscribe(
        xmtp_events::EventFilter::new([xmtp_events::EventKind::MessageDeleted]),
        Some(10),
    );

    assert_eq!(group.delete_message(message.clone())?, message);

    assert!(alix.context.db().get_group_message(&message)?.is_none());
    assert!(intents_in(&group, &UNRESOLVED, IntentKind::SendMessage).is_empty());
    assert_eq!(deletion_rows(&group), 0, "a deletion message was queued");
    assert_eq!(
        deleted_events(&events, &message),
        vec![xmtp_events::DeletionCause::DeletedLocally]
    );
    group.publish_messages().await?;
    bo_group.sync().await?;
    assert!(bo.context.db().get_group_message(&message)?.is_none());
    assert_eq!(deletion_rows(&bo_group), 0);
    Ok(())
}

/// A saved attempt may already be on the backend: the delete changes
/// nothing and returns `MessageInFlight`. After its echo, a repeat request
/// queues a deletion that applies at its own echo.
// verifies: SEND-026, PROC-037
#[xmtp_common::test(unwrap_try = true)]
async fn cancel_unsent_refuses_a_saved_attempt() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let message = group.send_message_optimistic(&text("in flight"), Default::default())?;
    // The attempt is saved and published; its echo is not yet processed.
    group.publish_intents().await?;
    let saved = intents_in(&group, &UNRESOLVED, IntentKind::SendMessage);
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].state, IntentState::Published);

    assert!(matches!(
        group.delete_message(message.clone()),
        Err(GroupError::DeleteMessage(
            DeleteMessageError::MessageInFlight
        ))
    ));
    assert_eq!(status(&group, &message), DeliveryStatus::Unpublished);
    assert_eq!(
        intents_in(&group, &UNRESOLVED, IntentKind::SendMessage),
        saved
    );

    group.sync().await?;
    assert_eq!(status(&group, &message), DeliveryStatus::Published);
    let deletion = group.delete_message(message.clone())?;
    assert_ne!(deletion, message);
    group.publish_messages().await?;
    let stored = alix.context.db().get_group_message(&message)?.unwrap();
    assert_eq!(stored.deleted_by_inbox_id.as_deref(), Some(alix.inbox_id()));
}

/// A delete that lands between the publisher's capture and its prepare
/// write wins: the prepare step finds no intent and sends nothing.
// verifies: SEND-026
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn cancel_unsent_races_the_publisher() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    bo.wait_for_welcomes().await?;
    let bo_group = bo.group(&group.group_id)?;
    let message = group.send_message_optimistic(&text("raced"), Default::default())?;
    let cancelling = group.clone();
    let cancelled = message.clone();
    super::on_next_prepare(
        group.group_id.as_ref(),
        Box::new(move || {
            Box::pin(async move {
                assert_eq!(
                    cancelling.delete_message(cancelled.clone()).unwrap(),
                    cancelled
                );
            })
        }),
    );

    // The publish loop ends without sending.
    group.publish_intents().await?;

    assert!(alix.context.db().get_group_message(&message)?.is_none());
    assert!(intents_in(&group, &UNRESOLVED, IntentKind::SendMessage).is_empty());
    bo_group.sync().await?;
    assert!(bo.context.db().get_group_message(&message)?.is_none());
}

/// A same-inbox copy that another installation stored is not this
/// installation's unsent message: a deletion is queued and marks it at the
/// echo in any status, and a retry of the marked row is refused.
// verifies: SEND-026, PROC-037
#[xmtp_common::test(unwrap_try = true)]
async fn cancel_unsent_skips_another_installations_row() {
    use xmtp_db::Store;
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    let imported = StoredGroupMessage {
        id: vec![0xCA; 32],
        group_id: group.group_id,
        decrypted_message_bytes: text("imported"),
        sent_at_ns: xmtp_common::time::now_ns(),
        kind: xmtp_db::group_message::GroupMessageKind::Application,
        sender_installation_id: vec![0x01; 32],
        sender_inbox_id: alix.inbox_id().to_string(),
        delivery_status: DeliveryStatus::Failed,
        content_type: ContentType::Text,
        version_major: 1,
        version_minor: 0,
        authority_id: "xmtp.org".into(),
        reference_id: None,
        sequence_id: 0,
        envelope_hash: None,
        expiry_ns: None,
        inserted_at_ns: 0,
        expire_at_ns: None,
        expiry_unknown: false,
        should_push: true,
        idempotency_key: "imported".into(),
        deleted_by_inbox_id: None,
    };
    imported.store(&alix.context.db())?;

    let deletion = group.delete_message(imported.id.clone())?;
    assert_ne!(deletion, imported.id);
    assert_eq!(deletion_rows(&group), 1);
    group.publish_messages().await?;
    let marked = alix.context.db().get_group_message(&imported.id)?.unwrap();
    assert_eq!(marked.deleted_by_inbox_id.as_deref(), Some(alix.inbox_id()));
    assert!(marked.decrypted_message_bytes.is_empty());
    assert_eq!(marked.delivery_status, DeliveryStatus::Failed);

    assert!(matches!(
        group.publish_stored_message(&imported.id).await,
        Err(GroupError::DeleteMessage(
            DeleteMessageError::MessageAlreadyDeleted
        ))
    ));
    assert!(intents_in(&group, &UNRESOLVED, IntentKind::SendMessage).is_empty());
}

/// An idempotent resend of a published message that was since deleted
/// returns the stored row without an error.
// verifies: SEND-003, PROC-037
#[rstest::rstest]
#[case::send(false)]
#[case::optimistic(true)]
#[xmtp_common::test(unwrap_try = true)]
async fn idempotent_resend_of_a_deleted_message_returns_it(
    #[case] optimistic: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    let opts = || SendMessageOpts {
        idempotency_key: Some("fixed".into()),
        ..Default::default()
    };
    let content = text("resent");
    let message = group.send_message(&content, opts()).await?;
    group.delete_message(message.clone())?;
    group.publish_messages().await?;
    assert!(
        alix.context
            .db()
            .get_group_message(&message)?
            .unwrap()
            .deleted_by_inbox_id
            .is_some()
    );
    let resent = if optimistic {
        group.send_message_optimistic(&content, opts())?
    } else {
        group.send_message(&content, opts()).await?
    };
    assert_eq!(resent, message);
    Ok(())
}

/// A queued send whose target was marked deleted without dropping its
/// intent (as the upgrade backfill marks a row) is dropped at publish, so
/// the deleted content never reaches the topic and the intent is consumed.
// verifies: PROC-037
#[xmtp_common::test(unwrap_try = true)]
async fn a_queued_send_for_a_marked_target_is_dropped() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    bo.wait_for_welcomes().await?;
    let bo_group = bo.group(&group.group_id)?;
    let message = group.send_message_optimistic(&text("queued"), Default::default())?;
    // Mark the row directly, leaving the ToPublish intent in place, as the
    // upgrade backfill does.
    assert_eq!(
        alix.context
            .db()
            .mark_message_deleted(&message, alix.inbox_id())?,
        1
    );
    assert_eq!(
        intents_in(&group, &UNRESOLVED, IntentKind::SendMessage).len(),
        1
    );

    group.publish_intents().await?;

    assert!(intents_in(&group, &UNRESOLVED, IntentKind::SendMessage).is_empty());
    bo_group.sync().await?;
    assert!(bo.context.db().get_group_message(&message)?.is_none());
}

/// A deletion of this installation's queued message, processed from the
/// topic (sent by another installation of the same inbox), removes the
/// queued send in the same write, so the deleted content never publishes.
// verifies: PROC-037
#[xmtp_common::test(unwrap_try = true)]
async fn topic_deletion_of_a_queued_send_removes_it() {
    use xmtp_content_types::delete_message::DeleteMessageCodec;
    use xmtp_proto::xmtp::mls::message_contents::content_types::DeleteMessage;
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    bo.wait_for_welcomes().await?;
    let bo_group = bo.group(&group.group_id)?;
    let message = group.send_message_optimistic(&text("queued"), Default::default())?;
    let queued = alix.context.db().get_group_message(&message)?.unwrap();
    let deletion = StoredGroupMessage {
        id: vec![0xdd; 32],
        decrypted_message_bytes: encoded_content_to_bytes(DeleteMessageCodec::encode(
            DeleteMessage {
                message_id: hex::encode(&message),
            },
        )?),
        sender_installation_id: vec![0x02; 32],
        delivery_status: DeliveryStatus::Published,
        content_type: ContentType::DeleteMessage,
        reference_id: Some(message.clone()),
        sent_at_ns: xmtp_common::time::now_ns(),
        ..queued
    };
    crate::state_tx::state_write_with_events(
        alix.context.mls_storage(),
        alix.context.events(),
        |tx, buffer| {
            let storage = tx.storage();
            group.store_external_application_message(&storage, &deletion, false, buffer)?;
            Ok::<_, crate::groups::mls_sync::GroupMessageProcessingError>(
                xmtp_db::TransactionOutcome::Continue(()),
            )
        },
    )?;
    let marked = alix.context.db().get_group_message(&message)?.unwrap();
    assert_eq!(marked.deleted_by_inbox_id.as_deref(), Some(alix.inbox_id()));
    assert!(intents_in(&group, &UNRESOLVED, IntentKind::SendMessage).is_empty());
    group.publish_intents().await?;
    bo_group.sync().await?;
    assert!(bo.context.db().get_group_message(&message)?.is_none());
}
