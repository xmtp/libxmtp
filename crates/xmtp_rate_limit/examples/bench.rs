//! Memory and speed of the in-memory limiter.
//!
//! `cargo run -p xmtp_rate_limit --release --example bench`

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use xmtp_rate_limit::{Decision, Limit, MemoryLimiter, RateLimiter, SLOT_BYTES, Spend};

/// The default `rate_limits.max_memory_mib`.
const DEFAULT_BUDGET: usize = 100 << 20;
const CALLERS: usize = 100_000;
const THREADS: usize = 8;
const RUN: Duration = Duration::from_secs(3);
/// Spends between two reads of the clock.
const BATCH: usize = 1024;
/// The default `request` limits.
const LIMITS: [Limit; 2] = [
    Limit {
        count: 6000,
        window: Duration::from_secs(60),
    },
    Limit {
        count: 200_000,
        window: Duration::from_secs(3600),
    },
];

/// Counts the bytes allocated while `COUNTING` is set.
struct Counting;

static COUNTING: AtomicBool = AtomicBool::new(false);
static ALLOCATED: AtomicUsize = AtomicUsize::new(0);

// SAFETY: every call forwards to `System` with the caller's arguments.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) {
            ALLOCATED.fetch_add(layout.size(), Ordering::Relaxed);
        }
        // SAFETY: the caller upholds `GlobalAlloc::alloc`'s contract.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller upholds `GlobalAlloc::dealloc`'s contract.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// A caller key the size of an IPv6 /64 key: an auth type tag and 8 bytes.
fn caller_key(caller: usize) -> [u8; 9] {
    let mut key = [0u8; 9];
    key[0] = 3;
    key[1..].copy_from_slice(&(caller as u64).to_be_bytes());
    key
}

/// Spends per second over `threads` threads, each cycling through `keys` from
/// its own offset, and the share of spends admitted.
fn spends_per_second(threads: usize, keys: &[[u8; 9]]) -> (f64, f64) {
    let limiter = MemoryLimiter::new(DEFAULT_BUDGET);
    let started = Instant::now();
    let (spends, admitted) = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|thread| {
                let limiter = &limiter;
                scope.spawn(move || {
                    let (mut spends, mut admitted) = (0u64, 0u64);
                    let mut next = thread * keys.len() / threads;
                    while started.elapsed() < RUN {
                        for _ in 0..BATCH {
                            let spend = Spend {
                                key: &keys[next],
                                category: 0,
                                limits: &LIMITS,
                                cost: 1,
                            };
                            let decision = black_box(limiter.try_spend(spend).into_inner());
                            admitted += u64::from(decision == Decision::Allowed);
                            next = (next + 1) % keys.len();
                        }
                        spends += BATCH as u64;
                    }
                    (spends, admitted)
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().expect("a bench thread panicked"))
            .fold((0, 0), |(s, a), (spends, admitted)| {
                (s + spends, a + admitted)
            })
    });
    let seconds = started.elapsed().as_secs_f64();
    (spends as f64 / seconds, admitted as f64 / spends as f64)
}

fn main() {
    COUNTING.store(true, Ordering::Relaxed);
    let limiter = MemoryLimiter::new(DEFAULT_BUDGET);
    COUNTING.store(false, Ordering::Relaxed);
    let bytes = ALLOCATED.load(Ordering::Relaxed);
    let slots = limiter.slots();
    drop(limiter);

    println!("slot size:                {SLOT_BYTES} bytes");
    println!("slots for 100 MiB:        {slots}");
    println!(
        "allocated for 100 MiB:    {bytes} bytes, {:.3} bytes per slot",
        bytes as f64 / slots as f64
    );

    let keys: Vec<[u8; 9]> = (0..CALLERS).map(caller_key).collect();
    let runs: [(&str, usize, &[[u8; 9]]); 4] = [
        // Every set in cache: the cost of a spend without memory latency.
        ("1 thread, 1 caller", 1, &keys[..1]),
        ("1 thread, 100k callers", 1, &keys),
        ("8 threads, 100k callers", THREADS, &keys),
        ("8 threads, 1 caller", THREADS, &keys[..1]),
    ];
    for (label, threads, keys) in runs {
        let (rate, admitted) = spends_per_second(threads, keys);
        println!(
            "{label:<25} {:>7.1} M spends/s ({:.1}% admitted)",
            rate / 1e6,
            admitted * 100.0
        );
    }
}
