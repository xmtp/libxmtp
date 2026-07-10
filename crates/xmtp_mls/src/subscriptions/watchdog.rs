//! Stream-liveness floor: a watchdog that detects a silently-dead subscription.
//!
//! A long-lived subscription whose transport wedges open (e.g. an L7 proxy keeps
//! answering HTTP/2 keepalive pings while the backend subscription is gone) delivers
//! neither an error nor a stream close — the consumer simply hangs forever. There is no
//! server-side keepalive on the v3 path today, so the client cannot distinguish a
//! healthy-but-idle stream from a dead one.
//!
//! [`WatchdogStream`] wraps an inner subscription and arms an idle timer that resets on
//! every item. If nothing arrives within the idle timeout (see [`Watchdog::idle_timeout`]),
//! it yields a single [`SubscribeError::StreamStale`] and then terminates. The consume
//! loops treat that error as a signal to tear down and reconnect (resuming from the
//! persisted cursor), turning a silent hang into a recoverable reconnect.
//!
//! Because there is no keepalive yet, the timeout is deliberately long: a healthy dormant
//! stream WILL trip and reconnect periodically. That cost is accepted for the floor and
//! shrinks once a server heartbeat exists (see XIP-83). This combinator is
//! endpoint-agnostic and is reused by later phases.
//!
//! **Opt-in.** The watchdog is *disabled by default* — with no env vars set, a stream never
//! trips and behaves exactly as it did before the watchdog existed. A deployment enables it
//! (and optionally tunes the knobs) via `XMTP_STREAM_WATCHDOG_ENABLED` and friends, captured
//! once at first use into [`WATCHDOG`]; see [`Watchdog`].

use std::{
    future::Future,
    pin::Pin,
    sync::LazyLock,
    task::{Context, Poll},
};

use futures::{
    StreamExt,
    stream::{FusedStream, Stream},
};
use pin_project::pin_project;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use xmtp_common::{
    BoxDynFuture, MaybeSend, StreamHandle,
    time::{Duration, Instant},
};

use super::SubscribeError;

/// The watchdog's timing knobs — read once from the environment at first use — plus the
/// behavior derived from them.
///
/// Every knob has a safe production default; the env vars exist so a deployment with
/// different liveness needs (a forgiving server-side proxy vs. a latency-sensitive mobile
/// client) can tune the floor without a rebuild. Values are captured once via [`WATCHDOG`]
/// so every stream in the process shares one configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Watchdog {
    /// Master switch. Off by default so streams behave exactly as they did before the
    /// watchdog existed (no idle-trip, no reconnect); a deployment that wants the liveness
    /// floor — herald, long-lived agents — opts in with `XMTP_STREAM_WATCHDOG_ENABLED`. Kept
    /// conservative for now; can flip to on-by-default later.
    enabled: bool,
    /// How long a stream may produce nothing before the watchdog considers it dead.
    idle_timeout: Duration,
    /// Minimum spacing between reconnect attempts (the throttle floor). Only bites when an
    /// attempt finishes faster than this; a normal long-idle trip reconnects immediately.
    reconnect_base: Duration,
    /// Random jitter added to the throttle floor, to de-sync clients that are all
    /// tight-looping against the same upstream.
    reconnect_jitter: Duration,
}

impl Watchdog {
    /// Long on purpose: with no server keepalive we cannot tell a dormant-healthy stream
    /// from a dead one, so this also bounds how often healthy idle streams reconnect.
    const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(300);
    const DEFAULT_RECONNECT_BASE: Duration = Duration::from_secs(1);
    const DEFAULT_RECONNECT_JITTER: Duration = Duration::from_millis(1000);

    /// Ceilings for the env-overridable knobs. These exist purely to keep a fat-fingered env
    /// value from overflowing `Duration` arithmetic (`base + jitter` panics on overflow) or
    /// handing `sleep` an absurd deadline. They sit far above any sane configuration, so they
    /// never bite a real deployment — they only catch garbage.
    const MAX_IDLE_TIMEOUT: Duration = Duration::from_secs(86_400); // 1 day
    const MAX_RECONNECT_BASE: Duration = Duration::from_secs(3_600); // 1 hour
    const MAX_RECONNECT_JITTER: Duration = Duration::from_secs(3_600); // 1 hour

