//! The client against a backend that enforces per-caller rate limits, so the
//! backend's own rejection details reach the client's parsing and retries.

use crate::{
    groups::send_message_opts::SendMessageOpts, tester, utils::test::backend::EphemeralBackend,
};
use xmtp_backend::{config::Category, test_support::SpendRecord};
use xmtp_common::{
    RetryableError,
    time::{Duration, Instant},
};

/// A backend that admits one published envelope for each `seconds` from
/// every caller, after a burst of `count`.
async fn limited(count: u32, seconds: u32) -> Result<EphemeralBackend, xmtp_common::BoxDynError> {
    EphemeralBackend::start(&format!(
        "[rate_limits]\nmax_memory_mib = 1\n\
         [rate_limits.categories.publish]\n\
         default = [{{ count = {count}, window_seconds = {seconds} }}]\n"
    ))
    .await
}

/// The backend's `publish` spends, in the order it decided them.
fn publishes(backend: &EphemeralBackend) -> Vec<SpendRecord> {
    backend
        .rate_limit_spends()
        .into_iter()
        .filter(|spend| spend.category == Category::Publish)
        .collect()
}

/// After each denied publish, the next publish to reach the backend is the
/// same request, whole, sent no earlier than the delay the backend gave.
/// The client is the backend's only caller and runs no workers, so nothing
/// else publishes in between.
fn assert_resent_whole_after_the_delay(spends: &[SpendRecord]) {
    for (index, denied) in spends.iter().enumerate() {
        let Some(delay) = denied.retry_after else {
            continue;
        };
        let Some(next) = spends.get(index + 1) else {
            continue;
        };
        assert_eq!(
            next.cost, denied.cost,
            "a refused publish was sent again in parts"
        );
        let waited = next.at.duration_since(denied.at);
        assert!(
            waited >= delay,
            "a refused publish was sent again after {waited:?}, inside its {delay:?} delay"
        );
    }
}

/// Publishes past the backend's limit, a batch of welcomes and messages
/// alike, reach the backend again whole and no earlier than the delay it
/// gave, and then succeed.
// verifies: RATE-041, RATE-040
#[xmtp_common::test(unwrap_try = true)]
async fn publishes_past_the_limit_are_sent_whole_after_the_delay() {
    // A burst of two envelopes, then one each second.
    let backend = limited(2, 2).await?;
    tester!(alix, backend: &backend, disable_workers);
    tester!(bo, backend: &backend, disable_workers);
    tester!(bo_phone, from: bo, disable_workers);
    let group = alix.create_group(None, None)?;
    // The commit spends a unit, and the welcomes to bo's two installations
    // then need two more in one request.
    group.add_members(&[bo.inbox_id()]).await?;
    let before = publishes(&backend).len();
    for _ in 0..3 {
        group
            .send_message(b"message", SendMessageOpts::default())
            .await?;
    }
    let spends = publishes(&backend);
    assert!(
        spends[..before]
            .iter()
            .any(|spend| spend.retry_after.is_some() && spend.cost >= 2),
        "a publish of several welcomes was refused"
    );
    assert!(
        spends[before..]
            .iter()
            .any(|spend| spend.retry_after.is_some()),
        "a message's publish was refused"
    );
    assert_resent_whole_after_the_delay(&spends);
    assert!(
        spends
            .last()
            .is_some_and(|spend| spend.retry_after.is_none()),
        "the last message was admitted"
    );
    drop(bo_phone);
    backend.stop().await?;
}

// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_delay_past_the_retry_budget_returns_the_rate_limit_error() {
    // Twenty envelopes, then one more each 36 hours.
    let backend = limited(20, 2_592_000).await?;
    tester!(alix, backend: &backend, disable_workers);
    let group = alix.create_group(None, None)?;
    let mut refused = None;
    for _ in 0..30 {
        let started = Instant::now();
        if let Err(error) = group
            .send_message(b"message", SendMessageOpts::default())
            .await
        {
            refused = Some((error, started.elapsed()));
            break;
        }
    }
    let (error, elapsed) = refused.expect("the publish limit refused a message");
    let delay = error
        .rate_limit_delay()
        .unwrap_or_else(|| panic!("the send failed for its rate: {error:?}"));
    assert!(delay > Duration::from_secs(3600), "{delay:?}");
    assert!(
        elapsed < Duration::from_secs(30),
        "the send waited {elapsed:?} for a delay it could not outlast"
    );
    let spends = publishes(&backend);
    assert_eq!(
        spends
            .iter()
            .filter(|spend| spend.retry_after.is_some())
            .count(),
        1,
        "the refused publish was sent again inside its delay"
    );
    backend.stop().await?;
}

/// The traffic that the limited backend above refuses is admitted whole by an
/// ephemeral backend with the default test configuration and by the shared
/// local backend, which both run with rate limits off.
#[xmtp_common::test(unwrap_try = true)]
async fn unlimited_test_backends_admit_the_same_traffic() {
    let backend = EphemeralBackend::start("").await?;
    tester!(alix, backend: &backend, disable_workers);
    tester!(bo, disable_workers);
    for client in [&alix, &bo] {
        let group = client.create_group(None, None)?;
        for _ in 0..30 {
            group
                .send_message(b"message", SendMessageOpts::default())
                .await?;
        }
    }
    // The default ephemeral backend builds no limiter at all.
    assert!(backend.rate_limit_spends().is_empty());
    backend.stop().await?;
}
