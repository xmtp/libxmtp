use std::time::Instant;

/// The limiter's time source.
///
/// TATs are nanoseconds since the clock's start, so a clock that tests set by
/// hand drives every decision the limiter makes.
pub trait Clock: Send + Sync {
    /// Nanoseconds since this clock's start, saturating at `u64::MAX`.
    fn now_ns(&self) -> u64;
}

/// A monotonic clock that starts when it is made.
#[derive(Debug, Clone, Copy)]
pub struct SystemClock {
    start: Instant,
}

impl SystemClock {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn now_ns(&self) -> u64 {
        u64::try_from(self.start.elapsed().as_nanos()).unwrap_or(u64::MAX)
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub use manual::ManualClock;

#[cfg(any(test, feature = "test-utils"))]
mod manual {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// A clock tests set by hand, starting at 0. Clones share one time.
    #[derive(Debug, Clone, Default)]
    pub struct ManualClock(Arc<AtomicU64>);

    impl ManualClock {
        pub fn new() -> Self {
            Self::default()
        }

        /// Sets the time, in nanoseconds since the clock's start.
        pub fn set(&self, now_ns: u64) {
            self.0.store(now_ns, Ordering::Relaxed);
        }

        /// Moves the time forward, saturating at `u64::MAX`.
        pub fn advance(&self, ns: u64) {
            self.set(self.now().saturating_add(ns));
        }

        /// The time, in nanoseconds since the clock's start.
        pub fn now(&self) -> u64 {
            self.0.load(Ordering::Relaxed)
        }
    }

    impl super::Clock for ManualClock {
        fn now_ns(&self) -> u64 {
            self.now()
        }
    }
}
