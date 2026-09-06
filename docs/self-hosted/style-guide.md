# libxmtp Style Guide

Use this guide when writing `apps/backend`, shared crates, or changes to `crates/xmtp_mls`.
Search for an existing helper before adding one. Move shared code to a shared crate; do not copy it.
Read the package's `AGENTS.md` before working in it. Package-specific conventions belong there.
Follow `docs/self-hosted/project.md`, approved specs in `docs/specs/`, and `docs/self-hosted/guidelines.md` when they override this guide.

Run commands through `just` or `dev/nix-shell '<cmd>'`. Run `just lint` before opening a PR; project guidelines allow mid-phase commits to skip it.
The lint recipes check Rust, configuration, and Markdown. `just lint-rust` runs Clippy with `-Dwarnings`, checks formatting, and checks Cargo hakari output.
See `justfile` for the commands.

## 1. Errors

- A module may contain several related error enums. Derive `ErrorCode` when an error needs a stable code across the FFI boundary.
- Wrap sub-errors with `#[from]`; use `#[error(transparent)]` when the variant adds no context.
- **Retryability** is a trait, not a bool field: `crates/xmtp_common/src/retry.rs:RetryableError` (`fn is_retryable(&self) -> bool`). Implement it by hand,
  delegating to inner errors (`crates/xmtp_mls/src/groups/error.rs`). Call it through `retry.rs:retryable!` — `if retryable!(err) { .. }`.
- **Stable error codes** cross the FFI boundary. `crates/xmtp_common/src/error_code.rs:ErrorCode` gives `error_code() -> &'static str` formatted
  `"TypeName::VariantName"` for an enum, `"TypeName"` for a struct. Derive with `crates/xmtp_macro/src/lib.rs:derive_error_code` (attributes parsed at
  `crates/xmtp_macro/src/error_code.rs`): variant-level `#[error_code(inherit)]` (return the inner error's code) and `#[error_code("Old::Name")]` (keep a code
  after a rename); type-level `#[error_code(internal)]` (drop the type from the glossary, `apps/error_glossary/src/main.rs`) and
  `#[error_code(remote = "path::Type")]` (implement for a foreign type; see `crates/xmtp_common/src/error_code.rs` `cryptography_error_codes`).
- Document each variant with a doc comment saying whether it is retryable; the glossary generator reads them. After adding variants regenerate
  `docs/error_glossary.md` with `dev/nix-shell 'dev/gen-error-glossary'` (source `apps/error_glossary/src/main.rs`).
- Preserve public error codes when renaming variants. Use an `#[error_code("Old::Name")]` override where needed.

## 2. Async and runtime

- **Read clocks and sleep through `crates/xmtp_common/src/time.rs`**, which re-exports `Duration` / `Instant` / `SystemTime` from `std` on native and `web_time` on
  wasm. A plain `std::time::Duration` stays valid as a value type in a constant or signature (`crates/xmtp_configuration/src/common/mls.rs`,
  `crates/xmtp_mls/src/context.rs`); it is `Instant`, `SystemTime`, and the timer functions that must come from `xmtp_common::time`.
  - `now_ns()`, `now_ms()`, `now_secs()` for wall-clock reads; `sleep(Duration)`; `timeout(Duration, fut) -> Result<T, Expired>`.
  - `interval_stream(period)` and `jittered_interval_stream(period, jitter)` for periodic workers; `rand_offset(jitter)` when a worker computes its own sleeps.
    Use jitter to spread worker activity across instances.
  - Nanosecond constants in `crates/xmtp_common/src/const.rs`: `NS_IN_SEC`, `NS_IN_MIN`, `NS_IN_HOUR`, `NS_IN_DAY`, `NS_IN_30_DAYS`.
- **Platform splits** use the macros in `crates/xmtp_common/src/macros.rs`, not raw `#[cfg]`: `if_native!`, `if_wasm!`,
  `wasm_or_native! { native => {..}, wasm => {..} }`, `wasm_or_native_expr!`, plus `if_d14n!`, `if_v3!`, `if_dev!`, `if_local!`, `if_test!`, `if_only_test!`,
  `if_not_test!`.
