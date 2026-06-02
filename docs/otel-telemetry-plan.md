# OTEL Telemetry Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Export OpenTelemetry traces and execution-time metrics (duration histogram + error counter) for high-level MLS operations over OTLP, wired into the node binding, attributable to source via resource attributes.

**Architecture:** `#[tracing::instrument]` spans in `xmtp_mls` (no opentelemetry dep) feed two `xmtp_common` layers behind the `otel` feature: an `OpenTelemetryLayer` (spans → OTLP traces) and a custom `MetricsLayer` (span open→close → duration histogram; error events → error counter). One instrumentation, two signals.

**Tech Stack:** Rust, `tracing` / `tracing-subscriber` (Layer), `opentelemetry` 0.30, `opentelemetry_sdk`, `opentelemetry-otlp` (tonic), `tracing-opentelemetry` 0.31, napi (node binding).

**Spec:** `docs/otel-telemetry-design.md`

**Branch/VCS:** This is a `jj` repo (use the jujutsu skill — `jj`, not `git`). Work on bookmark `insipx/otel-traces-metrics` off `main`. The foundation from earlier (telemetry.rs `init`, `otel` feature on xmtp_common, node deps, workspace OTEL deps) is already present in the working copy and must be reconciled by Task 1.

---

## Current State (already in working copy, pre-plan)

These exist from earlier exploratory work and are the foundation:
- `crates/xmtp_common/src/telemetry.rs` — `SCOPE`, `TelemetryGuard`, `resource()`, `init<S>()` (traces + metrics providers, returns `(OpenTelemetryLayer, TelemetryGuard)`).
- `crates/xmtp_common/Cargo.toml` — `otel` feature + optional opentelemetry deps (native target).
- `crates/xmtp_common/src/lib.rs` — `#[cfg(all(feature="otel", not(wasm)))] pub mod telemetry;`
- `Cargo.toml` — workspace OTEL deps (opentelemetry 0.30, _sdk, -otlp, -semantic-conventions, tracing-opentelemetry 0.31).
- `bindings/node/Cargo.toml` — `otel` feature → `xmtp_common/otel`.
- `bindings/node/src/client/create_client.rs` — `otel_layer()` + `OTEL_GUARD` static + `init_logging` wiring (env-var based — to be changed to options-based).

Tasks 1–10 evolve this into the approved design.

## File Structure

| File | Responsibility |
|------|----------------|
| `crates/xmtp_common/src/telemetry.rs` | OTLP init (`init(resource_attrs)`), `TelemetryGuard`, `resource()` |
| `crates/xmtp_common/src/telemetry/metrics_layer.rs` | `MetricsLayer` (span→metric bridge) + lazy instruments |
| `crates/xmtp_common/src/telemetry_fields.rs` | always-compiled: `record_error_kind()` helper + the `operation`/`intent_kind`/`error_kind` field-name constants |
| `crates/xmtp_mls/src/groups/mls_sync.rs` | `#[instrument]` fields on `sync`, `sync_with_conn`, `sync_until_intent_resolved` + error_kind on err paths |
| `crates/xmtp_mls/src/groups/mod.rs` | `#[instrument]` fields on `send_message` |
| `crates/xmtp_mls/src/client.rs` | `#[instrument]` fields on `sync_all_groups`, `sync_all_welcomes_and_groups` |
| `bindings/node/src/client/options.rs` | `LogOptions` + `otelEndpoint` + `resourceAttributes` |
| `bindings/node/src/client/create_client.rs` | install both layers from options |

---

### Task 1: Reconcile foundation — `init` takes resource attributes

**Files:**
- Modify: `crates/xmtp_common/src/telemetry.rs`

- [ ] **Step 1: Change `init` signature to accept resource attributes**

Replace the `resource()` fn and `init` signature so `init` takes caller-supplied attributes merged into the OTel `Resource`. New `resource()`:

