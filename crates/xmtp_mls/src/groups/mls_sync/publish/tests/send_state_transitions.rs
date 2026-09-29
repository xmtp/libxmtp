//! Durable send state across same-key retries, backend refusals, and
//! ambiguous publish failures.

use super::faults::refusing_group;
use super::*;
use crate::groups::send_message_opts::SendMessageOpts;
use xmtp_db::group_message::DeliveryStatus;

fn intents_in<C: XmtpSharedContext>(
    group: &MlsGroup<C>,
    states: &[IntentState],
    kind: IntentKind,
) -> Vec<StoredGroupIntent> {
    group
        .context
        .db()
        .find_group_intents(group.group_id, Some(states.to_vec()), Some(vec![kind]))
        .unwrap()
}

const UNRESOLVED: [IntentState; 3] = [
    IntentState::ToPublish,
    IntentState::Published,
    IntentState::Committed,
];

fn status<C: XmtpSharedContext>(group: &MlsGroup<C>, message_id: &[u8]) -> DeliveryStatus {
    let message: StoredGroupMessage = group
        .context
        .db()
        .fetch(&message_id.to_vec())
        .unwrap()
        .unwrap();
    message.delivery_status
}

/// Resending the same content under the same idempotency key must
/// resolve to one stored message with at most one unresolved intent. After a
/// definite refusal, the retry must return the message to `Unpublished` in the
/// same transaction that queues the retry intent, so that the app never sees a
/// failed message with work pending; a published message is never queued again.
// verifies: SEND-003
#[xmtp_common::test(unwrap_try = true)]
async fn send_state_transitions_same_key_reuses_one_message_and_one_unresolved_intent() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let events = alix.context.events().subscribe(
        xmtp_events::EventFilter::new([xmtp_events::EventKind::MessageStatusChanged]),
        Some(10),
    );
    let opts = || SendMessageOpts {
        idempotency_key: Some("same key".into()),
        ..Default::default()
    };
    let message_id = group.send_message_optimistic(b"same content", opts())?;
    assert_eq!(
        group.send_message_optimistic(b"same content", opts())?,
        message_id
    );
    let rows = group
        .find_messages(&Default::default())?
        .into_iter()
        .filter(|message| message.id == message_id)
        .count();
    assert_eq!(rows, 1, "a repeated key stored a second message");
    assert_eq!(
        intents_in(&group, &UNRESOLVED, IntentKind::SendMessage).len(),
        1
    );

    let refusing = refusing_group(&alix, &group.group_id, [tonic::Code::InvalidArgument]).await;
    assert!(refusing.publish_intents().await.is_err());
    assert_eq!(status(&group, &message_id), DeliveryStatus::Failed);
    events.drain();

    // Both entry points retry through the same writer.
    assert_eq!(
        group.send_message_optimistic(b"same content", opts())?,
        message_id
    );
    assert_eq!(status(&group, &message_id), DeliveryStatus::Unpublished);
    assert!(matches!(
        events.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(xmtp_events::ClientEvent::MessageStatusChanged(change)), ..
        }] if change.message_id == message_id
            && change.previous == xmtp_events::MessageStatus::Failed
            && change.current == xmtp_events::MessageStatus::Unpublished
    ));
    group.send_message_optimistic(b"same content", opts())?;
    assert_eq!(
        intents_in(&group, &UNRESOLVED, IntentKind::SendMessage).len(),
        1
    );
    assert_eq!(
        intents_in(&group, &[IntentState::Error], IntentKind::SendMessage).len(),
        1
    );

    group.publish_stored_message(&message_id).await?;
    assert_eq!(status(&group, &message_id), DeliveryStatus::Published);
    assert_eq!(
        group.send_message_optimistic(b"same content", opts())?,
        message_id
    );
    group.publish_stored_message(&message_id).await?;
    assert!(
        intents_in(
            &group,
            &[IntentState::ToPublish, IntentState::Published],
            IntentKind::SendMessage
        )
        .is_empty()
    );
    assert_eq!(status(&group, &message_id), DeliveryStatus::Published);
}

