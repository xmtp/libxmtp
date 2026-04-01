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
            crate::wasm::tokio::time::timeout(duration, future.into_future())
                .await
                .map_err(|_| Expired)
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
        wasm => {crate::wasm::tokio::time::sleep(duration).await},
    }
}

pub fn interval_stream(
    period: crate::time::Duration,
) -> impl futures::Stream<Item = crate::time::Instant> + Unpin {
    use futures::StreamExt;
    wasm_or_native! {
        wasm => {
            Box::pin(futures::stream::unfold(
                crate::wasm::tokio::time::interval(period),
                |mut interval| async move {
                    interval.tick().await;
                    Some((crate::time::Instant::now(), interval))
                },
            ))
        },
        native => {
            tokio_stream::wrappers::IntervalStream::new(tokio::time::interval(period))
                .map(|t| t.into_std())
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
}
