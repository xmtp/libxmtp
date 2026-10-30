//! Time primitives for native and WebAssembly

use crate::{if_native, if_wasm, wasm_or_native};
use std::fmt;

if_native! {
    pub use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
}
if_wasm! {
    pub use web_time::{Duration, Instant, SystemTime, UNIX_EPOCH};
}

#[derive(Debug)]
pub struct Expired;

impl std::error::Error for Expired {
    fn description(&self) -> &str {
        "Timer duration expired"
    }
}

if_native! {
    impl From<tokio::time::error::Elapsed> for Expired {
        fn from(_: tokio::time::error::Elapsed) -> Expired {
            Expired
        }
    }
}

impl fmt::Display for Expired {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        write!(f, "timer duration expired")
    }
}

impl crate::ErrorCode for Expired {
    fn error_code(&self) -> &'static str {
        "Expired"
    }
}

fn duration_since_epoch() -> Duration {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
}

pub fn now_ns() -> i64 {
    duration_since_epoch().as_nanos() as i64
}

pub fn now_ms() -> u64 {
    duration_since_epoch().as_millis() as u64
}
pub fn now_secs() -> i64 {
    duration_since_epoch().as_secs() as i64
}

pub async fn timeout<F>(duration: Duration, future: F) -> Result<F::Output, Expired>
where
    F: std::future::IntoFuture,
{
    wasm_or_native! {
        wasm => {
            use futures::future::Either::*;
            let millis = duration.as_millis().min(u32::MAX as u128) as u32;
            let timeout = gloo_timers::future::TimeoutFuture::new(millis);
            let future = future.into_future();
            futures::pin_mut!(future);
            match futures::future::select(timeout, future).await {
                Left(_) => Err(Expired),
                Right((value, _)) => Ok(value),
            }
        },
        native => {
            tokio::time::timeout(duration, future).await.map_err(Into::into)
        }
    }
}

/// A wait longer than any process lives. An instant or deadline that would
/// overflow saturates to this far ahead instead of panicking.
pub const FAR_FUTURE: Duration = Duration::from_secs(100 * 365 * 24 * 60 * 60);

/// The instant `duration` after `now`, saturating at [`FAR_FUTURE`].
pub fn saturating_after(now: Instant, duration: Duration) -> Instant {
    now.checked_add(duration)
        .unwrap_or_else(|| now + FAR_FUTURE)
}

/// The longest wait one browser timer holds: a longer one fires at once.
const MAX_TIMER: Duration = Duration::from_millis(i32::MAX as u64);

/// Split a wait into pieces one browser timer can hold, in order. gloo panics
/// past `u32::MAX` milliseconds and a timer fires at once past `i32::MAX`.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn timer_pieces(duration: Duration) -> impl Iterator<Item = Duration> {
    let (full, rest) = timer_split(duration);
    (0..full)
        .map(|_| MAX_TIMER)
        .chain((!rest.is_zero() || full == 0).then_some(rest))
}

/// The number of full browser timers in `duration`, and the remainder.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn timer_split(duration: Duration) -> (u128, Duration) {
    let timer = MAX_TIMER.as_nanos();
    let rest = u64::try_from(duration.as_nanos() % timer).expect("a remainder fits one timer");
    (duration.as_nanos() / timer, Duration::from_nanos(rest))
}

/// Waits at least `duration`, as [`Instant`] measures it.
#[doc(hidden)]
pub async fn sleep(duration: Duration) {
    wasm_or_native! {
        native => {tokio::time::sleep(duration).await},
        wasm => {
            // A wait too long for the clock saturates, and so do its timers.
            let now = Instant::now();
            let deadline = saturating_after(now, duration);
            for piece in timer_pieces(deadline.saturating_duration_since(now)) {
                gloo_timers::future::sleep(piece).await;
            }
            // A browser timer drops the part of a wait below 1 ms, and Node
            // can fire one up to 1 ms early, so a rate-limit wait could end
            // just before its delay. Wait out what is left.
            loop {
                let left = deadline.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    break;
                }
                gloo_timers::future::sleep(left.max(Duration::from_millis(1))).await;
            }
        },
    }
}

