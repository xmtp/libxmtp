//! A rate-limited publish is not sent again inside its delay.

use super::faults::{Fault, FaultyApi, faulty_group, group_message};
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
