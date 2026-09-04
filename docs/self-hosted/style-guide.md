# libxmtp Style Guide (for implementer agents)

Scope: writing `apps/backend`, new shared crates, and refactors of `crates/xmtp_mls`. Rule zero: **search before you build** — almost every helper
below already exists, and re-inventing one is a review blocker. This guide holds the cross-cutting conventions. Package-specific conventions live in
the `## Conventions` section of that package's `AGENTS.md`; read it before working in a package. Run `just lint` before every commit: `lint-rust` (clippy `-Dwarnings`,
`cargo fmt --check`, `cargo hakari generate --diff`, `cargo hakari manage-deps --dry-run`) plus `lint-config` and `lint-markdown` (`justfile:39-59`).

## 1. Errors

- A module may hold several related error enums (`crates/xmtp_mls/src/groups/error.rs` holds four). Derive `ErrorCode` only when the code must be stable
  across the FFI boundary.
- Wrap sub-errors with `#[from]`; use `#[error(transparent)]` when the variant adds no context.
- **Retryability** is a trait, not a bool field: `crates/xmtp_common/src/retry.rs:RetryableError` (`fn is_retryable(&self) -> bool`). Implement it by hand,
  delegating to inner errors (`crates/xmtp_mls/src/groups/error.rs:588`). Call it through `retry.rs:retryable!` — `if retryable!(err) { .. }`.
- **Stable error codes** cross the FFI boundary. `crates/xmtp_common/src/error_code.rs:ErrorCode` gives `error_code() -> &'static str` formatted
  `"TypeName::VariantName"` for an enum, `"TypeName"` for a struct. Derive with `crates/xmtp_macro/src/lib.rs:derive_error_code` (attributes parsed at
  `crates/xmtp_macro/src/error_code.rs:48-93`): variant-level `#[error_code(inherit)]` (return the inner error's code) and `#[error_code("Old::Name")]` (keep a code
  after a rename); type-level `#[error_code(internal)]` (drop the type from the glossary, `apps/error_glossary/src/main.rs:55`) and
  `#[error_code(remote = "path::Type")]` (implement for a foreign type; see `crates/xmtp_common/src/error_code.rs` `cryptography_error_codes`).
- Document each variant with a doc comment saying whether it is retryable; the glossary generator reads them. After adding variants regenerate
  `docs/error_glossary.md` with `dev/gen-error-glossary` (runs `cargo run -p error_glossary`; source `apps/error_glossary/src/main.rs`).
- Never rename a variant without an `#[error_code("...")]` override — the code is a public contract.

## 2. Async and runtime

- **Read clocks and sleep through `crates/xmtp_common/src/time.rs`**, which re-exports `Duration` / `Instant` / `SystemTime` from `std` on native and `web_time` on
  wasm. A plain `std::time::Duration` stays valid as a value type in a constant or signature (`crates/xmtp_configuration/src/common/mls.rs:7`,
  `crates/xmtp_mls/src/context.rs:220`); it is `Instant`, `SystemTime`, and the timer functions that must come from `xmtp_common::time`.
  - `now_ns()`, `now_ms()`, `now_secs()` for wall-clock reads; `sleep(Duration)`; `timeout(Duration, fut) -> Result<T, Expired>`.
  - `interval_stream(period)` and `jittered_interval_stream(period, jitter)` for periodic workers; `rand_offset(jitter)` when a worker computes its own sleeps.
    Fleet-wide de-synchronization is solved — do not hand-roll it.
  - Nanosecond constants in `crates/xmtp_common/src/const.rs`: `NS_IN_SEC`, `NS_IN_MIN`, `NS_IN_HOUR`, `NS_IN_DAY`, `NS_IN_30_DAYS`.
- **Platform splits** use the macros in `crates/xmtp_common/src/macros.rs`, not raw `#[cfg]`: `if_native!`, `if_wasm!`,
  `wasm_or_native! { native => {..}, wasm => {..} }`, `wasm_or_native_expr!`, plus `if_d14n!`, `if_v3!`, `if_dev!`, `if_local!`, `if_test!`, `if_only_test!`,
  `if_not_test!`.
- **`Send` bounds** must be platform-conditional: `crates/xmtp_common/src/wasm.rs:MaybeSend` / `MaybeSync` / `MaybeSendFuture` and the aliases `BoxDynError`,
  `BoxDynFuture`, `BoxDynStream` (blanket no-ops on wasm). Declare async traits with `#[xmtp_common::async_trait]` (`crates/xmtp_macro/src/lib.rs:async_trait`),
  which picks `async_trait(?Send)` on wasm.
