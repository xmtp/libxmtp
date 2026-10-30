//! A rate-limited publish is not sent again inside its delay.

use super::faults::{Fault, FaultyApi, faulty_group, group_message, welcome};
use super::send_state_transitions::{UNRESOLVED, intents_in};
use super::*;
use crate::groups::send_message_opts::SendMessageOpts;
use xmtp_common::{RetryableError, time::Duration};

/// The delay outlasts the sync call, so the call returns the rejection after
/// one publish instead of sending the intent again on its own backoff.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_rate_limited_publish_is_not_resent_by_the_sync_loop() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    let delay = Duration::from_secs(300);
    let api = FaultyApi::new(
        &alix,
        group_message,
        [Fault::RateLimit(delay), Fault::RateLimit(delay)],
    );
    let limited = faulty_group(&alix, &group.group_id, api.clone()).await;
    limited.send_message_optimistic(b"hello", SendMessageOpts::default())?;
    let intent = intents_in(&group, &UNRESOLVED, IntentKind::SendMessage).remove(0);

    let error = limited
        .sync_until_intent_resolved(intent.id)
        .await
        .unwrap_err();
    assert_eq!(api.requests.lock().len(), 1);
    assert_eq!(error.rate_limit_delay(), Some(delay));
    assert!(error.is_retryable());
}

/// The tester's group behind `api`, with a bounded sync of `barrier_timeout`.
async fn limited_group(
    tester: &crate::utils::ClientTester,
    group_id: &xmtp_proto::types::GroupId,
    api: FaultyApi,
    barrier_timeout: Duration,
) -> MlsGroup<super::faults::FaultyContext> {
    let client = crate::builder::ClientBuilder::from_client(tester.client.clone())
        .api_client(api)
        .stream_policy(crate::subscriptions::policy::StreamPolicy {
            barrier_timeout,
            ..Default::default()
        })
        .with_disable_workers(true)
        .with_allow_offline(Some(true))
        .build()
        .await
        .unwrap();
    MlsGroup::new_cached(client.context.clone(), group_id)
        .unwrap()
        .0
}

/// A delay that fits the retry budget but not the round's timeout returns
/// with the rejection, so the caller still sees the delay.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_rate_limit_past_the_round_timeout_keeps_its_delay() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    let delay = Duration::from_secs(3);
    let api = FaultyApi::new(&alix, group_message, [Fault::RateLimit(delay)]);
    let limited = limited_group(&alix, &group.group_id, api.clone(), Duration::from_secs(1)).await;
    limited.send_message_optimistic(b"hello", SendMessageOpts::default())?;
    let intent = intents_in(&group, &UNRESOLVED, IntentKind::SendMessage).remove(0);
    let error = limited
        .sync_until_intent_resolved(intent.id)
        .await
        .unwrap_err();
    assert_eq!(error.rate_limit_delay(), Some(delay));
    assert_eq!(api.requests.lock().len(), 1);
}

/// A later read error in the same round does not hide the publish's delay.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_read_error_after_a_rate_limited_publish_keeps_the_delay() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    let delay = Duration::from_secs(300);
    let api = FaultyApi::new(
        &alix,
        group_message,
        [
            Fault::RateLimit(delay),
            Fault::RateLimit(delay),
            Fault::RateLimit(delay),
        ],
    );
    api.fail_newest
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let limited = faulty_group(&alix, &group.group_id, api.clone()).await;
    limited.send_message_optimistic(b"hello", SendMessageOpts::default())?;
    let intent = intents_in(&group, &UNRESOLVED, IntentKind::SendMessage).remove(0);
    let error = limited
        .sync_until_intent_resolved(intent.id)
        .await
        .unwrap_err();
    assert_eq!(api.requests.lock().len(), 1);
    assert_eq!(error.rate_limit_delay(), Some(delay));
}

/// A sync whose target capture is refused for a short delay waits it out
/// inside its retry budget and succeeds, instead of failing its barrier.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_sync_waits_out_a_short_rate_limit_on_its_capture() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    let api = FaultyApi::new(&alix, group_message, []);
    let limited = limited_group(&alix, &group.group_id, api.clone(), Duration::from_secs(10)).await;
    *api.rate_limit_newest.lock() = Some(Duration::from_secs(1));
    let clear = api.rate_limit_newest.clone();
    let _cleared = xmtp_common::spawn(None, async move {
        xmtp_common::time::sleep(Duration::from_secs(1)).await;
        *clear.lock() = None;
    });
    let started = xmtp_common::time::Instant::now();
    let result = limited.sync_with_conn().await;
    assert!(result.is_ok(), "{:?}", result.err().map(|s| s.to_string()));
    assert!(started.elapsed() < Duration::from_secs(3));
}

/// When the delay ends within the call, the loop waits it out and sends the
/// same saved bytes again.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_rate_limited_round_resends_the_same_bytes_after_the_delay() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    let delay = Duration::from_millis(300);
    // The inner retry spends its five retries, so the loop sees the delay.
    let api = FaultyApi::new(&alix, group_message, [Fault::RateLimit(delay); 6]);
    let limited = faulty_group(&alix, &group.group_id, api.clone()).await;
    limited.send_message_optimistic(b"hello", SendMessageOpts::default())?;
    let intent = intents_in(&group, &UNRESOLVED, IntentKind::SendMessage).remove(0);
    limited.sync_until_intent_resolved(intent.id).await?;
    let requests = api.requests.lock().clone();
    let times = api.request_times.lock().clone();
    // Six refusals and the accepted send. Whether the inner retry or the
    // loop resends, no resend comes inside the delay, and each carries the
    // same saved bytes.
    assert!(requests.len() >= 7);
    for i in 1..7 {
        assert!(
            times[i] - times[i - 1] >= delay,
            "resend {i} inside the delay"
        );
        assert_eq!(requests[i], requests[0]);
    }
}

