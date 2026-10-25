//! Settlement of publishes that returned `OUT_OF_RANGE`. The backend may have
//! stored such a request and forbids sending it again, so a `Query` of each
//! topic settles every saved envelope by its hash, never a resend of the same
//! request or a re-encryption. A saved attempt or Welcome batch that no
//! longer fits in one request is settled the same way. A Welcome batch that
//! cannot be sent ends its intent in `FollowUpFailed`, and one intent's
//! follow-up failure never holds another intent.

use super::faults::{Fault, FaultyApi, FaultyContext, faulty_group, group_message, welcome};
use super::welcomes::{PreparedWelcomes, WelcomeEnvelopeKind};
use super::*;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use xmtp_configuration::MAX_PUBLISH_ENVELOPES;
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

/// The size of `envelope` that the deployment's envelope bound measures.
fn measured_len(envelope: &wire::ClientEnvelope) -> usize {
    xmtp_mls_validation::parse_envelope(envelope.clone())
        .unwrap()
        .canonical
        .bytes
        .len()
}

/// Rebuild against a deployment snapshot whose envelope bound became smaller
/// after the exact bytes were saved. The store and identity survive.
async fn group_after_envelope_limit_shrink(
    tester: &crate::utils::ClientTester,
    group_id: &GroupId,
    api: FaultyApi,
    max_envelope_bytes: usize,
) -> MlsGroup<FaultyContext> {
    let provider = xmtp_configuration::StaticConfigProvider::edited(|snapshot| {
        snapshot.limits.max_envelope_bytes = max_envelope_bytes;
    });
    let client = crate::builder::ClientBuilder::from_client(tester.client.clone())
        .api_client(api)
        .config_provider(Arc::new(provider))
        .with_disable_workers(true)
        .with_allow_offline(Some(true))
        .build()
        .await
        .unwrap();
    MlsGroup::new_cached(client.context.clone(), group_id)
        .unwrap()
        .0
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

/// Publish a membership change that adds `inbox_ids`, apply its echo, and
/// save its Welcome batch without sending it.
async fn save_welcome_batch<C: XmtpSharedContext>(
    group: &MlsGroup<C>,
    inbox_ids: &[&str],
) -> (i32, PreparedWelcomes) {
    let intent_id = commit_membership_change(group, inbox_ids).await;
    let welcomes = group
        .prepare_required_welcomes(intent_id)
        .unwrap()
        .and_then(|attempt| attempt.welcomes)
        .expect("the batch must be saved");
    (intent_id, welcomes)
}

/// The saved envelopes of `welcomes` that have the role `kind`.
fn of_kind(welcomes: &PreparedWelcomes, kind: WelcomeEnvelopeKind) -> Vec<wire::ClientEnvelope> {
    envelopes(&welcomes.envelopes)
        .into_iter()
        .zip(welcomes.kinds().unwrap())
        .filter(|(_, found)| *found == kind)
        .map(|(envelope, _)| envelope)
        .collect()
}

/// A client whose key packages do not advertise pointees, so it gets an
/// inline Welcome.
async fn inline_only_tester() -> crate::utils::ClientTester {
    crate::identity::ENABLE_WELCOME_POINTERS
        .scope(false, async {
            tester!(eve, disable_workers);
            eve
        })
        .await
}

/// A client whose newest key package has only a classical wrapper, so its
/// inline Welcome is smaller than one wrapped for a post-quantum key.
async fn classical_tester() -> crate::utils::ClientTester {
    tester!(fred, disable_workers);
    fred.identity()
        .rotate_and_upload_key_package(fred.context.api(), fred.context.mls_storage(), false)
        .await
        .unwrap();
    fred
}

/// A failed configuration check before dispatch sent nothing. Its status is
/// neither a backend refusal, which ends an intent, nor an `OUT_OF_RANGE`
/// that needs a recovery read. The same status from the backend is both.
///
/// The codes run in a loop, not as `#[rstest]` cases: this sync test is in
/// the wasm32 binary, where `RSTEST_TIMEOUT` makes rstest run a sync case on
/// a spawned thread, which wasm32 cannot do.
#[xmtp_common::test]
fn a_failed_configuration_check_is_not_a_backend_status() {
    for code in [tonic::Code::InvalidArgument, tonic::Code::OutOfRange] {
        let status = || {
            xmtp_proto::api::ApiClientError::client(xmtp_api_grpc::error::GrpcError::Status(
                tonic::Status::new(code, "injected"),
            ))
        };
        let classify = |error: &xmtp_api::ApiError| (definite_refusal(error), out_of_range(error));
        let backend = xmtp_api::dyn_err(status());
        assert_eq!(
            classify(&backend),
            (
                code == tonic::Code::InvalidArgument,
                code == tonic::Code::OutOfRange
            ),
            "{code:?} from the backend"
        );
        let preflight =
            xmtp_api::ApiError::Preflight(xmtp_api::preflight::PreflightError::new(status()));
        assert_eq!(
            classify(&preflight),
            (false, false),
            "{code:?} from a configuration check"
        );
    }
}

/// A smaller deployment limit can leave saved Welcome bytes that no longer
/// fit in one request, after a publish that returned `OUT_OF_RANGE` stored
/// none of them. The intent cannot end before a recovery read proves that,
/// and only the attempt that is still saved can end it. It then ends in
/// `FollowUpFailed` with nothing published, the call reports that the
/// membership change is applied but its Welcomes were not sent, no later
/// round selects it again, and a later send completes.
// verifies: SEND-023, SEND-025
#[xmtp_common::test(unwrap_try = true)]
async fn smaller_limit_fails_an_unstored_welcome_batch() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix.create_group(None, None)?;
    let api = FaultyApi::new(&alix, welcome, [Fault::Refuse(tonic::Code::OutOfRange)]);
    api.stop_before_recovery.store(true, Ordering::SeqCst);
    let interrupted = faulty_group(&alix, &group.group_id, api.clone()).await;
    assert!(interrupted.add_members(&[bo.inbox_id()]).await.is_err());
    let intent = only_intent(
        &group,
        IntentState::Committed,
        IntentKind::UpdateGroupMembership,
    );
    let attempt = saved_attempt(&group, intent.id);
    assert_eq!(attempt.unsettled, Some(Unsettled::Welcomes));
    let welcomes = attempt.welcomes.as_ref()?;
    assert!(welcomes.receipts.is_none());
    let batch = envelopes(&welcomes.envelopes);
    let largest = batch.iter().map(measured_len).max()?;
    let prior_requests = api.requests.lock().len();
    let restarted =
        group_after_envelope_limit_shrink(&alix, &group.group_id, api.clone(), largest - 1).await;

    // The recovery read fails, so the intent stays committed.
    assert!(restarted.publish_required_welcomes().await.is_err());
    assert_eq!(saved_attempt(&group, intent.id), attempt);
    let mut stale = attempt.clone();
    stale.welcomes.as_mut()?.commit_sequence_id += 1;
    restarted.end_follow_up(intent.id, &stale)?;
    only_intent(
        &group,
        IntentState::Committed,
        IntentKind::UpdateGroupMembership,
    );

    api.restart();
    let ended = restarted.publish_required_welcomes().await;
    assert!(welcomes_unsendable(&ended), "{ended:?}");
    assert_eq!(
        ended.unwrap_err().to_string(),
        "the membership change is applied, but its Welcomes were not sent"
    );
    let failed = only_intent(
        &group,
        IntentState::FollowUpFailed,
        IntentKind::UpdateGroupMembership,
    );
    assert_eq!(failed.id, intent.id);
    assert_eq!(saved_attempt(&group, intent.id), attempt);
    restarted.publish_required_welcomes().await?;
    assert_eq!(api.requests.lock().len(), prior_requests);
    for envelope in &batch {
        assert_eq!(stored_copies(&alix, envelope).await, 0);
    }
    assert!(welcomes_unsendable(
        &restarted.sync_until_intent_resolved(intent.id).await
    ));

    restarted
        .send_message(b"after the ended change", Default::default())
        .await?;
    only_intent(&group, IntentState::Processed, IntentKind::SendMessage);
}

