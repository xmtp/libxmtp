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

/// Paths served without a credential, whatever the auth settings are. Health
/// lets an orchestrator probe the process; configuration lets a client learn
/// what this deployment requires before it holds a credential. No other method
/// joins this list.
pub(super) const UNAUTHENTICATED_PREFIXES: [&str; 2] =
    ["/grpc.health.v1.", "/xmtp.backend.v1.ConfigurationService/"];

#[derive(Clone)]
pub(super) struct AuthLayer {
    pub verifier: Arc<Verifier>,
    /// Named on every rejection so one log stream can carry several deployments.
    pub identifier: Arc<str>,
}
#[cfg(test)]
impl AuthLayer {
    /// A layer with a fixed identifier, for tests that only vary the verifier.
    pub(super) fn for_test(verifier: Arc<Verifier>) -> Self {
        Self {
            verifier,
            identifier: Arc::from("org.xmtp.test"),
        }
    }
}

impl<S> Layer<S> for AuthLayer {
    type Service = Auth<S>;
    fn layer(&self, inner: S) -> Self::Service {
        Auth {
            inner,
            verifier: self.verifier.clone(),
            identifier: self.identifier.clone(),
        }
    }
}
#[derive(Clone)]
pub(super) struct Auth<S> {
    inner: S,
    verifier: Arc<Verifier>,
    identifier: Arc<str>,
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

    /// Admit the unauthenticated prefixes without a token. Every other path
    /// requires authentication, regardless of its HTTP method or content type.
    /// Streams are checked only here.
    fn call(&mut self, mut request: Request<Body>) -> Self::Future {
        let path = request.uri().path();
        if !UNAUTHENTICATED_PREFIXES
            .iter()
            .any(|prefix| path.starts_with(prefix))
        {
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
                        identifier = %self.identifier,
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
