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

/// Split `duration` into whole `max`-sized chunks plus a remainder.
/// Returns `(number_of_full_chunks, remainder)`. Pure; used for wasm-safe sleeping.
///
/// JS `setTimeout` (via gloo-timers) casts the millisecond value to `i32`,
/// so any duration > ~24.8 days overflows and fires immediately. Chunking into
/// at-most-1-day pieces lets callers sleep the full requested duration.
#[allow(dead_code)] // only called from wasm arm; also exercised by unit tests
fn sleep_chunks(duration: Duration, max: Duration) -> (u64, Duration) {
    let max_ns = max.as_nanos().max(1);
    let full = (duration.as_nanos() / max_ns) as u64;
    let rem = Duration::from_nanos((duration.as_nanos() % max_ns) as u64);
    (full, rem)
}

#[doc(hidden)]
pub async fn sleep(duration: Duration) {
    wasm_or_native! {
        native => {tokio::time::sleep(duration).await},
        wasm => {
            // JS setTimeout (via gloo-timers) caps near i32::MAX ms; chunk so a
            // multi-day sleep elapses fully instead of wrapping to ~0.
            const MAX_CHUNK: Duration = Duration::from_secs(60 * 60 * 24); // 1 day
            let (full, rem) = sleep_chunks(duration, MAX_CHUNK);
            for _ in 0..full {
                gloo_timers::future::sleep(MAX_CHUNK).await;
            }
            gloo_timers::future::sleep(rem).await;
        }
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

    #[test]
    fn test_now_ns_returns_positive() {
        let ns = now_ns();
        assert!(ns > 0, "now_ns should return a positive value");
    }

    #[test]
    fn test_now_ms_returns_positive() {
        let ms = now_ms();
        assert!(ms > 0, "now_ms should return a positive value");
    }

    #[test]
    fn test_now_secs_returns_positive() {
        let secs = now_secs();
        assert!(secs > 0, "now_secs should return a positive value");
    }

    #[test]
    fn sleep_chunks_splits_correctly() {
        let day = Duration::from_secs(86400);
        assert_eq!(
            sleep_chunks(Duration::from_secs(30 * 86400), day),
            (30, Duration::ZERO)
        );
        assert_eq!(
            sleep_chunks(Duration::from_secs(5), day),
            (0, Duration::from_secs(5))
        );
        assert_eq!(
            sleep_chunks(day + Duration::from_secs(5), day),
            (1, Duration::from_secs(5))
        );
        assert_eq!(sleep_chunks(Duration::ZERO, day), (0, Duration::ZERO));
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