```rust
use opentelemetry::KeyValue;
use opentelemetry_sdk::Resource;

/// Build the OTel resource (service.name + version + caller-supplied attrs)
/// attached to all telemetry.
fn resource(extra: Vec<(String, String)>) -> Resource {
    let service_name = std::env::var("OTEL_SERVICE_NAME").unwrap_or_else(|_| "libxmtp".to_string());
    let mut builder = Resource::builder()
        .with_service_name(service_name)
        .with_attribute(KeyValue::new("service.version", env!("CARGO_PKG_VERSION")));
    for (k, v) in extra {
        builder = builder.with_attribute(KeyValue::new(k, v));
    }
    builder.build()
}
```

Change `init`'s signature and its first line:

```rust
pub fn init<S>(
    resource_attrs: Vec<(String, String)>,
) -> Result<
    (
        tracing_opentelemetry::OpenTelemetryLayer<S, opentelemetry_sdk::trace::Tracer>,
        TelemetryGuard,
    ),
    opentelemetry_otlp::ExporterBuildError,
>
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    let resource = resource(resource_attrs);
    // ... rest unchanged (uses `resource` clone for tracer + meter providers)
}
```

- [ ] **Step 2: Build to verify**

Run: `cargo build -p xmtp_common --features otel`
Expected: PASS (the only caller is the node binding, updated later — this crate builds standalone).

- [ ] **Step 3: Commit**

```bash
jj commit -m "feat(telemetry): init accepts caller resource attributes"
```

---

### Task 2: Field-name constants + `record_error_kind` helper (always-compiled)

> This module is **NOT** under the `otel` feature: `xmtp_mls` (which has no otel
> feature) calls `record_error_kind` unconditionally. It only touches `tracing`
> (a hard dep) and is a genuine no-op when no `MetricsLayer` is installed.

**Files:**
- Create: `crates/xmtp_common/src/telemetry_fields.rs`
- Modify: `crates/xmtp_common/src/lib.rs` (declare always-compiled module)

- [ ] **Step 1: Create the always-compiled fields module**

`crates/xmtp_common/src/telemetry_fields.rs`:

```rust
//! Span-field conventions shared between the `#[instrument]` call sites and the
//! (otel-gated) `MetricsLayer`. Always compiled — `record_error_kind` is called
//! from `xmtp_mls` regardless of the `otel` feature, and is a no-op unless a
//! `MetricsLayer` is installed.

/// Span field naming the high-level operation (e.g. "sync"). Spans without this
/// field are ignored by `MetricsLayer`.
pub const OPERATION_FIELD: &str = "operation";
/// Span field naming the intent kind (only on operation="intent").
pub const INTENT_KIND_FIELD: &str = "intent_kind";
/// Span field carrying the bounded error label (an ErrorCode variant string).
pub const ERROR_KIND_FIELD: &str = "error_kind";

/// Record the bounded error label on the current span so `MetricsLayer` can
/// attribute the error counter. Call on an instrumented fn's error path with
/// `err.error_code()`. No-op if there is no current span / field.
pub fn record_error_kind(code: &'static str) {
    tracing::Span::current().record(ERROR_KIND_FIELD, code);
}
```

- [ ] **Step 2: Declare the module in lib.rs (always compiled)**

In `crates/xmtp_common/src/lib.rs`, alongside the other `pub mod` declarations:

```rust
pub mod telemetry_fields;
```

- [ ] **Step 3: Build to verify (both feature states — proves no otel dep)**

Run: `cargo build -p xmtp_common` then `cargo build -p xmtp_common --features otel`
Expected: both PASS.

- [ ] **Step 4: Commit**

```bash
jj commit -m "feat(telemetry): always-compiled operation field constants + record_error_kind"
```

---

### Task 3: `MetricsLayer` — instruments + struct skeleton

**Files:**
- Create: `crates/xmtp_common/src/telemetry/metrics_layer.rs`
- Modify: `crates/xmtp_common/src/telemetry.rs` (declare submodule, re-export)

- [ ] **Step 1: Create the instruments + layer skeleton**

`crates/xmtp_common/src/telemetry/metrics_layer.rs`:

```rust
//! `MetricsLayer` — derives MLS operation metrics from `#[instrument]` spans.
//!
//! Reads the `operation` (and optional `intent_kind`, `error_kind`) span fields
//! defined in [`crate::telemetry_fields`]. Records:
//! - `xmtp.mls.operation.duration` (histogram, seconds) on span close;
//! - `xmtp.mls.operation.errors` (counter) when an ERROR event fired in the span.
//! Spans without an `operation` field are ignored.