/// Saved Welcome bytes that no longer fit may already be stored. Here the
/// batch reached the backend before the deployment lowered its limit, and
/// its receipts were never recorded. The recovery read must find it and
/// complete the membership change with the receipts its publish assigned,
/// sending nothing again.
// verifies: SEND-023
#[xmtp_common::test(unwrap_try = true)]
async fn smaller_limit_settles_a_stored_welcome_batch() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let (intent_id, welcomes) = save_welcome_batch(&group, &[bo.inbox_id()]).await;
    let api = group.context.api();
    let stored = api
        .publish_units(
            welcomes
                .units(api.limits())?
                .into_iter()
                .collect::<Option<_>>()?,
        )
        .await?;
    let largest = envelopes(&welcomes.envelopes)
        .iter()
        .map(measured_len)
        .max()?;
    let recorder = FaultyApi::new(&alix, welcome, []);
    let restarted =
        group_after_envelope_limit_shrink(&alix, &group.group_id, recorder.clone(), largest - 1)
            .await;

    restarted.publish_required_welcomes().await?;
    only_intent(
        &group,
        IntentState::Processed,
        IntentKind::UpdateGroupMembership,
    );
    let settled = saved_attempt(&group, intent_id).welcomes?;
    assert_eq!(settled.envelopes, welcomes.envelopes);
    assert_eq!(
        settled.receipts,
        Some(stored.iter().map(Message::encode_to_vec).collect())
    );
    assert!(
        recorder.requests.lock().is_empty(),
        "a stored Welcome was sent again"
    );
}

