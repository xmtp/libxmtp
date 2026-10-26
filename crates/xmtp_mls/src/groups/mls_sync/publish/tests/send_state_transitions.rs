//! Durable send state across same-key retries, backend refusals, and
//! ambiguous publish failures.

use super::faults::{Fault, FaultyApi, faulty_group, group_message, refusing_group, welcome};
use super::membership_recovery::receive_completed_prefix;
use super::*;
use crate::groups::send_message_opts::SendMessageOpts;
use std::sync::atomic::Ordering;
use xmtp_db::group_message::DeliveryStatus;

pub(super) fn intents_in<C: XmtpSharedContext>(
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

pub(super) const UNRESOLVED: [IntentState; 3] = [
    IntentState::ToPublish,
    IntentState::Published,
    IntentState::Committed,
];

pub(super) fn status<C: XmtpSharedContext>(
    group: &MlsGroup<C>,
    message_id: &[u8],
) -> DeliveryStatus {
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

/// A send queued behind a state change must wait through that change's
/// receipt. `QueryNewest` can trail the receipt (OPS-009), so a head sampled
/// from it can sit below the change and leave its echo unprocessed in every
/// round until the retry budget ends. A member's first send in a group it
/// joined queues a key update ahead of the message, so this is that send.
// verifies: SEND-022, PROC-015
#[xmtp_common::test(unwrap_try = true)]
async fn send_state_transitions_send_behind_a_state_change_waits_for_its_receipt() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    let api = FaultyApi::new(&alix, group_message, []);
    api.lag_newest.store(true, Ordering::SeqCst);
    let lagged = faulty_group(&alix, &group.group_id, api).await;
    QueueIntent::key_update().queue(&lagged)?;

    let message_id = lagged
        .send_message(b"behind a key update", Default::default())
        .await?;
    assert!(intents_in(&lagged, &UNRESOLVED, IntentKind::KeyUpdate).is_empty());
    assert!(intents_in(&lagged, &UNRESOLVED, IntentKind::SendMessage).is_empty());
    assert_eq!(status(&lagged, &message_id), DeliveryStatus::Published);
}

/// No message can be built while a proposal is pending. A send queued then
/// must stay queued and must not stop the commit queued after it, which
/// clears the proposal. The queued send then publishes in the next epoch,
/// before every later send.
// verifies: SEND-022
#[xmtp_common::test(unwrap_try = true)]
async fn send_state_transitions_pending_proposal_defers_a_queued_send() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(caro, disable_workers);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let proposal = QueueIntent::propose_member_update()
        .data(Vec::<u8>::try_from(ProposeMemberUpdateIntentData::new(
            vec![caro.inbox_id().to_string()],
            vec![],
        ))?)
        .queue(&group)?;
    group.sync_until_intent_resolved(proposal.id).await?;
    assert!(group.with_group_snapshot(|mls| Ok(mls.pending_proposals().next().is_some()))?);
    let epoch = group.epoch().await?;

    // The optimistic send is queued ahead of the commit that `send_message`
    // queues for the pending proposal.
    let queued = group.send_message_optimistic(b"queued", SendMessageOpts::default())?;
    let sent = group
        .send_message(b"sent", SendMessageOpts::default())
        .await?;

    assert!(group.epoch().await? > epoch);
    assert!(group.with_group_snapshot(|mls| Ok(mls.pending_proposals().next().is_none()))?);
    assert!(
        group
            .members()
            .await?
            .iter()
            .any(|member| member.inbox_id == caro.inbox_id())
    );
    assert!(intents_in(&group, &UNRESOLVED, IntentKind::SendMessage).is_empty());
    let sequence = |id: &[u8]| {
        let message: StoredGroupMessage = group.context.db().fetch(&id.to_vec()).unwrap().unwrap();
        assert_eq!(message.delivery_status, DeliveryStatus::Published);
        message.sequence_id
    };
    assert!(sequence(&queued) < sequence(&sent));
}

// Alix proposes to add Caro. Bo has received the proposal.
async fn group_with_pending_proposal(
    alix: &crate::utils::FullXmtpClient,
    bo: &crate::utils::FullXmtpClient,
    caro: &crate::utils::FullXmtpClient,
) -> Result<[crate::utils::TestMlsGroup; 2], GroupError> {
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    bo.wait_for_welcomes().await?;
    let bo_group = bo.group(&group.group_id)?;
    let proposal = QueueIntent::propose_member_update()
        .data(Vec::<u8>::try_from(ProposeMemberUpdateIntentData::new(
            vec![caro.inbox_id().to_string()],
            vec![],
        ))?)
        .queue(&group)?;
    group.sync_until_intent_resolved(proposal.id).await?;
    receive_completed_prefix(&group, &bo_group).await?;
    for member in [&group, &bo_group] {
        assert!(member.with_group_snapshot(|mls| Ok(mls.pending_proposals().next().is_some()))?);
    }
    Ok([group, bo_group])
}

