use arc_swap::ArcSwap;
use prost::bytes::Bytes;
use std::sync::Arc;
use tokio::sync::OnceCell;
use xmtp_common::{BoxDynError, MaybeSend, MaybeSync};
use xmtp_proto::api::{ApiClientError, Client, IsConnectedCheck};

#[cfg(not(test))]
use xmtp_common::time::now_secs;
// override now_secs so we don't have flaky tests
#[cfg(test)]
fn now_secs() -> i64 {
    1_000_000
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Credential {
    name: http::header::HeaderName,
    value: http::header::HeaderValue,
    expires_at_seconds: i64,
}

impl Credential {
    pub fn new(
        name: Option<http::header::HeaderName>,
        value: http::header::HeaderValue,
        expires_at_seconds: i64,
    ) -> Self {
        Self {
            name: name.unwrap_or(http::header::AUTHORIZATION),
            value,
            expires_at_seconds,
        }
    }
}

#[derive(Default)]
struct AuthInner {
    handle: OnceCell<ArcSwap<Credential>>,
    mutex: tokio::sync::Mutex<()>,
}

#[derive(Default, Clone)]
pub struct AuthHandle {
    inner: Arc<AuthInner>,
}

impl AuthHandle {
    pub fn new() -> Self {
        Self::default()
    }
    pub async fn set(&self, credential: Credential) {
        let mut new = Some(credential);
        let inner = self
            .inner
            .handle
            .get_or_init(|| async {
                ArcSwap::from_pointee(new.take().expect("Credential not set"))
            })
            .await;
        if let Some(new) = new {
            inner.store(Arc::new(new));
        }
    }
    pub fn id(&self) -> usize {
        Arc::as_ptr(&self.inner) as usize
    }
}

#[xmtp_common::async_trait]
pub trait AuthCallback: MaybeSend + MaybeSync {
    async fn on_auth_required(&self) -> Result<Credential, BoxDynError>;
}

/// Middleware for adding authentication headers to requests.
///
/// This middleware will add authentication headers to requests if a callback or handle is provided.
///
/// If a callback is provided, it will be called to get the credential when it is expired.
/// If a handle is provided, it can be used to set the credential.
///
/// If only providing a handle, then expired credentials will still be used until the credential is set.
///
/// If creating multiple clients, if they share the same handle, then the credential will be shared between them
/// resulting in less auth callbacks. Auth callbacks are debounced internally to prevent excessive calls.
#[derive(Clone)]
pub struct AuthMiddleware<C> {
    inner: C,
    handle: AuthHandle,
    callback: Option<Arc<dyn AuthCallback>>,
}

impl<C> AuthMiddleware<C> {
    #[track_caller]
    pub fn new(
        inner: C,
        callback: Option<Arc<dyn AuthCallback>>,
        handle: Option<AuthHandle>,
    ) -> Self {
        assert!(
            callback.is_some() || handle.is_some(),
            "Either a callback or a handle must be provided"
        );
        Self {
            inner,
            handle: handle.unwrap_or_default(),
            callback,
        }
    }
    async fn get_credential(&self) -> Result<Option<&ArcSwap<Credential>>, BoxDynError> {
        let arc_swap = if let Some(callback) = &self.callback {
            let arc_swap = self
                .handle
                .inner
                .handle
                .get_or_try_init(|| async {
                    let credential = callback.on_auth_required().await?;
                    let arc_swap = ArcSwap::from_pointee(credential);
                    Ok::<_, BoxDynError>(arc_swap)
                })
                .await?;
            Some(arc_swap)
        } else {
            self.handle.inner.handle.get()
        };

        let Some(arc_swap) = arc_swap else {
            return Err("No auth callback provided and no credentials set. Please set credentials by calling `AuthHandle::set`.".into());
        };

        let needs_refresh = || arc_swap.load().expires_at_seconds <= now_secs();

        if let Some(callback) = &self.callback
            && needs_refresh()
        {
            // Multiple threads may be racing to run this, so this may require a lock in the future.
            let _guard = self.handle.inner.mutex.lock().await;
            // after acquiring the lock, we need to check again if the credential needs to be refreshed so that
            // if another thread has already refreshed the credential, we don't need to do it again.
            if needs_refresh() {
                let new_header = callback.on_auth_required().await?;
                arc_swap.store(Arc::new(new_header));
            }
        }
        Ok(Some(arc_swap))
    }
    async fn modify_request<E: std::error::Error>(
        &self,
        mut request: http::request::Builder,
    ) -> Result<http::request::Builder, ApiClientError<E>> {
        let maybe_credential = self
            .get_credential()
            .await
            .map_err(ApiClientError::<E>::OtherUnretryable)?;
        if let Some(credential) = maybe_credential {
            let credential = credential.load();
            request = request.header(credential.name.clone(), credential.value.clone());
        }
        Ok(request)
    }
}

#[xmtp_common::async_trait]
impl<C: Client> Client for AuthMiddleware<C> {
    type Error = C::Error;

    type Stream = C::Stream;

    async fn request(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Bytes>, ApiClientError<Self::Error>> {
        let request = self.modify_request(request).await?;
        self.inner.request(request, path, body).await
    }

    async fn stream(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Self::Stream>, ApiClientError<Self::Error>> {
        let request = self.modify_request(request).await?;
        self.inner.stream(request, path, body).await
    }
}

#[xmtp_common::async_trait]
impl<C: IsConnectedCheck> IsConnectedCheck for AuthMiddleware<C> {
    async fn is_connected(&self) -> bool {
        self.inner.is_connected().await
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use futures::StreamExt;

    fn credential(offset: i64) -> Credential {
        let random_name = xmtp_common::rand_string::<16>().to_lowercase();
        let header_name =
            http::header::HeaderName::try_from(format!("x-test-header-{random_name}")).unwrap();
        let random = xmtp_common::rand_string::<16>();
        let header_value = http::header::HeaderValue::try_from(format!("Bearer {random}")).unwrap();
        let now = now_secs();
        Credential::new(Some(header_name), header_value.clone(), now + offset)
    }

    #[xmtp_common::test]
    async fn test_auth_handle() {
        let credential = credential(0);
        let auth_handle = AuthHandle::new();
        auth_handle.set(credential.clone()).await;
        let inner = auth_handle
            .inner
            .handle
            .get()
            .map(|c| c.load_full())
            .unwrap();
        assert_eq!(inner.name, credential.name);
        assert_eq!(inner.value, credential.value);
        assert_eq!(inner.expires_at_seconds, credential.expires_at_seconds);
    }

    struct TestClient {
        expected_credential: Option<Credential>,
    }

    impl TestClient {
        pub fn new(expected_credential: Option<Credential>) -> Self {
            Self {
                expected_credential,
            }
        }
    }

    #[xmtp_common::async_trait]
    impl Client for TestClient {
        type Error = core::convert::Infallible;
        type Stream = futures::stream::Once<
            core::pin::Pin<Box<dyn Future<Output = Result<Bytes, Self::Error>> + Send + Sync>>,
        >;

        async fn request(
            &self,
            request: http::request::Builder,
            _path: http::uri::PathAndQuery,
            body: Bytes,
        ) -> Result<http::Response<Bytes>, ApiClientError<Self::Error>> {
            let headers = request.headers_ref().unwrap();
            if let Some(expected_credential) = &self.expected_credential {
                assert_eq!(
                    headers.get(&expected_credential.name).unwrap(),
                    &expected_credential.value
                );
            } else {
                assert!(headers.is_empty());
            }
            Ok(http::Response::new(body))
        }

        async fn stream(
            &self,
            request: http::request::Builder,
            _path: http::uri::PathAndQuery,
            body: Bytes,
        ) -> Result<http::Response<Self::Stream>, ApiClientError<Self::Error>> {
            let headers = request.headers_ref().unwrap();
            if let Some(expected_credential) = &self.expected_credential {
                assert_eq!(
                    headers.get(&expected_credential.name).unwrap(),
                    &expected_credential.value
                );
            } else {
                assert!(headers.is_empty());
            }
            Ok(http::Response::new(futures::stream::once(Box::pin(
                async move { Ok::<_, Self::Error>(body) },
            ))))
        }
    }

    impl<C: Client> AuthMiddleware<C> {
        pub async fn make_requests(&self, expected: Result<(), String>) {
            let request = http::request::Builder::new();
            let path = http::uri::PathAndQuery::from_static("/");
            let body = Bytes::new();
            let result = self.request(request, path.clone(), body.clone()).await;
            match (&expected, result) {
                (Ok(()), Ok(response)) => {
                    assert_eq!(response.status(), http::StatusCode::OK);
                }
                (Err(e), Ok(response)) => {
                    panic!("Expected error: {e}, got response: {response:?}");
                }
                (Ok(()), Err(e)) => {
                    panic!("Expected Ok, got error: {e}");
                }
                (Err(e), Err(res)) => {
                    assert_eq!(e, &res.to_string());
                }
            }

            let request = http::request::Builder::new();
            let result = self.stream(request, path, body).await;
            match (&expected, result) {
                (Ok(()), Ok(response)) => {
                    assert_eq!(response.status(), http::StatusCode::OK);
                }
                (Err(e), Ok(_)) => {
                    panic!("Expected error: {e}, got Ok");
                }
                (Ok(()), Err(e)) => {
                    panic!("Expected Ok, got error: {e}");
                }
                (Err(e), Err(res)) => {
                    assert_eq!(e, &res.to_string());
                }
            }
        }
    }

    struct TestCallback {
        inner: Credential,
        count: Arc<std::sync::atomic::AtomicI64>,
    }

    #[xmtp_common::async_trait]
    impl AuthCallback for TestCallback {
        async fn on_auth_required(&self) -> Result<Credential, BoxDynError> {
            // Add sleeps so we can test concurrent requests
            xmtp_common::time::sleep(std::time::Duration::from_millis(10)).await;
            let mut credential = self.inner.clone();
            xmtp_common::time::sleep(std::time::Duration::from_millis(10)).await;
            let count = self.count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            xmtp_common::time::sleep(std::time::Duration::from_millis(10)).await;
            credential.expires_at_seconds += count;
            xmtp_common::time::sleep(std::time::Duration::from_millis(10)).await;
            tracing::debug!("credential: {credential:?}, {}, {count}", now_secs());
            Ok(credential)
        }
    }

    impl TestCallback {
        pub fn new(credential: Credential, count: Arc<std::sync::atomic::AtomicI64>) -> Self {
            Self {
                inner: credential,
                count,
            }
        }
    }

    // Only run this test on native where we can catch the panic
    // This should never panic in practice because we only create auth middleware if there is a callback or handle.
    xmtp_common::if_native! {
        #[xmtp_common::test]
        async fn test_auth_middleware_no_callback_or_handle() {
            // expect a panic when creating the middleware without a callback or handle
            std::panic::catch_unwind(|| {
                AuthMiddleware::new(TestClient::new(None), None, None);
            })
            .unwrap_err();
        }
    }

    #[xmtp_common::test]
    async fn test_auth_middleware_with_no_callback_and_handle() {
        let credential = credential(0);
        let auth_handle = AuthHandle::new();
        let mut middleware =
            AuthMiddleware::new(TestClient::new(None), None, Some(auth_handle.clone()));
        middleware
            .make_requests(Err("No auth callback provided and no credentials set. Please set credentials by calling `AuthHandle::set`.".into()))
            .await;

        auth_handle.set(credential.clone()).await;
        middleware.inner.expected_credential = Some(credential.clone());
        middleware.make_requests(Ok(())).await;
    }

    #[xmtp_common::test]
    async fn test_auth_middleware_with_callback_and_no_handle() {
        let credential = credential(-1);
        let count = Arc::new(std::sync::atomic::AtomicI64::new(0));
        let callback = TestCallback::new(credential.clone(), count.clone());
        let middleware = AuthMiddleware::new(
            TestClient::new(Some(credential.clone())),
            Some(Arc::new(callback)),
            None,
        );
        middleware.make_requests(Ok(())).await;
        middleware.make_requests(Ok(())).await;
        middleware.make_requests(Ok(())).await;
        // 3 calls are expected because the credential starts out being one
        // second past expiry, then the second of expiry, then has one
        // second until expiry, so it doesn't need to be refreshed.
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 3);
    }

    #[xmtp_common::test]
    async fn test_auth_middleware_with_callback_and_handle() {
        let cred = credential(-1);
        let count = Arc::new(std::sync::atomic::AtomicI64::new(0));
        let auth_handle = AuthHandle::new();
        let callback = TestCallback::new(cred.clone(), count.clone());
        let mut middleware = AuthMiddleware::new(
            TestClient::new(Some(cred.clone())),
            Some(Arc::new(callback)),
            Some(auth_handle.clone()),
        );
        middleware.make_requests(Ok(())).await;
        middleware.make_requests(Ok(())).await;
        middleware.make_requests(Ok(())).await;
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 3);
        let handle_credential = credential(1);
        auth_handle.set(handle_credential.clone()).await;
        middleware.inner.expected_credential = Some(handle_credential.clone());
        middleware.make_requests(Ok(())).await;
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 3);
        auth_handle.set(cred.clone()).await;
        middleware.inner.expected_credential = Some(cred.clone());
        middleware.make_requests(Ok(())).await;
        middleware.make_requests(Ok(())).await;
        middleware.make_requests(Ok(())).await;
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 4);
    }

    #[xmtp_common::test]
    async fn test_auth_middleware_with_callback_and_handle_concurrent_requests() {
        let cred = credential(-1);
        let count = Arc::new(std::sync::atomic::AtomicI64::new(0));
        let auth_handle = AuthHandle::new();
        let mut middlewares = vec![];
        for _ in 0..10 {
            let middleware = AuthMiddleware::new(
                TestClient::new(Some(cred.clone())),
                Some(Arc::new(TestCallback::new(cred.clone(), count.clone()))),
                Some(auth_handle.clone()),
            );
            middlewares.push(middleware);
        }

        let mut tasks = middlewares
            .iter()
            .map(|middleware| async {
                middleware.make_requests(Ok(())).await;
                middleware.make_requests(Ok(())).await;
                middleware.make_requests(Ok(())).await;
            })
            .collect::<futures::stream::FuturesUnordered<_>>();

        while let Some(task) = tasks.next().await {
            let () = task;
        }
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 3);
    }
}
