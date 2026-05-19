use std::sync::{Arc, OnceLock};

use prost::Message;
use xmtp_common::RetryableError;
use xmtp_configuration::CUTOVER_REFRESH_TIME;
use xmtp_proto::api::mock::MockNetworkClient;
use xmtp_proto::api::{ApiClientError, BytesStream, Client, IsConnectedCheck};
use xmtp_proto::xmtp::migration::api::v1::FetchD14nCutoverResponse;

use super::*;
use crate::V3Client;
use crate::protocol::InMemoryCursorStore;
use crate::protocol::XmtpQuery;

/// Wrapper around `Arc<MockNetworkClient>` that also implements [`IsConnectedCheck`].
/// `Arc<MockNetworkClient>` already implements `Client` + `Clone`.
#[derive(Clone)]
struct TestNetworkClient(Arc<MockNetworkClient>);

impl TestNetworkClient {
    fn new() -> Self {
        Self(Arc::new(MockNetworkClient::new()))
    }

    fn from_mock(mock: MockNetworkClient) -> Self {
        Self(Arc::new(mock))
    }
}

#[xmtp_common::async_trait]
impl Client for TestNetworkClient {
    async fn request(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: prost::bytes::Bytes,
    ) -> Result<http::Response<prost::bytes::Bytes>, ApiClientError> {
        self.0.request(request, path, body).await
    }

    async fn stream(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: prost::bytes::Bytes,
    ) -> Result<http::Response<BytesStream>, ApiClientError> {
        self.0.stream(request, path, body).await
    }
}

#[xmtp_common::async_trait]
impl IsConnectedCheck for TestNetworkClient {
    async fn is_connected(&self) -> bool {
        true
    }
}

type TestMigrationClient =
    MigrationClient<TestNetworkClient, TestNetworkClient, InMemoryCursorStore>;

/// Build a `MigrationClient` for testing by constructing fields directly.
/// `v3_client` and `xmtpd_client` are `XmtpApiClient` (type-erased `Arc<dyn ...>`),
/// built from two separate `V3Client` instances so we can compare pointers.
fn build_test_client(
    v3: TestNetworkClient,
    d14n: TestNetworkClient,
    store: InMemoryCursorStore,
) -> TestMigrationClient {
    let v3_api = V3Client::new(v3.clone(), store.clone()).arced();
    let d14n_api = V3Client::new(d14n.clone(), store.clone()).arced();
    MigrationClient {
        v3_grpc: v3,
        xmtpd_grpc: d14n,
        store,
        v3_client: v3_api,
        xmtpd_client: d14n_api,
        always_check_once: OnceLock::new(),
    }
}

/// Create a `TestNetworkClient` that returns a `FetchD14nCutoverResponse` with the given
/// `timestamp_ns` when `request()` is called.
fn mock_v3_with_cutover(timestamp_ns: u64) -> TestNetworkClient {
    let mut mock = MockNetworkClient::new();
    let body = FetchD14nCutoverResponse { timestamp_ns }.encode_to_vec();
    mock.expect_request()
        .returning(move |_req, _path, _body| Ok(http::Response::new(body.clone().into())));
    TestNetworkClient::from_mock(mock)
}

/// A retryable error type for constructing migration-matching errors.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
struct FakeNetworkError(String);

impl RetryableError for FakeNetworkError {
    fn is_retryable(&self) -> bool {
        true
    }
}

#[xmtp_common::test]
fn regex_does_not_panic() {
    assert!(!ERROR_REGEX.is_match("hi"))
}

#[xmtp_common::test]
fn regex_matches_publishing_error() {
    assert!(ERROR_REGEX.is_match(
        "publishing to XMTP V3 is no longer available. Please upgrade your client to XMTP D14N."
    ))
}

#[xmtp_common::test]
fn regex_matches_streaming_error() {
    assert!(ERROR_REGEX.is_match(
        "XMTP V3 streaming is no longer available. Please upgrade your client to XMTP D14N."
    ))
}

#[xmtp_common::test(unwrap_try = true)]
async fn choose_client_returns_d14n_when_already_migrated() {
    let store = InMemoryCursorStore::new();
    store.set_has_migrated(true)?;

    let client = build_test_client(TestNetworkClient::new(), TestNetworkClient::new(), store);

    let chosen = client.choose_client().await?;
    let chosen_ptr = Arc::as_ptr(chosen);
    let expected_ptr = Arc::as_ptr(&client.xmtpd_client);
    assert_eq!(chosen_ptr as *const (), expected_ptr as *const ());
}

#[xmtp_common::test(unwrap_try = true)]
async fn choose_client_returns_v3_before_cutover() {
    let store = InMemoryCursorStore::new();
    // cutover far in the future
    let far_future = xmtp_common::time::now_ns() + CUTOVER_REFRESH_TIME * 10;
    store.set_cutover_ns(far_future)?;

    // The first call triggers `always_check_once`, which calls refresh_cutover.
    // Mock the v3_grpc request to return a far-future cutover.
    let v3 = mock_v3_with_cutover(far_future as u64);

    let client = build_test_client(v3, TestNetworkClient::new(), store);

    let chosen = client.choose_client().await?;
    let chosen_ptr = Arc::as_ptr(chosen);
    let expected_ptr = Arc::as_ptr(&client.v3_client);
    assert_eq!(chosen_ptr as *const (), expected_ptr as *const ());
}