- **Spawning**: not `tokio::spawn`, but `crates/xmtp_common/src/stream_handles.rs:spawn(ready: Option<oneshot::Receiver<()>>, future) -> impl StreamHandle` (native
  `:275`, wasm `:143`) — pass `None` for `ready`, or a receiver the task signals when live. Task API at `xmtp_common::task::*` (`crates/xmtp_common/src/wasm.rs`).
- **Cancellation**: keep the handle. `stream_handles.rs:26 StreamHandle` gives `wait_for_ready(&mut self)`, `end(&self)`, `join(self)`, `end_and_wait(&mut self)`,
  `abort_handle()`; the cloneable `AbortHandle` gives `end()` and `is_finished()`. Errors: `StreamHandleError`.
- **Retries**: `retry_async!(retry, (async { .. }))` (`crates/xmtp_common/src/retry.rs:retry_async`) with `Retry::default()` (5 retries, exponential backoff) or
  `Retry::builder().retries(n).with_strategy(..).build()`. Backoff:
  `ExponentialBackoff::builder().duration(..).max_jitter(..).multiplier(..).total_wait_max(..).build()`, or a custom `retry.rs:Strategy` impl. The macro already logs
  and honours `is_retryable`.
- `#[xmtp_common::timeout(..)]` is a **test-only** helper: it wraps an async test body and **panics** on expiry (`crates/xmtp_macro/src/timeout_macro.rs:5-16`,
  `:39-50`). In production call `xmtp_common::time::timeout(d, fut)` and handle `Expired`.
- **HTTP**: build every `reqwest` client with `crates/xmtp_common/src/http.rs:client` or `client_builder`. `.clippy.toml:12-15` forbids `reqwest::Client::new`,
  `Client::builder`, `ClientBuilder::new` — these helpers pin webpki roots on Android, where a default client aborts the process on its first TLS connection.

## 3. Logging and tracing

- `tracing` only: `println!` / `eprintln!` belong solely in `apps/*` binaries (CLI output), never in a library crate. `crates/xmtp_logging` owns the
  whole pipeline (see `crates/xmtp_logging/AGENTS.md`). Never add a second subscriber or pull `tracing-subscriber` into a new crate.
- **Spans**: prefer the canonical attribute macros over raw `#[tracing::instrument]` — they force `err, skip_all` and the `operation` / `sentry.op` / `sentry.name`
  fields the collector buckets on:
  - `#[xmtp_common::rpc_span]` / `db_span` / `mls_span` → `operation = "rpc.<fn>"` / `"db.<fn>"` / `"mls.<fn>"` (API example:
    `crates/xmtp_api_d14n/src/queries/combined.rs:73`); `#[xmtp_common::span(prefix = "stream")]` is the escape hatch for a new namespace.
  - `#[xmtp_common::err_span]` is **different** (`crates/xmtp_macro/src/span_macro.rs:73-118`): for FFI-exported fns, it sets `level = "trace"`, `skip_all`,
    `sentry.op = "ffi"`, `sentry.name = "<fn>"` and **no `operation` field**. An async fn is wrapped in `bind_task_hub` and logs the error inside that hub; a sync fn
    uses `err`; an `extern`-ABI fn passes through untouched, which keeps it napi-safe.
- Structured product events use `log_event!(Event::ClientCreated, ..)`, variants declared in `crates/xmtp_common/src/event_logging.rs:Event`
  (`#[xmtp_macro::build_logging_metadata]`). Add a variant there, not an ad-hoc `tracing::info!` (`crates/xmtp_mls/src/client.rs:266`).
- **Never log** raw key material, database encryption keys, user file paths, full installation ids, or message payloads; `skip_all` is mandatory on instrumented fns.
  Truncate ids with `crates/xmtp_common/src/fmt.rs:debug_hex` / `truncate_hex`, `crates/xmtp_common/src/snippet.rs:Snippet::snippet`, or
  `crates/xmtp_proto/src/traits/short_hex.rs:ShortHex::short_hex`.

## 4. Configuration

- Constants that **cross crate boundaries** — URLs, page sizes, shared timeouts — live in `crates/xmtp_configuration`. A private implementation constant stays in its
  own module. Never copy a shared value locally. Layout, naming, and the `prod/` vs `test/` split: `crates/xmtp_configuration/AGENTS.md`.

## 5. Database

This section covers **client** databases only.