    const ENABLED: &'static str = "XMTP_STREAM_WATCHDOG_ENABLED";
    const IDLE_TIMEOUT_SECS: &'static str = "XMTP_STREAM_WATCHDOG_IDLE_TIMEOUT_SECS";
    const RECONNECT_BASE_SECS: &'static str = "XMTP_STREAM_WATCHDOG_RECONNECT_BASE_SECS";
    const RECONNECT_JITTER_MS: &'static str = "XMTP_STREAM_WATCHDOG_RECONNECT_JITTER_MS";

    fn from_env() -> Self {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Build from a key→value lookup. Injectable so the parsing is unit-testable without
    /// mutating process-global env state. Each value is clamped to its `MAX_*` ceiling so a
    /// misconfigured env var can't overflow `Duration` arithmetic and crash the process.
    fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Self {
        let secs = |key| Self::parse_duration(get(key), Duration::from_secs);
        let millis = |key| Self::parse_duration(get(key), Duration::from_millis);
        Self {
            enabled: Self::parse_bool(get(Self::ENABLED)).unwrap_or(false),
            idle_timeout: secs(Self::IDLE_TIMEOUT_SECS)
                .unwrap_or(Self::DEFAULT_IDLE_TIMEOUT)
                .min(Self::MAX_IDLE_TIMEOUT),
            reconnect_base: secs(Self::RECONNECT_BASE_SECS)
                .unwrap_or(Self::DEFAULT_RECONNECT_BASE)
                .min(Self::MAX_RECONNECT_BASE),
            reconnect_jitter: millis(Self::RECONNECT_JITTER_MS)
                .unwrap_or(Self::DEFAULT_RECONNECT_JITTER)
                .min(Self::MAX_RECONNECT_JITTER),
        }
    }

    /// Parse a `u64` env value through `unit` (e.g. [`Duration::from_secs`]); `None` when
    /// unset or unparseable so the caller falls back to its default. An explicit `0` is
    /// honored (e.g. zero jitter), unlike a missing value.
    fn parse_duration(raw: Option<String>, unit: fn(u64) -> Duration) -> Option<Duration> {
        raw.and_then(|v| v.trim().parse::<u64>().ok()).map(unit)
    }

    /// Parse a boolean env value (`1`/`true`/`yes`/`on` vs `0`/`false`/`no`/`off`,
    /// case-insensitive); `None` when unset or unrecognized so the caller picks the default.
    fn parse_bool(raw: Option<String>) -> Option<bool> {
        match raw?.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        }
    }

    /// The idle timeout to apply to a new watchdog stream, or `None` when the watchdog is
    /// disabled — in which case the stream never trips and behaves exactly as it did before
    /// the watchdog existed.
    ///
    /// When enabled this is the configured [`idle_timeout`](Self::idle_timeout) field. Tests
    /// may force a (short) timeout via [`set_test_idle_timeout`] regardless of `enabled`, so
    /// the stale-trip and reconnect paths can be exercised in seconds without setting env.
    fn idle_timeout(&self) -> Option<Duration> {
        #[cfg(test)]
        {
            if let Some(timeout) = test_overrides::idle_timeout() {
                return Some(timeout);
            }
        }
        self.enabled.then_some(self.idle_timeout)
    }

