use super::Challenge;
use xmtp_common::time::{Duration, Instant};

#[xmtp_common::test(unwrap_try = true)]
async fn unrepresentable_pong_deadline_returns_an_error_without_panicking() {
    let (_, handed) = tokio::sync::oneshot::channel();
    let mut challenge = Challenge {
        nonce: 1,
        handed,
        deadline: None,
    };
    let error = challenge
        .start_deadline(Instant::now(), Duration::MAX)
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::Internal);
}