- **`Send` bounds** must be platform-conditional: `crates/xmtp_common/src/wasm.rs:MaybeSend` / `MaybeSync` / `MaybeSendFuture` and the aliases `BoxDynError`,
  `BoxDynFuture`, `BoxDynStream` (blanket no-ops on wasm). Declare async traits with `#[xmtp_common::async_trait]` (`crates/xmtp_macro/src/lib.rs:async_trait`),
  which picks `async_trait(?Send)` on wasm.
- Spawn shared tasks with `xmtp_common::stream_handles::spawn(ready, future) -> impl StreamHandle`.
  Pass `None` for `ready`, or an `Option<oneshot::Receiver<()>>` whose sender the task uses to signal readiness.
  Other task APIs are exposed through `xmtp_common::task` (`crates/xmtp_common/src/wasm.rs`).
- **Cancellation**: keep the handle. `StreamHandle` in `crates/xmtp_common/src/stream_handles.rs` gives `wait_for_ready(&mut self)`, `end(&self)`, `join(self)`, `end_and_wait(&mut self)`,
  `abort_handle()`; the cloneable `AbortHandle` gives `end()` and `is_finished()`. Errors: `StreamHandleError`.
- **Retries**: `retry_async!(retry, (async { .. }))` (`crates/xmtp_common/src/retry.rs:retry_async`) with `Retry::default()` (5 retries, exponential backoff) or
  `Retry::builder().retries(n).with_strategy(..).build()`. Backoff:
  `ExponentialBackoff::builder().duration(..).max_jitter(..).multiplier(..).total_wait_max(..).build()`, or a custom `retry.rs:Strategy` impl. The macro already logs
  and honours `is_retryable`.
- Use `#[xmtp_common::timeout(..)]` only in tests; it panics on expiry. In production, use `xmtp_common::time::timeout(d, fut)` and handle `Expired`.
- **HTTP**: build every `reqwest` client with `crates/xmtp_common/src/http.rs:client` or `client_builder`. `.clippy.toml` forbids `reqwest::Client::new`,
  `Client::builder`, `ClientBuilder::new` — these helpers pin webpki roots on Android, where a default client aborts the process on its first TLS connection.

## 3. Logging and tracing

- `tracing` only: `println!` / `eprintln!` belong solely in `apps/*` binaries (CLI output), never in a library crate. `crates/xmtp_logging` owns the
  whole pipeline (see `crates/xmtp_logging/AGENTS.md`). Never add a second subscriber or pull `tracing-subscriber` into a new crate.
- **Spans**: prefer the canonical attribute macros over raw `#[tracing::instrument]` — they force `err, skip_all` and the `operation` / `sentry.op` / `sentry.name`
  fields the collector buckets on:
  - `#[xmtp_common::rpc_span]` / `db_span` / `mls_span` → `operation = "rpc.<fn>"` / `"db.<fn>"` / `"mls.<fn>"` (API example:
    `crates/xmtp_api_d14n/src/queries/combined.rs`); `#[xmtp_common::span(prefix = "stream")]` is the escape hatch for a new namespace.
  - `#[xmtp_common::err_span]` is **different** (`crates/xmtp_macro/src/span_macro.rs`): for FFI-exported fns, it sets `level = "trace"`, `skip_all`,
    `sentry.op = "ffi"`, `sentry.name = "<fn>"` and **no `operation` field**. An async fn is wrapped in `bind_task_hub` and logs the error inside that hub; a sync fn
    uses `err`; an `extern`-ABI fn passes through untouched, which keeps it napi-safe.
- Structured product events use `log_event!(Event::ClientCreated, ..)`, variants declared in `crates/xmtp_common/src/event_logging.rs:Event`
  (`#[xmtp_macro::build_logging_metadata]`). Add a variant there, not an ad-hoc `tracing::info!` (`crates/xmtp_mls/src/client.rs`).
