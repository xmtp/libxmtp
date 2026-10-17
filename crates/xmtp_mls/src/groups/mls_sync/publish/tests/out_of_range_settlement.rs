//! Settlement of publishes that returned `OUT_OF_RANGE`. The backend may have
//! stored such a request and forbids sending it again, so a `Query` of each
//! topic settles every saved envelope by its hash, never a resend of the same
//! request or a re-encryption.

use super::faults::{Fault, FaultyApi, faulty_group, group_message, welcome};
use super::*;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use xmtp_db::group_message::DeliveryStatus;
use xmtp_proto::backend_v1 as wire;
use xmtp_proto::types::Cursor;

fn saved_attempt<C: XmtpSharedContext>(group: &MlsGroup<C>, intent_id: i32) -> PreparedAttempt {
    PreparedAttempt::decode(
        &group
            .context
            .db()
            .prepared_envelopes(intent_id)
            .unwrap()
            .unwrap(),
    )
    .unwrap()
}

fn only_intent<C: XmtpSharedContext>(
    group: &MlsGroup<C>,
    state: IntentState,
    kind: IntentKind,
) -> StoredGroupIntent {
    let mut intents = group
        .context
        .db()
        .find_group_intents(group.group_id, Some(vec![state]), Some(vec![kind]))
        .unwrap();
    assert_eq!(
        intents.len(),
        1,
        "expected one {kind:?} intent in {state:?}"
    );
    intents.remove(0)
}

fn status<C: XmtpSharedContext>(group: &MlsGroup<C>, message_id: &[u8]) -> DeliveryStatus {
    let message: StoredGroupMessage = group
        .context
        .db()
        .fetch(&message_id.to_vec())
        .unwrap()
        .unwrap();
    message.delivery_status
}

fn envelopes(bytes: &[Vec<u8>]) -> Vec<wire::ClientEnvelope> {
    bytes
        .iter()
        .map(|bytes| wire::ClientEnvelope::decode(bytes.as_slice()).unwrap())
        .collect()
}

/// How many times the backend stored `envelope` on its topic.
async fn stored_copies(
    tester: &crate::utils::ClientTester,
    envelope: &wire::ClientEnvelope,
) -> usize {
    let topic = xmtp_mls_validation::parse_envelope(envelope.clone())
        .unwrap()
        .topic;
    let api = tester.context.api();
    api.query_all(
        HashMap::from([(topic, Cursor(0))]),
        api.limits().max_query_limit as u32,
    )
    .await
    .unwrap()
    .into_iter()
    .filter(|stored| stored.envelope.as_ref() == Some(envelope))
    .count()
}

/// A publish can commit and then lose its response to `OUT_OF_RANGE`, which
/// forbids sending the same request again. The attempt must be marked
/// unsettled durably, so that a restarted client settles it with a `Query`
/// instead of resending it, and gets back the receipt the lost response
/// carried. The request reaches the backend once and the message confirms.
// verifies: SEND-007
#[xmtp_common::test(unwrap_try = true)]
async fn publish_out_of_range_settlement_recovers_a_committed_attempt_after_restart() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let message_id = group.send_message_optimistic(b"lost response", Default::default())?;
    let api = FaultyApi::new(&alix, group_message, [Fault::LoseResponse]);
    api.stop_before_recovery.store(true, Ordering::SeqCst);

    let lossy = faulty_group(&alix, &group.group_id, api.clone()).await;
    assert!(lossy.publish_intents().await.is_err());
    let intent = only_intent(&group, IntentState::Published, IntentKind::SendMessage);
    let unsettled = saved_attempt(&group, intent.id);
    assert_eq!(unsettled.unsettled, Some(Unsettled::Attempt));
    assert!(unsettled.receipts.is_none());
    assert_eq!(status(&group, &message_id), DeliveryStatus::Unpublished);

    api.restart();
    let restarted = faulty_group(&alix, &group.group_id, api.clone()).await;
    restarted.publish_intents().await?;
    let settled = saved_attempt(&group, intent.id);
    assert!(settled.same_attempt(&unsettled));
    assert_eq!(settled.unsettled, None);
    assert_eq!(
        settled.receipts,
        Some(api.lost.lock().iter().map(Message::encode_to_vec).collect()),
        "the recovery read must return the receipts the publish assigned"
    );
    assert_eq!(api.requests.lock().len(), 1, "the request was sent again");

    group.sync_until_intent_resolved(intent.id).await?;
    assert_eq!(status(&group, &message_id), DeliveryStatus::Published);
    for envelope in envelopes(&settled.envelopes) {
        assert_eq!(stored_copies(&alix, &envelope).await, 1);
    }
}