    /// Throttle reconnects so attempts are spaced at least a short floor apart — *without*
    /// adding latency to the common case.
    ///
    /// `attempt_started` is the instant the just-finished connection attempt began. After a
    /// normal trip the stream was alive for the whole idle timeout, so the floor has long
    /// since elapsed and this returns immediately: a couple seconds of backoff on top of a
    /// multi-minute idle would be pointless. It only sleeps when an attempt finished
    /// *quickly* — a stream that errors or trips right after creation, or a fast creation
    /// failure — capping a tight reconnect loop to one attempt per floor. The jitter de-syncs
    /// clients that are all tight-looping against the same upstream.
    async fn reconnect_delay_since(&self, attempt_started: Instant) {
        // `saturating_add` so the sum can never overflow `Duration` and panic, even though
        // the clamped config already keeps both operands small.
        let floor = self
            .reconnect_base
            .saturating_add(Self::rand_jitter(self.reconnect_jitter));
        let wait = Self::throttle_remaining(floor, attempt_started.elapsed());
        if !wait.is_zero() {
            xmtp_common::time::sleep(wait).await;
        }
    }

    /// How long to wait so the next attempt lands at least `floor` after the last one began.
    /// Zero once `elapsed` reaches `floor` — the common long-idle case, where we reconnect
    /// with no added delay.
    fn throttle_remaining(floor: Duration, elapsed: Duration) -> Duration {
        floor.saturating_sub(elapsed)
    }

    /// Draw a uniform random delay in `[0, max]`.
    ///
    /// Uses real randomness (not wall-clock low bits) so two clients booted in lockstep still
    /// draw independent delays; mirrors the jitter draw in [`xmtp_common::retry`].
    fn rand_jitter(max: Duration) -> Duration {
        if max.is_zero() {
            return Duration::ZERO;
        }
        use rand::RngExt;
        // `max > ZERO` here, so the inclusive range is always valid; the `Err` arm is
        // unreachable but handled without panicking (clippy forbids `unwrap`).
        match rand::distr::Uniform::new_inclusive(Duration::ZERO, max) {
            Ok(distr) => rand::rng().sample(distr),
            Err(_) => Duration::ZERO,
        }
    }
}

/// Process-wide watchdog configuration, read from the environment exactly once.
static WATCHDOG: LazyLock<Watchdog> = LazyLock::new(Watchdog::from_env);

#[cfg(test)]
pub(crate) use test_overrides::set_idle_timeout as set_test_idle_timeout;

/// Test-only knobs for the watchdog. The override is thread-local, so it only reaches a
/// spawned stream task when that task runs on the setting thread — i.e. under a
/// current-thread runtime such as `traced_test!`.
#[cfg(test)]
mod test_overrides {
    use super::Duration;
    use std::cell::Cell;

    thread_local! {
        static IDLE_TIMEOUT: Cell<Option<Duration>> = const { Cell::new(None) };
    }

    pub(crate) fn idle_timeout() -> Option<Duration> {
        IDLE_TIMEOUT.with(Cell::get)
    }

    /// Override the watchdog idle timeout on the current thread (`None` restores the
    /// production default).
    pub(crate) fn set_idle_timeout(timeout: Option<Duration>) {
        IDLE_TIMEOUT.with(|t| t.set(timeout));
    }
}

/// A factory for the idle-deadline future. Expressed as a trait with a blanket impl —
/// rather than two `cfg`-gated `Box<dyn Fn .. + Send>` aliases — so the `Send`-or-not split
/// lives entirely in [`MaybeSend`] (which is what it's for: `Send` on native, where stream
/// tasks spawn on a multi-thread runtime, and unconstrained on wasm). A factory shape rather
/// than a fixed `Duration` lets tests inject a clock they drive by hand, keeping the state
/// machine synchronously testable.
trait IdleTimer: MaybeSend {
    /// Build a fresh idle-deadline future.
    fn arm(&self) -> BoxDynFuture<'static, ()>;
}

impl<F> IdleTimer for F
where
    F: Fn() -> BoxDynFuture<'static, ()> + MaybeSend,
{
    fn arm(&self) -> BoxDynFuture<'static, ()> {
        self()
    }
}