- **Never log** raw key material, database encryption keys, user file paths, full installation ids, or message payloads; `skip_all` is mandatory on instrumented fns.
  Omit keys and payloads even when truncated. Truncate IDs with `crates/xmtp_common/src/fmt.rs:debug_hex` / `truncate_hex`, `crates/xmtp_common/src/snippet.rs:Snippet::snippet`, or
  `crates/xmtp_proto/src/traits/short_hex.rs:ShortHex::short_hex`.

## 4. Configuration

- Constants that **cross crate boundaries** — URLs, page sizes, shared timeouts — live in `crates/xmtp_configuration`. A private implementation constant stays in its
  own module. Never copy a shared value locally. Layout, naming, and the `prod/` vs `test/` split: `crates/xmtp_configuration/AGENTS.md`.

## 5. Database

- Client persistence (`crates/xmtp_db` and everything below `crates/xmtp_mls`) is Diesel + encrypted SQLite. Model traits, the `impl_*!` macros, `Query*` traits,
  transactions, migrations, and errors: `crates/xmtp_db/AGENTS.md`.
- Use `XmtpMlsStorageProvider::transaction` for client transactions. Both the closure and the method return `Result<TransactionOutcome<T>, E>`.
  `Ok(Continue(value))` commits; `Ok(Rollback)` rolls back without an error; `Err(error)` rolls back and propagates the error.
  Match the outcome or call `into_continued` to get the value. See `crates/xmtp_db/src/xmtp_openmls_provider.rs`.
- The backend uses Postgres and the access layer defined by spec 002. Client database conventions do not apply to backend storage.
  Do not add Postgres to a client crate.

## 6. Protobuf and types

- Generated prost code lives under `crates/xmtp_proto/src/gen/`. Regenerate with `dev/nix-shell 'dev/gen_protos.sh'`; never hand-edit a file under `gen/`.
- **Use the newtypes, not `Vec<u8>`/`String`**: `GroupId`, `InstallationId`, `Topic`, the payload wrappers, and the `Cursor` constructors in
  `crates/xmtp_proto/src/types/`. Check each type's API in `crates/xmtp_proto/AGENTS.md`.
  `InstallationId` has fewer methods than `GroupId`. `GroupId` Diesel conversions require the `diesel` feature.
  Use infallible `From` for fixed-size arrays and `TryFrom` with typed errors for variable-size input.
- Inbox ids are lowercase hex `String` (`crates/xmtp_common/src/types.rs:InboxId`). Normalize untrusted input with
  `crates/xmtp_common/src/hex.rs:NormalizeHex::normalize_hex` — never hand-roll `to_lowercase().trim_start_matches("0x")`.

## 7. Cryptography and identity

- `xmtp_cryptography` owns every primitive (hashing, randomness, signatures, installation credentials); `crates/xmtp_common` re-exports its `hash` and `rand`
  modules, so prefer the `xmtp_common::` path. Use `sha256_bytes` or `sha256_array` for SHA-256. See `crates/xmtp_cryptography/AGENTS.md` for entry points.
- Call `xmtp_cryptography::install_crypto_provider()` at native entry points before building a TLS client. The call is idempotent.
  Do not rely on the constructor fallback when linking a static library into an Apple binary.
- `xmtp_id` owns identity: `InboxOwner`, `Identifier`, inbox id derivation, association state, signature requests, smart-contract-wallet verification, key
  packages. Use `Identifier::inbox_id` for derivation; its address check covers Ethereum identifiers only.
  See `crates/xmtp_id/AGENTS.md` for input validation and APIs.
- **Message ids** are derived only by `xmtp_mls::utils::id::calculate_message_id` (`crates/xmtp_mls/AGENTS.md`). If `apps/backend` needs it without `xmtp_mls`,
  move the function to a shared crate. Preserve the separate rules for client MLS message IDs and envelope hashes.
- `ErrorCode` / `RetryableError` coverage is **not** uniform across these crates — check before assuming, wrap with `#[from]` rather than stringifying, and add the
  missing derive if a code must cross the FFI boundary.

## 8. Testing

Read `.claude/skills/writing-rust-tests/SKILL.md` and its required references before writing tests.
The project test rules in `docs/self-hosted/guidelines.md` take precedence.

