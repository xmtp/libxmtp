//! A rate-limit rejection is never split and is retried no sooner than its delay.
use super::*;
use xmtp_api_grpc::error::rate_limited_status;

fn rate_limited(delay: Duration) -> ApiClientError {
    ApiClientError::client(GrpcError::Status(rate_limited_status("publish", delay)))
}

// verifies: RATE-041, CONF-073
#[rstest]
#[case::rate_limited(rate_limited(Duration::from_secs(5)), false)]
#[case::plain_resource_exhausted(status(tonic::Code::ResourceExhausted), true)]
#[case::out_of_range(status(tonic::Code::OutOfRange), true)]
#[case::too_large(publish_refusal(wire::publish_error::Reason::TooLarge), true)]
#[case::other_invalid_argument(status(tonic::Code::InvalidArgument), false)]
#[xmtp_common::test(unwrap_try = true)]
fn size_error_excludes_rate_limits(#[case] error: ApiClientError, #[case] size: bool) {
    assert_eq!(crate::chunk::size_error(&error), size);
}

/// The client's default budget, on tokio's paused clock. Each attempt
/// records the paused time it started.
#[cfg(not(target_arch = "wasm32"))]
fn paused_client(
    mock: &mut MockBackendClient,
    errors: Vec<ApiClientError>,
) -> Arc<std::sync::Mutex<Vec<tokio::time::Instant>>> {
    tokio::time::pause();
    let starts = Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = starts.clone();
    let errors = std::sync::Mutex::new(errors.into_iter());
    mock.expect_publish().returning(move |request| {
        assert_eq!(
            request.envelopes.len(),
            2,
            "a rate-limited publish is never split"
        );
        seen.lock().unwrap().push(tokio::time::Instant::now());
        match errors.lock().unwrap().next() {
            Some(error) => Err(error),
            None => Ok(published(request)),
        }
    });
    starts
}

#[cfg(not(target_arch = "wasm32"))]
fn units() -> Vec<PublishUnit> {
    vec![
        PublishUnit::single(welcome(1)).unwrap(),
        PublishUnit::single(welcome(2)).unwrap(),
    ]
}

// verifies: RATE-041
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn rate_limited_publish_retries_after_the_delay_without_splitting() {
    let delay = Duration::from_secs(5);
    let mut mock = MockBackendClient::new();
    let starts = paused_client(&mut mock, vec![rate_limited(delay)]);
    let client = ApiClientWrapper::new(mock, crate::strategies::exponential_cooldown());
    let metas = client.publish_units(units()).await?;
    assert_eq!(metas.len(), 2);
    let starts = starts.lock().unwrap();
    assert_eq!(starts.len(), 2);
    assert!(starts[1] - starts[0] >= delay);
}

// verifies: RATE-041
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn rate_limit_delay_beyond_the_retry_budget_returns_at_once() {
    let delay = Duration::from_secs(200);
    let mut mock = MockBackendClient::new();
    let starts = paused_client(&mut mock, vec![rate_limited(delay)]);
    let client = ApiClientWrapper::new(mock, crate::strategies::exponential_cooldown());
    let started = tokio::time::Instant::now();
    let error = client.publish_units(units()).await.unwrap_err();
    assert_eq!(error.rate_limit_delay(), Some(delay));
    assert!(error.is_retryable());
    assert_eq!(starts.lock().unwrap().len(), 1);
    assert_eq!(tokio::time::Instant::now(), started);
}

/// A wait that fits the retry budget but not the 30-second notification
/// timeout returns the rejection at once, so the worker sees the delay
/// instead of a timeout.
// verifies: RATE-041
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn notification_rate_limit_beyond_its_timeout_returns_at_once() {
    tokio::time::pause();
    let delay = NOTIFICATION_REQUEST_TIMEOUT + Duration::from_secs(10);
    let mut mock = MockBackendClient::new();
    mock.expect_update_subscriptions()
        .times(1)
        .returning(move |_| Err(rate_limited(delay)));
    let client = ApiClientWrapper::new(mock, crate::strategies::exponential_cooldown());
    let started = tokio::time::Instant::now();
    let error = client
        .update_subscriptions(wire::UpdateSubscriptionsRequest::default())
        .await
        .unwrap_err();
    assert_eq!(error.rate_limit_delay(), Some(delay));
    assert_eq!(tokio::time::Instant::now(), started);
}
