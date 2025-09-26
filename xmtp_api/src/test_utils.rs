#![allow(clippy::unwrap_used)]

use xmtp_common::{ExponentialBackoff, Retry, RetryBuilder};

xmtp_common::if_v3! {
    pub type TestClient = xmtp_api_d14n::TestV3Client;
}

xmtp_common::if_d14n! {
    pub type TestClient = xmtp_api_d14n::TestD14nClient;
}

pub fn exponential() -> RetryBuilder<ExponentialBackoff> {
    let e = ExponentialBackoff::default();
    Retry::builder().with_strategy(e)
}
