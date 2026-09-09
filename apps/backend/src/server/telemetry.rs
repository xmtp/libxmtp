//! gRPC completion telemetry outside protocol conversion, with status capture inside it.

use crate::telemetry::{self, RpcLabels};
use bytes::Bytes;
use http::{Method, Request, Response, header::CONTENT_TYPE};
use http_body::{Body as HttpBody, Frame, SizeHint};
use opentelemetry::trace::TraceContextExt;
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
use xmtp_logging::propagation;

mod status;
pub(super) use status::GrpcStatusLayer;
use status::StatusSlot;

#[cfg(test)]
mod tests;

/// A server-generated identifier. Caller-provided correlation headers are ignored.
#[derive(Clone, Copy)]
pub(crate) struct RequestId(pub uuid::Uuid);

#[derive(Clone)]
pub(crate) struct GrpcTelemetryLayer(pub bool);

impl<S> Layer<S> for GrpcTelemetryLayer {
    type Service = GrpcTelemetry<S>;
    fn layer(&self, inner: S) -> Self::Service {
        GrpcTelemetry {
            inner,
            enabled: self.0,
        }
    }
}

#[derive(Clone)]
pub(crate) struct GrpcTelemetry<S> {
    inner: S,
    enabled: bool,
}

struct RequestLog {
    id: uuid::Uuid,
    method: String,
    started: Instant,
    request_bytes: AtomicU64,
    response_bytes: AtomicU64,
    dispatch: tracing::Dispatch,
    span: tracing::Span,
    enabled: bool,
    labels: RpcLabels,
    status: StatusSlot,
    trace_id: Option<String>,
}

/// Own completion exactly once, independently of the input body's lifetime.
/// Moving this guard into the response body keeps streaming requests open in logs.
struct Completion {
    state: Arc<RequestLog>,
    ended: bool,
}
impl Drop for Completion {
    fn drop(&mut self) {
        let state = &self.state;
        let code = state.status.0.lock().unwrap_or(if self.ended {
            tonic::Code::Unknown
        } else {
            tonic::Code::Cancelled
        });
        let elapsed = state.started.elapsed();
        let request_bytes = state.request_bytes.load(Ordering::Relaxed);
        let response_bytes = state.response_bytes.load(Ordering::Relaxed);
        telemetry::grpc_completed(state.labels, code, elapsed, request_bytes, response_bytes);
        tracing::dispatcher::with_default(&state.dispatch, || {
            state.span.record("rpc.grpc.status_code", code as i32);
            if !state.enabled {
                return;
            }
            let _span = state.span.enter();
            let grpc_code = telemetry::grpc_code(code);
            // Separate call sites omit trace_id completely when no context exists.
            if let Some(trace_id) = &state.trace_id {
                tracing::info!(request_id = %state.id, method = %state.method,
                    duration_ms = elapsed.as_millis() as u64,
                    request_size_bytes = request_bytes, response_size_bytes = response_bytes,
                    grpc_code, trace_id, "gRPC request completed");
            } else {
                tracing::info!(request_id = %state.id, method = %state.method,
                    duration_ms = elapsed.as_millis() as u64,
                    request_size_bytes = request_bytes, response_size_bytes = response_bytes,
                    grpc_code, "gRPC request completed");
            }
        });
    }
}

impl<S, B> Service<Request<B>> for GrpcTelemetry<S>
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
        let labels = RpcLabels::from_path(request.uri().path());
        let span = tracing::info_span!("grpc_request", request_id = %id,
            otel.kind = "server", otel.name = %format_args!("{}/{}", labels.service, labels.method),
            rpc.system = "grpc", rpc.service = labels.service, rpc.method = labels.method,
            rpc.grpc.status_code = tracing::field::Empty);
        let incoming = propagation::extract(request.headers());
        let mut trace_id = incoming
            .as_ref()
            .map(|context| context.span().span_context().trace_id().to_string());
        if let Some(parent) = incoming {
            propagation::set_parent(&span, parent);
        }
        if trace_id.is_none() {
            let mut headers = http::HeaderMap::new();
            propagation::inject(&span, &mut headers);
            trace_id = propagation::extract(&headers)
                .map(|context| context.span().span_context().trace_id().to_string());
        }
        let status = StatusSlot::default();
        telemetry::grpc_started(labels);
        let dispatch = tracing::dispatcher::get_default(Clone::clone);
        let state = Arc::new(RequestLog {
            id,
            method: request.uri().path().to_owned(),
            started: Instant::now(),
            request_bytes: AtomicU64::new(0),
            response_bytes: AtomicU64::new(0),
            dispatch: dispatch.clone(),
            span: span.clone(),
            enabled: self.enabled,
            labels,
            status: status.clone(),
            trace_id,
        });
        let completion = Completion {
            state: state.clone(),
            ended: false,
        };
        let mut request = request.map(|inner| Body::new(CountBody { inner, state }));
        request.extensions_mut().insert(RequestId(id));
        request.extensions_mut().insert(status.clone());
        let future = span.in_scope(|| self.inner.call(request));
        Box::pin(
            async move {
                let mut response = future.await?;
                status.record(response.headers());
                response.headers_mut().insert(
                    "x-request-id",
                    id.to_string().parse().expect("UUID is a header value"),
                );
                Ok(response.map(|inner| {
                    let mut completion = completion;
                    completion.ended = inner.is_end_stream();
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
        if let Poll::Ready(Some(Ok(frame))) = &frame {
            count_data(&this.state.request_bytes, frame);
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

    /// Count emitted data before finishing on EOS or body failure. HTTP trailers
    /// are not data; gRPC-Web trailer frames encoded in the body are counted.
    /// Dropping an unfinished body also
    /// drops its guard. Taking the guard prevents a second completion event.
    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
        let mut this = self.project();
        let frame = this.inner.as_mut().poll_frame(cx);
        if let Poll::Ready(Some(Ok(frame))) = &frame
            && let Some(completion) = this.completion.as_ref()
        {
            count_data(&completion.state.response_bytes, frame);
            if let Some(trailers) = frame.trailers_ref() {
                completion.state.status.record(trailers);
            }
        }
        if matches!(frame, Poll::Ready(None) | Poll::Ready(Some(Err(_))))
            || this.inner.is_end_stream()
        {
            if let Some(completion) = this.completion.as_mut() {
                completion.ended = true;
            }
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

/// Count body data without retaining it or including HTTP headers and trailers.
fn count_data(counter: &AtomicU64, frame: &Frame<Bytes>) {
    if let Some(data) = frame.data_ref() {
        let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |bytes| {
            Some(bytes.saturating_add(data.len() as u64))
        });
    }
}