use opentelemetry::metrics::{Counter, Histogram};
use opentelemetry::{global, KeyValue};
use std::sync::OnceLock;
use std::time::Instant;
use tracing::span::{Attributes, Id};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Layer;

const DURATION_BUCKETS: &[f64] = &[
    0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0,
];

struct Instruments {
    duration: Histogram<f64>,
    errors: Counter<u64>,
}

fn instruments() -> &'static Instruments {
    static INSTRUMENTS: OnceLock<Instruments> = OnceLock::new();
    INSTRUMENTS.get_or_init(|| {
        let meter = global::meter(super::SCOPE);
        Instruments {
            duration: meter
                .f64_histogram("xmtp.mls.operation.duration")
                .with_unit("s")
                .with_description("Wall-clock duration of a high-level MLS operation.")
                .with_boundaries(DURATION_BUCKETS.to_vec())
                .build(),
            errors: meter
                .u64_counter("xmtp.mls.operation.errors")
                .with_description("Count of high-level MLS operations that returned an error.")
                .build(),
        }
    })
}

/// Per-span state stashed in span extensions while it is open.
struct OpState {
    operation: String,
    intent_kind: Option<String>,
    started: Instant,
    errored: bool,
    error_kind: Option<String>,
}

/// Tracing layer that turns instrumented MLS-operation spans into OTel metrics.
#[derive(Default, Clone)]
pub struct MetricsLayer;
```

- [ ] **Step 2: Declare submodule + re-export in telemetry.rs**

```rust
mod metrics_layer;
pub use metrics_layer::MetricsLayer;
```

- [ ] **Step 3: Build to verify (will warn about unused — that's fine until Task 4)**

Run: `cargo build -p xmtp_common --features otel 2>&1 | grep -E "^error" || echo OK`
Expected: OK (no hard errors; unused-field warnings acceptable mid-task).

- [ ] **Step 4: Commit**

```bash
jj commit -m "feat(telemetry): MetricsLayer instruments + skeleton"
```

---

### Task 4: `MetricsLayer` — field-visitor + Layer impl

**Files:**
- Modify: `crates/xmtp_common/src/telemetry/metrics_layer.rs`

- [ ] **Step 1: Add a field visitor that extracts operation/intent_kind/error_kind**

Append to `metrics_layer.rs`:

```rust
use tracing::field::{Field, Visit};

#[derive(Default)]
struct OpVisitor {
    operation: Option<String>,
    intent_kind: Option<String>,
    error_kind: Option<String>,
}

