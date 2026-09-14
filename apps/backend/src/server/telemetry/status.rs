//! Capture native status before gRPC-Web conversion.
use super::*;

/// The first transport status wins, including trailers-only responses.
#[derive(Clone, Default)]
pub(super) struct StatusSlot(pub(super) Arc<parking_lot::Mutex<Option<tonic::Code>>>);
impl StatusSlot {
    pub(super) fn record(&self, headers: &http::HeaderMap) {
        if let Some(code) = headers
            .get("grpc-status")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<i32>().ok())
        {
            self.0.lock().get_or_insert(tonic::Code::from_i32(code));
        }
    }
}

#[derive(Clone, Copy)]
pub(in crate::server) struct GrpcStatusLayer;
impl<S> Layer<S> for GrpcStatusLayer {
    type Service = GrpcStatus<S>;
    fn layer(&self, inner: S) -> Self::Service {
        GrpcStatus { inner }
    }
}
#[derive(Clone)]
pub(in crate::server) struct GrpcStatus<S> {
    inner: S,
}
impl<S> Service<Request<Body>> for GrpcStatus<S>
where
    S: Service<Request<Body>, Response = Response<Body>>,
    S::Future: Send + 'static,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }
    fn call(&mut self, request: Request<Body>) -> Self::Future {
        let slot = request.extensions().get::<StatusSlot>().cloned();
        let notification = matches!(
            request.uri().path(),
            "/xmtp.backend.v1.NotificationService/Register"
                | "/xmtp.backend.v1.NotificationService/Unregister"
                | "/xmtp.backend.v1.NotificationService/UpdateSubscriptions"
        );
        let future = self.inner.call(request);
        Box::pin(async move {
            let mut response = future.await?;
            if notification {
                normalize_notification_decode(response.headers_mut());
            }
            if let Some(slot) = &slot {
                slot.record(response.headers());
            }
            Ok(response.map(|inner| {
                Body::new(StatusBody {
                    inner,
                    slot,
                    notification,
                })
            }))
        })
    }
}

/// Observe native trailers before GrpcWebLayer encodes them as body data.
/// This covers binary and base64 gRPC-Web without parsing or buffering payloads.
#[pin_project]
struct StatusBody<B> {
    #[pin]
    inner: B,
    slot: Option<StatusSlot>,
    notification: bool,
}
impl<B: HttpBody<Data = Bytes>> HttpBody for StatusBody<B> {
    type Data = Bytes;
    type Error = B::Error;
    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
        let this = self.project();
        let mut frame = this.inner.poll_frame(cx);
        if let Poll::Ready(Some(Ok(frame))) = &mut frame
            && let Some(trailers) = frame.trailers_mut()
        {
            if *this.notification {
                normalize_notification_decode(trailers);
            }
            if let Some(slot) = this.slot {
                slot.record(trailers);
            }
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

/// Prost reports malformed protobuf as INTERNAL before invoking the handler.
/// Rewrite only that decoder status on notification routes, before logs and
/// gRPC-Web conversion can observe its input-dependent message.
fn normalize_notification_decode(headers: &mut http::HeaderMap) {
    if let Some(status) = tonic::Status::from_header_map(headers)
        && status.code() == tonic::Code::Internal
        && (status
            .message()
            .starts_with("failed to decode Protobuf message")
            || status
                .message()
                .starts_with("protocol error: received message with")
            || status.message().starts_with("Error decompressing:")
            || matches!(
                status.message(),
                "Unexpected EOF decoding stream." | "Missing request message."
            ))
    {
        headers.insert("grpc-status", http::HeaderValue::from_static("3"));
        headers.insert(
            "grpc-message",
            http::HeaderValue::from_static(crate::service::notification::MALFORMED_REQUEST),
        );
        headers.remove("grpc-status-details-bin");
    }
}
