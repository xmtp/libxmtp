//! A per-caller rate limiter with a fixed memory bound.
//!
//! A [`Spend`] charges one cost in one category to one caller key and is
//! admitted or denied as a whole. Each category carries up to [`MAX_LIMITS`]
//! limits; a spend must fit every one of them, and a denied spend charges none.
//! Admission follows the generic cell rate algorithm (GCRA): each limit keeps
//! one theoretical arrival time (TAT), the instant the caller's admitted spends
//! under it are earned back.
//!
//! [`MemoryLimiter`] keeps the TATs in one fixed-size table, allocated at
//! construction and never grown. When the table is full it forgets the caller
//! that has spent least rather than refuse a request.
//!
//! The crate knows nothing about callers, categories, or configuration beyond
//! bytes and numbers: the server forms the caller key, names categories with a
//! byte, and resolves which limits apply.

mod clock;
mod gcra;
mod memory;
mod table;

use std::future::Future;
use std::time::Duration;

#[cfg(any(test, feature = "test-utils"))]
pub use clock::ManualClock;
pub use clock::{Clock, SystemClock};
pub use memory::MemoryLimiter;

/// The most limits one category can carry: a table slot holds this many TATs.
pub const MAX_LIMITS: usize = 3;

/// The bytes one table slot takes: a 64-bit key hash and [`MAX_LIMITS`] TATs.
pub const SLOT_BYTES: usize = size_of::<table::Slot>();

/// A sustained rate of `count` units per `window`, with a burst of up to `count`.
///
/// `count` is at least 1; a `count` of 0 is treated as 1. `window` is at least
/// 1 ns. A zero window earns back any cost at once, so release builds admit
/// every spend under it, and debug builds panic on it. The backend
/// configuration refuses both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limit {
    pub count: u32,
    pub window: Duration,
}

/// One charge of `cost` units in `category` to the caller named by `key`.
///
/// It has no `Debug`, so a caller key cannot reach a log through it.
#[derive(Clone, Copy)]
pub struct Spend<'a> {
    /// The caller key bytes; the server forms them.
    pub key: &'a [u8],
    pub category: u8,
    /// At most [`MAX_LIMITS`] limits, each on a different window. An empty
    /// slice admits every spend.
    ///
    /// The limiter matches its stored state to these limits by position, so a
    /// caller key must always be spent in a category with the same limits in
    /// the same order. A different list reads state recorded for another limit.
    /// Debug builds panic on a longer list; release builds ignore the limits
    /// after the last that fits.
    pub limits: &'a [Limit],
    /// A cost of 0 is admitted without a charge.
    pub cost: u32,
}

/// The outcome of a [`Spend`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Every limit was charged the cost.
    Allowed,
    /// No limit was charged. The same spend is admitted after `retry_after`,
    /// a whole number of milliseconds, if the caller spends nothing more in
    /// that category before then.
    Denied { retry_after: Duration },
}

/// A store of per-caller allowances.
///
/// Callers are generic over the limiter, so the in-memory limiter costs no box
/// or allocation per spend, and a limiter backed by a shared store can await
/// it without changing them.
pub trait RateLimiter: Send + Sync {
    /// Checks every limit of the spend and charges all of them, or none.
    fn try_spend(&self, spend: Spend<'_>) -> impl Future<Output = Decision> + Send;
}
