# Design: OpenTelemetry telemetry for libxmtp (traces + MLS operation metrics)

**Status:** Approved design, ready for implementation planning.
**Scope:** The EOD/nightly slice — execution-time metrics + traces for high-level
MLS operations, exported over OTLP, wired into the node binding. Deliberately
**not** the full logging refactor (see "Out of scope / follow-up").

## Context

libxmtp logging configuration is scattered across many sites with three distinct
patterns (FFI-explicit in `bindings/mobile`, implicit-in-`create_client` in
node/wasm, per-`main` in apps) and inconsistent `LogOptions` structs. The
eventual goal is a single `xmtp_logging` crate exposing one builder
(`XmtpLogging::builder().json().with_rolling_file_logging().with_telemetry().level()`)
consumed by every binding. **That refactor is the follow-up.**

This spec covers the immediate need: get working OpenTelemetry **traces** and
**execution-time metrics** for the high-level MLS sync/send/intent functions into
the nightly release, exported via OTLP, attributable to their source (herald
instance / environment / role).

## Goal

A consumer (herald, configured via the node SDK) sees, in an OTLP backend
(VictoriaMetrics for metrics, a trace store for spans):

- **Latency + call rate** per high-level MLS operation (`sync`, `send_message`,
  intent-driven operations, batch sync) as a duration histogram.
- **Error rate** per operation, broken down by a bounded error label.
- **Distributed traces** for those operations, from the existing `#[instrument]`
  spans.
- All signals tagged with **resource attributes** identifying the source
  (service.instance.id, deployment.environment, role).

## Architecture: span → metric bridge (one instrumentation, two signals)

`xmtp_mls` is annotated with `#[tracing::instrument]` and gains **no
opentelemetry dependency**. The spans it already emits feed two layers that live
in `xmtp_common` behind an `otel` feature:

```
xmtp_mls  (#[instrument] attrs only — NO opentelemetry types)
   MlsGroup::sync                  #[instrument(err, fields(operation="sync", error_kind=Empty))]
   MlsGroup::sync_with_conn        (operation="sync_with_conn")
   MlsGroup::send_message          (operation="send_message")
   sync_until_intent_resolved      (operation="intent", intent_kind=<IntentKind>)
   Client::sync_all_groups / …     (operation="sync_all_groups", …)
        │  tracing spans (+ error event on Err via instrument(err))
        ▼
xmtp_common::telemetry   (feature = "otel", native-only)
   ├─ MetricsLayer (custom tracing_subscriber::Layer)   — the bridge
   │     on_new_span:  if span has an `operation` field, record start instant in span extensions
   │     on_event:     if an ERROR-level event fires inside an instrumented op span,
   │                   mark the span errored and capture `error_kind` (from the span field)
   │     on_close:     record duration histogram {operation, intent_kind?};
   │                   if errored → error counter {operation, error_kind}
   │     spans without an `operation` field are ignored (only our chokepoints emit metrics)
   ├─ OpenTelemetryLayer (tracing-opentelemetry → OTLP traces)   — same spans, exported as traces
   └─ init(resource_attrs) → builds OTLP trace + metric exporters, installs the global
        tracer + meter providers, returns (OpenTelemetryLayer, MetricsLayer, TelemetryGuard)
```

Because the `#[instrument]` attributes are how `xmtp_mls` already logs, they are
**always compiled in** and near-free when no subscriber samples them. Metrics
only materialize when a binding installs `MetricsLayer`. No `xmtp_mls/otel`
feature is required to gate the attributes.

## Instrumented chokepoints

Instrument at chokepoints rather than the ~22 individual intent callers:

| Function | `operation` | Notes |
|----------|-------------|-------|
| `MlsGroup::sync` (`mls_sync.rs`) | `sync` | top-level group sync |
| `MlsGroup::sync_with_conn` | `sync_with_conn` | network-sync path (streams/receive) |
| `MlsGroup::send_message` (`groups/mod.rs`) | `send_message` | headline user op |
| `sync_until_intent_resolved` (`mls_sync.rs`) | `intent` | **covers all ~22 intent callers**; `intent_kind` attr from `IntentKind` |
| `Client::sync_all_groups` / `sync_all_welcomes_and_groups` (`client.rs`) | `sync_all_groups` etc. | batch fan-out |

`#[instrument]` form: `#[instrument(err, fields(operation = "…", error_kind = tracing::field::Empty))]`.
For `intent`, also `intent_kind = %intent_kind`. The `error_kind` field is left
`Empty` and recorded on the error path (below).

## Metrics

Two instruments, OTEL semantic-convention-aligned, **low cardinality**.

```
histogram  xmtp.mls.operation.duration   unit = s
   buckets  [.01, .025, .05, .1, .25, .5, 1, 2.5, 5, 10, 30]
   attrs    operation     (bounded enum: sync | sync_with_conn | send_message | intent | sync_all_groups | …)
            intent_kind   (only when operation = "intent"; from the IntentKind enum, ~10 variants)
   → count = call rate, p50/p95/p99 = latency, per operation

counter    xmtp.mls.operation.errors     monotonic
   attrs    operation     (same bounded set)
            error_kind    (the ErrorCode variant string, e.g. "GroupError::Sync"; bounded)
```

### Cardinality guardrails

- Attributes are a **fixed bounded set**. `operation` is a hand-written set of
  the chokepoints; `intent_kind` is the existing `IntentKind` enum; `error_kind`
  is the existing `ErrorCode` variant string.
- **Never** use `group_id`, `inbox_id`, cursor, request id, or a raw/formatted
  error message as an attribute.

### `error_kind` derivation