/// When the read returns a saved pointee that no longer fits, the pointers
/// it did not return still fit and go out again unchanged, and the batch
/// completes with the pointee's original receipt. If that publish returns
/// `OUT_OF_RANGE`, the batch is marked for a recovery read, so no later
/// round sends the same request again without one.
// verifies: SEND-023, SEND-007
#[rstest::rstest]
#[case::published(None)]
#[case::out_of_range(Some(Fault::Refuse(tonic::Code::OutOfRange)))]
#[xmtp_common::test(unwrap_try = true)]
async fn smaller_limit_republishes_the_pointers_of_a_stored_pointee(
    #[case] fault: Option<Fault>,
) -> Result<(), GroupError> {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(caro, disable_workers);
    tester!(dan, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let (intent_id, welcomes) =
        save_welcome_batch(&group, &[bo.inbox_id(), caro.inbox_id(), dan.inbox_id()]).await;
    let pointers = of_kind(&welcomes, WelcomeEnvelopeKind::Pointer);
    let pointees = of_kind(&welcomes, WelcomeEnvelopeKind::Pointee);
    let [pointee] = &pointees[..] else {
        panic!("the batch must carry one pointee");
    };
    assert_eq!(pointers.len(), 3);
    let limit = pointers.iter().map(measured_len).max().unwrap();
    assert!(measured_len(pointee) > limit);
    let api = group.context.api();
    let stored = api
        .publish_units(vec![PublishUnit::single_within(
            pointee.clone(),
            api.limits(),
        )?])
        .await?;
    let recorder = FaultyApi::new(&alix, welcome, fault);
    let restarted =
        group_after_envelope_limit_shrink(&alix, &group.group_id, recorder.clone(), limit).await;

    let result = restarted.publish_required_welcomes().await;
    assert_eq!(recorder.sends_of(pointee), 0);
    for pointer in &pointers {
        assert_eq!(recorder.sends_of(pointer), 1);
    }
    let saved = saved_attempt(&group, intent_id);
    if fault.is_some() {
        assert!(result.is_err());
        only_intent(
            &group,
            IntentState::Committed,
            IntentKind::UpdateGroupMembership,
        );
        assert_eq!(saved.unsettled, Some(Unsettled::Welcomes));
        return Ok(());
    }
    result?;
    only_intent(
        &group,
        IntentState::Processed,
        IntentKind::UpdateGroupMembership,
    );
    let index = envelopes(&welcomes.envelopes)
        .iter()
        .position(|envelope| envelope == pointee)
        .unwrap();
    let receipts = saved.welcomes.and_then(|saved| saved.receipts).unwrap();
    assert_eq!(receipts[index], stored[0].encode_to_vec());
    Ok(())
}

/// When some saved Welcome bytes no longer fit and the backend holds none of
/// them, the batch cannot complete, but every recipient that can still join
/// must get its envelope before the intent ends. An unreturned pointee that
/// fits goes out first, and the pointers follow unless the pointee does not
/// fit or the backend refused it; a pointee whose response was lost may be
/// stored. An inline Welcome that does not fit is never sent, the saved
/// batch keeps no receipts, and its bytes never change.
// verifies: SEND-025
#[rstest::rstest]
#[case::pointee_stored(true, false, None, true)]
#[case::pointee_lost(false, false, None, false)]
#[case::pointee_fits(false, true, None, true)]
#[case::pointee_refused(false, true, Some(Fault::Refuse(tonic::Code::InvalidArgument)), false)]
#[case::pointee_response_lost(false, true, Some(Fault::LoseResponse), true)]
#[xmtp_common::test(unwrap_try = true)]
async fn smaller_limit_publishes_pointers_whose_pointee_is_stored(
    #[case] pointee_stored: bool,
    #[case] pointee_fits: bool,
    #[case] fault: Option<Fault>,
    #[case] pointers_sent: bool,
) -> Result<(), GroupError> {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(caro, disable_workers);
    tester!(dan, disable_workers);
    let eve = inline_only_tester().await;
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let (intent_id, welcomes) = save_welcome_batch(
        &group,
        &[
            bo.inbox_id(),
            caro.inbox_id(),
            dan.inbox_id(),
            eve.inbox_id(),
        ],
    )
    .await;
    let pointers = of_kind(&welcomes, WelcomeEnvelopeKind::Pointer);
    let pointees = of_kind(&welcomes, WelcomeEnvelopeKind::Pointee);
    let [pointee] = &pointees[..] else {
        panic!("the batch must carry one pointee");
    };
    let inlines = of_kind(&welcomes, WelcomeEnvelopeKind::Inline);
    let [inline] = &inlines[..] else {
        panic!("the batch must carry one inline Welcome");
    };
    assert_eq!(pointers.len(), 3);
    let limit = if pointee_fits {
        measured_len(pointee)
    } else {
        pointers.iter().map(measured_len).max().unwrap()
    };
    assert!(measured_len(inline) > limit);
    assert!(pointee_fits || measured_len(pointee) > limit);
    let api = group.context.api();
    if pointee_stored {
        api.publish_units(vec![PublishUnit::single_within(
            pointee.clone(),
            api.limits(),
        )?])
        .await?;
    }
    let recorder = FaultyApi::new(&alix, welcome, fault);
    let restarted =
        group_after_envelope_limit_shrink(&alix, &group.group_id, recorder.clone(), limit).await;

    let ended = restarted.publish_required_welcomes().await;
    assert!(welcomes_unsendable(&ended), "{ended:?}");
    only_intent(
        &group,
        IntentState::FollowUpFailed,
        IntentKind::UpdateGroupMembership,
    );
    assert_eq!(saved_attempt(&group, intent_id).welcomes, Some(welcomes));
    assert_eq!(recorder.sends_of(pointee), usize::from(pointee_fits));
    assert_eq!(recorder.sends_of(inline), 0);
    for pointer in &pointers {
        assert_eq!(recorder.sends_of(pointer), usize::from(pointers_sent));
    }
    if pointers_sent {
        for member in [&bo, &caro, &dan] {
            xmtp_common::wait_for_ok(|| async {
                member
                    .sync_welcomes()
                    .await
                    .and_then(|_| member.group(&group.group_id).map_err(GroupError::from))
            })
            .await
            .expect("a recipient with a pointer to a stored pointee must join");
        }
    }
    Ok(())
}

/// An inline Welcome that still fits goes out before the batch ends, even
/// though another recipient's Welcome no longer fits.
// verifies: SEND-025
#[xmtp_common::test(unwrap_try = true)]
async fn smaller_limit_publishes_the_inline_welcomes_that_fit() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let fred = classical_tester().await;
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let (_, welcomes) = save_welcome_batch(&group, &[bo.inbox_id(), fred.inbox_id()]).await;
    let mut inline = of_kind(&welcomes, WelcomeEnvelopeKind::Inline);
    inline.sort_by_key(measured_len);
    let [classical, post_quantum] = &inline[..] else {
        panic!("each recipient must get an inline Welcome");
    };
    let limit = measured_len(classical);
    assert!(measured_len(post_quantum) > limit);
    let recorder = FaultyApi::new(&alix, welcome, []);
    let restarted =
        group_after_envelope_limit_shrink(&alix, &group.group_id, recorder.clone(), limit).await;

    assert!(welcomes_unsendable(
        &restarted.publish_required_welcomes().await
    ));
    only_intent(
        &group,
        IntentState::FollowUpFailed,
        IntentKind::UpdateGroupMembership,
    );
    assert_eq!(recorder.sends_of(classical), 1);
    assert_eq!(recorder.sends_of(post_quantum), 0);
}

