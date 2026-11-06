use std::fmt;
use std::future::{Ready, ready};

use crate::table::{Set, Table};
use crate::{Clock, Decision, RateLimiter, Spend, SystemClock};

/// The in-memory limiter: one fixed-size table, allocated by the constructor.
///
/// Limits apply per instance; nothing is shared with another process.
pub struct MemoryLimiter<C: Clock = SystemClock> {
    table: Table<parking_lot::Mutex<Box<[Set]>>>,
    clock: C,
}

impl MemoryLimiter {
    /// Allocates a table of at most `max_memory_bytes`, with slots for at
    /// least 95% of it from a budget of about 5.6 KB up. A budget too small
    /// for one set of slots still gets one.
    ///
    /// A spend allocates nothing of its own. `parking_lot` allocates a
    /// thread's wait state the first time that thread waits for a contended
    /// shard lock, and grows its wait table as the number of threads grows.
    ///
    /// # Panics
    ///
    /// When the operating system's random source fails.
    pub fn new(max_memory_bytes: usize) -> Self {
        Self::with_clock(max_memory_bytes, SystemClock::new())
    }
}

impl<C: Clock> MemoryLimiter<C> {
    /// [`MemoryLimiter::new`] on `clock`. TATs count from the clock's start.
    ///
    /// # Panics
    ///
    /// When the operating system's random source fails.
    pub fn with_clock(max_memory_bytes: usize, clock: C) -> Self {
        Self {
            table: Table::new(max_memory_bytes),
            clock,
        }
    }

    /// How many caller states the table holds at once, one per (caller key,
    /// category).
    pub fn slots(&self) -> usize {
        self.table.slots()
    }

    /// How many times the table forgot a caller that was not at full
    /// allowance, to record another.
    pub fn evictions(&self) -> u64 {
        self.table.evictions()
    }

    /// The clock the limiter decides on.
    pub fn clock(&self) -> &C {
        &self.clock
    }
}

impl<C: Clock> RateLimiter for MemoryLimiter<C> {
    // The concrete `Ready` lets a caller that holds this type take the
    // decision without polling.
    #[allow(refining_impl_trait)]
    fn try_spend(&self, spend: Spend<'_>) -> Ready<Decision> {
        ready(self.table.spend(|| self.clock.now_ns(), &spend))
    }
}

impl<C: Clock> fmt::Debug for MemoryLimiter<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemoryLimiter")
            .field("slots", &self.slots())
            .field("evictions", &self.evictions())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests;