- Client persistence (`crates/xmtp_db` and everything below `crates/xmtp_mls`) is Diesel + encrypted SQLite. Model traits, the `impl_*!` macros, `Query*` traits,
  transactions, migrations, and errors: `crates/xmtp_db/AGENTS.md`.
- The backend is not bound by any of it. Spec 002 decides the backend's database layer (Postgres, and whichever Rust access layer fits, `sqlx` included). Do not
  assume the Diesel-SQLite patterns transfer, and do not add Postgres to a client crate.

## 6. Protobuf and types

- Generated prost code lives under `crates/xmtp_proto/src/gen/`. Regenerate with `dev/gen_protos.sh`; never hand-edit a file under `gen/`.
- **Use the newtypes, not `Vec<u8>`/`String`**: `GroupId`, `InstallationId`, `Topic`, the payload wrappers, and the `Cursor` constructors in
  `crates/xmtp_proto/src/types/`. The full API and the conversion convention for a new newtype: `crates/xmtp_proto/AGENTS.md`.
- Inbox ids are lowercase hex `String` (`crates/xmtp_common/src/types.rs:InboxId`). Normalize untrusted input with
  `crates/xmtp_common/src/hex.rs:NormalizeHex::normalize_hex` — never hand-roll `to_lowercase().trim_start_matches("0x")`.

## 7. Cryptography and identity

- `xmtp_cryptography` owns every primitive (hashing, randomness, signatures, installation credentials); `crates/xmtp_common` re-exports its `hash` and `rand`
  modules, so prefer the `xmtp_common::` path. Entry points and the `install_crypto_provider()` rule: `crates/xmtp_cryptography/AGENTS.md`.
- `xmtp_id` owns identity: `InboxOwner`, `Identifier`, inbox id derivation, association state, signature requests, smart-contract-wallet verification, key
  packages. Never re-implement the inbox id hash. Entry points: `crates/xmtp_id/AGENTS.md`.
- **Message ids** are derived only by `xmtp_mls::utils::id::calculate_message_id` (`crates/xmtp_mls/AGENTS.md`). If `apps/backend` needs it without `xmtp_mls`,
  move the function to a shared crate.
- `ErrorCode` / `RetryableError` coverage is **not** uniform across these crates — check before assuming, wrap with `#[from]` rather than stringifying, and add the
  missing derive if a code must cross the FFI boundary.

## 8. Testing

Read `.claude/skills/writing-rust-tests/` first (SKILL.md plus `fixtures.md`, `assertions.md`, `parametrized.md`, `utilities.md`, `wasm.md`,
`running.md`) — it is authoritative, and this section only names the entry points.

- Write a **new** async test as `#[xmtp_common::test]` in any crate that can depend on `xmtp_common` (`crates/xmtp_macro/src/test_macro.rs`): it dispatches to
  `tokio::test` on native and `wasm_bindgen_test` on wasm and installs the logger. Add `unwrap_try = true` only for a `()`-returning test that uses `?`. Options at
  `test_macro.rs:174` — `flavor`, `worker_threads`, `unwrap_try`, `disable_logging`. This is **not** absolute: hundreds of plain `#[test]` fns remain for sync,
  single-platform cases (`crates/xmtp_proto/src/types.rs:30`, `crates/xmtp_common/src/time.rs:171`). Keep them; do not convert in bulk.
- The macro and every helper below sit behind `xmtp_common`'s `test-utils` feature or `cfg(test)` (`crates/xmtp_common/src/lib.rs:13-20`). A crate using them in its
  own tests must enable that feature.
- **Clients**: `tester!(alix)` and friends (`crates/xmtp_mls/AGENTS.md`). **Scope**: all of it sits inside `xmtp_mls` behind `cfg(test)` or
  `xmtp_mls/test-utils`; `apps/backend` and crates below `xmtp_mls` need their own fixture.
- **Assertions**: `assert_ok!`, `assert_err!` (`crates/xmtp_common/src/test/macros.rs`). Async polling (`crates/xmtp_common/src/test.rs:86-161`), all with a 20 s
  timeout — note the differing failure modes: `wait_for_some -> Option<T>` (`None` on timeout), `wait_for_ok -> Result<T, Expired>` (**discards** the last error),
  `wait_for_eq` and `wait_for_ge -> Result<(), Expired>`. Never write a bare `sleep` loop in a test.