/// A fresh Welcome batch that does not fit in one request under the current
/// limits was never sent, so no read can settle anything: its intent ends at
/// once, nothing is published, and the batch is not saved.
// verifies: SEND-025
#[xmtp_common::test(unwrap_try = true)]
async fn smaller_limit_fails_a_fresh_welcome_batch() {
    /// Below the size of any Welcome.
    const SMALL_ENVELOPE_BYTES: usize = 256;
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let intent_id = commit_membership_change(&group, &[bo.inbox_id()]).await;
    let recorder = FaultyApi::new(&alix, welcome, []);
    let restarted = group_after_envelope_limit_shrink(
        &alix,
        &group.group_id,
        recorder.clone(),
        SMALL_ENVELOPE_BYTES,
    )
    .await;

    let ended = restarted.publish_required_welcomes().await;
    assert!(welcomes_unsendable(&ended), "{ended:?}");
    let failed = only_intent(
        &group,
        IntentState::FollowUpFailed,
        IntentKind::UpdateGroupMembership,
    );
    assert_eq!(failed.id, intent_id);
    assert!(saved_attempt(&group, intent_id).welcomes.is_none());
    assert!(recorder.requests.lock().is_empty());
}

/// One intent's follow-up failure does not stop the others, and the call
/// reports the earliest intent's error after the last intent, whichever of
/// the two fails first. One batch fits and fails with a transient error, so
/// its intent stays owed; the other is a fresh batch that does not fit, so
/// its intent ends.
#[rstest::rstest]
#[case::earlier_owed(true)]
#[case::earlier_ended(false)]
#[xmtp_common::test(unwrap_try = true)]
async fn follow_up_errors_are_reported_from_the_earliest_intent(
    #[case] earlier_fits: bool,
) -> Result<(), GroupError> {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    // Fred's Welcome is classical and small; Bo's is post-quantum and larger.
    let fred = classical_tester().await;
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let (first, second) = if earlier_fits {
        (fred.inbox_id(), bo.inbox_id())
    } else {
        (bo.inbox_id(), fred.inbox_id())
    };
    let earlier = commit_membership_change(&group, &[first]).await;
    let later = commit_membership_change(&group, &[second]).await;
    let (fits, ends) = if earlier_fits {
        (earlier, later)
    } else {
        (later, earlier)
    };
    let welcomes = group
        .prepare_required_welcomes(fits)?
        .and_then(|attempt| attempt.welcomes)
        .expect("the batch must be saved");
    let limit = envelopes(&welcomes.envelopes)
        .iter()
        .map(measured_len)
        .max()
        .unwrap();
    let api =
        FaultyApi::new(&alix, welcome, []).then_always(Fault::Refuse(tonic::Code::Unavailable));
    let restarted =
        group_after_envelope_limit_shrink(&alix, &group.group_id, api.clone(), limit).await;

    let result = restarted.publish_required_welcomes().await;
    let unavailable = matches!(&result, Err(GroupError::WrappedApi(error))
        if xmtp_proto::api::grpc_status(error)
            .is_some_and(|status| status.code() == tonic::Code::Unavailable));
    if earlier_fits {
        assert!(unavailable, "{result:?}");
    } else {
        assert!(welcomes_unsendable(&result), "{result:?}");
    }
    let owed = only_intent(
        &group,
        IntentState::Committed,
        IntentKind::UpdateGroupMembership,
    );
    assert_eq!(owed.id, fits);
    let ended = only_intent(
        &group,
        IntentState::FollowUpFailed,
        IntentKind::UpdateGroupMembership,
    );
    assert_eq!(ended.id, ends);
    Ok(())
}

