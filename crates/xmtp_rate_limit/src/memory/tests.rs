use std::future::{Future, Ready};
use std::pin::pin;
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use proptest::prelude::*;
use proptest::test_runner::{Config, TestRunner};

use super::*;
use crate::Limit;
use crate::clock::ManualClock;
use crate::gcra::Tats;

mod admission;
mod denial;
mod model;

const SECOND: u64 = 1_000_000_000;
const MILLI: u64 = 1_000_000;
/// Room for every caller a test names, so none is evicted.
const BUDGET: usize = 1 << 20;
const KEY: &[u8] = b"caller";
const CATEGORY: u8 = 7;

fn limiter() -> (MemoryLimiter<ManualClock>, ManualClock) {
    let clock = ManualClock::default();
    (MemoryLimiter::with_clock(BUDGET, clock.clone()), clock)
}

fn limit(count: u32, window_secs: u64) -> Limit {
    Limit {
        count,
        window: Duration::from_secs(window_secs),
    }
}

fn spend<C: Clock>(limiter: &MemoryLimiter<C>, limits: &[Limit], cost: u32) -> Decision {
    spend_as(limiter, KEY, CATEGORY, limits, cost)
}

fn spend_as<C: Clock>(
    limiter: &MemoryLimiter<C>,
    key: &[u8],
    category: u8,
    limits: &[Limit],
    cost: u32,
) -> Decision {
    limiter
        .try_spend(Spend {
            key,
            category,
            limits,
            cost,
        })
        .into_inner()
}

fn tats<C: Clock>(limiter: &MemoryLimiter<C>) -> Option<Tats> {
    limiter.table.tats(CATEGORY, KEY)
}

/// Small counts, so sequences hit denials and oversized costs, with windows in
/// whole nanoseconds that do not divide evenly into units or milliseconds.
fn limits_strategy(lengths: std::ops::RangeInclusive<usize>) -> impl Strategy<Value = Vec<Limit>> {
    proptest::collection::vec((1u32..=40, 1..=3600 * SECOND), lengths).prop_map(|limits| {
        limits
            .into_iter()
            .map(|(count, window)| Limit {
                count,
                window: Duration::from_nanos(window),
            })
            .collect()
    })
}

/// Clock advances from none to well past every window.
fn advance_strategy() -> impl Strategy<Value = u64> {
    prop_oneof![Just(0), 0..MILLI, 0..10 * SECOND, 0..7200 * SECOND]
}

fn runner() -> TestRunner {
    TestRunner::new(Config {
        cases: 256,
        ..Config::default()
    })
}

/// Callers are generic over the limiter and await a `Send` future; the
/// in-memory limiter's future is a `Ready` that holds the decision.
#[xmtp_common::test]
fn memory_limiter_answers_with_a_ready_future() {
    fn spend_with<'a, L: RateLimiter>(
        limiter: &'a L,
        spend: Spend<'a>,
    ) -> impl Future<Output = Decision> + Send + 'a {
        limiter.try_spend(spend)
    }
    fn assert_send<T: Send>(_: &T) {}

    let limiter = MemoryLimiter::new(BUDGET);
    let limits = [limit(1, 60)];
    let spend = Spend {
        key: KEY,
        category: CATEGORY,
        limits: &limits,
        cost: 1,
    };

    let ready: Ready<Decision> = limiter.try_spend(spend);
    assert_eq!(ready.into_inner(), Decision::Allowed);

    let future = spend_with(&limiter, spend);
    assert_send(&future);
    let mut future = pin!(future);
    let polled = future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()));
    assert_eq!(
        polled,
        Poll::Ready(Decision::Denied {
            retry_after: Duration::from_secs(60)
        })
    );
}
