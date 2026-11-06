# xmtp_rate_limit

The per-caller rate limiter the backend charges requests against: GCRA
admission over up to three limits per category, in a fixed-size 8-way
set-associative table allocated once from a memory budget. RATE in
`docs/specs/` owns the behaviour; read `README.md` for the design and the
benchmark.

- Normal dependencies stay free of `apps/backend`, `xmtp_backend_config`,
  `tonic`, and `http`, so another server can reuse the crate. Check with
  `dev/nix-shell 'dev/agent-run cargo tree -p xmtp_rate_limit -e normal'`.
- `try_spend` allocates nothing of its own; `tests/memory_bound.rs` counts
  allocations. `parking_lot` allocates a thread's wait state the first time the
  thread waits for a contended shard lock.
- The `test-utils` feature exports `ManualClock` for other crates' tests.
- The table is generic over its shard lock so the shuttle tests in
  `src/table/tests/interleavings.rs` run in every default test build. Keep
  them free of features and `cfg` gates.
- After a change to the table or the arithmetic, rerun the benchmark and
  update the recorded run in `README.md`.

```bash
dev/nix-shell 'just check crate xmtp_rate_limit'
dev/nix-shell 'just test crate xmtp_rate_limit'
dev/nix-shell 'dev/agent-run cargo run -p xmtp_rate_limit --release --example bench'
```
