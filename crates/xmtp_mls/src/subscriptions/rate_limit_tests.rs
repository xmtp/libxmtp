//! A message stream against a backend that charges `stream_rent`, with the
//! backend's limiter on a clock the test sets. The test moves the clock to
//! end a rent period; the backend's keepalive turns each session's loop
//! often enough to see it.

use crate::{
    builder::ClientBuilder, context::XmtpSharedContext, groups::send_message_opts::SendMessageOpts,
    tester, utils::test::backend::EphemeralBackend,
};
use futures::{Stream, StreamExt};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use xmtp_api_backend::{AuthCallback, Credential, MIN_STABLE_UPTIME, RECONNECT_INITIAL_DELAY};
use xmtp_backend::{
    config::{CallerKey, Category},
    test_support::{ManualClock, SpendRecord},
};
use xmtp_common::{BoxDynError, time::Duration};
use xmtp_db::group_message::{GroupMessageKind, StoredGroupMessage};

const SECOND: u64 = 1_000_000_000;
const ALIX_KEY: &str = "alix-api-key-0123456789abcdefghijklmnopqrstu";
const BO_SETUP_KEY: &str = "bo-setup-api-key-0123456789abcdefghijklmnopq";
const BO_KEY: &str = "bo-api-key-0123456789abcdefghijklmnopqrstuvw";

