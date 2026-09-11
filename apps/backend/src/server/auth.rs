//! Path-based admission for native gRPC and gRPC-Web.
use crate::auth::verify::Verifier;
use futures::future::BoxFuture;
use http::{Request, Response};
use std::{
    sync::Arc,
    task::{Context, Poll},
};
use tonic::body::Body;
use tower::{Layer, Service};

#[derive(Clone)]
pub(super) struct AuthLayer(pub Arc<Verifier>);
impl<S> Layer<S> for AuthLayer {
    type Service = Auth<S>;
    fn layer(&self, inner: S) -> Self::Service {
        Auth {
            inner,
            verifier: self.0.clone(),
        }
    }
}
#[derive(Clone)]
pub(super) struct Auth<S> {
    inner: S,
    verifier: Arc<Verifier>,
}
impl<S> Service<Request<Body>> for Auth<S>
where
    S: Service<Request<Body>, Response = Response<Body>>,
    S::Future: Send + 'static,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    /// Admit health paths without a token. Every other path requires authentication,
    /// regardless of its HTTP method or content type. Streams are checked only here.
    fn call(&mut self, mut request: Request<Body>) -> Self::Future {
        if !request.uri().path().starts_with("/grpc.health.v1.") {
            match self.verifier.verify(request.headers()) {
                Ok(context) => {
                    request.extensions_mut().insert(context);
                }
                Err(reason) => {
                    let request_id = request
                        .extensions()
                        .get::<super::telemetry::RequestId>()
                        .map(|id| id.0)
                        .unwrap_or_else(uuid::Uuid::new_v4);
                    tracing::debug!(
                        ?request_id,
                        reason = reason.label(),
                        "authentication rejected"
                    );
                    crate::telemetry::auth_rejection(reason);
                    return Box::pin(async move { Ok(reason.status().into_http()) });
                }
            }
        }
        Box::pin(self.inner.call(request))
    }
}
#[cfg(test)]
mod tests;
