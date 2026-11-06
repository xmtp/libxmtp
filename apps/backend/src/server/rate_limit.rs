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

pub(crate) use limiter::{Limiter, limiter};
#[cfg(any(test, feature = "test-utils"))]
pub(crate) use limiter::{LimiterClock, limiter_on};

/// Spends one request's charges for its caller. The rate-limit layer puts one
/// in the extensions of every request it admits on a charged path; handlers
/// and the stream session spend through it and nothing else, so every denial
/// gets the same rejection, metric, and log line.
#[derive(Clone)]
pub(crate) struct Charger {
    /// `None` while rate limits are off.
    caller: Option<Arc<Caller>>,
    /// A digest of the request message a handler charges for, kept while a
    /// test records spends.
    #[cfg(any(test, feature = "test-utils"))]
    request: Option<[u8; 32]>,
}

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
        Self {
            caller: Some(Arc::new(Caller {
                limiter,
                key,
                bytes: bytes.into_boxed_slice(),
                categories,
                request_id,
                identifier,
            })),
            #[cfg(any(test, feature = "test-utils"))]
            request: None,
        }
    }

    /// A charger that admits every spend, for a request rate limits do not
    /// cover.
    fn off() -> Self {
        Self {
            caller: None,
            #[cfg(any(test, feature = "test-utils"))]
            request: None,
        }
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

    /// A charger that admits every spend, as one is while rate limits are
    /// off, for tests that run a stream session without the layer.
    #[cfg(test)]
    pub(crate) fn unlimited() -> Self {
        Self::off()
    }

    /// The charger the layer gave `request`. A request without one was not
    /// charged by the layer, because rate limits are off, so it admits every
    /// spend.
    pub(crate) fn of<T: prost::Message>(request: &tonic::Request<T>) -> Self {
        #[allow(unused_mut)]
        let mut charger = Self::of_stream(request);
        #[cfg(any(test, feature = "test-utils"))]
        if charger
            .caller
            .as_ref()
            .is_some_and(|caller| caller.limiter.recording())
        {
            charger.request = Some(xmtp_common::sha256_array(
                &request.get_ref().encode_to_vec(),
            ));
        }
        charger
    }

    /// [`of`](Self::of) for a streaming request, whose messages arrive after
    /// it: its spends record no request digest.
    pub(crate) fn of_stream<T>(request: &tonic::Request<T>) -> Self {
        request
            .extensions()
            .get::<Self>()
            .cloned()
            .unwrap_or_else(Self::off)
    }

    /// The time on the limiter's clock, which retry delays count on, or
    /// `None` while rate limits are off. A stream times its rent periods and
    /// paused reads on it.
    pub(crate) fn now(&self) -> Option<Duration> {
        self.caller.as_ref().map(|caller| caller.limiter.now())
    }

    /// Spend `cost` units of `category` for the request's caller. A cost of 0
    /// is admitted without a spend. A denial charges nothing, counts once in
    /// `xmtp_rate_limit_rejections_total`, logs one `debug` event that names
    /// no caller, and returns the rejection to send.
    pub(crate) async fn charge(&self, category: Category, cost: usize) -> Result<(), Status> {
        self.charge_covering(category, cost, cost)
            .await
            .map_err(|denied| denied.status)
    }

    /// [`charge`](Self::charge) `cost` units of `category` for a step whose
    /// retry spends `covered` units, at least `cost`: a stream that ends on
    /// the denial makes its replacement pay its open again (RATE-032,
    /// RATE-034). A denial reports the delay after which one spend of
    /// `covered` units is admitted. It reads that delay from a second spend
    /// of `covered` units, which every limit that denied `cost` also denies,
    /// and a denied spend charges nothing (RATE-022). The second spend is
    /// admitted only when the caller's state changed in between, as when
    /// the clock passed the first denial's delay; the step then goes ahead,
    /// having paid `covered` units. When `covered` is more than a limit's
    /// count, the delay runs to full allowance, when one spend of `covered`
    /// is admitted (RATE-020), which a replacement stream that opens before
    /// it tops up never makes.
    // implements: RATE-010
    pub(crate) async fn charge_covering(
        &self,
        category: Category,
        cost: usize,
        covered: usize,
    ) -> Result<(), Denied> {
        let Some(caller) = &self.caller else {
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
        debug_assert!(covered >= cost, "a retry covers the denied spend");
        let mut decision = caller.spend(category, cost).await;
        if matches!(decision, Decision::Denied { .. }) && covered > cost {
            decision = caller.spend(category, covered).await;
        }
        #[cfg(any(test, feature = "test-utils"))]
        caller.limiter.record(crate::test_support::SpendRecord {
            caller: caller.key.clone(),
            category,
            cost: u32::try_from(cost).unwrap_or(u32::MAX),
            retry_after: match decision {
                Decision::Allowed => None,
                Decision::Denied { retry_after } => Some(retry_after),
            },
            at: std::time::Instant::now(),
            request: self.request,
        });
        match decision {
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
                Err(Denied {
                    status: rejection(category, retry_after),
                    retry_after,
                })
            }
        }
    }
}

impl Caller {
    async fn spend(&self, category: Category, cost: usize) -> Decision {
        let cost = u32::try_from(cost).unwrap_or(u32::MAX);
        self.limiter
            .spend(&self.key, &self.bytes, category, cost)
            .await
    }
}

/// A denied spend: the rejection to send and the retry delay it carries.
pub(crate) struct Denied {
    pub status: Status,
    pub retry_after: Duration,
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