/// A target capture refused with a delay that fits the retry budget but not
/// the barrier's deadline reports the delay, not a pending target.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_rate_limited_target_capture_reports_its_delay() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    let delay = Duration::from_secs(90);
    let api = FaultyApi::new(&alix, group_message, []);
    *api.rate_limit_newest.lock() = Some(delay);
    let limited = faulty_group(&alix, &group.group_id, api.clone()).await;
    let topic = xmtp_proto::types::Topic::new_group_message(group.group_id);
    let started = xmtp_common::time::Instant::now();
    let error = crate::subscriptions::barrier::receive_through_current_until(
        &limited.context,
        vec![topic],
        started + Duration::from_secs(1),
    )
    .await
    .unwrap_err();
    assert_eq!(error.rate_limit_delay(), Some(delay));
    assert!(started.elapsed() < Duration::from_secs(10));
}

/// A target published before a later intent is refused is still confirmed:
/// the refusal does not end the round before the target's receipt.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_refused_later_intent_does_not_stop_confirming_the_target() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let api = FaultyApi::new(
        &alix,
        group_message,
        [Fault::Pass, Fault::RateLimit(Duration::from_secs(300))],
    );
    let limited = faulty_group(&alix, &group.group_id, api.clone()).await;
    limited.send_message_optimistic(b"first", SendMessageOpts::default())?;
    limited.send_message_optimistic(b"second", SendMessageOpts::default())?;
    let first = intents_in(&group, &UNRESOLVED, IntentKind::SendMessage)
        .into_iter()
        .min_by_key(|intent| intent.id)?;
    limited.sync_until_intent_resolved(first.id).await?;
    assert_eq!(api.requests.lock().len(), 2);
}

/// A commit whose Welcomes are refused for longer than the call reports it is
/// published but unconfirmed, with the Welcomes' delay.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_published_commit_keeps_its_welcomes_rate_limit_delay() {
    tester!(alix, disable_workers);
    tester!(bo);
    let group = alix.create_group(None, None)?;
    let delay = Duration::from_secs(300);
    let api = FaultyApi::new(&alix, welcome, [Fault::RateLimit(delay)]);
    let limited = faulty_group(&alix, &group.group_id, api.clone()).await;
    let error = limited
        .add_members(&[bo.inbox_id().to_string()])
        .await
        .unwrap_err();
    assert!(
        matches!(error, GroupError::PublishedButUnconfirmed { .. }),
        "{error:?}"
    );
    assert_eq!(error.rate_limit_delay(), Some(delay));
}

/// Syncing every group keeps a publish's delay past the deadline, as one
/// group's sync does, instead of reporting only a timeout.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn syncing_all_groups_keeps_a_rate_limit_delay() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    let delay = Duration::from_secs(3);
    let api = FaultyApi::new(&alix, group_message, [Fault::RateLimit(delay)]);
    let limited = limited_group(&alix, &group.group_id, api.clone(), Duration::from_secs(1)).await;
    limited.send_message_optimistic(b"hello", SendMessageOpts::default())?;
    let error = crate::groups::welcome_sync::WelcomeService::new(limited.context.clone())
        .sync_all_groups(vec![limited])
        .await
        .unwrap_err();
    assert_eq!(error.rate_limit_delay(), Some(delay));
}

/// A published-but-unconfirmed result reports the longer of its own delay and
/// the delay of the rate limit that stopped its confirmation.
// verifies: RATE-041
#[xmtp_common::test]
fn published_but_unconfirmed_reports_the_longer_delay() {
    use crate::subscriptions::barrier::{BarrierCause, BarrierError, BarrierFailure, BarrierTopic};
    use crate::subscriptions::incoming::IncomingError;
    use xmtp_common::Generate;
    let refused = xmtp_api::ApiError::Api(xmtp_proto::api::NetworkError::new(
        xmtp_api_grpc::error::GrpcError::Status(xmtp_api_grpc::error::rate_limited_status(
            "request",
            Duration::from_secs(60),
        )),
    ));
    let cause = BarrierError::Incomplete {
        reason: BarrierFailure::Deadline,
        unfinished: vec![BarrierTopic {
            topic: xmtp_proto::types::Topic::new_group_message(
                xmtp_proto::types::GroupId::generate(),
            ),
            scope_generation: None,
            target: None,
            received: xmtp_proto::types::Cursor(0),
            processed: xmtp_proto::types::Cursor(0),
            unresolved_welcomes: Vec::new(),
            inactive: false,
            cause: Some(BarrierCause::Receiver(std::sync::Arc::new(
                IncomingError::Store(crate::mls_store::MlsStoreError::Api(refused)),
            ))),
        }],
    };
    let error = GroupError::PublishedButUnconfirmed {
        intent_id: 1,
        cause: Some(Box::new(cause)),
        rate_limit_delay: Some(Duration::from_secs(5)),
    };
    assert_eq!(error.rate_limit_delay(), Some(Duration::from_secs(60)));
}
