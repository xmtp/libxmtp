use super::*;
use crate::groups::test_support::stalled_api::{StalledApi, StalledCall};
use crate::groups::test_support::{
    EnginePeer, EngineTestFeatures, EngineTestPeer, EngineTestPeerOps,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use xmtp_api::ApiClientWrapper;
use xmtp_common::time::{Duration, Instant, timeout};
use xmtp_mls_core::policy::StreamPolicy;
use xmtp_mls_identity::identity_lifecycle::test_support::PeerFeatures;

const BUDGET: Duration = Duration::from_millis(250);
const OUTER_BOUND: Duration = Duration::from_secs(2);

impl PeerFeatures<StalledApi> for EngineTestFeatures {
    fn new(
        _: &ApiClientWrapper<StalledApi>,
        policy: &StreamPolicy,
        own_inbox_stored_through: Option<i64>,
    ) -> Self {
        Self::query_only(policy, own_inbox_stored_through)
    }
}

async fn client_with_stalled_api(
    tester: &EngineTestPeer,
    call: StalledCall,
) -> EngineTestPeer<StalledApi> {
    client_with_stalled_api_within(tester, call, BUDGET).await
}

/// As [`client_with_stalled_api`], with `budget` as the intent wait deadline.
async fn client_with_stalled_api_within(
    tester: &EngineTestPeer,
    call: StalledCall,
    budget: Duration,
) -> EngineTestPeer<StalledApi> {
    let api = StalledApi {
        inner: tester.context.api().api_client.raw_for_test().clone(),
        call,
        entered: Arc::new(AtomicBool::new(false)),
        cancelled: Arc::new(AtomicBool::new(false)),
        group_messages: Arc::new(AtomicUsize::new(0)),
        welcomes: Arc::new(AtomicUsize::new(0)),
    };
    // Reuse registered tester state to inject one non-completing async RPC.
    tester
        .with_api(api, None, |options| {
            options.stream_policy.barrier_timeout = budget
        })
        .await
}

#[xmtp_common::test(unwrap_try = true)]
async fn intent_sync_deadline_bounds_a_stalled_target_query() {
    let alix = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    group.send_message_optimistic(b"waiting for a target", Default::default())?;
    let (intent, _) = prepare_message(&group).await?;
    let before = group.context.db().prepared_envelopes(intent.id)?.unwrap();
    let client = client_with_stalled_api(&alix, StalledCall::TargetQuery).await;
    let (group, _) = MlsGroup::new_cached(client.context.clone(), &group.group_id)?;

    let started = Instant::now();
    let result = timeout(OUTER_BOUND, group.sync_until_last_intent_resolved()).await?;
    assert!(matches!(result, Err(GroupError::SyncFailedToWait(_))));
    assert!(started.elapsed() < OUTER_BOUND);
    let api = client.context.api().api_client.raw_for_test();
    assert!(api.entered.load(Ordering::SeqCst));
    assert!(api.cancelled.load(Ordering::SeqCst));
    let current: StoredGroupIntent = group.context.db().fetch(&intent.id)?.unwrap();
    assert_eq!(current.state, IntentState::Published);
    assert_eq!(
        group.context.db().prepared_envelopes(intent.id)?.unwrap(),
        before
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn intent_sync_deadline_bounds_a_stalled_welcome_publish() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let data = group
        .get_membership_update_intent(&[bo.inbox_id()], &[])
        .await?;
    QueueIntent::update_group_membership()
        .data(data)
        .queue(&group)?;
    let (intent, _) = prepare_kind(&group, IntentKind::UpdateGroupMembership).await?;
    group.publish_intents().await?;
    assert!(!group.receive().await?.is_errored());
    group.prepare_required_welcomes(intent.id)?.unwrap();
    let before = group.context.db().prepared_envelopes(intent.id)?.unwrap();
    let client = client_with_stalled_api(&alix, StalledCall::WelcomePublish).await;
    let (group, _) = MlsGroup::new_cached(client.context.clone(), &group.group_id)?;

    let started = Instant::now();
    let result = timeout(OUTER_BOUND, group.sync_until_last_intent_resolved()).await?;
    assert!(matches!(
        result,
        Err(GroupError::PublishedButUnconfirmed { intent_id, .. }) if intent_id == intent.id
    ));
    assert!(started.elapsed() < OUTER_BOUND);
    let api = client.context.api().api_client.raw_for_test();
    assert!(api.entered.load(Ordering::SeqCst));
    assert!(api.cancelled.load(Ordering::SeqCst));
    let current: StoredGroupIntent = group.context.db().fetch(&intent.id)?.unwrap();
    assert_eq!(current.state, IntentState::Committed);
    assert_eq!(
        group.context.db().prepared_envelopes(intent.id)?.unwrap(),
        before
    );

    // The app's action for PublishedButUnconfirmed is the conversation's
    // sync. It finishes the pending intent from the saved attempt: it sends
    // the welcome once and does not publish the commit again.
    let recorded = client_with_stalled_api(&alix, StalledCall::Recorded).await;
    let (resumed, _) = MlsGroup::new_cached(recorded.context.clone(), &group.group_id)?;
    resumed.sync().await?;
    let finished: StoredGroupIntent = resumed.context.db().fetch(&intent.id)?.unwrap();
    assert_eq!(finished.state, IntentState::Processed);
    let api = recorded.context.api().api_client.raw_for_test();
    assert_eq!(
        api.group_messages.load(Ordering::SeqCst),
        0,
        "the commit was republished"
    );
    assert_eq!(api.welcomes.load(Ordering::SeqCst), 1);
    bo.sync_welcomes().await?;
    bo.group(&group.group_id)?;
}

/// An earlier membership change whose Welcome publish never answers must not
/// turn a later send into published but unconfirmed. The round completes the
/// send once its echo is applied, before it runs the earlier batch's publish.
/// That publish still holds the wait until its deadline, and then the send
/// reports success. The earlier change stays committed and still owes its
/// batch.
// verifies: SEND-024
#[xmtp_common::test(unwrap_try = true)]
async fn later_send_completes_while_earlier_welcomes_stall() {
    /// Long enough for the send's own round on a loaded test machine.
    const SEND_BUDGET: Duration = Duration::from_secs(5);
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let membership = commit_membership_change(&group, &[bo.inbox_id()]).await;
    let client =
        client_with_stalled_api_within(&alix, StalledCall::WelcomePublish, SEND_BUDGET).await;
    let (stalled, _) = MlsGroup::new_cached(client.context.clone(), &group.group_id)?;

    timeout(
        SEND_BUDGET * 2,
        stalled.send_message(b"after the membership change", Default::default()),
    )
    .await??;
    // The wait ran the stalled publish, the waived known limitation on
    // publish_required_welcomes. Delete this line when that limitation goes.
    let api = client.context.api().api_client.raw_for_test();
    assert!(api.entered.load(Ordering::SeqCst));
    let sends = group.context.db().find_group_intents(
        group.group_id,
        Some(vec![IntentState::Processed]),
        Some(vec![IntentKind::SendMessage]),
    )?;
    assert_eq!(sends.len(), 1);
    let owed: StoredGroupIntent = group.context.db().fetch(&membership)?.unwrap();
    assert_eq!(owed.state, IntentState::Committed);
}

/// A wait can end before any round reads its intent's state. When the
/// intent's follow-up batch was already ended, the call still reports that
/// the membership change is applied but its Welcomes were not sent.
// verifies: SEND-025
#[xmtp_common::test(unwrap_try = true)]
async fn waiter_on_an_ended_follow_up_reports_unsent_welcomes() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let membership = commit_membership_change(&group, &[bo.inbox_id()]).await;
    group
        .context
        .db()
        .set_group_intent_follow_up_failed(membership)?;
    let client = client_with_stalled_api(&alix, StalledCall::TargetQuery).await;
    let (stalled, _) = MlsGroup::new_cached(client.context.clone(), &group.group_id)?;

    let result = timeout(OUTER_BOUND, stalled.sync_until_intent_resolved(membership)).await?;
    assert!(welcomes_unsendable(&result), "{result:?}");
    let api = client.context.api().api_client.raw_for_test();
    assert!(api.entered.load(Ordering::SeqCst));
}

/// A send in a DM whose creation stopped before the peer was added runs the
/// repair while the add's Welcome cannot be published. The repair's own
/// unconfirmed outcome is not the send's: the message waits behind the add,
/// completes, and reaches the peer once a sync sends the Welcome.
// verifies: DMS-020, SEND-019
#[xmtp_common::test(unwrap_try = true)]
async fn an_unconfirmed_dm_repair_does_not_end_the_send() {
    /// Long enough for the add's and the send's rounds on a loaded machine.
    const SEND_BUDGET: Duration = Duration::from_secs(5);
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let orphan = MlsGroup::create_dm_without_peer(&alix.context, bo.inbox_id().to_string())?;
    let client =
        client_with_stalled_api_within(&alix, StalledCall::WelcomePublish, SEND_BUDGET).await;
    let (stalled, _) = MlsGroup::new_cached(client.context.clone(), &orphan.group_id)?;

    let message = b"queued behind the repair";
    let sent = timeout(
        SEND_BUDGET * 3,
        stalled.send_message(message, Default::default()),
    )
    .await?;
    assert!(sent.is_ok(), "{sent:?}");
    let api = client.context.api().api_client.raw_for_test();
    assert!(api.entered.load(Ordering::SeqCst));

    alix.group(&orphan.group_id)?.sync().await?;
    bo.wait_for_welcomes().await?;
    let bo_dm = bo.group(&orphan.group_id)?;
    bo_dm.sync().await?;
    let received = bo_dm.find_messages(&Default::default())?;
    assert!(
        received
            .iter()
            .any(|stored| stored.decrypted_message_bytes == message)
    );
}
