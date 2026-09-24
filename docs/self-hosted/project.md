# XMTP Self-Hosted Transition

The Rust backend in `apps/backend` replaces the v3 and v4 networks. The SDKs
connect to it with a backend URL and, when required, client authentication.
Clients start with a new local database. Live message migration and exact wire
compatibility with the old networks are outside this project.

This page tracks remaining project work. It is not agent context. Approved
requirements live in `docs/specs/`; working rules live in `AGENTS.md` and
`docs/self-hosted/agent-context.md`. Git history holds the completed phase plans.

## Completed

Phases 0 through 3 are complete. Phase 4.1 delivered the docs site; 4.3 added
authentication; 4.5 added metrics and telemetry; 4.7 added push subscriptions.
Phase 5.1 delivered durable message fetching and recovery.

## Remaining

| Item | Status | Work left |
| --- | --- | --- |
| 4.2 Message pruning | Open | Envelope expiry is stored but not enforced. Define the newest-watermark and stream behavior, then add an hourly, locked prune job for expired rows. |
| 4.4 Rate limiting | Partial | Bidi update and ping token buckets exist. Add per-request costs and a limiter keyed by JWT `sub` or client IP. Reject requests above the limit. |
| 4.6 Benchmarks and performance | Open | Benchmark backend database operations at 100k, 1M, and 10M messages with varied query shapes. Record results before changing indexes, writes, or partitioning. |
| 4.8 Self-publishing SDKs | Open | Test fork releases of the Node, Android, iOS, and Browser SDKs end to end. Add a guide for each SDK. |
| 4.9 Integration suite | Partial | `apps/chaos` has a local fault suite. Add repeatable fork and failure coverage to CI. |
| 5.2 SDK code generation | Open | Mobile uses UniFFI; Node and Browser use separate bindings. Reduce handwritten SDK code with generated surfaces where they fit. |

## Deferred fixes

| Item | Status |
| --- | --- |
| Make the keepalive probe's non-subscription mode honor a plaintext endpoint. | Open |
| Investigate the iOS `testCanStreamAndUpdateNameWithoutForkingGroup` parallel-test flake. | Open |
| Pin the iOS workflow's health probe to the flake input. | Open |
