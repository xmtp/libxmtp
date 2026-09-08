#![allow(clippy::unwrap_used)]

use xmtp_common::{ExponentialBackoff, Retry, RetryBuilder};

pub type TestClient = xmtp_api_d14n::TestClient;

pub fn exponential() -> RetryBuilder<ExponentialBackoff> {
    let e = ExponentialBackoff::default();
    Retry::builder().with_strategy(e)
}