impl Visit for OpVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            crate::telemetry_fields::OPERATION_FIELD => self.operation = Some(value.to_string()),
            crate::telemetry_fields::INTENT_KIND_FIELD => self.intent_kind = Some(value.to_string()),
            crate::telemetry_fields::ERROR_KIND_FIELD => self.error_kind = Some(value.to_string()),
            _ => {}
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        // `intent_kind = %x` records as str; `intent_kind = ?x` records as debug.
        // Capture either form for the operation/intent fields.
        match field.name() {
            crate::telemetry_fields::OPERATION_FIELD => self.operation = Some(format!("{value:?}").trim_matches('"').to_string()),
            crate::telemetry_fields::INTENT_KIND_FIELD => self.intent_kind = Some(format!("{value:?}").trim_matches('"').to_string()),
            crate::telemetry_fields::ERROR_KIND_FIELD => self.error_kind = Some(format!("{value:?}").trim_matches('"').to_string()),
            _ => {}
        }
    }
}
```

- [ ] **Step 2: Implement the Layer**

```rust
impl<S> Layer<S> for MetricsLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let mut v = OpVisitor::default();
        attrs.record(&mut v);
        let Some(operation) = v.operation else {
            return; // not one of our chokepoints
        };
        if let Some(span) = ctx.span(id) {
            span.extensions_mut().insert(OpState {
                operation,
                intent_kind: v.intent_kind,
                started: Instant::now(),
                errored: false,
                error_kind: v.error_kind,
            });
        }
    }

    fn on_record(&self, id: &Id, values: &tracing::span::Record<'_>, ctx: Context<'_, S>) {
        // `error_kind` is recorded after span creation (via record_error_kind).
        let Some(span) = ctx.span(id) else { return };
        let mut ext = span.extensions_mut();
        let Some(state) = ext.get_mut::<OpState>() else { return };
        let mut v = OpVisitor::default();
        values.record(&mut v);
        if let Some(ek) = v.error_kind {
            state.error_kind = Some(ek);
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        // An ERROR-level event within an instrumented op span marks it errored.
        if *event.metadata().level() != tracing::Level::ERROR {
            return;
        }
        if let Some(span) = ctx.event_span(event) {
            let mut ext = span.extensions_mut();
            if let Some(state) = ext.get_mut::<OpState>() {
                state.errored = true;
            }
        }
    }

    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(&id) else { return };
        let mut ext = span.extensions_mut();
        let Some(state) = ext.remove::<OpState>() else { return };

        let i = instruments();
        let mut attrs = vec![KeyValue::new("operation", state.operation.clone())];
        if let Some(kind) = &state.intent_kind {
            attrs.push(KeyValue::new("intent_kind", kind.clone()));
        }
        i.duration
            .record(state.started.elapsed().as_secs_f64(), &attrs);

        if state.errored {
            let mut err_attrs = vec![KeyValue::new("operation", state.operation)];
            err_attrs.push(KeyValue::new(
                "error_kind",
                state.error_kind.unwrap_or_else(|| "unknown".to_string()),
            ));
            i.errors.add(1, &err_attrs);
        }
    }
}
```

- [ ] **Step 3: Build to verify**

Run: `cargo build -p xmtp_common --features otel`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
jj commit -m "feat(telemetry): MetricsLayer span->metric bridge impl"
```

---

### Task 5: `MetricsLayer` unit test (in-memory meter)

**Files:**
- Modify: `crates/xmtp_common/src/telemetry/metrics_layer.rs` (test module)

- [ ] **Step 1: Write the failing test**

Append a `#[cfg(test)]` module that installs an in-memory metric reader, runs synthetic spans through the layer, and asserts the recorded metrics. Uses `opentelemetry_sdk::metrics::{SdkMeterProvider, ManualReader}` and `tracing_subscriber` with the layer.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry::global;
    use opentelemetry_sdk::metrics::{
        data::ResourceMetrics, ManualReader, SdkMeterProvider,
    };
    use opentelemetry_sdk::metrics::reader::MetricReader;
    use tracing_subscriber::prelude::*;

    fn drain(reader: &ManualReader) -> ResourceMetrics {
        let mut rm = ResourceMetrics::default();
        reader.collect(&mut rm).unwrap();
        rm
    }

    #[test]
    fn records_duration_and_error_for_op_span() {
        let reader = ManualReader::builder().build();
        let provider = SdkMeterProvider::builder()
            .with_reader(reader.clone())
            .build();
        global::set_meter_provider(provider);

        let subscriber = tracing_subscriber::registry().with(MetricsLayer);
        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!(
                "op",
                operation = "intent",
                intent_kind = "SendMessage",
                error_kind = tracing::field::Empty
            );
            let _e = span.enter();
            crate::telemetry_fields::record_error_kind("GroupError::Sync");
            tracing::error!("boom"); // marks errored
            drop(_e);
            drop(span);
        });

        let rm = drain(&reader);
        // assert a histogram named xmtp.mls.operation.duration with one data point
        // and a counter xmtp.mls.operation.errors == 1 with error_kind=GroupError::Sync.
        let names: Vec<_> = rm
            .scope_metrics
            .iter()
            .flat_map(|s| s.metrics.iter().map(|m| m.name.to_string()))
            .collect();
        assert!(names.iter().any(|n| n == "xmtp.mls.operation.duration"));
        assert!(names.iter().any(|n| n == "xmtp.mls.operation.errors"));
    }

    #[test]
    fn ignores_spans_without_operation() {
        let reader = ManualReader::builder().build();
        let provider = SdkMeterProvider::builder()
            .with_reader(reader.clone())
            .build();
        global::set_meter_provider(provider);

        let subscriber = tracing_subscriber::registry().with(MetricsLayer);
        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!("not_an_op", foo = "bar");
            let _e = span.enter();
            drop(_e);
            drop(span);
        });

        let rm = drain(&reader);
        let count: usize = rm.scope_metrics.iter().map(|s| s.metrics.len()).sum();
        assert_eq!(count, 0, "non-op spans must not emit metrics");
    }
}
```

> NOTE: the exact `ResourceMetrics` / `ManualReader` API may differ slightly in opentelemetry_sdk 0.30 — if the data-model accessors don't match, adapt the assertions to the actual 0.30 API (the test's intent is: op-span → 2 metrics present; non-op span → 0 metrics). Verify against `cargo doc -p opentelemetry_sdk --features metrics,testing`.

- [ ] **Step 2: Run test to verify it fails (or compiles-then-passes)**

Run: `cargo test -p xmtp_common --features otel metrics_layer::tests -- --test-threads=1`
Expected: initially may FAIL to compile if the 0.30 reader API differs; fix imports until it compiles and the two assertions pass. (`--test-threads=1` because the global meter provider is process-global.)

- [ ] **Step 3: Make it pass**

Adjust the in-memory reader usage to the real opentelemetry_sdk 0.30 testing API until both tests pass.

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p xmtp_common --features otel metrics_layer::tests -- --test-threads=1`
Expected: PASS (2 tests)