- Log assertions (native, `test-utils` only): `traced_test!` / `assert_logged!` (`crates/xmtp_common/src/test/traced_test.rs:96`, `:134`).
- Generators in `crates/xmtp_common/src/test.rs`: `rand_string::<N>`, `rand_vec::<N>`, `rand_hexstring`, `rand_account_address`, `rand_u64`, `rand_i64`, `rand_time`,
  `tmp_path()` (wasm-aware temp DB path), the `Generate` trait for OpenMLS fakes, and `toxiproxy_test` for fault injection. Use `rstest` `#[case(..)]` for
  table-driven tests; `mockall` auto-mocks are gated on `cfg_attr(any(feature = "test-utils", test), ..)`.
- **`#[cfg(test)]` vs `test-utils`**: `#[cfg(test)]` (or `if_only_test!`) for helpers only this crate's tests need; `#[cfg(any(test, feature = "test-utils"))]` (or
  `if_test!`) when another crate must import it. A crate exposing test helpers needs a `test-utils` feature forwarding to its dependencies'
  (`crates/xmtp_mls/Cargo.toml` `[features]`).
- Running: `just test`, `just test crate <name>`, `just test v3 <name>`, `just test d14n -E 'test(pat)'`, `just wasm test`.

## 9. Crate and module conventions

- `lib.rs` declares `mod` / `pub mod` and re-exports some of them: `pub mod retry; pub use retry::*;` (`crates/xmtp_common/src/lib.rs:29-35`). Import from the crate
  root **only for re-exported items**: in `xmtp_common` `retry`, `wasm`, `stream_handles`, `const`, `event_logging` are re-exported, while `time`, `fmt`, `hex`,
  `http`, `snippet`, `types` are module-only (`:37-42`) and need the full path.
- Large crates also expose a `prelude` of query traits (`crates/xmtp_db/src/lib.rs:prelude`). Prefer `foo.rs` + a `foo/` sibling directory to `foo/mod.rs` for new
  modules (`crates/xmtp_common/src/test.rs` + `test/`). Keep internals `pub(crate)`; export a type only when a binding or another crate needs it.
- `optify!(expr)` / `optify!(expr, "msg")` (`crates/xmtp_common/src/macros.rs:5`) turns a `Result` into an `Option` and logs the error — use it instead of `.ok()`
  where the error should still reach the logs.
- Reuse the established feature name for an established capability: `test-utils`, `bench`, `dev`, `d14n`, `sentry`. Narrower capabilities have their own names
  (`diesel`, `exposed-keys`, `update-schema`, `deserialize-paths`, `grpc_server_impls`, `v3`), so a genuinely new capability may take a new name — never a synonym
  for an old one.
- Prefer `name.workspace = true` (or `{ workspace = true, features = [..] }`) for anything shared, pinned once in root `Cargo.toml` `[workspace.dependencies]`.
  Crate-local versions exist and are allowed where justified, for target-specific or single-consumer deps (`crates/xmtp_logging/Cargo.toml:47`,
  `crates/xmtp_api_grpc/Cargo.toml:10`).
- **New** crates should carry `[lints] workspace = true` and `xmtp-workspace-hack.workspace = true`; the tree is not uniform (`crates/xmtp_configuration/Cargo.toml`
  has no `[lints]`, `apps/error_glossary/Cargo.toml` has neither). Let `cargo hakari manage-deps` decide hack membership; after touching dependencies run
  `cargo hakari generate` and `cargo hakari manage-deps` (`.config/hakari.toml`). `just lint-rust` checks both.
- New crates go in `crates/`, new binaries in `apps/`; both are globbed in by the root `Cargo.toml`. Four separate version knobs:
  `rust-toolchain.toml` pins the **active toolchain** (1.97.1); root `Cargo.toml:30` sets the **MSRV** (`rust-version = "1.94.0"`) and `:32` the
  **Cargo edition** (2024); `rustfmt.toml` sets its own `edition = "2024"` plus `reorder_imports = true` — the toolchain file sets no edition. Clippy
  config is the hidden `.clippy.toml` (no `clippy.toml`); workspace clippy allows only `arc_with_non_send_sync` and `uninlined_format_args`,
  everything else is `-Dwarnings`.

## 10. Bindings

Three surfaces, one rule: a binding is a thin translation layer; business logic belongs in `xmtp_mls` or a shared crate.

