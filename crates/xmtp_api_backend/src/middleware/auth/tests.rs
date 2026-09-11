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

#[xmtp_common::test(unwrap_try = true)]
async fn test_auth_handle() {
    let credential = credential(0);
    let auth_handle = AuthHandle::new();
    auth_handle.set(credential.clone()).await;
    let inner = auth_handle
        .inner
        .current
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
    fn host(&self) -> &str {
        "mock://auth"
    }

    async fn request(
        &self,
        request: http::request::Builder,
        _path: http::uri::PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Bytes>, ApiClientError> {
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
    ) -> Result<http::Response<BytesStream>, ApiClientError> {
        let headers = request.headers_ref().unwrap();
        if let Some(expected_credential) = &self.expected_credential {
            assert_eq!(
                headers.get(&expected_credential.name).unwrap(),
                &expected_credential.value
            );
        } else {
            assert!(headers.is_empty());
        }
        Ok(http::Response::new(BytesStream::new(
            futures::stream::once(Box::pin(async move { Ok(body) })),
        )))
    }

    async fn bidi_stream(
        &self,
        request: http::request::Builder,
        _path: http::uri::PathAndQuery,
        _body: xmtp_common::BoxDynStream<'static, Bytes>,
    ) -> Result<http::Response<BytesStream>, ApiClientError> {
        // Same header assertion as `stream`: this only passes if the
        // middleware actually forwarded `bidi_stream` (rather than inheriting
        // the trait's "unsupported" default) AND injected the auth header.
        let headers = request.headers_ref().unwrap();
        if let Some(expected_credential) = &self.expected_credential {
            assert_eq!(
                headers.get(&expected_credential.name).unwrap(),
                &expected_credential.value
            );
        } else {
            assert!(headers.is_empty());
        }
        Ok(http::Response::new(BytesStream::new(
            futures::stream::empty(),
        )))
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
        let result = self.stream(request, path.clone(), body).await;
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

        let request = http::request::Builder::new();
        // `Box::pin` (not `.boxed()`) so this coerces to `BoxDynStream` on
        // both native (`BoxStream`) and wasm (`LocalBoxStream`) targets.
        let bidi_body: xmtp_common::BoxDynStream<'static, Bytes> =
            Box::pin(futures::stream::empty());
        let result = self.bidi_stream(request, path, bidi_body).await;
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
    #[xmtp_common::test(unwrap_try = true)]
    async fn test_auth_middleware_no_callback_or_handle() {
        // expect a panic when creating the middleware without a callback or handle
        std::panic::catch_unwind(|| {
            AuthMiddleware::new(TestClient::new(None), None, None);
        })
        .unwrap_err();
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_auth_middleware_with_no_callback_and_handle() {
    let credential = credential(0);
    let auth_handle = AuthHandle::new();
    let mut middleware =
        AuthMiddleware::new(TestClient::new(None), None, Some(auth_handle.clone()));
    middleware
        .make_requests(Err("auth credential missing".into()))
        .await;

    auth_handle.set(credential.clone()).await;
    middleware.inner.expected_credential = Some(credential.clone());
    middleware.make_requests(Ok(())).await;
}

#[xmtp_common::test(unwrap_try = true)]
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

#[xmtp_common::test(unwrap_try = true)]
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

#[xmtp_common::test(unwrap_try = true)]
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
