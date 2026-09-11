use super::*;
use futures::StreamExt;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::{Mutex, oneshot};
use xmtp_api_grpc::error::GrpcError;
use xmtp_common::RetryableError;

#[derive(Clone, Debug, PartialEq)]
struct Marker(u32);

struct Reply {
    code: tonic::Code,
    gate: Option<oneshot::Receiver<()>>,
}

#[derive(Default)]
struct Peer {
    replies: Mutex<VecDeque<Reply>>,
    sent: Mutex<Vec<(http::request::Parts, http::uri::PathAndQuery, Bytes)>>,
}

impl Peer {
    async fn enqueue(&self, code: tonic::Code) {
        self.replies
            .lock()
            .await
            .push_back(Reply { code, gate: None });
    }

    async fn receive(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: Bytes,
    ) -> Result<Bytes, ApiClientError> {
        let parts = request.body(())?.into_parts().0;
        self.sent.lock().await.push((parts, path, body.clone()));
        let reply = self.replies.lock().await.pop_front();
        if let Some(reply) = reply {
            if let Some(gate) = reply.gate {
                let _ = gate.await;
            }
            if reply.code != tonic::Code::Ok {
                return Err(ApiClientError::client(GrpcError::Status(
                    tonic::Status::new(reply.code, "peer error"),
                )));
            }
        }
        Ok(body)
    }
}

