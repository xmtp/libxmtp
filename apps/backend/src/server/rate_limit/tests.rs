use super::limiter::*;
use super::*;
use crate::{
    config::RateLimitConfig,
    test_support::{TestResult, config_for, rate_limit_delay, rate_limits},
};
use xmtp_rate_limit::{ManualClock, MemoryLimiter};

mod caller;
mod charges;
mod isolation;
mod layer;
mod overrides;
mod telemetry;

/// The rate limits a `[rate_limits]` table resolves to, with limits on.
fn config(toml: &str) -> RateLimitConfig {
    let mut config = config_for("postgres://localhost/unused");
    config.rate_limits = rate_limits(toml);
    config
        .validate()
        .expect("the test rate limits are valid")
        .rate_limits()
        .clone()
}

/// A limiter for `toml` on `clock`.
fn limiter(toml: &str, clock: &ManualClock) -> Arc<Limiter> {
    limiter_on(&config(toml), LimiterClock::Manual(clock.clone())).expect("rate limits are on")
}

/// A charger for `key` that may spend every category, to read what a caller
/// has left.
fn probe(limiter: &Arc<Limiter>, key: CallerKey) -> Charger {
    Charger::for_test(limiter, key)
}

/// Add `bearer` to a request, as a client with that credential sends it.
fn authorized<T>(message: T, bearer: &str) -> TestResult<tonic::Request<T>> {
    let mut request = tonic::Request::new(message);
    request
        .metadata_mut()
        .insert("authorization", format!("Bearer {bearer}").parse()?);
    Ok(request)
}

fn millis(ms: u64) -> u64 {
    ms * 1_000_000
}