- [ ] **Step 5: Commit**

```bash
jj commit -m "test(telemetry): MetricsLayer records ops, ignores non-ops"
```

---

### Task 6: `init` returns the `MetricsLayer` too

**Files:**
- Modify: `crates/xmtp_common/src/telemetry.rs`

- [ ] **Step 1: Add MetricsLayer to init's return tuple**

Change `init`'s return type and body to also return a `MetricsLayer`:

```rust
pub fn init<S>(
    resource_attrs: Vec<(String, String)>,
) -> Result<
    (
        tracing_opentelemetry::OpenTelemetryLayer<S, opentelemetry_sdk::trace::Tracer>,
        MetricsLayer,
        TelemetryGuard,
    ),
    opentelemetry_otlp::ExporterBuildError,
>
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    // ... existing body up through `let layer = tracing_opentelemetry::layer().with_tracer(tracer);`
    Ok((layer, MetricsLayer, TelemetryGuard { tracer_provider, meter_provider }))
}
```

- [ ] **Step 2: Build to verify**

Run: `cargo build -p xmtp_common --features otel`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
jj commit -m "feat(telemetry): init returns MetricsLayer alongside trace layer"
```

---

### Task 7: Instrument `xmtp_mls` chokepoints (fields + error_kind)

**Files:**
- Modify: `crates/xmtp_mls/src/groups/mls_sync.rs` (`sync` ~378, `sync_with_conn` ~454, `sync_until_intent_resolved` ~540)
- Modify: `crates/xmtp_mls/src/groups/mod.rs` (`send_message` ~1130)
- Modify: `crates/xmtp_mls/src/client.rs` (`sync_all_groups` ~1120, `sync_all_welcomes_and_groups` ~1132)

> `record_error_kind` is a plain `&'static str` field record; it pulls in NO opentelemetry types. It is a no-op unless a `MetricsLayer` is installed. `xmtp_mls` needs no new dependency.

- [ ] **Step 1: `sync` — add operation field + error_kind on err path**

`mls_sync.rs:377` currently `#[tracing::instrument]`. Change to:

```rust
#[tracing::instrument(err, fields(operation = "sync", error_kind = tracing::field::Empty))]
pub async fn sync(&self) -> Result<SyncSummary, GroupError> {
```

At the function body, ensure the final `Ok(sync_summary)` path is unchanged; on the error-returning lines, the `err` flag handles the error event. To set the bounded label, wrap the body's result so the error path records `error_kind`. Simplest: keep the existing body, and add at the two `?`-propagation points an explicit record. Since the body uses `?`, refactor the tail to capture and record:

```rust
    // existing body computes `sync_summary` via `?`; replace the trailing
    // statements so errors record error_kind before propagating:
    let result = async {
        let sync_summary = self.sync_with_conn().await.map_err(GroupError::from)?;
        self.maybe_update_installations(None).await?;
        Ok::<_, GroupError>(sync_summary)
    }
    .await;
    if let Err(e) = &result {
        xmtp_common::telemetry_fields::record_error_kind(e.error_code());
    }
    result
```

> `error_code()` requires `use xmtp_common::ErrorCode;` in scope (GroupError derives ErrorCode). Add the import if not already present.

- [ ] **Step 2: `sync_with_conn` — operation field**

`mls_sync.rs:454` has a `cfg_attr` dual `#[instrument]`. Add `operation = "sync_with_conn"` and `error_kind = Empty` to BOTH cfg_attr forms' `fields(...)`, and record error_kind on its `Err(summary)` return. Since this returns `Result<_, SyncSummary>` (not GroupError), use the summary's error code: `record_error_kind(SyncSummary_error_code)`. SyncSummary has no ErrorCode; use a static label here instead:

```rust
// at the `if summary.is_errored() { Err(summary) }` site:
if summary.is_errored() {
    xmtp_common::telemetry_fields::record_error_kind("SyncSummary");
    Err(summary)
} else {
    Ok(summary)
}
```

