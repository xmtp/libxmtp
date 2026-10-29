# xmtp_rate_limit

A per-caller rate limiter with a fixed memory bound. The backend charges each
request to a caller key and spends its cost against the limits of a category of
work before it does that work. The behaviour it implements is specified in
[RATE](../../docs/specs/RATE-backend-rate-limits.md).

```rust
use std::time::Duration;
use xmtp_rate_limit::{Decision, Limit, Limits, MemoryLimiter, RateLimiter, Spend};

const PUBLISH: u8 = 1;
let limiter = MemoryLimiter::new(100 << 20);
// At most three limits, each with a nonzero window; the backend builds these
// once, when it starts.
let limits = Limits::new(&[
    Limit { count: 1000, window: Duration::from_secs(60) },
    Limit { count: 20_000, window: Duration::from_secs(3600) },
])?;
let spend = Spend { key: b"caller key bytes", category: PUBLISH, limits: &limits, cost: 25 };
match limiter.try_spend(spend).await {
    Decision::Allowed => { /* do the work */ }
    Decision::Denied { retry_after } => { /* refuse; the same spend succeeds after retry_after */ }
}
```

## Design

- **One operation.** `RateLimiter::try_spend` checks every limit that applies
  and charges all of them or none. There is no reservation, refund, or debt.
  The trait returns a `Send` future so a limiter backed by a shared store can
  implement it later; `MemoryLimiter` returns `std::future::Ready`.
- **GCRA.** Each limit keeps one theoretical arrival time (TAT): when the
  caller's admitted spends under it are earned back. A spend of cost n against
  a limit of count C and window W is admitted when the TAT plus n × W / C,
  compared exactly, ends up at most W past now; the TAT then advances by
  n × W / C rounded down to a whole nanosecond. A spend larger than C is
  admitted only at full allowance and then charged in full. The arithmetic is
  in `u128` nanoseconds; TATs saturate at `u64::MAX`. The retry delay is the
  exact wait, rounded up to a whole millisecond. TATs are matched to limits by
  position, so a caller key is always spent in a category with the same limits
  in the same order.
- **Validated limits.** A spend carries a `Limits`, which `Limits::new`
  builds from at most three limits, each with a nonzero window, and refuses
  otherwise. A list the table cannot record fails where it is built, such as
  when a server starts, not at a spend.
- **A fixed table.** One 8-way set-associative table, allocated once from the
  memory budget. A slot is 32 bytes: a SipHash-1-3 hash of (category, caller
  key) under a 128-bit key drawn from the operating system at construction, and
  three TATs. The set count need not be a power of two: one multiply-shift of
  the hash picks its set among all sets, so every set takes an equal share. A
  slot whose TATs are all past is at full
  allowance, the same as no slot, so it is reused in place and the table never
  sweeps. When a set has no such slot, the slot with the smallest largest TAT,
  the caller that has spent least, is forgotten, and `evictions()` counts it.
- **Locks.** Contiguous ranges of sets sit behind 1024 `parking_lot` mutexes.
  A budget below about 485 KiB, under the configuration's 1 MiB minimum, gets
  fewer, so the locks never take the slots below 95% of the budget. The whole
  check and charge of a spend, and its reading of the clock, run under one
  lock, so a retry delay counts from the decision that reports it. The table
  is generic over its lock so the shuttle tests can check every interleaving.
- **No allocation after construction.** A spend hashes on the stack and writes
  in place. The one exception is `parking_lot`'s own: the first time a thread
  waits for a contended lock, it allocates that thread's wait state, and its
  wait table grows with the number of threads.
- **Test clock.** The `test-utils` feature exports `ManualClock`, a clock tests
  set by hand, for `MemoryLimiter::with_clock`.

## Benchmark

```bash
dev/nix-shell 'dev/agent-run cargo run -p xmtp_rate_limit --release --example bench'
```

The bench uses the default budget (100 MiB) and the default `request` limits
(6000 per minute, 200000 per hour) with 9-byte caller keys. Each run lasts
3 seconds. The 100k callers' sets total about 25 MB, far more than the
caches nearest each core, so those runs include memory latency; the
single-caller, single-thread run shows the cost of a spend with its set in
cache. The 8-thread rates varied from 36 to 52 M spends/s
across runs on this machine.

Run of 2026-10-28 on a cloud VM with an AMD Ryzen 7 9700X (8 cores, 1 thread
per core), 15 GiB RAM, Linux 6.8, release profile:

```text
slot size:                32 bytes
slots for 100 MiB:        3276032
allocated for 100 MiB:    104857600 bytes, 32.008 bytes per slot
1 thread, 1 caller           18.4 M spends/s (0.0% admitted)
1 thread, 100k callers        6.7 M spends/s (100.0% admitted)
8 threads, 100k callers      48.8 M spends/s (100.0% admitted)
8 threads, 1 caller           6.3 M spends/s (0.0% admitted)
```

## Tests

- `src/memory/tests/model.rs` compares every decision, retry delay included,
  with an unbounded reference model.
- `src/memory/tests/admission.rs` and `denial.rs` check the admission rule from
  its definitions, that a denial charges nothing, and that the retry delay is
  exact to the millisecond.
- `src/table/tests.rs` checks the layout, that keys spread over every set, the
  eviction order, and three threads through the real shard lock;
  `src/table/tests/interleavings.rs` runs three threads under shuttle's
  exhaustive scheduler.
- `tests/memory_bound.rs` counts allocations with a global allocator: the
  table fits its budget, and spending on one thread allocates nothing.