#[xmtp_common::test(unwrap_try = true)]
async fn choose_client_returns_d14n_after_cutover() {
    let store = InMemoryCursorStore::new();

    // Return a cutover timestamp in the past
    let v3 = mock_v3_with_cutover(1);

    let client = build_test_client(v3, TestNetworkClient::new(), store.clone());

    let chosen = client.choose_client().await?;
    let chosen_ptr = Arc::as_ptr(chosen);
    let expected_ptr = Arc::as_ptr(&client.xmtpd_client);
    assert_eq!(chosen_ptr as *const (), expected_ptr as *const ());

    // The store should now be marked as migrated
    assert!(store.has_migrated()?);
}

#[xmtp_common::test(unwrap_try = true)]
async fn choose_client_refreshes_after_timeout() {
    let store = InMemoryCursorStore::new();
    let far_future = xmtp_common::time::now_ns() + CUTOVER_REFRESH_TIME * 10;
    // Set a stale last_checked so the refresh timeout has elapsed
    let stale_time = xmtp_common::time::now_ns() - CUTOVER_REFRESH_TIME - 1;
    store.set_last_checked_ns(stale_time)?;
    store.set_cutover_ns(far_future)?;

    let v3 = mock_v3_with_cutover(far_future as u64);

    let client = build_test_client(v3, TestNetworkClient::new(), store.clone());
    // Consume the always_check_once so it doesn't interfere
    client.always_check_once.set(()).ok();

    let chosen = client.choose_client().await?;
    let chosen_ptr = Arc::as_ptr(chosen);
    let expected_ptr = Arc::as_ptr(&client.v3_client);
    assert_eq!(chosen_ptr as *const (), expected_ptr as *const ());

    // Verify refresh was called by checking last_checked_ns was updated
    let last_checked = store.get_last_checked_ns()?;
    assert!(last_checked > stale_time);
}

#[xmtp_common::test(unwrap_try = true)]
async fn refresh_cutover_updates_store() {
    let store = InMemoryCursorStore::new();
    let v3 = mock_v3_with_cutover(12345);

    let client = build_test_client(v3, TestNetworkClient::new(), store.clone());

    let cutover = client.refresh_cutover().await?;
    assert_eq!(cutover, 12345);
    assert_eq!(store.get_cutover_ns()?, 12345);

    let last_checked = store.get_last_checked_ns()?;
    assert!(last_checked > 0);
}

#[xmtp_common::test(unwrap_try = true)]
async fn write_with_refresh_succeeds_without_retry() {
    let store = InMemoryCursorStore::new();
    let client = build_test_client(TestNetworkClient::new(), TestNetworkClient::new(), store);

    let result = client.write_with_refresh(|| async { Ok(42) }).await?;
    assert_eq!(result, 42);
}

#[xmtp_common::test(unwrap_try = true)]
async fn write_with_refresh_retries_on_migration_error() {
    let store = InMemoryCursorStore::new();
    // Mock the refresh_cutover call that happens on retry
    let v3 = mock_v3_with_cutover(1);

    let client = build_test_client(v3, TestNetworkClient::new(), store);

    let call_count = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let call_count_clone = call_count.clone();

    let result: Result<i32, ApiClientError> = client
        .write_with_refresh(|| {
            let cc = call_count_clone.clone();
            async move {
                let count = cc.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if count == 0 {
                    // First call: return a migration error
                    Err(ApiClientError::client(FakeNetworkError(
                        "publishing to XMTP V3 is no longer available. Please upgrade your client to XMTP D14N.".to_string(),
                    )))
                } else {
                    Ok(42)
                }
            }
        })
        .await;

    assert_eq!(result.unwrap(), 42);
    assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 2);
}

#[xmtp_common::test(unwrap_try = true)]
async fn write_with_refresh_does_not_retry_on_other_error() {
    let store = InMemoryCursorStore::new();
    let client = build_test_client(TestNetworkClient::new(), TestNetworkClient::new(), store);

    let call_count = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let call_count_clone = call_count.clone();

    let result: Result<i32, ApiClientError> = client
        .write_with_refresh(|| {
            let cc = call_count_clone.clone();
            async move {
                cc.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Err(ApiClientError::client(FakeNetworkError(
                    "some unrelated network error".to_string(),
                )))
            }
        })
        .await;

    assert!(result.is_err());
    // Should only be called once — no retry
    assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[xmtp_common::test(unwrap_try = true)]
async fn is_d14n_returns_false_before_migration() {
    let store = InMemoryCursorStore::new();
    let far_future = xmtp_common::time::now_ns() + CUTOVER_REFRESH_TIME * 10;
    store.set_cutover_ns(far_future)?;

    let v3 = mock_v3_with_cutover(far_future as u64);
    let client = build_test_client(v3, TestNetworkClient::new(), store);

    assert!(!client.is_d14n()?);
}

#[xmtp_common::test(unwrap_try = true)]
async fn is_d14n_returns_true_after_migration() {
    let store = InMemoryCursorStore::new();
    store.set_has_migrated(true)?;

    let client = build_test_client(TestNetworkClient::new(), TestNetworkClient::new(), store);

    assert!(client.is_d14n()?);
}
