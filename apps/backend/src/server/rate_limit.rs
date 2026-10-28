//! Per-caller rate limits ([RATE](../../../../docs/specs/RATE-backend-rate-limits.md)).
//!
//! The layer charges 1 `request` unit to every request on a charged path,
//! before tonic decodes it, and gives the request a [`Charger`]. Every other
//! category is spent whole, by the one handler step that does the work it
//! pays for, through that charger. A denied spend charges nothing and fails
//! the request with `RESOURCE_EXHAUSTED`, the category, and the delay after
//! which the same spend is admitted.

use crate::{
    auth::AuthContext,
    config::{CallerKey, Category},
};
use futures::future::BoxFuture;
use http::{Request, Response};
use std::{
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};
use tonic::{Code, Status, body::Body, transport::server::TcpConnectInfo};
use tonic_types::{ErrorDetails, StatusExt};
use tower::{Layer, Service};
use xmtp_rate_limit::Decision;

mod caller;
pub(crate) mod charges;
mod limiter;
#[cfg(test)]
mod tests;

pub(crate) use limiter::{Limiter, LimiterClock, limiter, limiter_on};

/// Spends one request's charges for its caller. The rate-limit layer puts one
/// in the extensions of every request it admits on a charged path; handlers
/// and the stream session spend through it and nothing else, so every denial
/// gets the same rejection, metric, and log line.
#[derive(Clone)]
pub(crate) struct Charger(Option<Arc<Caller>>);

struct Caller {
    limiter: Arc<Limiter>,
    key: CallerKey,
    /// `key` as the limiter hashes it.
    bytes: Box<[u8]>,
    /// The categories the charge table gives the request's path.
    categories: &'static [Category],
    request_id: uuid::Uuid,
    identifier: Arc<str>,
}

impl Charger {
    fn new(
        limiter: Arc<Limiter>,
        key: CallerKey,
        categories: &'static [Category],
        request_id: uuid::Uuid,
        identifier: Arc<str>,
    ) -> Self {
        let mut bytes = Vec::new();
        key.to_bytes(&mut bytes);
        Self(Some(Arc::new(Caller {
            limiter,
            key,
            bytes: bytes.into_boxed_slice(),
            categories,
            request_id,
            identifier,
        })))
    }

    /// A charger for `key` that may spend every category, for tests that
    /// read what a caller has left or call a handler without the layer.
    #[cfg(test)]
    pub(crate) fn for_test(limiter: &Arc<Limiter>, key: CallerKey) -> Self {
        Self::new(
            limiter.clone(),
            key,
            &Category::ALL,
            uuid::Uuid::new_v4(),
            Arc::from("org.xmtp.test"),
        )
    }

    /// The charger the layer gave `request`. A request without one was not
    /// charged by the layer, because rate limits are off, so it admits every
    /// spend.
    pub(crate) fn of<T>(request: &tonic::Request<T>) -> Self {
        request
            .extensions()
            .get::<Self>()
            .cloned()
            .unwrap_or(Self(None))
    }

    /// Spend `cost` units of `category` for the request's caller. A cost of 0
    /// is admitted without a spend. A denial charges nothing, counts once in
    /// `xmtp_rate_limit_rejections_total`, logs one `debug` event that names
    /// no caller, and returns the rejection to send.
    // implements: RATE-010
    pub(crate) async fn charge(&self, category: Category, cost: usize) -> Result<(), Status> {
        let Some(caller) = &self.0 else {
            return Ok(());
        };
        if cost == 0 {
            return Ok(());
        }
        debug_assert!(
            caller.categories.contains(&category),
            "the charge table does not give this path {}",
            category.as_str()
        );
        let cost = u32::try_from(cost).unwrap_or(u32::MAX);
        match caller
            .limiter
            .spend(&caller.key, &caller.bytes, category, cost)
            .await
        {
            Decision::Allowed => Ok(()),
            Decision::Denied { retry_after } => {
                let auth_type = caller.key.auth_type();
                crate::telemetry::rate_limit_rejection(category, auth_type);
                tracing::debug!(
                    request_id = %caller.request_id,
                    identifier = %caller.identifier,
                    category = category.as_str(),
                    auth_type = auth_type.as_str(),
                    "rate limit exceeded"
                );
                Err(rejection(category, retry_after))
            }
        }
    }
}

/// The rejection of a denied spend: the category and the retry delay, in the
/// standard gRPC details, and a message that names no caller.
// implements: RATE-040
fn rejection(category: Category, retry_after: Duration) -> Status {
    let mut details = ErrorDetails::with_error_info(
        xmtp_configuration::RATE_LIMITED_REASON,
        xmtp_configuration::RATE_LIMITED_DOMAIN,
        [("category".to_owned(), category.as_str().to_owned())],
    );
    details.set_retry_info(Some(retry_after));
    Status::with_error_details(
        Code::ResourceExhausted,
        format!("rate limit exceeded for {}", category.as_str()),
        details,
    )
}

/// Install after auth and drain admission, so a request either rejects is
/// never charged, and before the count gate, so a denied `request` spend
/// decides the status over every bound the gate checks.
#[derive(Clone)]
pub(super) struct RateLimitLayer {
    pub limiter: Arc<Limiter>,
    /// Named on every denial log, so one log stream can carry several
    /// deployments.
    pub identifier: Arc<str>,
}
impl<S> Layer<S> for RateLimitLayer {
    type Service = RateLimit<S>;
    fn layer(&self, inner: S) -> Self::Service {
        RateLimit {
            inner,
            limiter: self.limiter.clone(),
            identifier: self.identifier.clone(),
        }
    }
}

#[derive(Clone)]
pub(super) struct RateLimit<S> {
    inner: S,
    limiter: Arc<Limiter>,
    identifier: Arc<str>,
}
impl<S> Service<Request<Body>> for RateLimit<S>
where
    S: Service<Request<Body>, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    /// An exempt path, or one no service implements, goes to tonic
    /// uncharged. Any other request spends 1 `request` unit for its caller
    /// before its body is read; a denial answers at once, and an admitted
    /// request carries its [`Charger`] to the handler.
    // implements: RATE-011, RATE-014
    fn call(&mut self, mut request: Request<Body>) -> Self::Future {
        let Some(categories) = charges::charged(request.uri().path()) else {
            return Box::pin(self.inner.call(request));
        };
        let peer = request
            .extensions()
            .get::<TcpConnectInfo>()
            .and_then(TcpConnectInfo::remote_addr)
            .map(|address| address.ip());
        let key = caller::caller_key(
            request.extensions().get::<AuthContext>(),
            peer,
            request.headers(),
            self.limiter.trusted_proxies(),
        );
        let request_id = request
            .extensions()
            .get::<super::telemetry::RequestId>()
            .map(|id| id.0)
            .unwrap_or_else(uuid::Uuid::new_v4);
        let charger = Charger::new(
            self.limiter.clone(),
            key,
            categories,
            request_id,
            self.identifier.clone(),
        );
        // The readied service takes this request; the clone takes the next.
        let ready = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, ready);
        Box::pin(async move {
            if let Err(status) = charger.charge(Category::Request, 1).await {
                return Ok(status.into_http());
            }
            request.extensions_mut().insert(charger);
            inner.call(request).await
        })
    }
}