`#[instrument(err)]` emits an error *event* whose payload is the error's
`Display` (unbounded — unusable as a label). To get the bounded `ErrorCode`
variant onto the span, the instrumented function records the `error_kind` field
on its error path via a thin helper, e.g.:

```rust
// on the Err branch, before returning:
xmtp_common::telemetry::record_error_kind(err.error_code()); // sets the current span's `error_kind` field
```

`MetricsLayer` reads the `error_kind` span field in `on_close`. If an error event
fired but no `error_kind` was recorded, the counter uses `error_kind = "unknown"`
so error counts are never silently dropped.

### Timing semantics

The duration recorded is **span-open to span-close wall-clock time**: the layer
stamps an instant in span extensions on `on_new_span` and measures the delta on
`on_close`. For an `#[instrument]`-annotated `async fn`, the span opens when the
future is first polled and closes when it is dropped (completed), so this is the
full end-to-end operation latency — including time the future spends pending on
`.await`. This is the intended metric (operation latency), **not** CPU/busy time.
It does mean a span held open across a long idle await reflects that wait, which
is correct for these operations (they are awaiting network/DB/locks).

## Source differentiation: resource attributes

`telemetry::init` accepts caller-supplied **resource attributes** merged into the
OTel `Resource` alongside the built-in `service.name` + `service.version`. Set
once at startup, attached to **every** span and metric with no per-measurement
cost — the OTel-standard answer to "where is this coming from."

```rust
telemetry::init(resource_attrs: Vec<(String, String)>) -> Result<(…), …>
// herald passes e.g.
//   ("service.instance.id", "herald-pod-7")
//   ("deployment.environment", "production")
//   ("xmtp.role", "syncer")
```

Per-metric *extra* attributes (beyond the resource) are **out of scope** for this
slice — they carry cardinality risk and aren't needed for source attribution.

## Where the code lives

```
crates/xmtp_common/   (feature = "otel", native-only — partially built already)
  src/telemetry.rs
    ├─ init(resource_attrs) -> (OpenTelemetryLayer<S>, MetricsLayer, TelemetryGuard)
    ├─ MetricsLayer                     — span→metric bridge (NEW)
    ├─ instruments (lazy from global meter): duration histogram + errors counter (NEW)
    ├─ record_error_kind(code: &str)    — sets current span's error_kind field (NEW)
    └─ TelemetryGuard                   — owns SDK providers, flushes on drop
  Cargo: otel feature → opentelemetry, opentelemetry_sdk, opentelemetry-otlp,
         opentelemetry-semantic-conventions, tracing-opentelemetry, tracing-subscriber

crates/xmtp_mls/      (NO opentelemetry dependency)
  add #[instrument(err, fields(operation=…, error_kind=Empty))] to the chokepoints
  add the record_error_kind(..) call on each instrumented fn's error path
    (a plain field record — pulls in no otel types; calls a xmtp_common helper that
     is a no-op unless the otel feature/layer is active)

bindings/node/        (feature = "otel" → xmtp_common/otel)
  LogOptions { structured, level, otelEndpoint: Option<String>,
               resourceAttributes: HashMap<String, String> }   [new fields]
  init_logging: if otelEndpoint is set → telemetry::init(resource_attrs from options),
                add OpenTelemetryLayer + MetricsLayer to the subscriber, store the guard
                in a process-lifetime static.
```

## Data flow (node / herald)

1. herald calls `create_client({ logOptions: { level, otelEndpoint:
   "http://collector:4317", resourceAttributes: { "xmtp.role": "syncer", … } } })`.
2. `init_logging` sees `otelEndpoint` → `telemetry::init(resource_attrs)` builds
   OTLP exporters, installs global providers, returns the two layers + guard.
3. The subscriber registry gets `fmt`/`json` + filter + `OpenTelemetryLayer` +
   `MetricsLayer`. Guard stored in a static (kept alive for the process).
4. As MLS operations run, their `#[instrument]` spans are exported as traces and,
   on close, recorded as duration/error metrics — all tagged with the resource
   attributes.

## Error handling

- `telemetry::init` returns `Err` only on exporter build failure (e.g. malformed
  endpoint); the node binding logs a warning and continues **without** telemetry
  (logging still works).
- Exporter runtime failures (collector unreachable) are handled by the OTLP
  exporter's background retry; nothing blocks the hot path.
- `record_error_kind` and `MetricsLayer` are no-ops when the otel feature is off
  or no provider is installed — instrument call sites are always safe to compile
  and call.

## Testing

- **MetricsLayer unit tests** (xmtp_common, otel feature): drive synthetic spans
  through the layer with an in-memory/stdout meter reader; assert a span with
  `operation` + an error event produces one histogram observation and one error
  counter increment with the expected `{operation, error_kind}`; assert spans
  without `operation` produce nothing.
- **Instrumentation smoke test**: with the layer installed, call an instrumented
  MLS op in an existing integration test and assert a measurement was recorded
  (or, minimally, that the annotated functions compile + run unchanged with the
  feature off).
- **No-op safety**: build + test `xmtp_mls` and `bindings_node` with the otel
  feature OFF to prove the instrumentation is inert.
- Local verification: a `grafana/otel-lgtm` (or VictoriaMetrics) OTLP collector
  to eyeball traces + metrics end-to-end before merge.

## Out of scope / follow-up

- The `xmtp_logging` crate + `XmtpLogging::builder()` and migrating bindings off
  the scattered/implicit init (the larger refactor; this slice wires telemetry
  into the existing node init only).
- Mobile/wasm telemetry wiring, and integrating telemetry with the mobile
  dynamic-reload machinery.
- Per-metric extra attributes beyond resource attributes.
- RPC-level metrics at the retry combinator (`RetryQuery`) — deferred; the
  endpoint-name/`Endpoint`-bound coupling needs its own design.
```