#[xmtp_common::async_trait]
impl Client for Peer {
    fn host(&self) -> &str {
        "mock://auth"
    }
    async fn request(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<Bytes>, ApiClientError> {
        self.receive(request, path, body)
            .await
            .map(http::Response::new)
    }
    async fn stream(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<BytesStream>, ApiClientError> {
        let body = self.receive(request, path, body).await?;
        Ok(http::Response::new(BytesStream::new(
            futures::stream::once(async { Ok(body) }),
        )))
    }
    async fn bidi_stream(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        mut body: xmtp_common::BoxDynStream<'static, Bytes>,
    ) -> Result<http::Response<BytesStream>, ApiClientError> {
        let body = body.next().await.expect("test body");
        self.stream(request, path, body).await
    }
}

#[derive(Default)]
struct Callback {
    calls: AtomicUsize,
    fail: Mutex<bool>,
    gate: Mutex<Option<oneshot::Receiver<()>>>,
}

fn credential(value: &str) -> Credential {
    Credential::new(None, value.parse().expect("test header"), now_secs() + 100)
}

#[xmtp_common::async_trait]
impl AuthCallback for Callback {
    async fn on_auth_required(&self) -> Result<Credential, BoxDynError> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        if let Some(gate) = self.gate.lock().await.take() {
            let _ = gate.await;
        }
        if *self.fail.lock().await {
            return Err("secret callback error".into());
        }
        Ok(credential(&format!("token-{call}")))
    }
}

fn fixture() -> (AuthMiddleware<Arc<Peer>>, Arc<Peer>, Arc<Callback>) {
    let peer = Arc::new(Peer::default());
    let callback = Arc::new(Callback::default());
    (
        AuthMiddleware::new(peer.clone(), Some(callback.clone()), None),
        peer,
        callback,
    )
}

#[derive(Clone, Copy)]
enum Rpc {
    Unary,
    Stream,
    Bidi,
}

async fn call(client: &impl Client, rpc: Rpc) -> Result<Bytes, ApiClientError> {
    let builder = http::Request::builder()
        .method(http::Method::PATCH)
        .uri("https://example.org/original?key=value")
        .version(http::Version::HTTP_2)
        .header("x-preserved", "value")
        .header("authorization", "original")
        .extension(Marker(42));
    let path = http::uri::PathAndQuery::from_static("/rpc/path?query=kept");
    let body = Bytes::from_static(b"exact body");
    match rpc {
        Rpc::Unary => Ok(client.request(builder, path, body).await?.into_body()),
        Rpc::Stream => client
            .stream(builder, path, body)
            .await?
            .into_body()
            .next()
            .await
            .expect("response body"),
        Rpc::Bidi => client
            .bidi_stream(
                builder,
                path,
                Box::pin(futures::stream::once(async { body })),
            )
            .await?
            .into_body()
            .next()
            .await
            .expect("response body"),
    }
}

#[rstest::rstest]
#[case(Rpc::Unary)]
#[case(Rpc::Stream)]
#[xmtp_common::test(unwrap_try = true)]
async fn rejection_refetches_once_and_preserves_all_parts(#[case] rpc: Rpc) {
    let (client, peer, callback) = fixture();
    peer.enqueue(tonic::Code::Unauthenticated).await;
    assert_eq!(
        call(&client, rpc).await.expect("successful replay"),
        "exact body"
    );
    assert_eq!(callback.calls.load(Ordering::SeqCst), 2);
    let sent = peer.sent.lock().await;
    assert_eq!(sent.len(), 2);
    for (index, (parts, path, body)) in sent.iter().enumerate() {
        assert_eq!(parts.method, http::Method::PATCH);
        assert_eq!(parts.uri, "https://example.org/original?key=value");
        assert_eq!(parts.version, http::Version::HTTP_2);
        assert_eq!(parts.headers.len(), 2);
        assert_eq!(parts.headers["x-preserved"], "value");
        assert_eq!(parts.headers.get_all("authorization").iter().count(), 1);
        assert_eq!(
            parts.headers["authorization"],
            format!("token-{}", index + 1)
        );
        assert_eq!(parts.extensions.get::<Marker>(), Some(&Marker(42)));
        assert_eq!(path.as_str(), "/rpc/path?query=kept");
        assert_eq!(body, "exact body");
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn bidi_rejection_returns_to_caller_and_next_open_refetches() {
    let (client, peer, callback) = fixture();
    peer.enqueue(tonic::Code::Unauthenticated).await;
    let error = call(&client, Rpc::Bidi).await.err()?;
    assert!(matches!(
        error,
        ApiClientError::Auth(AuthError::CredentialRejected { retryable: true })
    ));
    assert_eq!(peer.sent.lock().await.len(), 1);
    assert_eq!(callback.calls.load(Ordering::SeqCst), 1);
    call(&client, Rpc::Bidi).await?;
    assert_eq!(callback.calls.load(Ordering::SeqCst), 2);
}

#[xmtp_common::test(unwrap_try = true)]
async fn second_rejection_returns_and_success_resets_failures() {
    let (client, peer, callback) = fixture();
    for _ in 0..2 {
        peer.enqueue(tonic::Code::Unauthenticated).await;
    }
    assert!(matches!(
        call(&client, Rpc::Unary).await,
        Err(ApiClientError::Auth(AuthError::CredentialRejected {
            retryable: true
        }))
    ));
    assert_eq!(peer.sent.lock().await.len(), 2);
    call(&client, Rpc::Unary).await?;
    for _ in 0..2 {
        peer.enqueue(tonic::Code::Unauthenticated).await;
    }
    assert!(matches!(
        call(&client, Rpc::Stream).await,
        Err(ApiClientError::Auth(AuthError::CredentialRejected {
            retryable: true
        }))
    ));
    assert_eq!(callback.calls.load(Ordering::SeqCst), 4);
    assert_eq!(client.handle.inner.state.lock().await.failures, 2);
}

#[rstest::rstest]
#[case(tonic::Code::Ok)]
#[case(tonic::Code::Unauthenticated)]
#[xmtp_common::test(unwrap_try = true)]
async fn old_response_cannot_change_new_generation(#[case] code: tonic::Code) {
    let (client, peer, callback) = fixture();
    let (release, gate) = oneshot::channel();
    peer.replies.lock().await.push_back(Reply {
        code,
        gate: Some(gate),
    });
    let pending = call(&client, Rpc::Unary);
    futures::pin_mut!(pending);
    assert!(futures::poll!(&mut pending).is_pending());
    client.handle.set(credential("manual")).await;
    peer.enqueue(tonic::Code::Unauthenticated).await;
    assert!(call(&client, Rpc::Bidi).await.is_err());
    release.send(()).expect("pending response");
    let result = pending.await;
    assert_eq!(result.is_ok(), code == tonic::Code::Ok);
    let state = *client.handle.inner.state.lock().await;
    assert_eq!(state.failures, 1);
    assert!(state.stale);
    assert_eq!(callback.calls.load(Ordering::SeqCst), 1);
}

async fn exhaust(client: &AuthMiddleware<Arc<Peer>>, peer: &Peer) {
    for _ in 0..MAX_CONSECUTIVE_AUTH_FAILURES {
        peer.enqueue(tonic::Code::Unauthenticated).await;
        assert!(call(client, Rpc::Bidi).await.is_err());
    }
}

async fn assert_locked(client: &AuthMiddleware<Arc<Peer>>) {
    for rpc in [Rpc::Unary, Rpc::Stream, Rpc::Bidi] {
        assert!(matches!(
            call(client, rpc).await,
            Err(ApiClientError::Auth(AuthError::Exhausted))
        ));
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn lockout_blocks_all_paths_and_set_clears_it() {
    let (client, peer, callback) = fixture();
    exhaust(&client, &peer).await;
    assert_locked(&client).await;
    assert_eq!(
        callback.calls.load(Ordering::SeqCst),
        MAX_CONSECUTIVE_AUTH_FAILURES as usize
    );
    assert_eq!(
        peer.sent.lock().await.len(),
        MAX_CONSECUTIVE_AUTH_FAILURES as usize
    );
    client.handle.set(credential("manual")).await;
    call(&client, Rpc::Unary).await?;
    assert_eq!(
        callback.calls.load(Ordering::SeqCst),
        MAX_CONSECUTIVE_AUTH_FAILURES as usize
    );
    assert_eq!(
        peer.sent.lock().await.last()?.0.headers["authorization"],
        "manual"
    );
}

#[rstest::rstest]
#[case(false, false)]
#[case(true, false)]
#[case(false, true)]
#[xmtp_common::test(unwrap_try = true)]
async fn cooldown_allows_one_probe(#[case] rejected: bool, #[case] callback_fails: bool) {
    let (client, peer, callback) = fixture();
    exhaust(&client, &peer).await;
    xmtp_common::time::sleep(AUTH_LOCKOUT_COOLDOWN).await;
    *callback.fail.lock().await = callback_fails;
    if rejected {
        peer.enqueue(tonic::Code::Unauthenticated).await;
    }
    // `exhaust` ran the callback once per allowed failure. The probe is the
    // one extra call the cool-down permits.
    let exhausted = MAX_CONSECUTIVE_AUTH_FAILURES as usize;
    let probe = exhausted + 1;
    let probe_started = Instant::now();
    let result = call(&client, Rpc::Unary).await;
    assert_eq!(callback.calls.load(Ordering::SeqCst), probe);
    if rejected || callback_fails {
        assert!(!result.expect_err("probe failure").is_retryable());
        assert_locked(&client).await;
        let until = client
            .handle
            .inner
            .state
            .lock()
            .await
            .locked_until
            .expect("probe lockout");
        assert!(until >= probe_started + AUTH_LOCKOUT_COOLDOWN);
        // The lockout restarts, so no further callback runs in this window.
        assert_eq!(callback.calls.load(Ordering::SeqCst), probe);
        assert_eq!(
            peer.sent.lock().await.len(),
            if callback_fails { exhausted } else { probe }
        );
    } else {
        result.expect("successful request");
        assert_eq!(client.handle.inner.state.lock().await.failures, 0);
        call(&client, Rpc::Bidi).await.expect("successful open");
        assert_eq!(callback.calls.load(Ordering::SeqCst), probe);
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn concurrent_failures_are_capped() {
    let (client, peer, callback) = fixture();
    // Send one call per allowed failure, all in flight at once. The count must
    // stop at the limit even though every response arrives after every send.
    let concurrent = MAX_CONSECUTIVE_AUTH_FAILURES as usize;
    let mut releases = Vec::new();
    for _ in 0..concurrent {
        let (release, gate) = oneshot::channel();
        releases.push(release);
        peer.replies.lock().await.push_back(Reply {
            code: tonic::Code::Unauthenticated,
            gate: Some(gate),
        });
    }
    let calls: Vec<_> = (0..concurrent)
        .map(|_| Box::pin(call(&client, Rpc::Bidi)))
        .collect();
    let mut pending = Box::pin(futures::future::join_all(calls));
    assert!(futures::poll!(&mut pending).is_pending());
    assert_eq!(peer.sent.lock().await.len(), concurrent);
    for release in releases {
        release.send(()).expect("pending response");
    }
    for result in pending.await {
        assert!(result.is_err());
    }
    assert_eq!(
        client.handle.inner.state.lock().await.failures,
        MAX_CONSECUTIVE_AUTH_FAILURES
    );
    assert_locked(&client).await;
    assert_eq!(callback.calls.load(Ordering::SeqCst), 1);
    assert_eq!(peer.sent.lock().await.len(), concurrent);
}

#[xmtp_common::test(unwrap_try = true)]
async fn shared_handles_debounce_stale_credentials_but_callbacks_do_not_share_state() {
    let (client, peer, callback) = fixture();
    peer.enqueue(tonic::Code::Unauthenticated).await;
    assert!(call(&client, Rpc::Bidi).await.is_err());
    let middlewares: Vec<_> = (0..10)
        .map(|_| {
            AuthMiddleware::new(
                peer.clone(),
                Some(callback.clone()),
                Some(client.handle.clone()),
            )
        })
        .collect();
    let (release, gate) = oneshot::channel();
    *callback.gate.lock().await = Some(gate);
    let mut pending = Box::pin(futures::future::join_all(
        middlewares.iter().map(|client| call(client, Rpc::Unary)),
    ));
    assert!(futures::poll!(&mut pending).is_pending());
    assert_eq!(callback.calls.load(Ordering::SeqCst), 2);
    assert_eq!(peer.sent.lock().await.len(), 1);
    release.send(()).expect("pending callback");
    for result in pending.await {
        result.expect("successful request");
    }
    assert_eq!(callback.calls.load(Ordering::SeqCst), 2);
    let middlewares: Vec<_> = (0..10)
        .map(|_| AuthMiddleware::new(peer.clone(), Some(callback.clone()), None))
        .collect();
    for result in
        futures::future::join_all(middlewares.iter().map(|client| call(client, Rpc::Unary))).await
    {
        result.expect("successful request");
    }
    assert_eq!(callback.calls.load(Ordering::SeqCst), 12);
}

#[xmtp_common::test(unwrap_try = true)]
async fn callback_errors_are_redacted_and_count_toward_lockout() {
    let (client, peer, callback) = fixture();
    peer.enqueue(tonic::Code::Unauthenticated).await;
    assert!(call(&client, Rpc::Bidi).await.is_err());
    *callback.fail.lock().await = true;
    // One failure is already counted, so this many more reach the limit. The
    // call that locks out reports `Exhausted`, so a long-lived transport waits
    // for the cool-down instead of reading the failure as permanent.
    let remaining = MAX_CONSECUTIVE_AUTH_FAILURES as usize - 1;
    for attempt in 1..=remaining {
        let error = call(&client, Rpc::Unary).await.err()?;
        assert!(!format!("{error:?}").contains("secret"));
        assert!(std::error::Error::source(&error).is_none());
        if attempt < remaining {
            assert!(matches!(
                error,
                ApiClientError::Auth(AuthError::CallbackFailed { .. })
            ));
            assert_eq!(error.to_string(), "auth callback failed");
            assert!(error.is_retryable());
        } else {
            assert!(matches!(error, ApiClientError::Auth(AuthError::Exhausted)));
            assert!(!error.is_retryable());
        }
    }
    assert_locked(&client).await;
    assert_eq!(peer.sent.lock().await.len(), 1);
    assert_eq!(
        callback.calls.load(Ordering::SeqCst),
        MAX_CONSECUTIVE_AUTH_FAILURES as usize
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn callback_cancellation_does_not_change_state() {
    let (client, peer, callback) = fixture();
    let (_release, gate) = oneshot::channel();
    *callback.gate.lock().await = Some(gate);
    let mut pending = Box::pin(call(&client, Rpc::Unary));
    assert!(futures::poll!(&mut pending).is_pending());
    drop(pending);
    assert!(client.handle.inner.current.get().is_none());
    let state = *client.handle.inner.state.lock().await;
    assert_eq!(state.generation, 0);
    assert_eq!(state.failures, 0);
    assert!(!state.stale);
    assert!(state.locked_until.is_none());
    call(&client, Rpc::Unary).await?;
    assert_eq!(callback.calls.load(Ordering::SeqCst), 2);
    assert_eq!(peer.sent.lock().await.len(), 1);
}

#[xmtp_common::test(unwrap_try = true)]
async fn set_during_a_refetch_wins() {
    let (client, peer, callback) = fixture();
    peer.enqueue(tonic::Code::Unauthenticated).await;
    assert!(call(&client, Rpc::Bidi).await.is_err());
    let (release, gate) = oneshot::channel();
    *callback.gate.lock().await = Some(gate);
    let mut pending = Box::pin(call(&client, Rpc::Unary));
    assert!(futures::poll!(&mut pending).is_pending());
    // `set` completes while the callback is still running. It must not wait for
    // the state lock, or a callback that pushes through the handle deadlocks.
    client.handle.set(credential("manual")).await;
    release.send(()).expect("pending callback");
    pending.await?;
    // The newer credential from `set` wins: the in-flight refetch discards its
    // own result rather than overwriting the one the handle just stored.
    assert_eq!(
        peer.sent.lock().await.last()?.0.headers["authorization"],
        "manual"
    );
    let calls_after_set = callback.calls.load(Ordering::SeqCst);
    call(&client, Rpc::Unary).await?;
    assert_eq!(
        peer.sent.lock().await.last()?.0.headers["authorization"],
        "manual"
    );
    // The stored credential is fresh, so the next request runs no callback.
    assert_eq!(callback.calls.load(Ordering::SeqCst), calls_after_set);
}

#[xmtp_common::test(unwrap_try = true)]
async fn permission_denied_and_other_errors_pass_through() {
    let (client, peer, callback) = fixture();
    for code in [tonic::Code::PermissionDenied, tonic::Code::Unavailable] {
        for rpc in [Rpc::Unary, Rpc::Stream, Rpc::Bidi] {
            peer.enqueue(code).await;
            let error = call(&client, rpc).await.err()?;
            assert_eq!(grpc_status(&error)?.code(), code);
        }
    }
    assert_eq!(callback.calls.load(Ordering::SeqCst), 1);
    let state = *client.handle.inner.state.lock().await;
    assert!(!state.stale);
    assert_eq!(state.failures, 0);
}

#[xmtp_common::test(unwrap_try = true)]
async fn handle_only_rejections_never_lock_out() {
    let peer = Arc::new(Peer::default());
    let handle = AuthHandle::new();
    let client = AuthMiddleware::new(peer.clone(), None, Some(handle.clone()));
    assert!(matches!(
        call(&client, Rpc::Unary).await,
        Err(ApiClientError::Auth(AuthError::MissingCredential))
    ));
    handle.set(credential("manual")).await;
    for rpc in [
        Rpc::Unary,
        Rpc::Stream,
        Rpc::Bidi,
        Rpc::Unary,
        Rpc::Stream,
        Rpc::Bidi,
    ] {
        peer.enqueue(tonic::Code::Unauthenticated).await;
        assert!(matches!(
            call(&client, rpc).await,
            Err(ApiClientError::Auth(AuthError::CredentialRejected {
                retryable: false
            }))
        ));
    }
    assert_eq!(peer.sent.lock().await.len(), 6);
    let state = *handle.inner.state.lock().await;
    assert!(state.stale);
    assert_eq!(state.failures, 0);
    assert!(state.locked_until.is_none());
}

/// The lockout must be out of reach of one caller request. `Retry::default()`
/// makes 6 attempts and the middleware replays each one, so a single call can
/// count twice that many failures. If this fails, raise the lockout limit or
/// lower the retry budget; do not weaken the assertion.
#[xmtp_common::test(unwrap_try = true)]
async fn retry_budget_cannot_reach_the_auth_lockout() {
    let attempts = xmtp_common::Retry::default().retries() + 1;
    let worst_case = attempts * 2;
    assert!(
        (MAX_CONSECUTIVE_AUTH_FAILURES as usize) > worst_case,
        "lockout at {MAX_CONSECUTIVE_AUTH_FAILURES} is reachable by one call worth {worst_case} failures"
    );
}

/// A callback may push its credential through the handle instead of returning
/// it. That must not deadlock: the state lock cannot be held while the callback
/// runs, because `AuthHandle::set` takes the same lock and tokio's mutex is not
/// reentrant.
#[xmtp_common::test(unwrap_try = true)]
async fn callback_may_use_the_handle_without_deadlocking() {
    struct SetsThroughHandle {
        handle: Mutex<Option<AuthHandle>>,
        calls: AtomicUsize,
    }
    #[xmtp_common::async_trait]
    impl AuthCallback for SetsThroughHandle {
        async fn on_auth_required(&self) -> Result<Credential, BoxDynError> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            let handle = self.handle.lock().await.clone().expect("handle");
            handle.set(credential(&format!("pushed-{call}"))).await;
            Ok(credential(&format!("pushed-{call}")))
        }
    }

    let peer = Arc::new(Peer::default());
    let handle = AuthHandle::new();
    let callback = Arc::new(SetsThroughHandle {
        handle: Mutex::new(Some(handle.clone())),
        calls: AtomicUsize::new(0),
    });
    let client = AuthMiddleware::new(peer.clone(), Some(callback.clone()), Some(handle));
    // Without a timeout a deadlock hangs the whole test binary.
    xmtp_common::time::timeout(xmtp_common::time::Duration::from_secs(5), async {
        call(&client, Rpc::Unary).await
    })
    .await
    .expect("get_credential must not hold the state lock across the callback")?;
    assert_eq!(callback.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        peer.sent.lock().await.last()?.0.headers["authorization"],
        "pushed-1"
    );
}

xmtp_common::if_native! {
    #[xmtp_common::test(unwrap_try = true)]
    async fn transport_keeps_cold_open_and_established_reopen_policy() {
        use crate::queries::{BackendBinding, BidiConnection};
        use crate::queries::{BidiTransport, LeaseEvent, OpenError};
        use xmtp_proto::{backend_v1::subscribe_response, types::Topic};

        let (client, peer, callback) = fixture();
        let client = Arc::new(client);
        let servers = Arc::new(std::sync::Mutex::new(VecDeque::new()));
        let sink = servers.clone();
        let transport = BidiTransport::<BackendBinding>::new(move |initial| {
            let client = client.clone();
            let sink = sink.clone();
            async move {
                if let Err(error) = call(&*client, Rpc::Bidi).await {
                    return Err(OpenError::new(error));
                }
                let (api, server) = crate::test::bidi::mock_pair();
                server.send(subscribe_response::Response::Started(subscribe_response::Started {
                    keepalive_interval_ms: 30_000,
                }));
                sink.lock().expect("server queue").push_back(server);
                BidiConnection::open(&api, initial).await.map_err(OpenError::new)
            }
        }, false);
        let topic = Topic::new_group_message([7; 16]);
        peer.enqueue(tonic::Code::Unauthenticated).await;
        assert!(transport.lease(vec![(topic.clone(), 0)], 8).await.is_err());
        assert_eq!(callback.calls.load(Ordering::SeqCst), 1);
        assert_eq!(peer.sent.lock().await.len(), 1);
        let mut lease = transport.lease(vec![(topic, 0)], 8).await?;
        let mut server = servers.lock().expect("server queue").pop_front()?;
        let update = server.next_mutate().await;
        server.ack_empty(update.id);
        assert!(matches!(lease.next().await, Some(LeaseEvent::CatchUpComplete)));
        peer.enqueue(tonic::Code::Unauthenticated).await;
        drop(server);
        let mut reopened = xmtp_common::wait_for_some(|| async {
            servers.lock().expect("server queue").pop_front()
        }).await?;
        let update = reopened.next_mutate().await;
        reopened.ack_empty(update.id);
        assert_eq!(callback.calls.load(Ordering::SeqCst), 3);
        assert_eq!(peer.sent.lock().await.len(), 4);
    }

    /// A lockout must pause the wire, not end it. Reconnect backoff starts at
    /// 100 ms, so repeated reopen rejections reach the failure limit in well
    /// under a second. A shutdown there would lose every subscription for the
    /// life of the process, because nothing restarts the transport task.
    #[xmtp_common::test(unwrap_try = true)]
    async fn transport_waits_out_the_auth_lockout_instead_of_shutting_down() {
        use crate::queries::{BackendBinding, BidiConnection};
        use crate::queries::{BidiTransport, LeaseEvent, OpenError};
        use xmtp_proto::{backend_v1::subscribe_response, types::Topic};

        let (client, peer, callback) = fixture();
        // A second middleware on the same handle drives the shared state to
        // lockout without waiting for the transport's reconnect backoff.
        let client_for_lockout =
            AuthMiddleware::new(peer.clone(), Some(callback.clone()), Some(client.handle.clone()));
        let client = Arc::new(client);
        let servers = Arc::new(std::sync::Mutex::new(VecDeque::new()));
        let sink = servers.clone();
        let transport = BidiTransport::<BackendBinding>::new(move |initial| {
            let client = client.clone();
            let sink = sink.clone();
            async move {
                if let Err(error) = call(&*client, Rpc::Bidi).await {
                    return Err(OpenError::new(error));
                }
                let (api, server) = crate::test::bidi::mock_pair();
                server.send(subscribe_response::Response::Started(subscribe_response::Started {
                    keepalive_interval_ms: 30_000,
                }));
                sink.lock().expect("server queue").push_back(server);
                BidiConnection::open(&api, initial).await.map_err(OpenError::new)
            }
        }, false);

        // Establish a wire, then lock the credential out and kill the wire, so
        // the reopen meets the cool-down. Reaching the limit through reopens
        // alone would take minutes, because reconnect backoff doubles.
        let topic = Topic::new_group_message([9; 16]);
        let mut lease = transport.lease(vec![(topic, 0)], 8).await?;
        let mut server = servers.lock().expect("server queue").pop_front()?;
        let update = server.next_mutate().await;
        server.ack_empty(update.id);
        assert!(matches!(lease.next().await, Some(LeaseEvent::CatchUpComplete)));
        let before = callback.calls.load(Ordering::SeqCst);
        exhaust(&client_for_lockout, &peer).await;
        assert!(matches!(
            call(&client_for_lockout, Rpc::Unary).await,
            Err(ApiClientError::Auth(AuthError::Exhausted))
        ));
        drop(server);

        // The cool-down ends and the next reopen succeeds on its own. Nothing
        // outside the transport acts, so this fails if the wire shut down.
        let mut reopened = xmtp_common::wait_for_some(|| async {
            servers.lock().expect("server queue").pop_front()
        }).await?;
        let update = reopened.next_mutate().await;
        reopened.ack_empty(update.id);
        assert!(callback.calls.load(Ordering::SeqCst) > before);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn backend_without_auth_accepts_callback_on_all_rpc_paths() {
        use prost::Message;
        use xmtp_api_grpc::GrpcClient;
        use xmtp_proto::{api_client::{ApiBuilder, NetConnectConfig}, backend_v1 as wire, types::Topic};
        let host = std::env::var("XMTP_BACKEND_URL")
            .unwrap_or_else(|_| xmtp_configuration::BACKEND_TEST_URL.into());
        let mut builder = GrpcClient::builder();
        builder.set_host(url::Url::parse(&host)?);
        let callback = Arc::new(Callback::default());
        let client = AuthMiddleware::new(builder.build()?, Some(callback.clone()), None);
        let topic = wire::TopicQuery { topic: Some(wire::Topic {
            topic: Topic::new_group_message(xmtp_common::rand_array::<16>()).cloned_vec(),
        }), cursor: None };
        let path = http::uri::PathAndQuery::from_static("/xmtp.backend.v1.QueryService/Query");
        for _ in 0..3 {
            let body = wire::QueryRequest { queries: vec![topic.clone()], limit: 1 }.encode_to_vec().into();
            let response = client.request(http::Request::builder(), path.clone(), body).await?;
            let response = wire::QueryResponse::decode(response.into_body())?;
            assert!(response.envelopes.is_empty());
        }
        let body = wire::SubscribeStaticRequest { topics: vec![topic] }.encode_to_vec().into();
        let response = client.stream(http::Request::builder(),
            http::uri::PathAndQuery::from_static("/xmtp.backend.v1.SubscriptionService/SubscribeStatic"), body).await?;
        let first = response.into_body().next().await??;
        assert!(matches!(wire::SubscribeStaticResponse::decode(first)?.response,
            Some(wire::subscribe_static_response::Response::Started(_))));
        let response = client.bidi_stream(http::Request::builder(),
            http::uri::PathAndQuery::from_static("/xmtp.backend.v1.SubscriptionService/Subscribe"),
            Box::pin(futures::stream::pending())).await?;
        let first = response.into_body().next().await??;
        assert!(matches!(wire::SubscribeResponse::decode(first)?.response,
            Some(wire::subscribe_response::Response::Started(_))));
        assert_eq!(callback.calls.load(Ordering::SeqCst), 1);
    }
}

xmtp_common::if_wasm! {
    #[xmtp_common::test(unwrap_try = true)]
    async fn wasm_bidi_keeps_unsupported_error() {
        use xmtp_api_grpc::GrpcClient;
        use xmtp_proto::api_client::{ApiBuilder, NetConnectConfig};
        let mut builder = GrpcClient::builder();
        builder.set_host(url::Url::parse(xmtp_configuration::BACKEND_TEST_URL)?);
        let client = AuthMiddleware::new(builder.build()?, Some(Arc::new(Callback::default())), None);
        let error = call(&client, Rpc::Bidi).await.err()?;
        assert!(matches!(error, ApiClientError::OtherUnretryable(_)));
        assert!(error.to_string().contains("not supported"));
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn concurrent_callback_errors_stop_at_the_limit() {
    let (client, peer, callback) = fixture();
    *callback.fail.lock().await = true;
    let (release, gate) = oneshot::channel();
    *callback.gate.lock().await = Some(gate);
    // Run more calls than the limit, so some must be refused after lockout.
    const EXTRA: usize = 7;
    let limit = MAX_CONSECUTIVE_AUTH_FAILURES as usize;
    let mut pending = Box::pin(futures::future::join_all(
        (0..limit + EXTRA).map(|_| call(&client, Rpc::Unary)),
    ));
    assert!(futures::poll!(&mut pending).is_pending());
    release.send(()).expect("pending callback");
    let results = pending.await;
    // The callback runs exactly `limit` times. Every later call is refused
    // without running it, so concurrency cannot push the count past the limit.
    // The failure that locks out reports `Exhausted`, like the refusals do.
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(
                result,
                Err(ApiClientError::Auth(AuthError::CallbackFailed { .. }))
            ))
            .count(),
        limit - 1
    );
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Err(ApiClientError::Auth(AuthError::Exhausted))))
            .count(),
        EXTRA + 1
    );
    assert_eq!(callback.calls.load(Ordering::SeqCst), limit);
    assert!(peer.sent.lock().await.is_empty());
}

#[xmtp_common::test(unwrap_try = true)]
async fn cancelled_probe_keeps_lockout_state_until_next_probe() {
    let (client, peer, callback) = fixture();
    exhaust(&client, &peer).await;
    xmtp_common::time::sleep(AUTH_LOCKOUT_COOLDOWN).await;
    let before = *client.handle.inner.state.lock().await;
    let (_release, gate) = oneshot::channel();
    *callback.gate.lock().await = Some(gate);
    let mut pending = Box::pin(call(&client, Rpc::Unary));
    assert!(futures::poll!(&mut pending).is_pending());
    drop(pending);
    let after = *client.handle.inner.state.lock().await;
    assert_eq!(after.generation, before.generation);
    assert_eq!(after.failures, before.failures);
    assert_eq!(after.stale, before.stale);
    assert_eq!(after.locked_until, before.locked_until);
    assert_eq!(
        peer.sent.lock().await.len(),
        MAX_CONSECUTIVE_AUTH_FAILURES as usize
    );
    call(&client, Rpc::Unary).await?;
    assert_eq!(
        callback.calls.load(Ordering::SeqCst),
        MAX_CONSECUTIVE_AUTH_FAILURES as usize + 2
    );
    assert_eq!(client.handle.inner.state.lock().await.failures, 0);
}