/// Sends one API key, so each client is its own caller.
struct ApiKey(&'static str);
#[xmtp_common::async_trait]
impl AuthCallback for ApiKey {
    async fn on_auth_required(&self) -> Result<Credential, BoxDynError> {
        Ok(Credential::new(
            None,
            format!("Bearer {}", self.0).parse()?,
            i64::MAX,
        ))
    }
}

/// Bo's credential: a setup key until `streaming` is set, then the key the
/// test limits. A setup credential expires within a second, so the client
/// asks again after the switch.
#[derive(Default)]
struct BoKeys {
    streaming: AtomicBool,
}
#[xmtp_common::async_trait]
impl AuthCallback for BoKeys {
    async fn on_auth_required(&self) -> Result<Credential, BoxDynError> {
        let (key, expires) = if self.streaming.load(Ordering::SeqCst) {
            (BO_KEY, i64::MAX)
        } else {
            (BO_SETUP_KEY, xmtp_common::time::now_secs() + 1)
        };
        Ok(Credential::new(
            None,
            format!("Bearer {key}").parse()?,
            expires,
        ))
    }
}

/// The `stream_rent` spends the backend decided for bo after its first
/// `from` spends of any kind, in order.
fn rents(backend: &EphemeralBackend, from: usize) -> Vec<SpendRecord> {
    let bo = CallerKey::ApiKey("bo".to_owned());
    backend.rate_limit_spends()[from..]
        .iter()
        .filter(|spend| spend.category == Category::StreamRent && spend.caller == bo)
        .cloned()
        .collect()
}

/// The `index`th denied rent spend after `from`, once the backend decided it.
async fn denial(backend: &EphemeralBackend, from: usize, index: usize) -> SpendRecord {
    xmtp_common::wait_for_some(|| async {
        rents(backend, from)
            .into_iter()
            .filter(|spend| spend.retry_after.is_some())
            .nth(index)
    })
    .await
    .expect("the backend denied a rent spend")
}

/// The first rent spend after `denied`: the replacement stream's open.
async fn reopen(backend: &EphemeralBackend, from: usize, denied: &SpendRecord) -> SpendRecord {
    xmtp_common::wait_for_some(|| async {
        rents(backend, from)
            .into_iter()
            .find(|spend| spend.at > denied.at)
    })
    .await
    .expect("the client opened a replacement stream")
}

/// The text of the next application message `stream` yields, skipping
/// membership changes.
async fn next_text<S, E>(stream: &mut S) -> String
where
    S: Stream<Item = Result<StoredGroupMessage, E>> + Unpin,
    E: std::fmt::Debug,
{
    loop {
        let message = xmtp_common::time::timeout(Duration::from_secs(30), stream.next())
            .await
            .expect("the stream yields a message")
            .expect("the stream stays open")
            .expect("the stream yields messages, not errors");
        if message.kind == GroupMessageKind::Application {
            return String::from_utf8_lossy(&message.decrypted_message_bytes).into_owned();
        }
    }
}

/// A rent denial ends the client's stream. The client opens a replacement no
/// earlier than the rejection's delay, without resetting its reconnect
/// backoff, and the replacement delivers every message sent while the stream
/// was down.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_stream_ended_for_its_rent_reopens_after_the_delay_and_loses_nothing() {
    let clock = ManualClock::new();
    // A stream with topics pays 2 units each period. 2 units per 65 s admit
    // the first period and deny the renewal at 60 s until 5 s later.
    let backend = EphemeralBackend::start_with_rate_limit_clock(
        &format!(
            "[auth]\nenabled = true\n\
             [auth.api_keys]\nalix = '{ALIX_KEY}'\nbo-setup = '{BO_SETUP_KEY}'\nbo = '{BO_KEY}'\n\
             [streams]\nkeepalive_interval_ms = 250\n\
             [rate_limits]\nmax_memory_mib = 1\n\
             [[rate_limits.overrides]]\napi_key = 'bo'\n\
             [rate_limits.overrides.limits]\n\
             stream_rent = [{{ count = 2, window_seconds = 65 }}]\n"
        ),
        clock.clone(),
    )
    .await?;
    tester!(alix, backend: &backend, auth: Arc::new(ApiKey(ALIX_KEY)), disable_workers);
    let bo_keys = Arc::new(BoKeys::default());
    tester!(bo, backend: &backend, auth: bo_keys.clone(), disable_workers);
    let group = alix.create_group(None, None)?;
    group.add_members(&[bo.inbox_id()]).await?;
    // Joining opens short-lived streams of its own, so bo joins under the
    // setup key. The message stream is then the only stream the limited key
    // ever pays for.
    bo.sync_welcomes().await?;
    bo.group(&group.group_id)?.sync().await?;
    bo_keys.streaming.store(true, Ordering::SeqCst);
    // A failed receiver waits its 1 s fallback before it asks the transport
    // again, which would hide the transport's own reconnect backoff.
    let mut policy = bo.context.incoming_runtime().policy().clone();
    policy.receiver_fallback_interval = Duration::from_millis(10);
    let bo = ClientBuilder::from_client(bo.client.clone())
        .stream_policy(policy)
        .with_disable_workers(true)
        .with_allow_offline(Some(true))
        .build()
        .await?;
    // The setup credential expires, and nothing holds the joining streams'
    // wire.
    xmtp_common::time::sleep(Duration::from_secs(2)).await;
    let start = 1000 * SECOND;
    clock.set(start);
    let from = backend.rate_limit_spends().len();
    let mut stream = Box::pin(bo.stream_all_messages(None, None).await?);
    group
        .send_message(b"before", SendMessageOpts::default())
        .await?;
    assert_eq!(next_text(&mut stream).await, "before");
    assert_eq!(
        rents(&backend, from)
            .iter()
            .map(|spend| (spend.cost, spend.retry_after))
            .collect::<Vec<_>>(),
        [(1, None), (1, None)],
        "one stream opened and registered its topic"
    );

    // The first period ends, and its renewal is refused for 5 s.
    clock.set(start + 60 * SECOND);
    let denied = denial(&backend, from, 0).await;
    assert_eq!(denied.retry_after, Some(Duration::from_secs(5)));
    clock.set(start + 65 * SECOND);
    for text in ["one", "two", "three"] {
        group
            .send_message(text.as_bytes(), SendMessageOpts::default())
            .await?;
    }
    let sent = std::time::Instant::now();
    let reopened = reopen(&backend, from, &denied).await;
    assert!(
        reopened.at.duration_since(denied.at) >= Duration::from_secs(5),
        "the stream reopened {:?} after a 5 s rent delay",
        reopened.at.duration_since(denied.at)
    );
    assert!(
        sent < reopened.at,
        "the messages were sent while it was down"
    );
    for text in ["one", "two", "three"] {
        assert_eq!(next_text(&mut stream).await, text);
    }

    // After `MIN_STABLE_UPTIME` the client counts the stream as healthy and
    // would reset its backoff to the minimum, but not after a rate-limit end.
    // The next renewal is refused 50 ms short of fitting, so the doubled
    // backoff decides when the replacement opens.
    xmtp_common::time::sleep(MIN_STABLE_UPTIME + Duration::from_secs(1)).await;
    clock.set(start + 130 * SECOND - 50_000_000);
    let denied = denial(&backend, from, 1).await;
    assert_eq!(denied.retry_after, Some(Duration::from_millis(50)));
    clock.set(start + 130 * SECOND);
    let reopened = reopen(&backend, from, &denied).await;
    // Two rate-limit ends doubled the backoff twice. A reset to its minimum
    // would open within twice the minimum.
    assert!(
        reopened.at.duration_since(denied.at) >= RECONNECT_INITIAL_DELAY * 4,
        "the stream reopened {:?} after its end",
        reopened.at.duration_since(denied.at)
    );
    group
        .send_message(b"after", SendMessageOpts::default())
        .await?;
    assert_eq!(next_text(&mut stream).await, "after");
    drop(stream);
    backend.stop().await?;
}
