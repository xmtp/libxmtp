use arc_swap::ArcSwap;
use prost::bytes::Bytes;
use std::sync::Arc;
use tokio::sync::OnceCell;
use xmtp_common::{BoxDynError, MaybeSend, MaybeSync, time::Instant};
#[cfg(not(test))]
use xmtp_configuration::AUTH_LOCKOUT_COOLDOWN;
use xmtp_configuration::MAX_CONSECUTIVE_AUTH_FAILURES;
use xmtp_proto::api::{
    ApiClientError, AuthError, BytesStream, Client, IsConnectedCheck, grpc_status,
};

#[cfg(test)]
/// Longer than the transport's first reconnect delay (100 ms), so a reopen
/// during a lockout reliably meets it instead of racing it.
const AUTH_LOCKOUT_COOLDOWN: std::time::Duration = std::time::Duration::from_millis(500);

#[cfg(not(test))]
use xmtp_common::time::now_secs;
// Use a fixed clock to keep expiry tests stable.
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

#[derive(Clone, Copy, Default)]
struct AuthState {
    generation: u64,
    stale: bool,
    failures: u32,
    locked_until: Option<Instant>,
}

impl AuthState {
    fn fail(&mut self) {
        self.failures = (self.failures + 1).min(MAX_CONSECUTIVE_AUTH_FAILURES);
        if self.failures == MAX_CONSECUTIVE_AUTH_FAILURES && self.locked_until.is_none() {
            self.locked_until = Some(Instant::now() + AUTH_LOCKOUT_COOLDOWN);
        }
    }
}

#[derive(Default)]
struct AuthInner {
    current: OnceCell<ArcSwap<Credential>>,
    state: tokio::sync::Mutex<AuthState>,
    /// Held across the callback so only one runs at a time. It is separate from
    /// `state` because `AuthHandle::set` takes `state`, and a callback that
    /// pushes its credential through the handle would deadlock on a lock this
    /// function held across the await. Tokio's mutex is not reentrant.
    refresh: tokio::sync::Mutex<()>,
}

impl AuthInner {
    /// Store only while the state lock is held. This operation cannot be cancelled.
    fn store(&self, credential: Credential) {
        if let Some(current) = self.current.get() {
            current.store(Arc::new(credential));
        } else {
            self.current
                .set(ArcSwap::from_pointee(credential))
                .unwrap_or_else(|_| unreachable!("state lock protects initialization"));
        }
    }
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
        let mut state = self.inner.state.lock().await;
        self.inner.store(credential);
        *state = AuthState {
            generation: state.generation + 1,
            ..AuthState::default()
        };
    }

    pub fn id(&self) -> usize {
        Arc::as_ptr(&self.inner) as usize
    }
}

#[xmtp_common::async_trait]
pub trait AuthCallback: MaybeSend + MaybeSync {
    async fn on_auth_required(&self) -> Result<Credential, BoxDynError>;
}