pub fn interval_stream(
    period: crate::time::Duration,
) -> impl futures::Stream<Item = crate::time::Instant> {
    use futures::StreamExt;
    wasm_or_native! {
        wasm => {gloo_timers::future::IntervalStream::new(period.as_millis().min(u32::MAX as u128) as u32).map(|_| crate::time::Instant::now())},
        native => {tokio_stream::wrappers::IntervalStream::new(tokio::time::interval(period)).map(|t| t.into_std())},
    }
}

/// Draw a random delay in `[0, jitter]`. Mirrors the jitter draw in
/// [`crate::retry`] so the same cross-platform `rand` path is exercised.
///
/// Public so workers that compute their own per-iteration sleeps (rather than
/// driving [`jittered_interval_stream`]) can de-synchronize fleet-wide wakes.
pub fn rand_offset(jitter: crate::time::Duration) -> crate::time::Duration {
    use rand::RngExt;
    if jitter.is_zero() {
        return crate::time::Duration::ZERO;
    }
    let distr = rand::distr::Uniform::new_inclusive(crate::time::Duration::ZERO, jitter).unwrap();
    rand::rng().sample(distr)
}

/// Like [`interval_stream`], but de-synchronizes fleets of clients that boot
/// together:
/// - a one-time startup offset in `[0, jitter]` is awaited before the first
///   tick, so clients booted at the same time don't tick in lockstep;
/// - each subsequent tick adds a fresh random delay in `[0, jitter]`, so they
///   don't re-converge over time.
///
/// `jitter == Duration::ZERO` degenerates to [`interval_stream`] and pulls no
/// randomness — the zero-jitter path is byte-for-byte the old behavior.
pub fn jittered_interval_stream(
    base: crate::time::Duration,
    jitter: crate::time::Duration,
) -> impl futures::Stream<Item = crate::time::Instant> + Unpin {
    use futures::StreamExt;

    // The jittered branch composes `then`/`flat_map` over per-tick sleep
    // futures, which makes the stream `!Unpin`. Workers consume the stream by
    // value in a `while let` loop (like `interval_stream`), so box it to keep
    // the same `Unpin` ergonomics. Native must stay `Send` (workers spawn on a
    // multi-thread runtime); wasm cannot require `Send`.
    let jittered = move || {
        let startup_offset = rand_offset(jitter);
        // Sleep the startup offset once, then on every base-interval tick sleep
        // an additional per-tick jitter before yielding the instant.
        futures::stream::once(async move {
            sleep(startup_offset).await;
        })
        .flat_map(move |_| {
            interval_stream(base).then(move |instant| async move {
                sleep(rand_offset(jitter)).await;
                instant
            })
        })
    };

    wasm_or_native! {
        wasm => {
            if jitter.is_zero() {
                interval_stream(base).boxed_local()
            } else {
                jittered().boxed_local()
            }
        },
        native => {
            if jitter.is_zero() {
                interval_stream(base).boxed()
            } else {
                jittered().boxed()
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ErrorCode;

    /// Every piece fits one browser timer, and the pieces sum to the wait.
    #[xmtp_macro::test]
    fn a_long_wait_splits_into_timer_pieces() {
        for duration in [
            Duration::ZERO,
            MAX_TIMER,
            MAX_TIMER + Duration::from_millis(1),
            Duration::from_secs(60 * 24 * 60 * 60),
        ] {
            let pieces: Vec<_> = timer_pieces(duration).collect();
            assert!(!pieces.is_empty());
            assert!(pieces.iter().all(|piece| *piece <= MAX_TIMER));
            assert_eq!(pieces.iter().sum::<Duration>(), duration);
        }
    }

    /// A wait of more than `u32::MAX` timers still splits into pieces that each
    /// fit one timer.
    #[xmtp_macro::test]
    fn a_wait_past_u32_max_timers_keeps_every_piece_bounded() {
        let extra = Duration::from_millis(5);
        let duration = MAX_TIMER * u32::MAX + MAX_TIMER * 2 + extra;
        assert_eq!(timer_split(duration), (u128::from(u32::MAX) + 2, extra));
        assert!(
            timer_pieces(Duration::MAX)
                .take(3)
                .all(|piece| piece == MAX_TIMER)
        );
    }

    /// A sleep never ends before its duration, including a part below 1 ms,
    /// which a browser timer drops.
    #[xmtp_macro::test]
    async fn a_sleep_lasts_at_least_its_duration() {
        for duration in [Duration::from_micros(700), Duration::from_micros(1_500)] {
            for _ in 0..10 {
                let started = Instant::now();
                sleep(duration).await;
                assert!(
                    started.elapsed() >= duration,
                    "{duration:?} sleep ended early"
                );
            }
        }
    }

    /// A delay no clock can hold saturates instead of panicking.
    #[xmtp_macro::test]
    fn an_unrepresentable_delay_saturates() {
        let now = Instant::now();
        let until = saturating_after(now, Duration::MAX);
        assert!(until >= now + FAR_FUTURE);
        assert_eq!(
            saturating_after(now, Duration::from_secs(5)),
            now + Duration::from_secs(5)
        );
    }

    #[test]
    fn test_expired_error_code() {
        let expired = Expired;
        assert_eq!(expired.error_code(), "Expired");
    }

    #[test]
    fn test_expired_display() {
        let expired = Expired;
        assert_eq!(format!("{}", expired), "timer duration expired");
    }

    #[test]
    fn test_expired_description() {
        use std::error::Error;
        #[allow(deprecated)]
        let desc = Expired.description();
        assert_eq!(desc, "Timer duration expired");
    }

    // Jitter tests rely on tokio's paused virtual clock, native-only.
    #[cfg(not(target_arch = "wasm32"))]
    mod jitter {
        use super::super::jittered_interval_stream;
        use crate::time::Duration;
        use futures::StreamExt;

        // Under `start_paused`, the runtime auto-advances the virtual clock to
        // the next pending timer whenever all tasks are idle. So we measure how
        // far the clock moved (`Instant::now` elapsed) rather than driving it by
        // hand — that sidesteps ordering pitfalls between `advance` and when the
        // stream's sleeps get armed.

        #[tokio::test(start_paused = true)]
        async fn zero_jitter_uses_base_period() {
            let base = Duration::from_secs(10);
            let start = tokio::time::Instant::now();
            let mut s = Box::pin(jittered_interval_stream(base, Duration::ZERO));
            // First tick is immediate (tokio interval fires at t=0, no offset).
            s.next().await.unwrap();
            assert_eq!(start.elapsed(), Duration::ZERO, "first tick is immediate");
            // Second tick lands exactly `base` later — no jitter.
            s.next().await.unwrap();
            assert_eq!(start.elapsed(), base, "second tick after exactly base");
        }

        #[tokio::test(start_paused = true)]
        async fn jitter_keeps_ticks_within_bounds() {
            let base = Duration::from_secs(10);
            let jitter = Duration::from_secs(5);
            let start = tokio::time::Instant::now();
            let mut s = Box::pin(jittered_interval_stream(base, jitter));
            // First yield = startup_offset + first per-tick jitter, each in
            // [0, jitter]; the base interval fires at t=0. So [0, 2*jitter].
            s.next().await.unwrap();
            let first = start.elapsed();
            assert!(
                first <= jitter * 2,
                "first tick within 2*jitter, got {first:?}"
            );
            // The underlying tokio interval anchors tick N at N*base, so its own
            // schedule self-corrects for prior per-tick jitter. Yield N lands at
            // startup_offset + N*base + jitter_N. The gap between consecutive
            // yields is therefore base + (jitter_next - jitter_prev), bounded by
            // [base - jitter, base + jitter].
            s.next().await.unwrap();
            let gap = start.elapsed() - first;
            assert!(
                gap >= base.saturating_sub(jitter),
                "gap >= base - jitter, got {gap:?}"
            );
            assert!(gap <= base + jitter, "gap <= base + jitter, got {gap:?}");
        }
    }
}
