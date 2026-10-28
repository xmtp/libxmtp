//! The limiter the backend charges requests against: the in-memory table,
//! with the deployment's limits resolved for each caller.

use crate::config::{CallerKey, Category, RateLimitConfig};
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use xmtp_rate_limit::{Clock, Decision, Limit, MAX_LIMITS, MemoryLimiter, RateLimiter, Spend};

/// The limiter the backend charges requests against.
pub(crate) type Limiter = BackendLimiter<MemoryLimiter<LimiterClock>>;

/// The limiter for a validated configuration, or `None` while rate limits
/// are off: a disabled backend allocates no table and installs no layer.
pub(crate) fn limiter(config: &RateLimitConfig) -> Option<Arc<Limiter>> {
    limiter_on(
        config,
        LimiterClock::System(xmtp_rate_limit::SystemClock::new()),
    )
}

/// [`limiter`] on `clock`.
pub(crate) fn limiter_on(config: &RateLimitConfig, clock: LimiterClock) -> Option<Arc<Limiter>> {
    config.enabled.then(|| {
        Arc::new(BackendLimiter::new(
            MemoryLimiter::with_clock(budget_bytes(config.max_memory_mib), clock),
            config.clone(),
        ))
    })
}

fn budget_bytes(max_memory_mib: u32) -> usize {
    (max_memory_mib as usize).saturating_mul(1024 * 1024)
}

/// The limiter's time source. Tests set it by hand.
#[derive(Clone)]
pub(crate) enum LimiterClock {
    System(xmtp_rate_limit::SystemClock),
    #[cfg(test)]
    Manual(xmtp_rate_limit::ManualClock),
}
impl Clock for LimiterClock {
    fn now_ns(&self) -> u64 {
        match self {
            Self::System(clock) => clock.now_ns(),
            #[cfg(test)]
            Self::Manual(clock) => clock.now_ns(),
        }
    }
}

/// A limiter that counts the caller states it forgot before they were earned
/// back, to make room for another.
pub(crate) trait CountsEvictions {
    fn evictions(&self) -> u64;
}
impl<C: Clock> CountsEvictions for MemoryLimiter<C> {
    fn evictions(&self) -> u64 {
        MemoryLimiter::evictions(self)
    }
}

/// A limiter with the deployment's limits: it resolves the limits that apply
/// to a caller in a category and records evictions in the metric.
pub(crate) struct BackendLimiter<L> {
    pub(super) limiter: L,
    config: RateLimitConfig,
    /// The eviction count the metric holds.
    recorded_evictions: AtomicU64,
    /// Every spend decided, in order, for tests of a running server.
    #[cfg(any(test, feature = "test-utils"))]
    spends: parking_lot::Mutex<Vec<crate::test_support::SpendRecord>>,
}

impl<L: RateLimiter + CountsEvictions> BackendLimiter<L> {
    pub(crate) fn new(limiter: L, config: RateLimitConfig) -> Self {
        Self {
            limiter,
            config,
            recorded_evictions: AtomicU64::new(0),
            #[cfg(any(test, feature = "test-utils"))]
            spends: Default::default(),
        }
    }

    /// Spend `cost` units of `category` for `caller`, whose bytes are `key`,
    /// against the most specific limits that apply to it.
    pub(super) async fn spend(
        &self,
        caller: &CallerKey,
        key: &[u8],
        category: Category,
        cost: u32,
    ) -> Decision {
        let configured = self.config.limits(category, caller);
        // Configuration holds at most three limits per list; the limiter
        // matches state to limits by position, so the order is the file's.
        let mut limits = [Limit {
            count: 1,
            window: Duration::ZERO,
        }; MAX_LIMITS];
        let len = configured.len().min(MAX_LIMITS);
        for (limit, configured) in limits.iter_mut().zip(configured) {
            *limit = Limit {
                count: configured.count,
                window: Duration::from_secs(configured.window_seconds.into()),
            };
        }
        let decision = self
            .limiter
            .try_spend(Spend {
                key,
                category: category as u8,
                limits: &limits[..len],
                cost,
            })
            .await;
        self.record_evictions();
        #[cfg(any(test, feature = "test-utils"))]
        self.spends.lock().push(crate::test_support::SpendRecord {
            category,
            cost,
            retry_after: match decision {
                Decision::Allowed => None,
                Decision::Denied { retry_after } => Some(retry_after),
            },
            at: std::time::Instant::now(),
        });
        decision
    }

    /// Every spend decided so far, in order.
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) fn spends(&self) -> Vec<crate::test_support::SpendRecord> {
        self.spends.lock().clone()
    }

    /// Add the evictions since the last record to the metric, each once,
    /// whichever spend observes them first.
    fn record_evictions(&self) {
        let evictions = self.limiter.evictions();
        let recorded = self
            .recorded_evictions
            .fetch_max(evictions, Ordering::Relaxed);
        if evictions > recorded {
            crate::telemetry::rate_limit_evictions(evictions - recorded);
        }
    }

    pub(super) fn trusted_proxies(&self) -> &[ipnet::IpNet] {
        &self.config.trusted_proxies
    }
}
