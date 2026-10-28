//! The admission arithmetic for one caller key in one category.

use std::time::Duration;

use crate::{Limit, MAX_LIMITS};

const NANOS_PER_MILLI: u64 = 1_000_000;

/// The theoretical arrival times (TATs) of one caller key in one category, in
/// nanoseconds since the limiter's start. Entry `i` belongs to the spend's
/// `i`th limit. A TAT not later than now means that limit is at full allowance.
pub(crate) type Tats = [u64; MAX_LIMITS];

/// Every limit at full allowance: the state of a caller key with no slot.
pub(crate) const FULL_ALLOWANCE: Tats = [0; MAX_LIMITS];

/// Decides a spend of `cost` units at `now` against `limits`, given the
/// caller's `tats`.
///
/// Returns the TATs to store when every limit admits the spend, or the
/// shortest wait in nanoseconds after which every limit admits it. The caller
/// stores nothing on a denial, so a denial charges no limit.
///
/// For each limit with count C and window W, `excess` is how far the TAT lies
/// past now. A cost of at most C is admitted when `excess + cost × W / C` is at
/// most W, compared exactly: TATs and W are whole nanoseconds, so comparing the
/// added time rounded up gives the exact answer. The TAT then advances
/// by the added time rounded down, so `count` single units always fit one
/// window. A cost above C is admitted only at full allowance and then charged
/// in full, so the caller waits until the rate has earned back the whole cost.
/// Every product is formed in `u128`: a `u32` cost times the largest `Duration`
/// in nanoseconds fits, so no per-unit interval is truncated first. Stored TATs
/// saturate at `u64::MAX`.
///
/// The TATs are matched to `limits` by position, so a caller key and category
/// must always be decided against the same limits in the same order.
///
/// The caller handles a cost of 0 and an empty `limits` first, and passes at
/// most [`MAX_LIMITS`] limits; a count of 0 is treated as 1.
// implements: RATE-020
pub(crate) fn decide(tats: &Tats, limits: &[Limit], cost: u32, now: u64) -> Result<Tats, u64> {
    let now = u128::from(now);
    let cost = u128::from(cost);
    let mut next = FULL_ALLOWANCE;
    let mut wait = 0u128;
    for ((limit, &tat), next) in limits.iter().zip(tats).zip(&mut next) {
        let window = limit.window.as_nanos();
        let count = u128::from(limit.count.max(1));
        let excess = u128::from(tat).saturating_sub(now);
        // `cost × W / C`, rounded up for the exact check and down for the TAT.
        let (checked, charged) = ((cost * window).div_ceil(count), cost * window / count);
        if cost > count {
            wait = wait.max(excess);
        } else if excess + checked > window {
            wait = wait.max(excess + checked - window);
        }
        *next = saturate(now + excess + charged);
    }
    if wait > 0 {
        // At most the largest excess, which is below `u64::MAX`.
        return Err(saturate(wait));
    }
    Ok(next)
}

/// The retry delay for a wait in nanoseconds: whole milliseconds, rounded up,
/// so the same spend made then is admitted and one made 1 ms earlier is not.
// implements: RATE-024
pub(crate) fn retry_after(wait_ns: u64) -> Duration {
    Duration::from_millis(wait_ns.div_ceil(NANOS_PER_MILLI))
}

fn saturate(ns: u128) -> u64 {
    u64::try_from(ns).unwrap_or(u64::MAX)
}