/// Publishing a failed message again by ID must return it to `Unpublished` in
/// the same transaction that queues its new intent, as a same-key resend does.
// verifies: SEND-003
#[xmtp_common::test(unwrap_try = true)]
async fn send_state_transitions_publish_stored_message_returns_failed_message_to_unpublished() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let events = alix.context.events().subscribe(
        xmtp_events::EventFilter::new([xmtp_events::EventKind::MessageStatusChanged]),
        Some(10),
    );
    let message_id = group.send_message_optimistic(b"retry by id", Default::default())?;
    let refusing = refusing_group(&alix, &group.group_id, [tonic::Code::InvalidArgument]).await;
    assert!(refusing.publish_intents().await.is_err());
    assert_eq!(status(&group, &message_id), DeliveryStatus::Failed);
    events.drain();

    group.publish_stored_message(&message_id).await?;
    let changes: Vec<_> = events
        .drain()
        .into_iter()
        .filter_map(|event| match event.client {
            Some(xmtp_events::ClientEvent::MessageStatusChanged(change))
                if change.message_id == message_id =>
            {
                Some((change.previous, change.current))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        changes.first(),
        Some(&(
            xmtp_events::MessageStatus::Failed,
            xmtp_events::MessageStatus::Unpublished
        ))
    );
    assert_eq!(status(&group, &message_id), DeliveryStatus::Published);
}

/// A backend `INVALID_ARGUMENT` proves the atomic publish stored
/// nothing, so no echo can arrive. The refused intent must end as `Error`, its
/// message as `Failed`, and later intents must publish in the same round,
/// including after a refused state change that would otherwise block the group.
// verifies: SEND-009
#[rstest::rstest]
#[case::message(IntentKind::SendMessage)]
#[case::state_change(IntentKind::KeyUpdate)]
#[xmtp_common::test(unwrap_try = true)]
async fn send_state_transitions_definite_refusal_fails_both_records_and_releases_later_work(
    #[case] refused: IntentKind,
) -> Result<(), GroupError> {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let refused_message = match refused {
        IntentKind::SendMessage => {
            Some(group.send_message_optimistic(b"refused", Default::default())?)
        }
        _ => {
            QueueIntent::key_update().queue(&group)?;
            None
        }
    };
    let later_id = group.send_message_optimistic(b"later work", Default::default())?;
    let refusing = refusing_group(&alix, &group.group_id, [tonic::Code::InvalidArgument]).await;

    assert!(matches!(
        refusing.publish_intents().await,
        Err(GroupError::WrappedApi(xmtp_api::ApiError::Api(_)))
    ));
    let failed = intents_in(&group, &[IntentState::Error], refused);
    assert_eq!(failed.len(), 1, "the refused intent is not terminal");
    assert!(
        intents_in(
            &group,
            &[IntentState::ToPublish, IntentState::Published],
            refused
        )
        .iter()
        .all(|intent| intent.id != failed[0].id)
    );
    if let Some(id) = &refused_message {
        assert_eq!(status(&group, id), DeliveryStatus::Failed);
    }
    let later: StoredGroupMessage = group.context.db().fetch(&later_id)?.unwrap();
    assert!(
        later.envelope_hash.is_some(),
        "later work waited for an impossible echo"
    );

    assert!(
        group
            .sync_until_intent_resolved(failed[0].id)
            .await
            .is_err()
    );
    group.sync_until_last_intent_resolved().await?;
    assert_eq!(status(&group, &later_id), DeliveryStatus::Published);
    Ok(())
}

/// A status that can follow a committed publish is not a refusal.
/// The attempt must stay published with its exact bytes and no receipt, and
/// its message must stay `Unpublished`, so that the outcome can still settle.
/// `OUT_OF_RANGE` is settled by a recovery read instead; see
/// `out_of_range_settlement`.
// verifies: SEND-009
#[rstest::rstest]
#[case::permission_denied(tonic::Code::PermissionDenied)]
#[case::internal(tonic::Code::Internal)]
#[xmtp_common::test(unwrap_try = true)]
async fn send_state_transitions_ambiguous_failure_keeps_the_attempt_pending(
    #[case] code: tonic::Code,
) -> Result<(), GroupError> {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let message_id = group.send_message_optimistic(b"ambiguous", Default::default())?;
    let (intent, attempt) = prepare_message(&group).await?;
    // Enough failures to outlast any retry of a retryable status.
    let refusing = refusing_group(&alix, &group.group_id, std::iter::repeat_n(code, 64)).await;

    assert!(refusing.publish_intents().await.is_err());
    let current: StoredGroupIntent = group.context.db().fetch(&intent.id)?.unwrap();
    assert_eq!(current.state, IntentState::Published);
    let saved =
        PreparedAttempt::decode(&group.context.db().prepared_envelopes(intent.id)?.unwrap())?;
    assert_eq!(saved, attempt);
    assert!(saved.receipts.is_none());
    assert_eq!(status(&group, &message_id), DeliveryStatus::Unpublished);
    Ok(())
}