/// `OUT_OF_RANGE` can also mean that the backend stored nothing. When the
/// recovery read returns none of the attempt, no echo can arrive: the intent
/// must fail with its message, later work must publish in the same round, and
/// the refused request must not be sent again.
// verifies: SEND-007
#[xmtp_common::test(unwrap_try = true)]
async fn publish_out_of_range_settlement_fails_an_attempt_the_backend_did_not_store() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let refused_id = group.send_message_optimistic(b"never stored", Default::default())?;
    let later_id = group.send_message_optimistic(b"later work", Default::default())?;
    let api = FaultyApi::new(
        &alix,
        group_message,
        [Fault::Refuse(tonic::Code::OutOfRange)],
    );
    let lossy = faulty_group(&alix, &group.group_id, api.clone()).await;

    assert!(matches!(
        lossy.publish_intents().await,
        Err(GroupError::OutgoingPreparation(
            OutgoingPreparationError::Unstored
        ))
    ));
    let failed = only_intent(&group, IntentState::Error, IntentKind::SendMessage);
    assert_eq!(status(&group, &refused_id), DeliveryStatus::Failed);
    let refused = envelopes(&saved_attempt(&group, failed.id).envelopes);
    assert_eq!(
        api.sends_of(&refused[0]),
        1,
        "the refused request was sent again"
    );
    assert_eq!(stored_copies(&alix, &refused[0]).await, 0);
    let later: StoredGroupMessage = group.context.db().fetch(&later_id)?.unwrap();
    assert!(
        later.envelope_hash.is_some(),
        "later work was not published"
    );

    group.sync_until_last_intent_resolved().await?;
    assert_eq!(status(&group, &later_id), DeliveryStatus::Published);
}

/// The recovery read must follow every page of the topic: the settled envelope
/// is newer than a full page of earlier messages.
// verifies: SEND-007
#[xmtp_common::test(unwrap_try = true)]
async fn publish_out_of_range_settlement_reads_every_page_of_the_topic() {
    tester!(alix, disable_workers, configured: |c| c.limits.max_query_limit = 2);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    for index in 0..5 {
        group.send_message_optimistic(format!("earlier {index}").as_bytes(), Default::default())?;
    }
    group.sync_until_last_intent_resolved().await?;
    let message_id = group.send_message_optimistic(b"on a later page", Default::default())?;
    let api = FaultyApi::new(&alix, group_message, [Fault::LoseResponse]);
    let lossy = faulty_group(&alix, &group.group_id, api.clone()).await;

    lossy.publish_intents().await?;
    let intent = only_intent(&group, IntentState::Published, IntentKind::SendMessage);
    let settled = saved_attempt(&group, intent.id);
    assert_eq!(
        settled.receipts,
        Some(api.lost.lock().iter().map(Message::encode_to_vec).collect())
    );
    assert_eq!(api.requests.lock().len(), 1, "the request was sent again");
    group.sync_until_intent_resolved(intent.id).await?;
    assert_eq!(status(&group, &message_id), DeliveryStatus::Published);
}