fn published_sequence<C: XmtpSharedContext>(group: &MlsGroup<C>, message_id: &[u8]) -> i64 {
    let message: StoredGroupMessage = group
        .context
        .db()
        .fetch(&message_id.to_vec())
        .unwrap()
        .unwrap();
    assert_eq!(message.delivery_status, DeliveryStatus::Published);
    message.sequence_id
}

/// Another member's commit can clear the proposal after the client deferred
/// a message and before it reaches the next one. The later message must
/// still publish after the deferred one.
// verifies: SEND-022
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn send_state_transitions_later_send_never_overtakes_a_deferred_send() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(caro, disable_workers);
    let [group, bo_group] = group_with_pending_proposal(&alix, &bo, &caro).await?;
    let first = group.send_message_optimistic(b"first", SendMessageOpts::default())?;
    let second = group.send_message_optimistic(b"second", SendMessageOpts::default())?;

    // Bo commits the proposal. Alix processes that commit only after it
    // deferred the first message.
    let commit = QueueIntent::commit_pending_proposals().queue(&bo_group)?;
    bo_group.sync_until_intent_resolved(commit.id).await?;
    let epoch = group.epoch().await?;
    let (committer, receiver) = (bo_group.clone(), group.clone());
    super::on_next_deferral(
        group.group_id.as_ref(),
        Box::new(move || {
            Box::pin(async move {
                receive_completed_prefix(&committer, &receiver)
                    .await
                    .expect("receive Bo's commit");
            })
        }),
    );
    group.publish_messages().await?;

    assert!(group.epoch().await? > epoch, "the deferral hook ran");
    assert_eq!(
        status(&group, &first),
        DeliveryStatus::Published,
        "a later message must not publish while the deferred one waits"
    );
    assert!(published_sequence(&group, &first) < published_sequence(&group, &second));
}

/// A later message that already holds a saved attempt must not overtake a
/// deferred one either. An echo rejected for an old epoch returns only that
/// message to the queue; a later message keeps the attempt it saved.
// verifies: SEND-022
#[xmtp_common::test(unwrap_try = true)]
async fn send_state_transitions_prepared_send_never_overtakes_a_deferred_send() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(caro, disable_workers);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    bo.wait_for_welcomes().await?;
    let bo_group = bo.group(&group.group_id)?;
    let first = group.send_message_optimistic(b"first", SendMessageOpts::default())?;
    let second = group.send_message_optimistic(b"second", SendMessageOpts::default())?;
    let (first_intent, _) = prepare_kind(&group, IntentKind::SendMessage).await?;
    let (second_intent, _) = prepare_kind(&group, IntentKind::SendMessage).await?;
    assert!(first_intent.id < second_intent.id);
    group
        .context
        .db()
        .set_group_intent_to_publish(first_intent.id)?;

    // Bo proposes to add Caro. Alix receives the proposal.
    let proposal = QueueIntent::propose_member_update()
        .data(Vec::<u8>::try_from(ProposeMemberUpdateIntentData::new(
            vec![caro.inbox_id().to_string()],
            vec![],
        ))?)
        .queue(&bo_group)?;
    bo_group.sync_until_intent_resolved(proposal.id).await?;
    receive_completed_prefix(&bo_group, &group).await?;
    assert!(group.with_group_snapshot(|mls| Ok(mls.pending_proposals().next().is_some()))?);

    group.publish_messages().await?;

    assert_eq!(
        status(&group, &first),
        DeliveryStatus::Published,
        "a later message must not publish while the deferred one waits"
    );
    assert!(published_sequence(&group, &first) < published_sequence(&group, &second));
}

/// A queue operation commits a pending proposal itself when nothing queued
/// after a deferred message does, so the message publishes. The commit is
/// queued after the message, and the wait must still observe its receipt:
/// `QueryNewest` can trail it.
// verifies: SEND-022
#[rstest::rstest]
#[case::current_head(false)]
#[case::lagging_head(true)]
#[xmtp_common::test(unwrap_try = true)]
async fn send_state_transitions_publish_commits_a_pending_proposal(
    #[case] lag_newest: bool,
) -> Result<(), GroupError> {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(caro, disable_workers);
    let [group, _bo_group] = group_with_pending_proposal(&alix, &bo, &caro).await?;
    let api = FaultyApi::new(&alix, group_message, []);
    api.lag_newest.store(lag_newest, Ordering::SeqCst);
    let group = faulty_group(&alix, &group.group_id, api).await;
    let epoch = group.epoch().await?;
    let message = group.send_message_optimistic(b"optimistic", SendMessageOpts::default())?;

    group.publish_messages().await?;

    assert!(group.epoch().await? > epoch);
    assert!(group.with_group_snapshot(|mls| Ok(mls.pending_proposals().next().is_none()))?);
    // Under the lagging API, selection cannot resolve Caro's identity and
    // omits the addition; the commit still clears the proposal.
    if !lag_newest {
        assert!(
            group
                .members()
                .await?
                .iter()
                .any(|member| member.inbox_id == caro.inbox_id())
        );
    }
    assert!(intents_in(&group, &UNRESOLVED, IntentKind::SendMessage).is_empty());
    published_sequence(&group, &message);
    Ok(())
}