- New Rust tests use `#[xmtp_common::test(unwrap_try = true)]`. The project exception is a crate that `xmtp_common` depends on.
  The macro selects `tokio::test` on native and `wasm_bindgen_test` on wasm and installs logging.
  Options include `flavor`, `worker_threads`, `unwrap_try`, and `disable_logging`; see `crates/xmtp_macro/src/test_macro.rs`.
  Existing plain tests do not require a bulk conversion.
- The macro and every helper below sit behind `xmtp_common`'s `test-utils` feature or `cfg(test)` (`crates/xmtp_common/src/lib.rs`). A crate using them in its
  own tests must enable that feature.
- **Clients**: `tester!(alix)` and friends (`crates/xmtp_mls/AGENTS.md`). **Scope**: all of it sits inside `xmtp_mls` behind `cfg(test)` or
  `xmtp_mls/test-utils`; `apps/backend` and crates below `xmtp_mls` need their own fixture.
- **Assertions**: `assert_ok!`, `assert_err!` (`crates/xmtp_common/src/test/macros.rs`). Async polling (`crates/xmtp_common/src/test.rs`), all with a 20 s
  timeout — note the differing failure modes: `wait_for_some -> Option<T>` (`None` on timeout), `wait_for_ok -> Result<T, Expired>` (**discards** the last error),
  `wait_for_eq` and `wait_for_ge -> Result<(), Expired>`. Never write a bare `sleep` loop in a test.
- Log assertions (native, `test-utils` only): `traced_test!` / `assert_logged!` (`crates/xmtp_common/src/test/traced_test.rs`).
- Generators in `crates/xmtp_common/src/test.rs`: `rand_string::<N>`, `rand_vec::<N>`, `rand_hexstring`, `rand_account_address`, `rand_u64`, `rand_i64`, `rand_time`,
  `tmp_path()` (wasm-aware temp DB path), the `Generate` trait for OpenMLS fakes, and `toxiproxy_test` for fault injection. Use `rstest` `#[case(..)]` for
  table-driven tests; `mockall` auto-mocks are gated on `cfg_attr(any(feature = "test-utils", test), ..)`.
- **`#[cfg(test)]` vs `test-utils`**: `#[cfg(test)]` (or `if_only_test!`) for helpers only this crate's tests need; `#[cfg(any(test, feature = "test-utils"))]` (or
  `if_test!`) when another crate must import it. A crate exposing test helpers needs a `test-utils` feature forwarding to its dependencies'
  (`crates/xmtp_mls/Cargo.toml` `[features]`).
- Running: `just test`, `just test crate <name>`, `just test v3 -p <name>`, `just test d14n -E 'test(pat)'`, `just wasm test`.

## 9. Crate and module conventions

- `lib.rs` declares `mod` / `pub mod` and re-exports some of them: `pub mod retry; pub use retry::*;` (`crates/xmtp_common/src/lib.rs`). Import from the crate
  root **only for re-exported items**: in `xmtp_common` `retry`, `wasm`, `stream_handles`, `const`, `event_logging` are re-exported, while `time`, `fmt`, `hex`,
  `http`, `snippet`, `types` are module-only and need the full path.
- Large crates also expose a `prelude` of query traits (`crates/xmtp_db/src/lib.rs:prelude`). Prefer `foo.rs` + a `foo/` sibling directory to `foo/mod.rs` for new
  modules (`crates/xmtp_common/src/test.rs` + `test/`). Keep internals `pub(crate)`; export a type only when a binding or another crate needs it.
- `optify!(expr)` / `optify!(expr, "msg")` (`crates/xmtp_common/src/macros.rs`) turns a `Result` into an `Option` and logs the error — use it instead of `.ok()`
  where the error should still reach the logs.
- Reuse the established feature name for an established capability: `test-utils`, `bench`, `dev`, `d14n`, `sentry`. Narrower capabilities have their own names
  (`diesel`, `exposed-keys`, `update-schema`, `deserialize-paths`, `grpc_server_impls`, `v3`), so a genuinely new capability may take a new name — never a synonym
  for an old one.
