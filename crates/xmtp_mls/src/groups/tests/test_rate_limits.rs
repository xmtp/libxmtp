//! The client against a backend that enforces per-caller rate limits, so the
//! backend's own rejection details reach the client's parsing and retries.

use crate::{
    groups::send_message_opts::SendMessageOpts,
    utils::{LocalTesterBuilder, TesterBuilder, test::backend::EphemeralBackend},
};
use std::future::Future;
use xmtp_backend::{
    config::Category,
    test_support::{ManualClock, SpendRecord},
};
use xmtp_common::{
    RetryableError,
    time::{Duration, Instant},
};

/// A backend on `clock` that admits one published envelope for each
/// `seconds / count` of the clock's time, after a burst of `count`.
async fn limited(
    count: u32,
    seconds: u32,
    clock: &ManualClock,
) -> Result<EphemeralBackend, xmtp_common::BoxDynError> {
    EphemeralBackend::start_with_rate_limit_clock(
        &format!(
            "[rate_limits]\nmax_memory_mib = 1\n\
             [rate_limits.categories.publish]\n\
             default = [{{ count = {count}, window_seconds = {seconds} }}]\n"
        ),
        clock.clone(),
    )
    .await
}

/// Run `future` while the limiter's clock stays still, except that each
/// refusal moves it on by the refusal's delay. The client waits that delay in
/// real time before it sends the request again, which is then admitted.
/// Which spends are refused depends only on the requests, never on how fast
/// the client runs.
async fn past_each_refusal<T>(
    backend: &EphemeralBackend,
    clock: &ManualClock,
    future: impl Future<Output = T>,
) -> T {
    let mut seen = backend.rate_limit_spends().len();
    let mut future = std::pin::pin!(future);
    loop {
        tokio::select! {
            output = &mut future => return output,
            _ = xmtp_common::time::sleep(Duration::from_millis(5)) => {
                let spends = backend.rate_limit_spends();
                for spend in &spends[seen..] {
                    if let Some(delay) = spend.retry_after {
                        clock.advance(delay.as_nanos() as u64);
                    }
                }
                seen = spends.len();
            }
        }
    }
}

/// What one run of [`traffic`] sent.
struct Sent {
    /// Publish requests the clients sent, a resend of a refused one included.
    publishes: u64,
    /// The length of the backend's spend record before the messages.
    spends_before_messages: usize,
}

/// Register alix and two installations of bo, let alix add bo to a new group,
/// which publishes a commit of several envelopes and then the welcomes to
/// bo's two installations in one request, and send `messages` messages. The clients use `backend`,
/// or the shared local backend when it is `None`. With a `clock`, each step
/// runs [`past_each_refusal`].
async fn traffic(
    backend: Option<&EphemeralBackend>,
    clock: Option<&ManualClock>,
    messages: usize,
) -> Result<Sent, xmtp_common::BoxDynError> {
    macro_rules! step {
        ($future:expr) => {
            match (backend, clock) {
                (Some(backend), Some(clock)) => past_each_refusal(backend, clock, $future).await,
                _ => $future.await,
            }
        };
    }
    let builder = |name: &str| {
        let builder = TesterBuilder::new().with_name(name).disable_workers();
        match backend {
            Some(backend) => builder.backend(backend),
            None => builder,
        }
    };
    let alix = step!(builder("alix").build());
    let bo = step!(builder("bo").build());
    let bo_phone = step!(bo.builder.clone().with_name("bo_phone").build());
    let group = alix.create_group(None, None)?;
    step!(group.add_members(&[bo.inbox_id()]))?;
    let spends_before_messages = backend.map_or(0, |backend| backend.rate_limit_spends().len());
    for _ in 0..messages {
        step!(group.send_message(b"message", SendMessageOpts::default()))?;
    }
    let publishes = [&alix, &bo, &bo_phone]
        .iter()
        .map(|client| client.api_stats().publish.get_count() as u64)
        .sum();
    Ok(Sent {
        publishes,
        spends_before_messages,
    })
}

/// After each denied publish, the next publish to reach the backend is the
/// refused request itself, whole and with the same bytes, sent no earlier
/// than the delay the backend gave. The client is the backend's only active
/// caller and runs no workers, so nothing else publishes in between.
fn assert_resent_whole_after_the_delay(spends: &[SpendRecord]) {
    for (index, denied) in spends.iter().enumerate() {
        let Some(delay) = denied.retry_after else {
            continue;
        };
        let Some(next) = spends.get(index + 1) else {
            continue;
        };
        assert!(
            denied.request.is_some(),
            "a publish spend names its request"
        );
        assert_eq!(
            next.request, denied.request,
            "a refused publish was sent again as another request"
        );
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

/// The backend's `publish` spends, in the order it decided them.
fn publishes(backend: &EphemeralBackend) -> Vec<SpendRecord> {
    backend
        .rate_limit_spends()
        .into_iter()
        .filter(|spend| spend.category == Category::Publish)
        .collect()
}

/// Publishes past the backend's limit, a batch of welcomes and messages
/// alike, reach the backend again as the same request, whole and no earlier
/// than the delay it gave, and then succeed.
// verifies: RATE-041, RATE-040
#[xmtp_common::test(unwrap_try = true)]
async fn publishes_past_the_limit_are_sent_whole_after_the_delay() {
    // A burst of two envelopes, then one for each second of the clock.
    let clock = ManualClock::new();
    let backend = limited(2, 2, &clock).await?;
    let sent = traffic(Some(&backend), Some(&clock), 2).await?;
    let all = backend.rate_limit_spends();
    let spends = publishes(&backend);
    // The clock moves only past refusals, so these refusals do not depend on
    // timing. The commit is larger than the count and waits for full
    // allowance; the welcomes that follow it at that instant do not fit, and
    // nor does the first message after them.
    assert!(
        spends
            .iter()
            .any(|spend| spend.retry_after.is_some() && spend.cost >= 2),
        "a publish of several envelopes was refused"
    );
    assert!(
        all[sent.spends_before_messages..]
            .iter()
            .any(|spend| spend.category == Category::Publish && spend.retry_after.is_some()),
        "a message's publish was refused"
    );
    assert_resent_whole_after_the_delay(&spends);
    assert!(
        spends
            .last()
            .is_some_and(|spend| spend.retry_after.is_none()),
        "the last message was admitted"
    );
    backend.stop().await?;
}

// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_delay_past_the_retry_budget_returns_the_rate_limit_error() {
    // Twenty envelopes, then one more each 36 hours. The clock stays still.
    let clock = ManualClock::new();
    let backend = limited(20, 2_592_000, &clock).await?;
    let alix = TesterBuilder::new()
        .with_name("alix")
        .backend(&backend)
        .disable_workers()
        .build()
        .await;
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

/// The traffic that the limited backend above refuses is admitted at the
/// first attempt by an ephemeral backend with the default test configuration
/// and by the shared local backend, which both run with rate limits off: the
/// clients send the same number of publish requests to each, so neither
/// refused one that the client sent again.
#[xmtp_common::test(unwrap_try = true)]
async fn unlimited_test_backends_admit_the_same_traffic() {
    let backend = EphemeralBackend::start("").await?;
    assert!(!backend.config().rate_limits().enabled);
    let ephemeral = traffic(Some(&backend), None, 2).await?;
    let shared = traffic(None, None, 2).await?;
    assert_eq!(shared.publishes, ephemeral.publishes);
    backend.stop().await?;
}
