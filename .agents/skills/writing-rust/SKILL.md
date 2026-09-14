---
name: writing-rust
description: Use when writing or reviewing Rust in crates/, apps/backend, or bindings/ - covers the shared helpers to reach for (errors, retry, time, spawn, spans, newtypes, transactions), the crate and module conventions, and the anti-patterns lint does not catch
---

# Writing Rust in libxmtp

Search for an existing helper before adding one. Shared code lives in a shared
crate (`xmtp_common`, `xmtp_proto`, `xmtp_configuration`, `xmtp_mls_common`).
Never copy a function between crates; move it and import it. Read the package's
`AGENTS.md` before working in it.

## Essentials

- **Errors**: `thiserror` enums, `#[from]` on sub-errors, `#[derive(ErrorCode)]`
  when the code crosses FFI, a hand-written `RetryableError`. Never stringify an
  inner error. See [errors.md](errors.md).
- **Time**: `xmtp_common::time::{now_ns, sleep, timeout, Duration, Instant}`.
  Never `std::time::Instant` or `tokio::time` in shared code.
- **Tasks**: `xmtp_common::spawn(ready, fut)` and keep the `StreamHandle`.
  Never bare `tokio::spawn`. See [async.md](async.md).
- **Send bounds**: `MaybeSend + MaybeSync`, `BoxDynError`,
  `#[xmtp_common::async_trait]`. Wasm has no `Send`.
- **Logging**: `tracing` only. `#[xmtp_common::rpc_span]` / `db_span` /
  `mls_span` / `err_span`; `log_event!` for product events. Never log keys,
  payloads, or full ids. See [logging.md](logging.md).
- **Ids**: `GroupId`, `InstallationId`, `Topic`, `Cursor` from
  `xmtp_proto::types`, not `Vec<u8>`. See [types.md](types.md).
- **Constants**: every number has a name. A value shared by more than one crate
  goes in `xmtp_configuration`.
- **Client database**: `XmtpMlsStorageProvider::transaction` returning
  `TransactionOutcome`; `impl_store!` / `impl_fetch!` for plain tables.
  See [database.md](database.md).
- **Tests**: `#[xmtp_common::test(unwrap_try = true)]`. See the
  `writing-rust-tests` skill.

## Reach for the helper

| Need | Use | Path |
| --- | --- | --- |
| Retry with backoff | `retry_async!(Retry::default(), (async { .. }))` | `xmtp_common` |
| Is this error retryable | `retryable!(err)` | `xmtp_common` |
| Wall clock | `time::now_ns() -> i64`, `now_secs() -> i64` | `xmtp_common::time` |
| Bounded wait | `time::timeout(d, fut) -> Result<_, Expired>` | `xmtp_common::time` |
| Periodic worker | `time::jittered_interval_stream(base, jitter)` | `xmtp_common::time` |
| Spawn a task | `spawn(None, fut) -> impl StreamHandle` | `xmtp_common` |
| Platform split | `if_native! {}` / `if_wasm! {}` / `wasm_or_native! {}` | `xmtp_common` |
| Test-only item | `if_test! {}` (also `test-utils`), `if_only_test! {}` | `xmtp_common` |
| HTTP client | `http::client()` / `http::client_builder()` | `xmtp_common::http` |
| Token bucket | `rate_limit::Bucket::new(rate, burst)` | `xmtp_common::rate_limit` |
| Log an error, keep going | `optify!(result, "msg") -> Option<T>` | `xmtp_common` |
| Short id in a log line | `id.short_hex()` | `xmtp_proto::ShortHex` |
| SHA-256 | `sha256_array(&[u8]) -> [u8; 32]` | `xmtp_common` |
| Normalize a hex id | `s.normalize_hex()` | `xmtp_common::hex::NormalizeHex` |
| Nanosecond constants | `NS_IN_SEC`, `NS_IN_HOUR`, `NS_IN_DAY` | `xmtp_common` |
| Group or installation id | `GroupId::try_from(&[u8])`, `InstallationId::from([u8; 32])` | `xmtp_proto::types` |
| Topic bytes | `Topic::new_group_message(id)`, `Topic::parse(bytes)` | `xmtp_proto::types` |
| MLS message id | `utils::id::calculate_message_id(..)` | `xmtp_mls` |

`xmtp_common` re-exports `retry`, `wasm`, `stream_handles`, `const`,
`event_logging`, `hash`, and `rand` at its root. `time`, `fmt`, `hex`, `http`,
`snippet`, `types`, and `rate_limit` need the module path.

## Anti-patterns

| Do not | Use instead |
| --- | --- |
| `loop { .. sleep .. }` retry | `retry_async!(Retry::default(), (async { .. }))` |
| `reqwest::Client::new()` / `Client::builder()` | `xmtp_common::http::client()` / `client_builder()` |
| `std::time::SystemTime::now()` / `Instant::now()` in shared code | `xmtp_common::time::{now_ns, Instant}` |
| `tokio::spawn` in a shared crate | `xmtp_common::spawn(None, fut)`; keep the handle |
| `#[xmtp_common::timeout]` in production (it panics) | `xmtp_common::time::timeout(..)`; handle `Expired` |
| Bare `#[tracing::instrument]` on an API, DB, MLS, or FFI fn | `rpc_span` / `db_span` / `mls_span` / `err_span` |
| Logging a full id, key, or payload | omit keys and payloads; `short_hex()` ids; `skip_all` |
| `Vec<u8>` for a group or installation id | `GroupId` / `InstallationId` |
| Raw `conn.transaction(..)` in a client crate | `XmtpMlsStorageProvider::transaction(\|tx\| ..)` |
| Hand-written `Store` / `Fetch` for a plain table | `impl_store!` / `impl_fetch!` |
| TLS client before provider install (native) | `install_crypto_provider()` at the entry point |
| Re-deriving an MLS message id | `utils::id::calculate_message_id(..)` |
| Renaming an error variant silently | `#[error_code("Type::OldName")]` |
| Editing `schema_gen.rs`, `dist/`, or `src/gen/**` | the regeneration command in the owning `AGENTS.md` |
| `println!` in a library crate | `tracing`; `println!` only in `apps/*` binaries |
| `.ok()` that hides an error | `optify!(result, "why")` |

## Reference

Read one when you start that kind of work.

- [errors.md](errors.md) — `ErrorCode` derive forms, `RetryableError`, the glossary, binding error wrappers
- [async.md](async.md) — time, spawn and `StreamHandle`, retry, `Send` bounds, platform macros, locks, HTTP
- [logging.md](logging.md) — span macros, `log_event!`, the never-log list, id truncation
- [types.md](types.md) — newtypes, configuration layout, cryptography and identity entry points
- [database.md](database.md) — client transactions and table macros, backend storage rules
- [crates.md](crates.md) — crate layout, features, workspace deps, hakari, RustDoc, bindings
