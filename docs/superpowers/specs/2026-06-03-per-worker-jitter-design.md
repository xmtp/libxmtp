# Per-Worker Jitter for WorkerConfig

**Date:** 2026-06-03
**Status:** Approved design, ready for implementation plan

## Problem

`WorkerConfig` (shipped in libxmtp #3706) carries a single **global** `jitter_ns`
that applies to *every* background worker. Consumers who only want to jitter one
worker — e.g. herald-lite scheduling the CommitLog fork-recovery worker daily
with 6h jitter to avoid a fleet stampede — end up jittering the fast workers too.
A 1s-interval worker (DisappearingMessages) inheriting up to 6h of jitter is
effectively broken.

We want jitter to be configurable **per worker**, the same way interval already
is via `interval_overrides`.

## Decision summary

- **Replace** the global `jitter_ns: Option<u64>` with a per-worker
  `jitter_overrides: HashMap<WorkerKind, u64>`. (Breaking change — acceptable;
  the only consumers are nightly bindings, where breakage is expected.)
- **No global default jitter.** Absent entry => 0 jitter (deterministic) for that
  worker. This is deliberate: a global default is the exact foot-gun we are
  removing.
- Mirror the existing `interval_overrides` shape and resolution exactly.

## Non-goals

- No `default_jitter_ns` fallback (rejected — reintroduces the foot-gun).
- No change to interval resolution, enable/disable, or the `default_interval_ns`
  fallback (those stay as-is).
- No change to `jittered_interval_stream` in `xmtp_common` — it already takes
  `(base, jitter)` per worker; only the *source* of `jitter` changes.
- No DB or protocol change.

## Core change (`crates/xmtp_mls/src/worker.rs`)

### `WorkerConfig`

```rust
#[derive(Clone, Debug, Default)]
pub struct WorkerConfig {
    pub default_interval_ns: Option<u64>,
    pub interval_overrides: HashMap<WorkerKind, u64>,
    /// Per-worker jitter (ns). Absent => 0 (deterministic). Replaces the
    /// former global `jitter_ns`.
    pub jitter_overrides: HashMap<WorkerKind, u64>,
    pub enabled: HashMap<WorkerKind, bool>,
}
```

`jitter_ns` field is **removed**.

### `interval()` resolution

Base-interval logic is unchanged. Jitter changes from the global field to a
per-kind lookup:

```rust
let jitter = self
    .jitter_overrides
    .get(&kind)
    .copied()
    .map(Duration::from_nanos)
    .unwrap_or(Duration::ZERO);
```

`Default` (empty maps) still reproduces historical behavior: every worker
deterministic, each on its const default interval.

### Tests (`worker.rs`)

- The existing `jitter_is_carried` unit test constructs `WorkerConfig { jitter_ns:
  Some(..), .. }` — update it to use `jitter_overrides.insert(kind, ns)` and
  assert the jitter is returned for that kind.
- Add a test asserting per-kind scoping: jitter set for worker A, absent for
  worker B → `interval(A)` returns nonzero jitter, `interval(B)` returns
  `Duration::ZERO`.
- Other `WorkerConfig` tests (interval precedence, zero-clamp, enable) are
  unaffected.

## Bindings (all three, symmetric)

Each binding currently exposes a scalar `jitterNs` on its `WorkerConfigOptions`
analogue. Replace it with a per-worker array mirroring `workerIntervalsNs`. Use a
distinct override type per binding (do not overload the interval-override type)
so the generated surface is self-documenting.

### Generated TS/Swift/Kotlin surface

```ts
workerConfig.workerJittersNs?: Array<{ kind: WorkerKind; jitterNs: bigint }>
```

`jitterNs` (scalar) is removed from each `WorkerConfigOptions`.

### Node (`bindings/node/src/client/options.rs`)

- Add `WorkerJitterOverride { kind: WorkerKind, jitter_ns: BigInt }`
  (`#[napi(object)]`), parallel to the existing `WorkerIntervalOverride`.
- In `WorkerConfigOptions`: remove `jitter_ns`, add
  `worker_jitters_ns: Option<Vec<WorkerJitterOverride>>`.
- In the `From<WorkerConfigOptions> for XmtpWorkerConfig` impl: drop the
  `jitter_ns: o.jitter_ns.map(to_u64)` line; instead, after building `cfg`,
  populate `cfg.jitter_overrides` from `worker_jitters_ns` exactly as
  `interval_overrides` is populated from `worker_intervals_ns`.

### WASM (`bindings/wasm/src/client.rs`)

- Add `WorkerJitterOverride { kind: WorkerKind, jitter_ns: u64 }` with the same
  `Serialize, Deserialize, Tsify` derives + `#[serde(rename_all = "camelCase")]`
  as `WorkerIntervalOverride`.
- In `WorkerConfigOptions`: remove the `jitter_ns: Option<u64>` field (and its
  tsify/serde attrs), add `worker_jitters_ns:
  Option<Vec<WorkerJitterOverride>>`.
- In the `From` impl: drop `jitter_ns: o.jitter_ns`; populate
  `cfg.jitter_overrides` from `worker_jitters_ns`.

### Mobile (`bindings/mobile/src/worker_config.rs`)

- Add `FfiWorkerJitterOverride { kind: FfiWorkerKind, jitter_ns: u64 }`
  (`#[derive(uniffi::Record, ...)]`), parallel to `FfiWorkerIntervalOverride`.
- In `FfiWorkerConfig`: remove `jitter_ns: Option<u64>`, add
  `worker_jitters_ns: Vec<FfiWorkerJitterOverride>` (Vec, matching the
  interval-override field style for uniffi).
- In the `From` impl: drop `jitter_ns: o.jitter_ns`; populate
  `cfg.jitter_overrides` from `worker_jitters_ns`.

## Caller / FFI-signature impact

This change is confined to the **inside** of the `WorkerConfigOptions` /
`FfiWorkerConfig` types. The `create_client` / `createClient` function
signatures do **not** change (still one `worker_config` param), so the large
test/bench caller sweep from #3706 is **not** needed. Verify anyway:

- `cargo check -p xmtpv3 --all-targets` and `cargo check -p bindings_wasm
  --target wasm32-unknown-unknown --tests` compile.
- No SDK wrapper changes (`Client.kt`, `Client.swift`) — they pass
  `workerConfig`/`worker_config` through opaquely and don't reference the
  removed field.

## Downstream: herald-lite PR #81

herald-lite currently sets `jitterNs: <6h>` (global). After this lands and a new
node-sdk nightly is published, that PR must change to:

```ts
workerJittersNs: [{ kind: "CommitLog", jitterNs: BigInt(jitterSeconds) * 1_000_000_000n }]
```

dropping the top-level `jitterNs`. The `buildWorkerConfig` helper and its tests
update accordingly. This is tracked separately (the herald-lite review), not part
of this libxmtp change.

## Testing

- Core: `cargo nextest run -p xmtp_mls worker` — updated + new jitter tests pass;
  existing interval/enable tests unchanged.
- Lint: `just lint-rust` (clippy `-Dwarnings`, fmt, hakari).
- Bindings compile: `cargo check -p bindings_node`, `just wasm check`,
  `cargo check -p xmtpv3`.
- Node lint (per CLAUDE.md, bindings_node changed): `just node lint`.

## Backward compatibility

- `WorkerConfig::default()` unchanged in behavior (empty maps, deterministic).
- Breaking only at the binding `WorkerConfigOptions.jitterNs` field, which is
  nightly-only. No stable consumers. No DB/protocol change.
