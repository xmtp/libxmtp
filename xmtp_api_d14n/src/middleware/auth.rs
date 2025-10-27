use std::sync::Arc;

use arc_swap::ArcSwap;
use prost::bytes::Bytes;
use tokio::sync::OnceCell;
use xmtp_common::{BoxDynError, MaybeSend, MaybeSync, time::now_secs};
use xmtp_proto::api::{ApiClientError, Client, IsConnectedCheck};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Credential {
    name: http::header::HeaderName,
    value: http::header::HeaderValue,
    expires_at_seconds: i64,
}

impl Credential {
    pub fn new(
        name: http::header::HeaderName,
        value: http::header::HeaderValue,
        expires_at_seconds: i64,
    ) -> Self {
        Self {
            name,
            value,
            expires_at_seconds,
        }
    }
}

#[derive(Default, Clone)]
pub struct AuthHandle {
    handle: Arc<OnceCell<ArcSwap<Credential>>>,
}

impl AuthHandle {
    pub fn new() -> Self {
        Self::default()
    }
    pub async fn set(&self, credential: Credential) {
        let mut new = Some(credential);
        let inner = self
            .handle
            .get_or_init(|| async {
                ArcSwap::from_pointee(new.take().expect("Credential not set"))
            })
            .await;
        if let Some(new) = new {
            inner.store(Arc::new(new));
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait AuthCallback: MaybeSend + MaybeSync {
    async fn on_auth_required(&self) -> Result<Credential, BoxDynError>;
}

pub struct AuthMiddleware<C> {
    inner: C,
    handle: AuthHandle,
    callback: Option<Arc<dyn AuthCallback>>,
}

impl<C> AuthMiddleware<C> {
    pub fn new(
        inner: C,
        callback: Option<Arc<dyn AuthCallback>>,
        handle: Option<AuthHandle>,
    ) -> Self {
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
                .handle
                .get_or_try_init(|| async {
                    let credential = callback.on_auth_required().await?;
                    let arc_swap = ArcSwap::from_pointee(credential);
                    Ok::<_, BoxDynError>(arc_swap)
                })
                .await?;
            Some(arc_swap)
        } else {
            self.handle.handle.get()
        };

        let Some(arc_swap) = arc_swap else {
            tracing::warn!(
                "No auth callback provided and no credentials set. Auth headers will be empty."
            );
            return Ok(None);
        };

        if let Some(callback) = &self.callback
            && arc_swap.load().expires_at_seconds <= now_secs()
        {
            // Multiple threads may be racing to run this, so this may require a lock in the future.
            let new_header = callback.on_auth_required().await?;
            arc_swap.store(Arc::new(new_header));
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

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C> Client for AuthMiddleware<C>
where
    C: Client,
{
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

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C: IsConnectedCheck + MaybeSend + MaybeSync> IsConnectedCheck for AuthMiddleware<C> {
    async fn is_connected(&self) -> bool {
        self.inner.is_connected().await
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    fn credential(offset: i64) -> Credential {
        let header_name = http::header::AUTHORIZATION;
        let random = xmtp_common::rand_string::<16>();
        let header_value = http::header::HeaderValue::try_from(format!("Bearer {random}")).unwrap();
        let now = now_secs();
        Credential::new(header_name.clone(), header_value.clone(), now + offset)
    }

    #[xmtp_common::test]
    async fn test_auth_handle() {
        let credential = credential(0);
        let auth_handle = AuthHandle::new();
        auth_handle.set(credential.clone()).await;
        let inner = auth_handle.handle.get().map(|c| c.load_full()).unwrap();
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

    #[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
    #[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
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
        pub async fn make_requests(&self) {
            let request = http::request::Builder::new();
            let path = http::uri::PathAndQuery::from_static("/");
            let body = Bytes::new();
            self.request(request, path.clone(), body.clone())
                .await
                .unwrap();
            let request = http::request::Builder::new();
            let _ = self.stream(request, path, body).await.unwrap();
        }
    }

    struct TestCallback {
        inner: Credential,
        count: Arc<std::sync::atomic::AtomicI64>,
    }

    #[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
    #[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
    impl AuthCallback for TestCallback {
        async fn on_auth_required(&self) -> Result<Credential, BoxDynError> {
            let mut credential = self.inner.clone();
            let count = self.count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            credential.expires_at_seconds += count;
            println!("credential: {credential:?}, {}, {count}", now_secs());
            Ok(credential.clone())
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

    #[xmtp_common::test]
    async fn test_auth_middleware_no_callback_or_handle() {
        let middleware = AuthMiddleware::new(TestClient::new(None), None, None);
        middleware.make_requests().await;
    }

    #[xmtp_common::test]
    async fn test_auth_middleware_with_no_callback_and_handle() {
        let credential = credential(0);
        let auth_handle = AuthHandle::new();
        let mut middleware =
            AuthMiddleware::new(TestClient::new(None), None, Some(auth_handle.clone()));
        middleware.make_requests().await;
        auth_handle.set(credential.clone()).await;
        middleware.inner.expected_credential = Some(credential.clone());
        middleware.make_requests().await;
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
        middleware.make_requests().await;
        middleware.make_requests().await;
        middleware.make_requests().await;
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
        middleware.make_requests().await;
        middleware.make_requests().await;
        middleware.make_requests().await;
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 3);
        let handle_credential = credential(1);
        auth_handle.set(handle_credential.clone()).await;
        middleware.inner.expected_credential = Some(handle_credential.clone());
        middleware.make_requests().await;
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 3);
        auth_handle.set(cred.clone()).await;
        middleware.inner.expected_credential = Some(cred.clone());
        middleware.make_requests().await;
        middleware.make_requests().await;
        middleware.make_requests().await;
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 4);
    }
}