/// A required Welcome batch spans several topics and several requests. A
/// request that commits can still return `OUT_OF_RANGE`, so the status must
/// reach the recovery read, never smaller requests with the same envelopes.
/// The stored envelopes must settle with the receipts their publish assigned,
/// including after a restart, and only the envelopes the read proved unstored
/// may be published again, with their saved bytes. No envelope is stored
/// twice, and every new member can join.
// verifies: SEND-007, SEND-011
#[xmtp_common::test(unwrap_try = true)]
async fn publish_out_of_range_settlement_completes_a_mixed_topic_welcome_batch() {
    tester!(alix, disable_workers, configured: |c| c.limits.max_publish_topics = 2);
    tester!(bo, disable_workers);
    tester!(caro, disable_workers);
    tester!(dan, disable_workers);
    let group = alix.create_group(None, None)?;
    // The batch spans at least two requests. The first to reach the backend
    // commits with its response lost, which ends the batch before the others
    // are sent.
    let api = FaultyApi::new(&alix, welcome, [Fault::LoseResponse]);
    api.stop_before_recovery.store(true, Ordering::SeqCst);
    let lossy = faulty_group(&alix, &group.group_id, api.clone()).await;

    assert!(
        lossy
            .add_members(&[bo.inbox_id(), caro.inbox_id(), dan.inbox_id()])
            .await
            .is_err()
    );
    let intent = only_intent(
        &group,
        IntentState::Committed,
        IntentKind::UpdateGroupMembership,
    );
    let unsettled = saved_attempt(&group, intent.id);
    assert_eq!(unsettled.unsettled, Some(Unsettled::Welcomes));
    let welcomes = unsettled.welcomes.clone()?;
    assert!(welcomes.receipts.is_none());
    let batch = envelopes(&welcomes.envelopes);
    let topics: HashSet<_> = batch
        .iter()
        .map(|envelope| {
            xmtp_mls_validation::parse_envelope(envelope.clone())
                .unwrap()
                .topic
        })
        .collect();
    assert_eq!(
        topics.len(),
        batch.len(),
        "the batch must span a topic per envelope"
    );
    assert!(
        batch.len() >= 4,
        "the batch must carry pointers and a pointee"
    );
    let sent = api.requests.lock().clone();
    assert_eq!(sent.len(), 1, "the lost request was sent again");
    let stored = &sent[0].envelopes;
    let unstored: Vec<_> = batch.iter().filter(|e| !stored.contains(e)).collect();
    assert!(!unstored.is_empty());

    api.restart();
    let restarted = faulty_group(&alix, &group.group_id, api.clone()).await;
    restarted.sync().await?;
    only_intent(
        &group,
        IntentState::Processed,
        IntentKind::UpdateGroupMembership,
    );
    let settled = saved_attempt(&group, intent.id);
    assert_eq!(settled.unsettled, None);
    let settled_welcomes = settled.welcomes?;
    assert_eq!(
        settled_welcomes.envelopes, welcomes.envelopes,
        "the batch was re-encrypted"
    );
    let receipts = settled_welcomes.receipts?;
    let lost = api.lost.lock().clone();
    assert_eq!(lost.len(), stored.len());
    for (envelope, meta) in stored.iter().zip(&lost) {
        let index = batch.iter().position(|e| e == envelope)?;
        assert_eq!(
            receipts[index],
            meta.encode_to_vec(),
            "the recovery read must return the receipts the publish assigned"
        );
    }
    let resent = api.requests.lock()[1..].to_vec();
    for envelope in stored {
        assert!(
            resent
                .iter()
                .all(|request| !request.envelopes.contains(envelope)),
            "a stored envelope was sent again"
        );
    }
    for envelope in unstored {
        assert_eq!(
            resent
                .iter()
                .filter(|request| request.envelopes.contains(envelope))
                .count(),
            1,
            "an unstored envelope was not published again exactly once"
        );
    }
    for envelope in &batch {
        assert_eq!(stored_copies(&alix, envelope).await, 1);
    }
    for member in [&bo, &caro, &dan] {
        xmtp_common::wait_for_ok(|| async {
            member
                .sync_welcomes()
                .await
                .and_then(|_| member.group(&group.group_id).map_err(GroupError::from))
        })
        .await
        .expect("every new member must receive its Welcome");
    }
}
