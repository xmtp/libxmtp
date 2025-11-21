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

#[doc(hidden)]
pub async fn sleep(duration: Duration) {
    wasm_or_native! {
        native => {tokio::time::sleep(duration).await},
        wasm => {gloo_timers::future::sleep(duration).await},
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