#[pin_project]
pub(crate) struct WatchdogStream<S> {
    #[pin]
    inner: S,
    /// Constructs a fresh idle-deadline future each time the stream goes idle. `None` when the
    /// watchdog is disabled: the timer is never armed, so the wrapper is a pure passthrough
    /// that never trips — identical to consuming the inner stream directly.
    new_timer: Option<Box<dyn IdleTimer>>,
    /// Armed lazily while the inner stream is idle; dropped (reset) on every item so the
    /// deadline is always measured from the most recent activity.
    timer: Option<BoxDynFuture<'static, ()>>,
    /// Once tripped, the stream is finished and must not be polled again.
    tripped: bool,
}

impl<S> WatchdogStream<S> {
    /// Wrap `inner` with a real wall-clock idle timer of `idle_timeout`, or — when
    /// `idle_timeout` is `None` (watchdog disabled) — as a passthrough that never trips.
    pub(crate) fn new(inner: S, idle_timeout: Option<Duration>) -> Self {
        let new_timer = idle_timeout.map(|idle_timeout| -> Box<dyn IdleTimer> {
            Box::new(move || {
                Box::pin(xmtp_common::time::sleep(idle_timeout)) as BoxDynFuture<'static, ()>
            })
        });
        Self {
            inner,
            new_timer,
            timer: None,
            tripped: false,
        }
    }

    /// Wrap `inner` with a caller-supplied idle-timer factory. Tests use this to drive the
    /// deadline deterministically; production goes through [`WatchdogStream::new`].
    #[cfg(test)]
    fn with_timer(inner: S, new_timer: Box<dyn IdleTimer>) -> Self {
        Self {
            inner,
            new_timer: Some(new_timer),
            timer: None,
            tripped: false,
        }
    }
}

impl<S, T> Stream for WatchdogStream<S>
where
    S: Stream<Item = Result<T, SubscribeError>>,
{
    type Item = Result<T, SubscribeError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();
        if *this.tripped {
            return Poll::Ready(None);
        }

        match this.inner.poll_next(cx) {
            // Activity: disarm the idle timer and pass the item through.
            Poll::Ready(Some(item)) => {
                *this.timer = None;
                Poll::Ready(Some(item))
            }
            // The inner stream ended on its own; nothing left to watch. Mark tripped so we
            // never poll the inner stream again after it returned `None` (fused behavior,
            // per the `Stream` contract).
            Poll::Ready(None) => {
                *this.tripped = true;
                Poll::Ready(None)
            }
            // Idle: arm the timer if needed, then check whether we have gone stale.
            Poll::Pending => {
                // Disabled watchdog: no factory, so never arm and never trip — just relay the
                // inner stream's `Pending`, exactly as if the stream were consumed unwrapped.
                let Some(new_timer) = this.new_timer.as_ref() else {
                    return Poll::Pending;
                };
                if this.timer.is_none() {
                    *this.timer = Some(new_timer.arm());
                }
                // `BoxDynFuture` is `Pin<Box<..>>`, which is `Unpin`, so polling through
                // the projected `&mut` is sound.
                let timer = this.timer.as_mut().expect("armed above");
                match timer.as_mut().poll(cx) {
                    Poll::Ready(()) => {
                        tracing::debug!(
                            "stream watchdog tripped: no activity within idle timeout, \
                             terminating stream to force reconnect"
                        );
                        *this.tripped = true;
                        *this.timer = None;
                        Poll::Ready(Some(Err(SubscribeError::StreamStale)))
                    }
                    Poll::Pending => Poll::Pending,
                }
            }
        }
    }
}

impl<S, T> FusedStream for WatchdogStream<S>
where
    S: Stream<Item = Result<T, SubscribeError>>,
{
    /// Terminated once tripped — by either a stale trip or the inner stream ending. After
    /// that, `poll_next` only ever returns `None`.
    fn is_terminated(&self) -> bool {
        self.tripped
    }
}