- **Core-error mapping is solved for any error that has an `ErrorCode` — do not write a new conversion.** Each surface has one `ErrorWrapper` / `FfiError` that
  emits `"[{code}] {message}"`; the per-surface details are in `bindings/{mobile,node,wasm}/AGENTS.md`.
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
| Logging a full id, key, or payload | `debug_hex`, `Snippet::snippet`, `ShortHex::short_hex`, and `skip_all` |
| A raw `conn.transaction(..)` in a client crate | `XmtpMlsStorageProvider::transaction(\|tx\| .. )` → `Result<TransactionOutcome<T>, E>` |
| `Vec<u8>` for a group or installation id | `GroupId` / `InstallationId` newtypes |
| Editing `schema_gen.rs` or `src/gen/**` by hand | `cargo run --bin update-schema --features update-schema`; `dev/gen_protos.sh` |
| `#[tokio::test]`, or `#[test]` on a new async test | `#[xmtp_common::test]` (`unwrap_try = true` when using `?`) |
| Building a test client by hand inside `xmtp_mls` | `tester!(alix)` (not available outside `xmtp_mls`) |
| Renaming an error variant silently | keep the code with `#[error_code("Old::Name")]` |

## Review status

Adversarial review thread `01a06265-8e16-7183-9ee7-925ab36232a2` (Codex, gpt-5.6-sol, read-only). Every finding was re-checked against the source
before the text changed; all were confirmed and applied, none rejected.

| Finding (document area) | Applied | Note |
| --- | --- | --- |
| Transaction signature — **blocker** | yes | closure **and** method return `Result<TransactionOutcome<T>, E>`; `into_continued` |
| `InstallationId` API — **blocker** | yes | real, smaller API listed; absent `as_bytes`/`to_openmls`/`random`/`FromStr`/Diesel named |
| Errors: "one enum per module"; ErrorCode defaults, `internal` | yes | 5 enums in one file; struct default and `internal` added |
| Errors: ErrorCode/RetryableError coverage claim | yes | exact types named; remote derives cited |
| Async: "never use `std::time`"; `spawn` API; `#[timeout]` | yes | scoped to clocks/timers; `ready` arg + handle methods; timeout is test-only and panics |
| Logging: `err_span` described like the other spans | yes | no `operation`; trace level, per-call hub, extern pass-through |
| Config: "all tunable constants in `xmtp_configuration`" | yes | narrowed to cross-crate values |
| DB: PostgreSQL claim; `Store`/`Fetch`/`Delete` impl side; `impl_*!` macros omitted | yes | `apps/xnet` infra separated; per-trait side and signature; macros added |
| DB: stale migration count; "one scope one connection" | yes | count removed; restated as a consuming conversion |
| Types: `GroupId` Diesel feature; omitted payload wrappers and `Cursor` constructors | yes | `diesel` feature cited; wrappers added |
| Crypto: `sha256` alias; `inbox_id` signature/validation; `install_crypto_provider` omitted | yes | alias marked legacy; Ethereum-only check; native entry-point rule |
| Shared: `xmtp_common::http` and `calculate_message_id` omitted | yes | added, with the `.clippy.toml` ban |
| Testing: "always use `xmtp_common::test`"; `test-utils` gate; `wait_for_*` returns; `tester!` scope | yes | scoped; plain `#[test]` kept; per-helper failure modes; `tester!` is `xmtp_mls`-only |
| Macros: `optify!`, `traced_test!`, `assert_logged!` omitted | yes | added to sections 8 and 9 |
| Crates: root-import claim; "fixed" feature names; workspace deps; `[lints]`/hack uniformity | yes | all four softened to match the tree |
| Tooling: `just lint` scope; edition vs toolchain vs MSRV; `.clippy.toml` | yes | Nix and markdownlint added; four version knobs separated |
| Bindings: "error mapping solved"; `dev/bindings` "missing" | yes | `E: ErrorCode` bound and wasm `BackendBuilder` gap; SDK-local scripts cited |

Residual risk: line-number citations (`groups/error.rs:588`, `traits.rs:23-66`) drift as the tree changes — treat the `path:symbol` part as
authoritative and the number as a hint. Claims were verified against branch `self-hosted` at commit `cc878025d`; a later commit can invalidate any of
them. Inventories the review did not enumerate exhaustively (the `Query*` trait list, `xmtp_configuration` areas, binding attribute lists) were not
re-checked item by item — confirm one by grep before depending on it. `apps/backend` did not exist at review time, so every claim about what it can
reuse is a projection from the current crates, not an observed fact.

2026-09-04, PR review: the database section is scoped to client databases (the backend chooses its own layer in spec 002), and the package-specific
material of sections 3 to 7 and 10 moved into the `## Conventions` section of the owning package's `AGENTS.md`.
