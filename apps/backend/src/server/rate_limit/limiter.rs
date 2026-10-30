//! The limiter the backend charges requests against: the in-memory table,
//! with the deployment's limits resolved for each caller.

use crate::config::{CallerKey, CallerLimits, Category, LimitConfig, RateLimitConfig};
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use xmtp_rate_limit::{
    Clock, Decision, Limit, Limits, LimitsError, MemoryLimiter, RateLimiter, Spend,
};

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

/// A configured limit list in the limiter's form, in the file's order.
fn limits_of(list: &[LimitConfig]) -> Result<Limits, LimitsError> {
    let list: Vec<Limit> = list
        .iter()
        .map(|limit| Limit {
            count: limit.count,
            window: Duration::from_secs(limit.window_seconds.into()),
        })
        .collect();
    Limits::new(&list)
}

/// The limiter's time source. Tests set it by hand.
#[derive(Clone)]
pub(crate) enum LimiterClock {
    System(xmtp_rate_limit::SystemClock),
    #[cfg(any(test, feature = "test-utils"))]
    Manual(xmtp_rate_limit::ManualClock),
}
impl Clock for LimiterClock {
    fn now_ns(&self) -> u64 {
        match self {
            Self::System(clock) => clock.now_ns(),
            #[cfg(any(test, feature = "test-utils"))]
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
    /// The configuration's limit lists in the limiter's form, built once.
    limits: CallerLimits<Limits>,
    /// The eviction count the metric holds.
    recorded_evictions: AtomicU64,
    /// The spends chargers made, in order, once a test turns recording on.
    #[cfg(any(test, feature = "test-utils"))]
    spends: std::sync::OnceLock<parking_lot::Mutex<Vec<crate::test_support::SpendRecord>>>,
}

impl<L: RateLimiter + CountsEvictions> BackendLimiter<L> {
    /// # Panics
    ///
    /// When a limit list does not fit the limiter. Configuration validation
    /// refuses such a list, so a panic here means the two disagree
    /// and the backend stops at startup rather than apply other limits.
    pub(crate) fn new(limiter: L, config: RateLimitConfig) -> Self {
        let limits = config
            .caller_limits()
            .try_map(|list| limits_of(list))
            .expect("a validated configuration's limit lists fit the limiter");
        Self {
            limiter,
            config,
            limits,
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
        // The limiter matches state to limits by position, so the order is
        // the file's.
        let decision = self
            .limiter
            .try_spend(Spend {
                key,
                category: category as u8,
                limits: self.limits.get(category, caller),
                cost,
            })
            .await;
        self.record_evictions();
        decision
    }

    /// Add the evictions since the last record to the metric, each once,
    /// whichever spend observes them first. Without a new eviction, a spend
    /// only reads the shared count, so spends on different shards do not
    /// contend for it.
    fn record_evictions(&self) {
        let evictions = self.limiter.evictions();
        if evictions <= self.recorded_evictions.load(Ordering::Relaxed) {
            return;
        }
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

/// The spend record a test reads to see what reached a running server.
#[cfg(any(test, feature = "test-utils"))]
impl<L> BackendLimiter<L> {
    /// Record every spend a charger makes from now on.
    pub(crate) fn record_spends(&self) {
        self.spends.get_or_init(Default::default);
    }

    pub(super) fn recording(&self) -> bool {
        self.spends.get().is_some()
    }

    pub(super) fn record(&self, spend: crate::test_support::SpendRecord) {
        if let Some(spends) = self.spends.get() {
            spends.lock().push(spend);
        }
    }

    /// The spends recorded so far, in order; empty unless recording is on.
    pub(crate) fn spends(&self) -> Vec<crate::test_support::SpendRecord> {
        self.spends
            .get()
            .map(|spends| spends.lock().clone())
            .unwrap_or_default()
    }
}