/// `INVALID_ARGUMENT` proves that the backend stored nothing, and the same
/// request may not be sent again. On the first publish and on the publish
/// after a recovery read alike, the membership change must end at once, the
/// waiting call must report that its Welcomes were not sent, and no later
/// round may send the refused request again.
// verifies: SEND-025
#[rstest::rstest]
#[case::first_publish(vec![Fault::Refuse(tonic::Code::InvalidArgument)])]
#[case::after_recovery_read(vec![
    Fault::Refuse(tonic::Code::OutOfRange),
    Fault::Refuse(tonic::Code::InvalidArgument),
])]
#[xmtp_common::test(unwrap_try = true)]
async fn refused_welcome_publish_ends_the_batch(
    #[case] faults: Vec<Fault>,
) -> Result<(), GroupError> {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix.create_group(None, None)?;
    let requests = faults.len();
    let api = FaultyApi::new(&alix, welcome, faults);
    let refusing = faulty_group(&alix, &group.group_id, api.clone()).await;

    let added = refusing.add_members(&[bo.inbox_id()]).await;
    assert!(welcomes_unsendable(&added), "{added:?}");
    only_intent(
        &group,
        IntentState::FollowUpFailed,
        IntentKind::UpdateGroupMembership,
    );
    refusing.publish_required_welcomes().await?;
    assert_eq!(
        api.requests.lock().len(),
        requests,
        "the refused request was sent again"
    );
    Ok(())
}