/// Spawn a self-healing subscription: wrap each underlying stream in a [`WatchdogStream`],
/// forward every item to `callback`, and transparently re-subscribe when the watchdog trips
/// on a silently-dead stream — resuming from the persisted cursor.
///
/// `subscribe` builds a fresh underlying stream on each call (it owns its inputs and clones
/// per call, so the returned stream is `'static`). The first call establishes the stream
/// before readiness is signaled: a failure there is a *startup* failure and is propagated to
/// the caller. Once the stream has been live, a creation failure is a transient hiccup and is
/// retried with the reconnect throttle instead of terminating the subscription.
///
/// On a stale trip the next subscription is established *before* the throttle wait and while
/// the stale stream is still in scope — so the new stream (and, for the conversation stream,
/// its `LocalEvents` broadcast receiver) is already buffering during the wait, and events
/// arriving mid-reconnect are not dropped. `on_close` runs exactly once when the loop ends
/// (clean end, cancellation, or startup error).
///
/// This is the single implementation behind `stream_messages_with_callback`,
/// `stream_conversations_with_callback`, and `stream_all_messages_with_callback`; keeping it
/// in one place is what stops those three from drifting apart.
pub(crate) fn spawn_watchdog_stream<T, S, Fut, Sub, Cb, Close>(
    cancel: CancellationToken,
    label: &'static str,
    mut subscribe: Sub,
    mut callback: Cb,
    on_close: Close,
) -> impl StreamHandle<StreamOutput = Result<(), SubscribeError>>
where
    T: 'static,
    S: Stream<Item = Result<T, SubscribeError>> + MaybeSend + 'static,
    Fut: Future<Output = Result<S, SubscribeError>> + MaybeSend + 'static,
    Sub: FnMut() -> Fut + MaybeSend + 'static,
    Cb: FnMut(Result<T, SubscribeError>) + MaybeSend + 'static,
    Close: FnOnce() + MaybeSend + 'static,
{
    let (tx, rx) = oneshot::channel();
    xmtp_common::spawn(Some(rx), async move {
        tracing::debug!(stream = label, "starting watchdog stream");

        // Initial subscription. A failure here is a startup failure: propagate it so the
        // caller's stream setup errors out rather than closing as a silent `Ok`.
        let mut attempt_started = Instant::now();
        let mut current = match subscribe().await {
            Ok(stream) => stream,
            Err(e) => {
                tracing::warn!(stream = label, "failed to create stream: {e}");
                on_close();
                return Err(e);
            }
        };
        let _ = tx.send(());

        let result = 'reconnect: loop {
            // Consume the current subscription under the watchdog until it goes stale
            // (silent), ends cleanly, or we're cancelled.
            let watched = WatchdogStream::new(current, WATCHDOG.idle_timeout());
            futures::pin_mut!(watched);
            let mut stale = false;
            let cancelled = loop {
                tokio::select! {
                    _ = cancel.cancelled() => break true,
                    next = watched.next() => match next {
                        Some(item) => {
                            if matches!(&item, Err(SubscribeError::StreamStale)) {
                                stale = true;
                                break false;
                            }
                            callback(item);
                        }
                        None => break false,
                    }
                }
            };
            // Reconnect only on a watchdog stale-trip; a clean end or cancellation ends it.
            if cancelled || !stale {
                break 'reconnect Ok(());
            }
            tracing::debug!(stream = label, "stream went stale; reconnecting");

            // Re-subscribe *before* the throttle, while the stale `watched` is still in
            // scope, so the new subscription is already buffering during the wait. A
            // transient creation failure is retried with the throttle, not fatal.
            let next = loop {
                match subscribe().await {
                    Ok(stream) => break stream,
                    Err(e) => {
                        tracing::warn!(
                            stream = label,
                            "failed to recreate stream, will retry: {e}"
                        );
                        tokio::select! {
                            _ = cancel.cancelled() => break 'reconnect Ok(()),
                            _ = WATCHDOG.reconnect_delay_since(attempt_started) => {}
                        }
                    }
                }
            };
            // Throttle: never resubscribe faster than the floor. A long-idle trip waits ~0;
            // only a tight loop is paced. `next` buffers during the wait.
            tokio::select! {
                _ = cancel.cancelled() => break 'reconnect Ok(()),
                _ = WATCHDOG.reconnect_delay_since(attempt_started) => {}
            }
            attempt_started = Instant::now();
            current = next;
        };
        tracing::debug!(stream = label, "watchdog stream ended, dropping stream");
        on_close();
        result
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::{StreamExt, stream};
    use futures_test::task::noop_context;
    use proptest::prelude::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    fn ok(n: u8) -> Result<u8, SubscribeError> {
        Ok(n)
    }

    /// An idle-timer factory the test fires by hand: pending until `fired` is set, then
    /// ready forever. Lets the state machine be polled synchronously with no real clock.
    fn manual_timer(fired: Arc<AtomicBool>) -> Box<dyn IdleTimer> {
        Box::new(move || {
            let fired = fired.clone();
            Box::pin(std::future::poll_fn(move |_| {
                if fired.load(Ordering::Relaxed) {
                    Poll::Ready(())
                } else {
                    Poll::Pending
                }
            })) as BoxDynFuture<'static, ()>
        })
    }

    /// Items pass through and reset the timer; the trip happens exactly once, only once
    /// idle, and the stream is fused afterward. Fully synchronous — no runtime, no clock.
    #[test]
    fn passes_items_then_trips_once_when_idle() {
        let mut cx = noop_context();
        let fired = Arc::new(AtomicBool::new(false));
        let inner = stream::iter(vec![ok(1), ok(2)]).chain(stream::pending());
        let watchdog = WatchdogStream::with_timer(inner, manual_timer(fired.clone()));
        futures::pin_mut!(watchdog);

        assert!(matches!(
            watchdog.as_mut().poll_next(&mut cx),
            Poll::Ready(Some(Ok(1)))
        ));
        assert!(matches!(
            watchdog.as_mut().poll_next(&mut cx),
            Poll::Ready(Some(Ok(2)))
        ));
        // Idle, but the timer hasn't fired: armed and waiting, not tripped.
        assert!(watchdog.as_mut().poll_next(&mut cx).is_pending());

        fired.store(true, Ordering::Relaxed);
        assert!(matches!(
            watchdog.as_mut().poll_next(&mut cx),
            Poll::Ready(Some(Err(SubscribeError::StreamStale)))
        ));
        // Tripped: terminated and `None` forever after.
        assert!(matches!(
            watchdog.as_mut().poll_next(&mut cx),
            Poll::Ready(None)
        ));
        assert!(watchdog.is_terminated());
    }

    /// A stream that ends on its own is passed through cleanly — no spurious trip — and is
    /// fused afterward even though the timer never fired.
    #[test]
    fn clean_end_is_not_a_trip() {
        let mut cx = noop_context();
        let fired = Arc::new(AtomicBool::new(false));
        let inner = stream::iter(vec![ok(1)]);
        let watchdog = WatchdogStream::with_timer(inner, manual_timer(fired));
        futures::pin_mut!(watchdog);

        assert!(matches!(
            watchdog.as_mut().poll_next(&mut cx),
            Poll::Ready(Some(Ok(1)))
        ));
        assert!(
            matches!(watchdog.as_mut().poll_next(&mut cx), Poll::Ready(None)),
            "clean end must yield None, never StreamStale"
        );
        assert!(watchdog.is_terminated());
    }

    /// While idle, the watchdog stays `Pending` until the timer actually fires — it never
    /// trips early.
    #[test]
    fn does_not_trip_until_timer_fires() {
        let mut cx = noop_context();
        let fired = Arc::new(AtomicBool::new(false));
        let inner = stream::pending::<Result<u8, SubscribeError>>();
        let watchdog = WatchdogStream::with_timer(inner, manual_timer(fired.clone()));
        futures::pin_mut!(watchdog);

        // Poll repeatedly while the timer is unfired: must stay pending every time.
        for _ in 0..5 {
            assert!(watchdog.as_mut().poll_next(&mut cx).is_pending());
        }
        fired.store(true, Ordering::Relaxed);
        assert!(matches!(
            watchdog.as_mut().poll_next(&mut cx),
            Poll::Ready(Some(Err(SubscribeError::StreamStale)))
        ));
    }

    proptest! {
        /// For any sequence of items, the watchdog yields each in order, then — once idle
        /// and the timer fires — exactly one `StreamStale`, then terminates. Synchronous,
        /// so proptest can hammer the state machine with no real time involved.
        #[test]
        fn yields_every_item_then_exactly_one_stale(items in proptest::collection::vec(any::<u8>(), 0..32)) {
            let mut cx = noop_context();
            let fired = Arc::new(AtomicBool::new(false));
            let expected = items.clone();
            let inner = stream::iter(items.into_iter().map(ok)).chain(stream::pending());
            let watchdog = WatchdogStream::with_timer(inner, manual_timer(fired.clone()));
            futures::pin_mut!(watchdog);

            for n in expected {
                prop_assert!(matches!(
                    watchdog.as_mut().poll_next(&mut cx),
                    Poll::Ready(Some(Ok(m))) if m == n
                ));
            }
            // Inner is now idle (pending); fire the deadline and expect a single stale trip.
            fired.store(true, Ordering::Relaxed);
            prop_assert!(matches!(
                watchdog.as_mut().poll_next(&mut cx),
                Poll::Ready(Some(Err(SubscribeError::StreamStale)))
            ));
            prop_assert!(matches!(watchdog.as_mut().poll_next(&mut cx), Poll::Ready(None)));
            prop_assert!(watchdog.is_terminated());
        }
    }

    /// Smoke test of the production constructor: [`WatchdogStream::new`] wires a real
    /// wall-clock timer, so an idle stream trips after the real timeout. The synchronous
    /// tests above cover the state machine; this guards the real-clock wiring.
    #[xmtp_common::test(unwrap_try = true)]
    async fn new_uses_a_real_timer() {
        let inner = stream::pending::<Result<u8, SubscribeError>>();
        let watchdog = WatchdogStream::new(inner, Some(Duration::from_millis(50)));
        futures::pin_mut!(watchdog);

        let item = watchdog.next().await;
        assert!(
            matches!(item, Some(Err(SubscribeError::StreamStale))),
            "expected StreamStale, got {item:?}"
        );
        assert!(watchdog.next().await.is_none());
    }

    /// A disabled watchdog (`None` timeout) never trips: an idle inner stream stays pending
    /// instead of yielding `StreamStale`, so the stream behaves as if unwrapped.
    #[xmtp_common::test(unwrap_try = true)]
    async fn disabled_watchdog_never_trips() {
        let inner = stream::pending::<Result<u8, SubscribeError>>();
        let watchdog = WatchdogStream::new(inner, None);
        futures::pin_mut!(watchdog);

        // Well past any real timeout: a disabled watchdog must still be waiting, never stale.
        let polled = xmtp_common::time::timeout(Duration::from_millis(100), watchdog.next()).await;
        assert!(polled.is_err(), "disabled watchdog should never trip");
    }

    /// Env parsing: present + valid values win, an explicit `0` is honored, and
    /// missing/garbage falls back to the defaults.
    #[test]
    fn config_reads_env_with_defaults() {
        let custom = Watchdog::from_lookup(|key| match key {
            Watchdog::IDLE_TIMEOUT_SECS => Some("42".into()),
            Watchdog::RECONNECT_JITTER_MS => Some("0".into()),
            Watchdog::RECONNECT_BASE_SECS => Some("not-a-number".into()),
            _ => None,
        });
        assert_eq!(custom.idle_timeout, Duration::from_secs(42));
        assert_eq!(custom.reconnect_jitter, Duration::ZERO);
        // Garbage and missing both fall back to defaults.
        assert_eq!(custom.reconnect_base, Watchdog::DEFAULT_RECONNECT_BASE);

        let empty = Watchdog::from_lookup(|_| None);
        assert_eq!(empty.idle_timeout, Watchdog::DEFAULT_IDLE_TIMEOUT);
        assert_eq!(empty.reconnect_jitter, Watchdog::DEFAULT_RECONNECT_JITTER);
    }

    /// The watchdog is opt-in: disabled by default (no env), enabled only when explicitly
    /// switched on. `idle_timeout()` reflects this — `None` (never trip) unless enabled.
    #[test]
    fn watchdog_is_opt_in() {
        let off = Watchdog::from_lookup(|_| None);
        assert!(!off.enabled, "must default to disabled");
        // Disabled -> `idle_timeout()` is `None`, so the stream never arms a timer / trips.
        assert_eq!(off.idle_timeout(), None);
        // A configured timeout alone doesn't enable it.
        let configured_but_off =
            Watchdog::from_lookup(|key| (key == Watchdog::IDLE_TIMEOUT_SECS).then(|| "30".into()));
        assert!(!configured_but_off.enabled);
        assert_eq!(configured_but_off.idle_timeout(), None);

        let on = Watchdog::from_lookup(|key| (key == Watchdog::ENABLED).then(|| "true".into()));
        assert!(on.enabled);
        assert_eq!(on.idle_timeout(), Some(Watchdog::DEFAULT_IDLE_TIMEOUT));

        // Bool parsing accepts the usual spellings and rejects garbage (-> default off).
        let parse = |s: &str| Watchdog::parse_bool(Some(s.into()));
        assert_eq!(parse("1"), Some(true));
        assert_eq!(parse("ON"), Some(true));
        assert_eq!(parse("off"), Some(false));
        assert_eq!(parse("maybe"), None);
    }

    /// A fat-fingered (or hostile) oversized env value is clamped to the ceiling instead of
    /// flowing through to `Duration` arithmetic, where it could overflow and panic.
    #[test]
    fn config_clamps_oversized_env_values() {
        let huge = Watchdog::from_lookup(|key| match key {
            Watchdog::IDLE_TIMEOUT_SECS => Some(u64::MAX.to_string()),
            Watchdog::RECONNECT_BASE_SECS => Some(u64::MAX.to_string()),
            Watchdog::RECONNECT_JITTER_MS => Some(u64::MAX.to_string()),
            _ => None,
        });
        assert_eq!(huge.idle_timeout, Watchdog::MAX_IDLE_TIMEOUT);
        assert_eq!(huge.reconnect_base, Watchdog::MAX_RECONNECT_BASE);
        assert_eq!(huge.reconnect_jitter, Watchdog::MAX_RECONNECT_JITTER);
        // The clamped pair can't overflow when summed (the panic macroscope flagged).
        let _ = huge.reconnect_base.saturating_add(huge.reconnect_jitter);
    }

    /// Zero jitter draws no randomness and returns exactly zero (degenerate range guard).
    #[test]
    fn rand_jitter_zero_is_zero() {
        assert_eq!(Watchdog::rand_jitter(Duration::ZERO), Duration::ZERO);
    }

    /// Non-zero jitter always lands within `[0, max]`.
    #[test]
    fn rand_jitter_stays_in_bounds() {
        let max = Duration::from_millis(1000);
        for _ in 0..1000 {
            assert!(Watchdog::rand_jitter(max) <= max);
        }
    }

    /// After a long idle (elapsed dwarfs the floor) the reconnect throttle adds no delay.
    #[test]
    fn throttle_is_immediate_after_long_idle() {
        let floor = Duration::from_secs(2);
        assert_eq!(
            Watchdog::throttle_remaining(floor, Duration::from_secs(300)),
            Duration::ZERO
        );
        // Exactly at the floor also waits nothing.
        assert_eq!(Watchdog::throttle_remaining(floor, floor), Duration::ZERO);
    }

    /// A fast attempt (tight loop) waits only the remainder up to the floor.
    #[test]
    fn throttle_caps_a_tight_loop() {
        let floor = Duration::from_secs(2);
        assert_eq!(
            Watchdog::throttle_remaining(floor, Duration::from_millis(50)),
            Duration::from_millis(1950)
        );
    }
}
