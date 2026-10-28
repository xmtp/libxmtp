//! Allocation counts for the limiter, under a counting global allocator. The
//! allocator lives in this test binary only, and it counts only the thread
//! that asks it to.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use xmtp_rate_limit::{Clock, Decision, Limit, MemoryLimiter, RateLimiter, SLOT_BYTES, Spend};

const MIB: usize = 1 << 20;
const SECOND: u64 = 1_000_000_000;

struct Counting;

thread_local! {
    static COUNTING: Cell<bool> = const { Cell::new(false) };
    static BYTES: Cell<usize> = const { Cell::new(0) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

fn record(bytes: usize) {
    let _ = COUNTING.try_with(|counting| {
        if counting.get() {
            let _ = BYTES.try_with(|total| total.set(total.get() + bytes));
            let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
        }
    });
}

// SAFETY: every call forwards to `System` with the caller's arguments; the
// counters are const-initialised thread locals with no destructor, so reading
// them never allocates.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        // SAFETY: the caller upholds `GlobalAlloc::alloc`'s contract.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        // SAFETY: the caller upholds `GlobalAlloc::alloc_zeroed`'s contract.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record(new_size);
        // SAFETY: the caller upholds `GlobalAlloc::realloc`'s contract.
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller upholds `GlobalAlloc::dealloc`'s contract.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Runs `f` and returns its result with the bytes and the number of
/// allocations it made on this thread.
fn measure<R>(f: impl FnOnce() -> R) -> (R, usize, usize) {
    BYTES.set(0);
    ALLOCATIONS.set(0);
    COUNTING.set(true);
    let result = f();
    COUNTING.set(false);
    (result, BYTES.get(), ALLOCATIONS.get())
}

#[derive(Clone, Default)]
struct TestClock(Arc<AtomicU64>);

impl Clock for TestClock {
    fn now_ns(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}

// verifies: RATE-050
#[xmtp_common::test]
fn the_table_fits_its_budget() {
    for budget in [MIB, 3 * MIB + 17, 100 * MIB] {
        let (limiter, bytes, _) = measure(|| MemoryLimiter::new(budget));
        assert!(bytes <= budget, "{bytes} bytes for a budget of {budget}");
        assert!(
            limiter.slots() * 100 >= budget / SLOT_BYTES * 95,
            "{} slots for a budget of {budget}",
            limiter.slots()
        );
    }
}

/// A flood of fresh callers in a full table, with denials, oversized costs,
/// and every limit count, allocates nothing once the limiter exists.
// verifies: RATE-050
#[xmtp_common::test]
fn spending_never_allocates() {
    const CALLERS: u64 = 200_000;
    let clock = TestClock::default();
    let limiter = MemoryLimiter::with_clock(MIB, clock.clone());
    let limit_sets: [&[Limit]; 4] = [
        &[],
        &[Limit {
            count: 2,
            window: Duration::from_secs(60),
        }],
        &[
            Limit {
                count: 5,
                window: Duration::from_secs(60),
            },
            Limit {
                count: 20,
                window: Duration::from_secs(3600),
            },
        ],
        &[
            Limit {
                count: 1,
                window: Duration::from_secs(1),
            },
            Limit {
                count: 3,
                window: Duration::from_secs(60),
            },
            Limit {
                count: 10,
                window: Duration::from_secs(3600),
            },
        ],
    ];

    let ((admitted, denied), _, allocations) = measure(|| {
        let (mut admitted, mut denied) = (0u64, 0u64);
        for caller in 0..CALLERS {
            let key = caller.to_le_bytes();
            let limits = limit_sets[(caller % 4) as usize];
            for cost in [1, 1, 7, 0, 1] {
                let spend = Spend {
                    key: &key,
                    category: (caller % 7) as u8,
                    limits,
                    cost,
                };
                match limiter.try_spend(spend).into_inner() {
                    Decision::Allowed => admitted += 1,
                    Decision::Denied { .. } => denied += 1,
                }
            }
            clock.0.fetch_add(SECOND / 1000, Ordering::Relaxed);
        }
        (admitted, denied)
    });

    assert_eq!(allocations, 0);
    assert!(admitted > 0 && denied > 0);
    assert!(limiter.evictions() > 0, "the flood never filled the table");
}