/// An earlier change can stay `Committed` while its Welcome publication
/// fails. Its receipt is already processed, so the wait must follow the
/// published commit queued for the deferred message instead. The failing
/// Welcome still holds every later intent in `Committed`, so the call
/// itself cannot finish; check the commit and the message.
// verifies: SEND-022
#[xmtp_common::test(unwrap_try = true)]
async fn send_state_transitions_deferred_send_waits_for_the_published_commit() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(caro, disable_workers);
    tester!(dave, disable_workers);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    bo.wait_for_welcomes().await?;
    let bo_group = bo.group(&group.group_id)?;
    let api = FaultyApi::new(
        &alix,
        welcome,
        std::iter::repeat_n(Fault::Refuse(tonic::Code::Unavailable), 100),
    );
    let refusing = faulty_group(&alix, &group.group_id, api).await;

    // Adding Dave commits, but its Welcome is refused, so the change stays
    // `Committed`.
    assert!(refusing.add_members(&[dave.inbox_id()]).await.is_err());
    assert_eq!(
        intents_in(
            &refusing,
            &[IntentState::Committed],
            IntentKind::UpdateGroupMembership
        )
        .len(),
        1
    );

    // Bo proposes to add Caro. Alix receives the proposal.
    let proposal = QueueIntent::propose_member_update()
        .data(Vec::<u8>::try_from(ProposeMemberUpdateIntentData::new(
            vec![caro.inbox_id().to_string()],
            vec![],
        ))?)
        .queue(&bo_group)?;
    bo_group.sync_until_intent_resolved(proposal.id).await?;
    receive_completed_prefix(&bo_group, &group).await?;
    assert!(refusing.with_group_snapshot(|mls| Ok(mls.pending_proposals().next().is_some()))?);
    let epoch = refusing.epoch().await?;

    let message = refusing.send_message_optimistic(b"deferred", SendMessageOpts::default())?;
    let _ = refusing.publish_messages().await;

    assert!(refusing.epoch().await? > epoch);
    assert!(
        intents_in(
            &refusing,
            &[IntentState::Published],
            IntentKind::CommitPendingProposals
        )
        .is_empty(),
        "the wait must process the published commit"
    );
    published_sequence(&refusing, &message);
}

/// The proposal of an own published change enters the proposal store only
/// with its echo, after `send_message` checked for pending proposals. The
/// send must still complete.
// verifies: SEND-022
#[xmtp_common::test(unwrap_try = true)]
async fn send_state_transitions_send_commits_a_proposal_that_lands_during_the_send() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(caro, disable_workers);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    QueueIntent::propose_member_update()
        .data(Vec::<u8>::try_from(ProposeMemberUpdateIntentData::new(
            vec![caro.inbox_id().to_string()],
            vec![],
        ))?)
        .queue(&group)?;
    group.publish_intents().await?;
    assert!(group.with_group_snapshot(|mls| Ok(mls.pending_proposals().next().is_none()))?);
    let epoch = group.epoch().await?;

    let message = group
        .send_message(b"during", SendMessageOpts::default())
        .await?;

    assert!(group.epoch().await? > epoch);
    assert!(group.with_group_snapshot(|mls| Ok(mls.pending_proposals().next().is_none()))?);
    published_sequence(&group, &message);
}

/// Ambiguous failures leave the DM repair's add without a receipt when its
/// wait ends, so the add may still be refused. The send-side repair fails
/// instead of letting a message queue behind an add that could leave the
/// peer out.
// verifies: DMS-020
#[xmtp_common::test(unwrap_try = true)]
async fn send_state_transitions_dm_repair_without_a_receipt_fails_before_the_send() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let orphan = MlsGroup::create_dm_without_peer(&alix.context, bo.inbox_id().to_string())?;
    let refusing = refusing_group(
        &alix,
        &orphan.group_id,
        std::iter::repeat_n(tonic::Code::Unavailable, 1000),
    )
    .await;

    let result = refusing.ensure_dm_peer().await;
    assert!(
        matches!(result, Err(GroupError::SyncFailedToWait(_))),
        "{result:?}"
    );
}