/// Add credentials and refetch after expiry or rejection.
/// Shared handles share credentials, callback serialization, and lockout state.
/// Without a callback, expired or rejected credentials remain in use until `set`.
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

    /// Keep the credential and its generation together across each network call.
    /// Commit callback state only after the callback completes, including a probe.
    async fn get_credential(&self) -> Result<(Arc<Credential>, u64), AuthError> {
        let inner = &self.handle.inner;
        // Take the refresh lock first so only one callback runs at a time. The
        // state lock is taken and released around it, never held across the
        // await, because `AuthHandle::set` needs it while a callback runs.
        let _refresh = inner.refresh.lock().await;
        let mut state = *inner.state.lock().await;
        if let Some(until) = state.locked_until {
            if until > Instant::now() {
                return Err(AuthError::Exhausted);
            }
            state.locked_until = None;
            state.failures = MAX_CONSECUTIVE_AUTH_FAILURES - 1;
            state.stale = true;
        }
        let mut credential = inner.current.get().map(|current| current.load_full());
        let needs_refresh = state.stale
            || credential
                .as_ref()
                .is_none_or(|credential| credential.expires_at_seconds <= now_secs());
        if needs_refresh && let Some(callback) = &self.callback {
            let generation_before = state.generation;
            let result = callback.on_auth_required().await;
            let mut guard = inner.state.lock().await;
            // `AuthHandle::set` may have run during the callback. Its credential
            // is newer, so keep it and leave its state alone.
            if guard.generation != generation_before {
                return Ok((
                    inner
                        .current
                        .get()
                        .map(|current| current.load_full())
                        .ok_or(AuthError::MissingCredential)?,
                    guard.generation,
                ));
            }
            match result {
                Ok(new) => {
                    inner.store(new);
                    credential = inner.current.get().map(|current| current.load_full());
                    state.generation += 1;
                    state.stale = false;
                }
                Err(_) => {
                    state.fail();
                    *guard = state;
                    // A callback failure that trips the lockout reports
                    // `Exhausted`, like a rejection that trips it. The wire must
                    // wait for the cool-down, not read this as permanent.
                    return Err(if state.locked_until.is_some() {
                        AuthError::Exhausted
                    } else {
                        AuthError::CallbackFailed { retryable: true }
                    });
                }
            }
            *guard = state;
        } else {
            *inner.state.lock().await = state;
        }
        let credential = credential.ok_or(AuthError::MissingCredential)?;
        Ok((credential, state.generation))
    }

    /// Return whether a current rejection permits one immediate replay.
    async fn finish<T>(
        &self,
        generation: u64,
        result: Result<T, ApiClientError>,
    ) -> (Result<T, ApiClientError>, bool) {
        let rejected = result
            .as_ref()
            .err()
            .and_then(|error| grpc_status(error))
            .is_some_and(|status| status.code() == tonic::Code::Unauthenticated);
        let mut state = self.handle.inner.state.lock().await;
        let current = generation == state.generation;
        if current {
            if result.is_ok() {
                state.failures = 0;
            } else if rejected {
                state.stale = true;
                if self.callback.is_some() {
                    state.fail();
                }
            }
        }
        if rejected {
            // A rejection that trips the lockout reports `Exhausted`, the same
            // error every later call gets, so the cool-down has one error. A
            // long-lived transport can then tell a timed lockout apart from a
            // permanent rejection and wait instead of shutting down.
            if self.callback.is_some()
                && state
                    .locked_until
                    .is_some_and(|until| until > Instant::now())
            {
                return (Err(AuthError::Exhausted.into()), false);
            }
            let retryable = self.callback.is_some();
            (
                Err(AuthError::CredentialRejected { retryable }.into()),
                current && retryable,
            )
        } else {
            (result, false)
        }
    }

    /// Rebuild all request parts. Replace the credential header instead of appending it.
    fn request_builder(
        parts: &http::request::Parts,
        credential: &Credential,
    ) -> http::request::Builder {
        let mut request = http::Request::builder()
            .method(parts.method.clone())
            .uri(parts.uri.clone())
            .version(parts.version);
        let headers = request.headers_mut().expect("validated request parts");
        *headers = parts.headers.clone();
        headers.insert(credential.name.clone(), credential.value.clone());
        *request.extensions_mut().expect("validated request parts") = parts.extensions.clone();
        request
    }
}

#[xmtp_common::async_trait]
impl<C: Client> Client for AuthMiddleware<C> {
    fn host(&self) -> &str {
        self.inner.host()
    }

    async fn request(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Bytes>, ApiClientError> {
        let (parts, ()) = request.body(())?.into_parts();
        let (credential, generation) = self.get_credential().await?;
        let result = self
            .inner
            .request(
                Self::request_builder(&parts, &credential),
                path.clone(),
                body.clone(),
            )
            .await;
        let (result, replay) = self.finish(generation, result).await;
        if !replay {
            return result;
        }
        let (credential, generation) = self.get_credential().await?;
        let result = self
            .inner
            .request(Self::request_builder(&parts, &credential), path, body)
            .await;
        self.finish(generation, result).await.0
    }

    async fn stream(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<BytesStream>, ApiClientError> {
        let (parts, ()) = request.body(())?.into_parts();
        let (credential, generation) = self.get_credential().await?;
        let result = self
            .inner
            .stream(
                Self::request_builder(&parts, &credential),
                path.clone(),
                body.clone(),
            )
            .await;
        let (result, replay) = self.finish(generation, result).await;
        if !replay {
            return result;
        }
        let (credential, generation) = self.get_credential().await?;
        let result = self
            .inner
            .stream(Self::request_builder(&parts, &credential), path, body)
            .await;
        self.finish(generation, result).await.0
    }

    async fn bidi_stream(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: xmtp_common::BoxDynStream<'static, Bytes>,
    ) -> Result<http::Response<BytesStream>, ApiClientError> {
        let (parts, ()) = request.body(())?.into_parts();
        let (credential, generation) = self.get_credential().await?;
        let result = self
            .inner
            .bidi_stream(Self::request_builder(&parts, &credential), path, body)
            .await;
        self.finish(generation, result).await.0
    }
}

#[xmtp_common::async_trait]
impl<C: IsConnectedCheck> IsConnectedCheck for AuthMiddleware<C> {
    async fn is_connected(&self) -> bool {
        self.inner.is_connected().await
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod refetch_tests;
