# Logging and tracing

`tracing` only. `println!` and `eprintln!` belong in `apps/*` binaries.
`xmtp_logging` owns the subscriber; never add a second one or pull
`tracing-subscriber` into a crate. A new crate's target must be added to
`crates/xmtp_logging/src/filter.rs` or its logs are invisible.

## Never log

Raw key material, database encryption keys, user file paths, full installation
ids, message payloads. Not even truncated keys or payloads. `skip_all` is
mandatory on instrumented functions; the span macros below set it.

Truncate ids:

```rust
use xmtp_proto::ShortHex;                           // first 4 bytes as hex
tracing::debug!(group_id = self.group_id.short_hex(), "syncing");
xmtp_common::fmt::debug_hex(bytes);                 // "0x1234...abcd", for Display impls
use xmtp_common::snippet::Snippet;
value.snippet();                                    // "abcdef.."
```

## Spans

Prefer the attribute macros over raw `#[tracing::instrument]`. They set
`err, skip_all` and the `operation` / `sentry.op` / `sentry.name` fields the
collector buckets on. They take no arguments; anything passed is dropped.

| Attribute | On | Emits |
| --- | --- | --- |
| `#[xmtp_common::rpc_span]` | API client and backend RPC fns | `operation = "rpc.<fn>"` |
| `#[xmtp_common::db_span]` | database helpers | `operation = "db.<fn>"` |
| `#[xmtp_common::mls_span]` | MLS operations | `operation = "mls.<fn>"` |
| `#[xmtp_common::span(prefix = "publish")]` | a new namespace | `operation = "publish.<fn>"` |
| `#[xmtp_common::err_span]` | FFI-exported fns | `level = "trace"`, `sentry.op = "ffi"`, no `operation` |

```rust
#[xmtp_common::rpc_span]
pub async fn query_group_messages(&self, group_id: GroupId) -> Result<Vec<GroupMessage>> { .. }

#[napi]
#[xmtp_common::err_span]      // async: wrapped in bind_task_hub, error logged inside the hub
pub async fn set(&self, credential: Credential) -> Result<(), Error> { .. }
```

`err_span` passes an `extern`-ABI fn through untouched, which keeps napi safe.

## Product events

Structured events go through `log_event!`, not an ad-hoc `tracing::info!`.
Variants live in `crates/xmtp_common/src/event_logging.rs`. Each has a doc
comment (the message) and a `#[context(..)]` list of required fields. A missing
field is a compile error.

```rust
use xmtp_common::{Event, log_event};

log_event!(
    Event::GroupSyncIntentErrored,
    self.context.installation_id(),   // second argument is always the installation id
    level = warn,                     // optional; info by default
    group_id = self.group_id,         // names containing group_id / installation_id are short-hexed
    intent_kind = ?kind               // ? Debug, % Display, # short_hex, $ JSON
);
```

Add a variant: doc comment, `#[context(field_a, field_b, icon = "…")]`, then
call it with every listed field.

## Backend

Request-completion summaries belong in transport middleware, not in handlers.
Stream-interest logs carry counts and request correlation, never topic values or
payloads.

## Swallow an error on purpose

```rust
xmtp_common::optify!(event, "Missed message due to event queue lag")   // Option<T>; logs at error
```

Use it instead of `.ok()` where the error should still reach the logs.