- Prefer `name.workspace = true` (or `{ workspace = true, features = [..] }`) for anything shared, pinned once in root `Cargo.toml` `[workspace.dependencies]`.
  Crate-local versions are allowed where justified, for target-specific or single-consumer deps (`crates/xmtp_logging/Cargo.toml`,
  `crates/xmtp_api_grpc/Cargo.toml`).
- New crates should use `[lints] workspace = true`. Let Cargo hakari determine `xmtp-workspace-hack` membership.
  After dependency changes, run `dev/nix-shell 'cargo hakari generate'` and `dev/nix-shell 'cargo hakari manage-deps'`.
  See `.config/hakari.toml`; `just lint-rust` checks both outputs.
- Put new crates in `crates/` and new binaries in `apps/`. Root `Cargo.toml` includes both through workspace globs.
- Read version settings from their source files: `rust-toolchain.toml` selects the active toolchain; root `Cargo.toml` sets `rust-version` (MSRV) and the Cargo edition;
  `rustfmt.toml` sets the formatter edition and import ordering. Clippy configuration is in `.clippy.toml` and the workspace lint tables.

## 10. Bindings

Three surfaces, one rule: a binding is a thin translation layer; business logic belongs in `xmtp_mls` or a shared crate.

- Use the existing error conversion for errors that implement `ErrorCode`. Each surface has one `ErrorWrapper` / `FfiError` that
  emits `"[{code}] {message}"`; the per-surface details are in `bindings/{mobile,node,wasm}/AGENTS.md`.
  In wasm, use the wrapper so the JavaScript error retains its `code` property.
- Node and wasm use bare, deliberately identical names (`Client`, `Conversation`, `BackendBuilder`) so the two JS SDKs stay symmetric — pick the same name on
  both. Mobile uses an `Ffi*` prefix.
- **Builders**: use the generated ones (`#[xmtp_macro::napi_builder]`, `wasm_builder`, `uniffi_builder`); `build()` is always hand-written
  (`crates/xmtp_macro/src/builders.rs`).
- `dist/` output and `crates/xmtp_proto/src/gen/**` are build products. Never hand-edit them. Regeneration commands per surface: the binding's `AGENTS.md`.

## 11. Anti-patterns

| Do not | Use instead |
| --- | --- |
| Hand-rolled `loop { .. sleep .. }` retry | `retry_async!(Retry::default(), (async { .. }))` |
| `reqwest::Client::new()` / `Client::builder()` | `xmtp_common::http::client()` / `client_builder()` |
| A hand-written `Store`/`Fetch` impl for a plain client table | `impl_store!` / `impl_fetch!` / `impl_fetch_list!` |
| `#[xmtp_common::timeout]` in production code (it panics) | `xmtp_common::time::timeout(..)` and handle `Expired` |
| Building a rustls/TLS client before provider install | call `install_crypto_provider()` at the native entry point |
| Re-deriving an MLS message id | `utils::id::calculate_message_id(..)` |
| `std::time::SystemTime::now()` / `Instant::now()` in shared code | `xmtp_common::time::{now_ns, Instant}` |
| `tokio::spawn` in a shared crate | `xmtp_common::stream_handles::spawn(None, fut)`; keep the `StreamHandle` |
| Bare `#[tracing::instrument]` on an API/DB/MLS/FFI fn | `#[xmtp_common::rpc_span]` / `db_span` / `mls_span` / `err_span` |
| Logging a full ID, key, or payload | Omit keys and payloads; truncate IDs; use `skip_all` |
| A raw `conn.transaction(..)` in a client crate | `XmtpMlsStorageProvider::transaction(\|tx\| .. )` → `Result<TransactionOutcome<T>, E>` |
| `Vec<u8>` for a group or installation id | `GroupId` / `InstallationId` newtypes |
| Editing `schema_gen.rs` or `src/gen/**` by hand | Regeneration commands in the owning package's `AGENTS.md` |
| Raw test attributes where the project test macro is available | `#[xmtp_common::test(unwrap_try = true)]` |
| Building a test client by hand inside `xmtp_mls` | `tester!(alix)` (not available outside `xmtp_mls`) |
| Renaming an error variant silently | keep the code with `#[error_code("Old::Name")]` |