/// A send first refreshes the group's installations. When the Welcomes for
/// an installation that refresh adds cannot be sent, the membership change
/// is still applied, so the send goes on and completes. The new installation
/// has no Welcome.
#[xmtp_common::test(unwrap_try = true)]
async fn send_goes_on_when_refreshed_installations_get_no_welcome() {
    use xmtp_db::{ConnectionExt, diesel::prelude::*, schema::groups::dsl};
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    tester!(bo2, from: bo, disable_workers);
    let api = FaultyApi::new(
        &alix,
        welcome,
        [Fault::Refuse(tonic::Code::InvalidArgument)],
    );
    let refusing = faulty_group(&alix, &group.group_id, api).await;
    // The send refreshes installations now, not after its usual interval.
    alix.context.db().raw_query(|conn| {
        xmtp_db::diesel::update(dsl::groups.find(&group.group_id))
            .set(dsl::installations_last_checked.eq(0))
            .execute(conn)
    })?;

    refusing
        .send_message(b"after a new installation", Default::default())
        .await?;
    only_intent(&group, IntentState::Processed, IntentKind::SendMessage);
    only_intent(
        &group,
        IntentState::FollowUpFailed,
        IntentKind::UpdateGroupMembership,
    );
    bo2.sync_welcomes().await?;
    assert!(bo2.group(&group.group_id).is_err());
}

/// A process can stop after a membership change's echo is applied and before
/// its Welcomes are sent, and nothing else resumes that batch. The next send
/// in the conversation must send it, so the new member can join.
// verifies: SEND-011
#[xmtp_common::test(unwrap_try = true)]
async fn next_send_publishes_owed_welcomes() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let membership = commit_membership_change(&group, &[bo.inbox_id()]).await;

    group
        .send_message(b"after a restart", Default::default())
        .await?;
    let completed = only_intent(
        &group,
        IntentState::Processed,
        IntentKind::UpdateGroupMembership,
    );
    assert_eq!(completed.id, membership);
    bo.sync_welcomes().await?;
    bo.group(&group.group_id)?;
}

