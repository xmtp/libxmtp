//! Request completion logging at the HTTP body boundary, before gRPC-Web decoding.

use bytes::Bytes;
use http::{Method, Request, Response, header::CONTENT_TYPE};
use http_body::{Body as HttpBody, Frame, SizeHint};
use pin_project::pin_project;
use std::{
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    task::{Context, Poll},
};
use tonic::body::Body;
use tower::{Layer, Service};
use tracing::{Instrument, instrument::WithSubscriber};
use xmtp_common::time::Instant;

#[cfg(test)]
mod tests;

/// A server-generated identifier. Caller-provided correlation headers are ignored.
#[derive(Clone, Copy)]
pub(crate) struct RequestId(pub uuid::Uuid);

#[derive(Clone)]
pub(crate) struct RequestLoggerLayer(pub bool);

impl<S> Layer<S> for RequestLoggerLayer {
    type Service = RequestLogger<S>;
    fn layer(&self, inner: S) -> Self::Service {
        RequestLogger {
            inner,
            enabled: self.0,
        }
    }
}

#[derive(Clone)]
pub(crate) struct RequestLogger<S> {
    inner: S,
    enabled: bool,
}

struct RequestLog {
    id: uuid::Uuid,
    method: String,
    started: Instant,
    bytes: AtomicU64,
    dispatch: tracing::Dispatch,
    span: tracing::Span,
    enabled: bool,
}

/// Own completion exactly once, independently of the input body's lifetime.
/// Moving this guard into the response body keeps streaming requests open in logs.
struct Completion(Arc<RequestLog>);
impl Drop for Completion {
    fn drop(&mut self) {
        let state = &self.0;
        if !state.enabled {
            return;
        }
        tracing::dispatcher::with_default(&state.dispatch, || {
            let _span = state.span.enter();
            tracing::info!(request_id = %state.id, method = %state.method,
                duration_ms = state.started.elapsed().as_millis() as u64,
                request_size_bytes = state.bytes.load(Ordering::Relaxed), "gRPC request completed");
        });
    }
}

impl<S, B> Service<Request<B>> for RequestLogger<S>
where
    S: Service<Request<Body>, Response = Response<Body>>,
    S::Future: Send + 'static,
    B: HttpBody<Data = Bytes> + Send + 'static,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    /// Count only consumed data frames. The guard also covers cancellation before
    /// response headers, and never buffers or trusts a declared content length.
    fn call(&mut self, request: Request<B>) -> Self::Future {
        if !is_grpc(&request) {
            return Box::pin(self.inner.call(request.map(Body::new)));
        }
        let id = uuid::Uuid::new_v4();
        let span = tracing::info_span!("grpc_request", request_id = %id);
        let dispatch = tracing::dispatcher::get_default(Clone::clone);
        let state = Arc::new(RequestLog {
            id,
            method: request.uri().path().to_owned(),
            started: Instant::now(),
            bytes: AtomicU64::new(0),
            dispatch: dispatch.clone(),
            span: span.clone(),
            enabled: self.enabled,
        });
        let completion = Completion(state.clone());
        let mut request = request.map(|inner| Body::new(CountBody { inner, state }));
        request.extensions_mut().insert(RequestId(id));
        let future = span.in_scope(|| self.inner.call(request));
        Box::pin(
            async move {
                let mut response = future.await?;
                response.headers_mut().insert(
                    "x-request-id",
                    id.to_string().parse().expect("UUID is a header value"),
                );
                Ok(response.map(|inner| {
                    Body::new(CompleteBody {
                        inner,
                        completion: Some(completion),
                    })
                }))
            }
            .instrument(span)
            .with_subscriber(dispatch),
        )
    }
}

fn is_grpc<B>(request: &Request<B>) -> bool {
    request.method() == Method::POST
        && request
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("application/grpc"))
}

#[pin_project]
struct CountBody<B> {
    #[pin]
    inner: B,
    state: Arc<RequestLog>,
}
impl<B: HttpBody<Data = Bytes>> HttpBody for CountBody<B> {
    type Data = Bytes;
    type Error = B::Error;
    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
        let this = self.project();
        let frame = this.inner.poll_frame(cx);
        if let Poll::Ready(Some(Ok(frame))) = &frame
            && let Some(data) = frame.data_ref()
        {
            let _ = this
                .state
                .bytes
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |bytes| {
                    Some(bytes.saturating_add(data.len() as u64))
                });
        }
        frame
    }
    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }
    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}

#[pin_project]
struct CompleteBody<B> {
    #[pin]
    inner: B,
    completion: Option<Completion>,
}
impl<B: HttpBody<Data = Bytes>> HttpBody for CompleteBody<B> {
    type Data = Bytes;
    type Error = B::Error;

    /// Finish on confirmed EOS or body failure; dropping an unfinished body also
    /// drops its guard. Taking the guard prevents a second completion event.
    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
        let mut this = self.project();
        let frame = this.inner.as_mut().poll_frame(cx);
        if matches!(frame, Poll::Ready(None) | Poll::Ready(Some(Err(_))))
            || this.inner.is_end_stream()
        {
            drop(this.completion.take());
        }
        frame
    }
    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }
    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}
