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
/// A `count` of 0 is treated as 1. [`Limits::new`] refuses a zero `window`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limit {
    pub count: u32,
    pub window: Duration,
}

/// The limits of one category: at most [`MAX_LIMITS`], each with a window of
/// at least 1 ns. An empty list admits every spend.
///
/// The limiter matches its stored state to these limits by position, so a
/// caller key must always be spent in a category with the same limits in the
/// same order. A different list reads state recorded for another limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Entries from `len` on are [`Limits::UNUSED`].
    list: [Limit; MAX_LIMITS],
    len: usize,
}

impl Limits {
    /// No limits: every spend is admitted.
    pub const NONE: Self = Self {
        list: [Self::UNUSED; MAX_LIMITS],
        len: 0,
    };

    const UNUSED: Limit = Limit {
        count: 0,
        window: Duration::ZERO,
    };

    /// `limits`, in order, or why the limiter cannot apply them.
    pub const fn new(limits: &[Limit]) -> Result<Self, LimitsError> {
        if limits.len() > MAX_LIMITS {
            return Err(LimitsError::TooMany { len: limits.len() });
        }
        let mut list = Self::NONE.list;
        let mut index = 0;
        while index < limits.len() {
            // A zero window earns back any cost at once: it would admit every spend.
            if limits[index].window.is_zero() {
                return Err(LimitsError::ZeroWindow { index });
            }
            list[index] = limits[index];
            index += 1;
        }
        Ok(Self {
            list,
            len: limits.len(),
        })
    }

    pub fn as_slice(&self) -> &[Limit] {
        &self.list[..self.len]
    }
}

impl Default for Limits {
    fn default() -> Self {
        Self::NONE
    }
}

/// Why [`Limits::new`] refused a list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LimitsError {
    /// A table slot records [`MAX_LIMITS`] limits per category.
    #[error("a category carries at most {MAX_LIMITS} limits, not {len}")]
    TooMany { len: usize },
    #[error("limit {index} has a zero window")]
    ZeroWindow { index: usize },
}

/// One charge of `cost` units in `category` to the caller named by `key`.
///
/// It has no `Debug`, so a caller key cannot reach a log through it.
#[derive(Clone, Copy)]
pub struct Spend<'a> {
    /// The caller key bytes; the server forms them.
    pub key: &'a [u8],
    pub category: u8,
    pub limits: &'a Limits,
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
