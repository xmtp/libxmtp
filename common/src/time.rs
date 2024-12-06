//! Time primitives for native and WebAssembly

use std::fmt;

#[derive(Debug)]
pub struct Expired;

impl std::error::Error for Expired {
    fn description(&self) -> &str {
        "Timer duration expired"
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<tokio::time::error::Elapsed> for Expired {
    fn from(_: tokio::time::error::Elapsed) -> Expired {
        Expired
    }
}

impl fmt::Display for Expired {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        write!(f, "timer duration expired")
    }
}

#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
pub use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
pub use web_time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub fn now_ns() -> i64 {
    let now = SystemTime::now();

    now.duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_nanos() as i64
}

#[cfg(target_arch = "wasm32")]
pub async fn timeout<F>(duration: Duration, future: F) -> Result<F::Output, Expired>
where
    F: std::future::IntoFuture,
{
    use futures::future::Either::*;
    let timeout = gloo_timers::future::TimeoutFuture::new(duration.as_millis() as u32);
    let future = future.into_future();
    futures::pin_mut!(future);
    match futures::future::select(timeout, future).await {
        Left(_) => Err(Expired),
        Right((value, _)) => Ok(value),
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn timeout<F>(duration: Duration, future: F) -> Result<F::Output, Expired>
where
    F: std::future::IntoFuture,
{
    tokio::time::timeout(duration, future)
        .await
        .map_err(Into::into)
}

// WASM Shims
#[cfg(target_arch = "wasm32")]
#[doc(hidden)]
pub async fn sleep(duration: Duration) {
    gloo_timers::future::TimeoutFuture::new(duration.as_millis() as u32).await;
}

#[cfg(not(target_arch = "wasm32"))]
#[doc(hidden)]
pub async fn sleep(duration: Duration) {
    tokio::time::sleep(duration).await
}