/// Rewrite the saved one-envelope attempt of `intent_id` as an earlier build
/// could have saved it, with more envelopes than one request may carry. Each
/// copy is the same envelope, so the backend stores at most one.
fn save_oversized_attempt<C: XmtpSharedContext>(
    group: &MlsGroup<C>,
    intent_id: i32,
) -> PreparedAttempt {
    let db = group.context.db();
    let encoded = db.prepared_envelopes(intent_id).unwrap().unwrap();
    let mut attempt = PreparedAttempt::decode(&encoded).unwrap();
    assert_eq!(attempt.envelopes.len(), 1);
    attempt.envelopes = vec![attempt.envelopes[0].clone(); MAX_PUBLISH_ENVELOPES + 1];
    let replacement = xmtp_db::db_serialize(&attempt).unwrap();
    assert!(
        db.compare_and_set_prepared_envelopes(intent_id, Some(&encoded), Some(&replacement))
            .unwrap()
    );
    attempt
}

/// An earlier build could save an attempt of more envelopes than one request
/// may now carry, and may not have sent it. The bytes cannot be sent again,
/// so a recovery read settles them. When it finds none of them, the intent
/// must fail with its message and later work must publish in the same round,
/// instead of every round stopping on the saved attempt.
// verifies: SEND-007, SEND-009
#[xmtp_common::test(unwrap_try = true)]
async fn publish_settlement_fails_an_oversized_saved_attempt_the_backend_did_not_store() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let saved_id =
        group.send_message_optimistic(b"saved by an earlier build", Default::default())?;
    let later_id = group.send_message_optimistic(b"later work", Default::default())?;
    let (intent, _) = prepare_kind(&group, IntentKind::SendMessage).await?;
    save_oversized_attempt(&group, intent.id);

    assert!(matches!(
        group.publish_intents().await,
        Err(GroupError::OutgoingPreparation(
            OutgoingPreparationError::Unstored
        ))
    ));
    let failed = only_intent(&group, IntentState::Error, IntentKind::SendMessage);
    assert_eq!(failed.id, intent.id);
    assert_eq!(status(&group, &saved_id), DeliveryStatus::Failed);
    let later: StoredGroupMessage = group.context.db().fetch(&later_id)?.unwrap();
    assert!(
        later.envelope_hash.is_some(),
        "later work was not published"
    );

    group.sync_until_last_intent_resolved().await?;
    assert_eq!(status(&group, &later_id), DeliveryStatus::Published);
}

/// An earlier build could also have stored such an attempt and lost the
/// response. The recovery read must then return the receipts its publish
/// assigned, so that the message confirms. Failing it would drop a message
/// that is on the network.
// verifies: SEND-007
#[xmtp_common::test(unwrap_try = true)]
async fn publish_settlement_recovers_an_oversized_saved_attempt_the_backend_stored() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let message_id =
        group.send_message_optimistic(b"stored before the upgrade", Default::default())?;
    let (intent, attempt) = prepare_kind(&group, IntentKind::SendMessage).await?;
    let api = group.context.api();
    let stored = api
        .send_group_messages(vec![attempt.publish_unit(api.limits())?])
        .await?;
    let oversized = save_oversized_attempt(&group, intent.id);

    group.publish_intents().await?;
    let settled = saved_attempt(&group, intent.id);
    assert!(settled.same_attempt(&oversized));
    assert_eq!(
        settled.receipts,
        Some(vec![stored[0].encode_to_vec(); MAX_PUBLISH_ENVELOPES + 1]),
        "the recovery read must return the receipt the publish assigned"
    );

    group.sync_until_intent_resolved(intent.id).await?;
    assert_eq!(status(&group, &message_id), DeliveryStatus::Published);
}