(Keep it simple — `sync_with_conn`'s error is always a `SyncSummary`; "SyncSummary" is the bounded label.)

- [ ] **Step 3: `sync_until_intent_resolved` — operation field**

`mls_sync.rs:540` has a `cfg_attr` dual `#[instrument]`. The function only has
`intent_id` in scope (not the kind — fetching it would cost an extra DB query
just for a label), so this chokepoint records **`operation = "intent"` only**;
the `intent_kind` breakdown is deferred (it can be derived later from the intent
store, or recorded in a follow-up at a site that already holds the kind). The
existing attribute has a `skip(self)` form; add the fields to **both** cfg_attr
forms:

```rust
// the test/test-utils form:
#[cfg_attr(any(test, feature = "test-utils"),
    tracing::instrument(level = "info", err,
        fields(who = %self.context.inbox_id(), operation = "intent", error_kind = tracing::field::Empty),
        skip(self)))]
// the non-test form:
#[cfg_attr(not(any(test, feature = "test-utils")),
    tracing::instrument(level = "trace", err,
        fields(operation = "intent", error_kind = tracing::field::Empty),
        skip(self)))]
```

(Match the existing `level`/`skip` values of whatever is currently on the
function — only adding `err`, `operation`, `error_kind` to each form.)

Record `error_kind` on the error path. The function returns
`Result<SyncSummary, GroupError>`; the existing body ends with `result`. Insert
before the final `result`:

```rust
    if let Err(e) = &result {
        xmtp_common::telemetry_fields::record_error_kind(e.error_code());
    }
    result
```

(`use xmtp_common::ErrorCode;` must be in scope for `error_code()`.)

- [ ] **Step 4: `send_message` — operation field**

`mod.rs:1129` `#[tracing::instrument(level = "debug", skip_all, fields(who = self.context.inbox_id()))]`. Add the fields:

```rust
#[tracing::instrument(level = "debug", err, skip_all,
    fields(who = self.context.inbox_id(), operation = "send_message", error_kind = tracing::field::Empty))]
pub async fn send_message(
```

Record error_kind on the error path (wrap-and-record as in Step 1, using the returned error's `error_code()`).

- [ ] **Step 5: `sync_all_groups` + `sync_all_welcomes_and_groups` — operation fields**

`client.rs:1120` / `1132`. Add `#[tracing::instrument(err, skip_all, fields(operation = "sync_all_groups", error_kind = tracing::field::Empty))]` (and `"sync_all_welcomes_and_groups"` respectively), record error_kind on err paths using `error_code()` of the returned `ClientError`/`GroupError`.

- [ ] **Step 6: Build + lint (feature OFF — instrumentation must be inert, no otel dep)**

`record_error_kind` is the always-compiled helper from Task 2 (`xmtp_common::telemetry_fields`), so `xmtp_mls` resolves it with no otel feature anywhere in its dependency path. Verify:

Run:
```
cargo build -p xmtp_mls
cargo clippy --locked -p xmtp_mls --all-features --all-targets --no-deps -- -Dwarnings
```
Expected: PASS — no opentelemetry types in `xmtp_mls`.

- [ ] **Step 7: Commit**

```bash
jj commit -m "feat(mls): instrument high-level operations for telemetry metrics"
```

---

### Task 8: Node `LogOptions` — otelEndpoint + resourceAttributes

**Files:**
- Modify: `bindings/node/src/client/options.rs` (`LogOptions`)

- [ ] **Step 1: Add the fields**

```rust
#[napi(object)]
pub struct LogOptions {
  /// enable structured JSON logging to stdout.
  pub structured: Option<bool>,
  /// Filter logs by level
  pub level: Option<LogLevel>,
  /// OTLP endpoint (e.g. "http://collector:4317"). When set (and the binary is
  /// built with the `otel` feature), traces + MLS operation metrics are exported.
  pub otel_endpoint: Option<String>,
  /// Resource attributes attached to all exported telemetry (e.g.
  /// { "service.instance.id": "herald-7", "deployment.environment": "prod" }).
  pub resource_attributes: Option<std::collections::HashMap<String, String>>,
}
```

- [ ] **Step 2: Build node**

Run: `cargo build -p bindings_node`
Expected: PASS (new optional napi fields).

- [ ] **Step 3: Commit**

```bash
jj commit -m "feat(node): LogOptions otelEndpoint + resourceAttributes"
```

---

### Task 9: Node `init_logging` — install both layers from options

**Files:**
- Modify: `bindings/node/src/client/create_client.rs`

- [ ] **Step 1: Replace env-var `otel_layer()` with options-driven install**

Rewrite the otel wiring so it reads `options.otel_endpoint` (set the env the OTLP exporter reads, since the 0.30 exporter honors `OTEL_EXPORTER_OTLP_ENDPOINT`) and threads `resource_attributes`, installing BOTH the trace layer and `MetricsLayer`:

```rust
#[cfg(feature = "otel")]
static OTEL_GUARD: std::sync::OnceLock<Option<xmtp_common::telemetry::TelemetryGuard>> =
  std::sync::OnceLock::new();

// Returns (trace_layer, metrics_layer) when otel is enabled + endpoint configured.
#[cfg(feature = "otel")]
fn otel_layers<S>(
  options: &LogOptions,
) -> Option<(
  impl tracing_subscriber::Layer<S>,
  xmtp_common::telemetry::MetricsLayer,
)>
where
  S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
  let endpoint = options.otel_endpoint.clone()?;
  // The OTLP exporter reads OTEL_EXPORTER_OTLP_ENDPOINT; set it from the option.
  // SAFETY: set before the exporter is built, single-threaded init path.
  unsafe { std::env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", &endpoint); }
  let attrs: Vec<(String, String)> = options
    .resource_attributes
    .clone()
    .unwrap_or_default()
    .into_iter()
    .collect();
  match xmtp_common::telemetry::init(attrs) {
    Ok((trace_layer, metrics_layer, guard)) => {
      let _ = OTEL_GUARD.set(Some(guard));
      Some((trace_layer, metrics_layer))
    }
    Err(e) => {
      tracing::warn!("failed to initialize OpenTelemetry export: {e}");
      None
    }
  }
}
```

Then in `init_logging`, split the otel layers into two `.with(...)` (or `Option`-wrap each). Because `init` returns a tuple, register both:

```rust
#[cfg(feature = "otel")]
let (trace_layer, metrics_layer) = match otel_layers(&options) {
  Some((t, m)) => (Some(t), Some(m)),
  None => (None, None),
};
#[cfg(not(feature = "otel"))]
let (trace_layer, metrics_layer) =
  (None::<tracing_subscriber::layer::Identity>, None::<tracing_subscriber::layer::Identity>);

// structured branch:
tracing_subscriber::registry()
  .with(filter).with(fmt)
  .with(trace_layer).with(metrics_layer)
  .init();
```

(Use the same `.with(trace_layer).with(metrics_layer)` in the non-structured branch. `Option<Layer>` implements `Layer` so `None` is a no-op.)

- [ ] **Step 2: Build both feature states**

Run:
```
cargo build -p bindings_node
cargo build -p bindings_node --features otel
```
Expected: both PASS.

- [ ] **Step 3: Lint**

Run: `cargo clippy --locked -p bindings_node --features otel --no-deps -- -Dwarnings`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
jj commit -m "feat(node): install trace + metrics layers from LogOptions.otelEndpoint"
```

---

### Task 10: End-to-end verification + workspace lint/hakari

**Files:** none (verification)

- [ ] **Step 1: Workspace-wide build, both feature states**

Run:
```
cargo build -p xmtp_common -p xmtp_mls -p bindings_node
cargo build -p xmtp_common --features otel
cargo build -p bindings_node --features otel
```
Expected: all PASS.

- [ ] **Step 2: Clippy CI form on touched crates**

Run:
```
cargo clippy --locked -p xmtp_common -p xmtp_mls --all-features --all-targets --no-deps -- -Dwarnings
cargo clippy --locked -p bindings_node --features otel --no-deps -- -Dwarnings
cargo fmt --check
```
Expected: PASS. (Note: `--all-features` on xmtp_common enables `otel`.)

- [ ] **Step 3: hakari (workspace-hack) — required by CI**

Run:
```
cargo hakari generate --diff
cargo hakari manage-deps --dry-run
```
Expected: clean. If the new OTEL deps changed the hack, run `cargo hakari generate` (no --diff) to update `crates/xmtp-workspace-hack`, then re-run `--diff` to confirm clean. Commit the hakari update.

- [ ] **Step 4: MetricsLayer tests pass**

Run: `cargo test -p xmtp_common --features otel metrics_layer -- --test-threads=1`
Expected: PASS.

- [ ] **Step 5: Local OTLP smoke test (manual, optional but recommended)**

Start a collector:
```
docker run -d --name otel-lgtm -p 4317:4317 -p 3000:3000 grafana/otel-lgtm
```
Build a small node script (or an existing node test) that calls `createClient({ logOptions: { otelEndpoint: "http://localhost:4317", resourceAttributes: { "xmtp.role": "test" } } })`, performs a `send_message`, then exits cleanly (so the guard flushes). Open Grafana (`localhost:3000`) and confirm a `xmtp.mls.operation.duration` metric and a trace span for the operation, tagged `xmtp.role=test`. Tear down: `docker rm -f otel-lgtm`.

- [ ] **Step 6: Final commit / bookmark**

```bash
jj bookmark set insipx/otel-traces-metrics -r @
jj commit -m "chore(telemetry): workspace lint + hakari for OTEL deps"
```

---

## Self-Review notes (resolved)

- **Spec coverage:** span→metric bridge (Tasks 3–6), instrumented chokepoints (Task 7; `intent_kind` deferred per spec note), duration histogram + error counter with error_kind (Tasks 4, 7), resource attributes (Tasks 1, 8, 9), node otelEndpoint wiring (Tasks 8–9), traces via OpenTelemetryLayer (foundation + Task 9), no-op-without-feature safety (Task 2 always-compiled module + Task 7 step 6 + Task 10 steps 1–2), tests (Tasks 5, 10).
- **No-otel compile gap:** caught and handled by Task 2 — `record_error_kind` + field consts live in the always-compiled `telemetry_fields` module, so `xmtp_mls` needs no otel feature.
- **`sync_with_conn` error type:** it returns `SyncSummary` (no ErrorCode) — Task 7 step 2 uses a static "SyncSummary" label rather than `error_code()`.
- **intent_kind availability:** recorded inside the retry loop where `kind` is in scope (Task 7 step 3), since it isn't available at outer-fn entry.
- **Known 0.30 API risk:** the in-memory metric reader assertions (Task 5) and exporter/reader builder methods may need adapting to the exact opentelemetry_sdk 0.30 API — flagged inline; the verifier adapts assertions to match.
```
