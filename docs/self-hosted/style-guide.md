# libxmtp Style Guide (for implementer agents)

Scope: writing `apps/backend`, new shared crates, and refactors of `crates/xmtp_mls`. Rule zero: **search before you build** — almost every helper
below already exists, and re-inventing one is a review blocker. Run `just lint` before every commit: `lint-rust` (clippy `-Dwarnings`,
`cargo fmt --check`, `cargo hakari generate --diff`, `cargo hakari manage-deps --dry-run`) plus `lint-config` and `lint-markdown` (`justfile:39-59`).

## 1. Errors

- A module may hold several related error enums — `crates/xmtp_mls/src/groups/error.rs` defines `GroupError:93`, `DeleteMessageError:471`,
  `MetadataPermissionsError:507`, `DmValidationError:560`. Derive `ErrorCode` only when the code must be stable across the FFI boundary (`:92` derives it; `:470`
  does not).
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

- `tracing` only: `println!` / `eprintln!` belong solely in `apps/*` binaries (CLI output), never in a library crate.
  `crates/xmtp_logging/src/lib.rs` owns the whole pipeline: `XmtpLoggingBuilder`, `LoggingHandle`, `filter_directive`, `Level` / `Rotation` / `ProcessType`
  (`src/config.rs`), OTLP `init` (native only), and the optional Sentry backend (`src/sentry.rs`, feature `sentry`). Never add a second subscriber or pull
  `tracing-subscriber` into a new crate.
- **Component tag**: Sentry events carry a `component` tag defaulting to `"libxmtp"`; a caller-supplied one wins (`sentry.rs:148-155`) — set it, do not shadow it.
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
  own module (`crates/xmtp_mls/src/worker.rs:26`, `crates/xmtp_mls/src/groups/change_callbacks.rs:88`). Never copy a shared value locally.
- Layout (`crates/xmtp_configuration/src/lib.rs`): `common/` is always compiled and re-exported; `test/` replaces `prod/` under
  `cfg(any(test, feature = "test-utils"))`, and both halves must export the **same symbol names**. Put a new constant in `common/<area>.rs` if one value fits every
  build, otherwise in both `prod/<area>.rs` and `test/<area>.rs` (example: `prod/api.rs:MAX_PAGE_SIZE` = 100, `test/api.rs:MAX_PAGE_SIZE` = 20).
- Areas: `common/{api,db,d14n,env,metadata,mls,tracing}.rs`. Add a new file plus a `mod` / `pub use` line, not more lines in one file. `SCREAMING_SNAKE_CASE` for a
  free constant; group related endpoints as an empty struct with associated consts (`common/api.rs:DeviceSyncUrls`, `GrpcUrls`, `DockerUrls`), selected by `if_dev!`
  / `if_local!`. Crate features are `test-utils` and `dev` only; keep it dependency-light.

## 5. Database

- Rust persistence is Diesel + encrypted SQLite: no `sqlx`, no `PgConnection`, no `diesel::pg` in any Rust data path; the root `Cargo.toml` pins
  `diesel = { version = "2.3", default-features = false }` patched to a SQLite-focused fork. PostgreSQL is present, but only as **infrastructure** the `apps/xnet`
  harness runs for the Go services (`apps/xnet/lib/src/services/mlsdb.rs:53`, `replication_db.rs:69`, `v3_db.rs`, `apps/xnet/lib/src/services.rs:72`) — not a Rust
  data-access convention. If `apps/backend` needs Postgres from Rust, that is **new**: state it in the design and do not assume these Diesel-SQLite patterns
  transfer.
- Model traits (`crates/xmtp_db/src/traits.rs:23-66`) — note which side implements each:
  - **For the model**: `Store<C>` (`store(&self, into: &C) -> Result<Self::Output, StorageError>`, errors on conflict) and `StoreOrIgnore<C>` (silent no-op on a
    unique-constraint violation).
  - **For the connection or query type**: `Fetch<Model>` (`fetch(&self, key: &Self::Key) -> Result<Option<Model>, _>`), `FetchList<Model>`
    (`-> Result<Vec<Model>, _>`), `FetchListWithKey<Model>` (takes `&[Self::Key]`), and `Delete<Model>` (`delete(&self, key: Self::Key) -> Result<usize, _>` — key
    **by value**, returns the row count).
  - `IntoConnection` is plain: `into_connection(self) -> Self::Connection`, no `Result`.
- For a standard table use the macros in `crates/xmtp_db/src/encrypted_store/mod.rs` instead of hand-written impls: `impl_fetch!(Model, table[, Key])` (`:352`),
  `impl_fetch_list!` (`:382`), `impl_store!` (`:399`), `impl_store_or_ignore!` (`:420`). Example: `crates/xmtp_db/src/encrypted_store/group.rs:176`.
- Query methods group into per-table `Query*` traits (`QueryGroup`, `QueryConsentRecord`, …), aggregated by `crates/xmtp_db/src/traits.rs:DbQuery`. A new table needs
  a new `Query*` trait, a `pub use` in `crates/xmtp_db/src/lib.rs` `prelude`, and a line in the `DbQuery` supertrait list. Callers take `impl DbQuery`.
- Connections: `encrypted_store/mod.rs:209 ConnectionExt` (`raw_query`, `disconnect`, `reconnect`) is the low-level contract, blanket-implemented for `&C`, `&mut C`,
  `Arc<C>`, and `encrypted_store/db_connection.rs:DbConnection<C>`; `XmtpDb` (same `mod.rs`) ties a `Connection` to a `DbQuery`. Converting a `DbConnection<C>` into
  `XmtpOpenMlsProvider<SqlKeyStore<C>>` **consumes** the wrapper and moves the inner `C` out (`db_connection.rs:49`).
- **Transactions are on the storage provider, not the connection.** There is no `DbConnection::transaction`. Use
  `crates/xmtp_db/src/xmtp_openmls_provider.rs:69 XmtpMlsStorageProvider::transaction` (`savepoint:80` when nested):
  `fn transaction<T, E, F>(&self, f: F) -> Result<TransactionOutcome<T>, E>` where `F: FnOnce(&mut Self::TxQuery) -> Result<TransactionOutcome<T>, E>`. The closure
  **and** the method both return `Result<TransactionOutcome<T>, E>`: `Ok(Continue(v))` commits, `Ok(Rollback)` rolls back with no error, `Err(e)` rolls back and
  propagates. The value is **not** unwrapped for you — match the outcome, or call `into_continued` (`:34`). Implementation:
  `crates/xmtp_db/src/sql_key_store/transactions.rs:59` (`BEGIN IMMEDIATE`, so SQLite honours `BUSY_TIMEOUT`). Inside a transaction, reach the MLS key
  store via `crates/xmtp_db/src/traits.rs:TransactionalKeyStore::key_store`.
- **Migrations**: `crates/xmtp_db/migrations/<YYYY-MM-DD-HHMMSS>[-NNNN]_<snake_description>/{up.sql,down.sql}` (recent ones add the `-0000` suffix). Generate with
  `diesel migration generate <name>`, then `cargo run --bin update-schema --features update-schema` (`crates/xmtp_db/migrations/README.md`, `diesel.toml`). Embedded
  at `encrypted_store/mod.rs:68 MIGRATIONS`; runtime control via `encrypted_store/migrations.rs:QueryMigrations`.
- `encrypted_store/schema_gen.rs` is **generated by Diesel CLI** — never hand-edit; `encrypted_store/schema.rs` is hand-written, re-exporting
  `schema_gen::*` and adding the `conversation_list` view the CLI cannot generate.
- Errors (`crates/xmtp_db/src/errors.rs`): `StorageError:17`, `NotFound:127` (the single enum for every "lost item" case), `DuplicateItem:217`, plus
  `ConnectionError` in `encrypted_store/mod.rs`. All derive `ErrorCode` and hand-implement `RetryableError`. Add a `NotFound` variant, not a bespoke enum; wrap with
  `#[from]`, never strings.

## 6. Protobuf and types

- Generated prost code lives under `crates/xmtp_proto/src/gen/`, surfaced by `pub use generated::*` in `crates/xmtp_proto/src/lib.rs` and the aliases
  `xmtp_proto::mls_v1` / `identity_v1`. Regenerate with `dev/gen_protos.sh`; never hand-edit a file under `gen/`.
- **Use the newtypes, not `Vec<u8>`/`String`** (`crates/xmtp_proto/src/types/`):
  - `types/ids/group_id.rs:22 GroupId` — `[u8; 16]`; `as_slice`, `as_bytes`, `into_bytes`, `to_vec`, `to_openmls`, `random(rand)`, `ZERO` / `ONE`.. `FOUR`, `Deref`,
    `FromStr` (error `GroupIdParseError:175`). Its Diesel `ToSql` / `FromSql<Binary, Sqlite>` needs the crate feature `diesel` (`group_id.rs:3`,
    `crates/xmtp_proto/Cargo.toml:78`).
  - `types/ids/installation_id.rs:6 InstallationId` — `[u8; 32]` with a **smaller** API than `GroupId`: only `to_vec`, `Deref` / `AsRef`, `From<[u8; 32]>`,
    `Into<Vec<u8>>`, `TryFrom<Vec<u8>>` / `TryFrom<&[u8]>` → `ConversionError`. No `as_bytes`, `into_bytes`, `to_openmls`, `random`, `FromStr`, or Diesel impl.
  - `types/topic.rs:Topic` / `TopicKind` — build with `Topic::new_group_message(..)`, `new_welcome_message(..)`, `new_identity_update(..)`, `new_key_package(..)`.
    Never concatenate topic bytes by hand.
  - Payload wrappers, each with a `derive_builder` `builder()` — pass these between layers, not raw prost structs: `types/group_message.rs:11 GroupMessage`
    (`is_commit`), `types/welcome_message.rs:15 WelcomeMessage` (`as_v1`), `types/orphaned_envelope.rs:11 OrphanedEnvelope`,
    `types/message_metadata.rs:GroupMessageMetadata`, `types/cursor_list.rs:CursorList`.
  - `types/cursor.rs:20 Cursor` — prefer the named constructors to `Cursor::new`: `commit_log`, `v3_welcomes`, `v3_messages`, `installations`, `mls_commits`,
    `inbox_log` (`:33-73`) each pin the right originator id.
  - Also present: `types/{global_cursor,topic_cursor,app_version,api_identifier}.rs`; scalar aliases `types.rs:27 OriginatorId` (`u32`), `SequenceId` (`u64`).
- Conversion convention for a new newtype: infallible `From` for fixed-size arrays (`From<[u8; 16]> for GroupId`), `TryFrom` for `Vec<u8>` / `&[u8]` with a typed
  error.
- Inbox ids are lowercase hex `String` (`crates/xmtp_common/src/types.rs:InboxId`). Normalize untrusted input with
  `crates/xmtp_common/src/hex.rs:NormalizeHex::normalize_hex` (lowercases, strips `0x`) — never hand-roll `to_lowercase().trim_start_matches("0x")`.

## 7. Cryptography and identity

- `xmtp_cryptography` owns every primitive. `crates/xmtp_common/src/lib.rs` re-exports its `hash` and `rand` modules, so prefer the `xmtp_common::` path:
  - Hashing (`crates/xmtp_cryptography/src/hash.rs`): `sha256_bytes(&[u8]) -> Vec<u8>`, `sha256_array(&[u8]) -> [u8; 32]` (allocation-free). Neither
    `xmtp_cryptography` nor `xmtp_common` has a plain `sha256`; the alias at `crates/xmtp_mls/src/utils/mod.rs:15` is legacy — do not reach for it in new code.
  - **Native entry points must call `crates/xmtp_cryptography/src/lib.rs:29 install_crypto_provider()` first.** It installs the process-default rustls provider and
    is idempotent. Its `#[ctor]` fallback does not fire when the static library is linked into an Apple binary, so a `reqwest` client built before this call panics
    with "No provider set" (`bindings/node/src/client/create_client.rs:231`, `bindings/mobile/src/mls.rs:152`). Native only.
  - Randomness (`src/rand.rs`, ChaCha20-backed): `rng()`, `seeded_rng(seed)`, `rand_string::<N>()`, `rand_array::<N>()`, `rand_vec::<N>()`, `rand_secret::<N>()`.
    Signatures (`src/signature.rs`): `SignatureError`, `RecoverableSignature::recover_address`, `is_valid_ethereum_address`, `sanitize_evm_addresses`,
    `h160addr_to_string`.
  - Installation credentials (`src/basic_credential.rs`, re-exported at the crate root): `XmtpInstallationCredential`, traits `CredentialSign` / `CredentialVerify` /
    `SigningContextProvider`. Ciphersuites and key lengths: `src/configuration.rs` (`CIPHERSUITE`, `ED25519_KEY_LENGTH`); test wallet:
    `src/utils.rs:generate_local_wallet()`.
- `xmtp_id` owns identity:
  - `crates/xmtp_id/src/lib.rs:InboxOwner` — `get_identifier() -> Result<Identifier, IdentifierValidationError>` and
    `sign(&str) -> Result<UnverifiedSignature, SignatureError>`; blanket impls for `&T` and alloy's `PrivateKeySigner`. Same file defines `InboxId = String` and
    `InboxIdRef<'a> = &'a str`. Build a `crates/xmtp_id/src/associations/member.rs:32 Identifier` with `Identifier::eth(..)`, `::passkey(..)`, `::from_proto(..)`,
    and call `sanitize()` before trusting input.
  - **Inbox id derivation**: `crates/xmtp_id/src/associations/member.rs:171 Identifier::inbox_id(&self, nonce: u64) -> Result<String, AssociationError>`. Its
    `is_valid_address` check (`:182`) covers **Ethereum identifiers only** — a passkey passes unchecked. It hashes the displayed identifier plus the nonce with
    SHA-256, hex-encoded by a private `sha256_string` via `format!("{:x}", ..)`, **not** `hex::encode`. Always go through this method; never re-implement the hash.
  - State: `associations/state.rs:AssociationState` (immutable updates `add`, `remove`, `set_recovery_identifier`, `diff`), driven by
    `associations/mod.rs:apply_update` / `get_state`. Signature requests: `associations/builder.rs:SignatureRequestBuilder::new(inbox_id)` → `.create_inbox(..)` /
    `.add_association(..)` / `.revoke_association(..)` → `.build() -> SignatureRequest`, then async `add_signature(sig, scw_verifier)` and `build_identity_update()`.
  - Smart-contract wallets: `scw_verifier/mod.rs:84 SmartContractSignatureVerifier` (async `is_valid_signature`, ERC-6492), with
    `MultiSmartContractSignatureVerifier` and blanket impls for `Arc<T>` / `&T` / `Box<T>`. Take `impl SmartContractSignatureVerifier`, never a concrete verifier.
    Key packages: `crates/xmtp_id/src/key_package/verified_key_package_v2.rs` — do not parse them inline.
- **Message ids**: `crates/xmtp_mls/src/utils/mod.rs:36 id::calculate_message_id(group_id, bytes, idempotency_key)` and `:61 calculate_message_id_for_intent(intent)`
  (`crates/xmtp_mls/src/groups/mls_sync.rs:41`). Never re-derive the `group_id \t key \t payload` hash. If `apps/backend` needs it without `xmtp_mls`, move the
  function to a shared crate.
- Coverage is **not** uniform — check before assuming. `SignatureError` and `IdentifierValidationError` get `ErrorCode` from remote derives in
  `crates/xmtp_common/src/error_code.rs:48-78`, not from their own crate; `SignerError`, `IdentityError` (`crates/xmtp_id/src/lib.rs:20`), and `GroupIdParseError`
  implement neither trait today. Wrap with `#[from]` rather than stringifying, and add the missing derive if a code must cross the FFI boundary.

## 8. Testing

Read `.claude/skills/writing-rust-tests/` first (SKILL.md plus `fixtures.md`, `assertions.md`, `parametrized.md`, `utilities.md`, `wasm.md`,
`running.md`) — it is authoritative, and this section only names the entry points.

- Write a **new** async test as `#[xmtp_common::test]` in any crate that can depend on `xmtp_common` (`crates/xmtp_macro/src/test_macro.rs`): it dispatches to
  `tokio::test` on native and `wasm_bindgen_test` on wasm and installs the logger. Add `unwrap_try = true` only for a `()`-returning test that uses `?`. Options at
  `test_macro.rs:174` — `flavor`, `worker_threads`, `unwrap_try`, `disable_logging`. This is **not** absolute: hundreds of plain `#[test]` fns remain for sync,
  single-platform cases (`crates/xmtp_proto/src/types.rs:30`, `crates/xmtp_common/src/time.rs:171`). Keep them; do not convert in bulk.
- The macro and every helper below sit behind `xmtp_common`'s `test-utils` feature or `cfg(test)` (`crates/xmtp_common/src/lib.rs:13-20`). A crate using them in its
  own tests must enable that feature.
- **Clients**: `tester!(alix)` / `tester!(bo, from: alix)` / `tester!(alix2, snapshot: snap)` (`crates/xmtp_mls/src/utils/test/tester_utils.rs:822`, builder
  `:406 TesterBuilder`); any `TesterBuilder` method works as a `key: value` or bare `key` argument, and `utils/test/mod.rs` adds `ClientBuilder::temp_store()`,
  `.dev()`, `.local()`. **Scope**: all of this sits inside `xmtp_mls` behind `cfg(test)` or `xmtp_mls/test-utils` (`crates/xmtp_mls/src/utils/mod.rs:8`);
  `apps/backend` and crates below `xmtp_mls` need their own fixture.
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

- **Core-error mapping is solved for any error that has an `ErrorCode` — do not write a new conversion.** Both `ErrorWrapper` types are bounded on `E: ErrorCode`
  (`bindings/node/src/lib.rs:43`), so callback errors (`bindings/mobile/src/inbox_owner.rs:9`) and serde errors keep their own separate conversions. Known gap:
  `bindings/wasm/src/client/backend.rs:59` builds a plain `JsError` and **drops the code** — use `ErrorWrapper::js` in new code rather than copying that line.
  - Mobile (both in `bindings/mobile/src/lib.rs`, no `error.rs`): `:34 GenericError` (variants use `#[from]` + `#[error_code(inherit)]`) and `:147 FfiError`
    (`#[uniffi(flat_error)]`). A blanket `impl<T: Into<GenericError>> From<T> for FfiError` (`:167`) means a new core error only needs a `GenericError` variant;
    `Display for FfiError` (`:151`) emits `"[{error_code}] {message}"`, how mobile SDKs read the code.
  - Node: `bindings/node/src/lib.rs:ErrorWrapper<E: ErrorCode>` → `napi::Error::from_reason("[{code}] {msg}")`; call sites use `.map_err(ErrorWrapper::from)?`
    (`bindings/node/src/client/backend.rs:88`).
  - Wasm: `bindings/wasm/src/errors.rs:ErrorWrapper` → `JsError` with the same string **and** a real `code` property set via `js_sys::Reflect::set`. Use
    `ErrorWrapper::js(e)`, and `errors.rs:to_value` for serde payloads (BigInt-safe).
- **Naming**: mobile uses an `Ffi*` prefix (`FfiIdentifier`, `FfiSentryConfig`) — a convention, not a rule (`bindings/mobile/src/mls.rs:127 XmtpApiClient`,
  `mls.rs:348 DbOptions` predate it). Follow it for new mobile types. Node and wasm use bare, deliberately identical names (`Client`, `Conversation`,
  `BackendBuilder`) so the two JS SDKs stay symmetric — pick the same name on both.
- **Exporting**:
  - Mobile: `#[uniffi::export]`, `#[derive(uniffi::Record)]` for plain data, `uniffi::Object` for opaque handles, `uniffi::Enum`. Async **must** name the runtime:
    `#[uniffi::export(async_runtime = "tokio")]` (`bindings/mobile/src/mls.rs:142`); foreign-implemented callback traits use `#[uniffi::export(with_foreign)]`
    (`src/inbox_owner.rs:29`). Scaffolding: `src/lib.rs:31 uniffi::setup_scaffolding!("xmtpv3")`.
  - Node: `#[napi]`, `#[napi(object)]`, `#[napi(getter)]`, `#[napi(string_enum)]`, `#[napi(js_name = "...")]`; `pub async fn` becomes a Promise. Add
    `#[xmtp_common::err_span]` to exported methods (`bindings/node/src/client/mod.rs:54`).
  - Wasm: `#[wasm_bindgen]`, `#[wasm_bindgen(js_name = camelCase)]`, `#[wasm_bindgen(constructor)]`, and `#[wasm_bindgen_numbered_enum]` from `bindings_wasm_macros`
    (`crates/wasm_macros`); `async fn` → Promise.
- **Builders**: use the generated ones — `#[xmtp_macro::napi_builder]` (`bindings/node/src/client/backend.rs:9`), `#[xmtp_macro::wasm_builder]`
  (`bindings/wasm/src/client/backend.rs:7`), `#[xmtp_macro::uniffi_builder]` (so far only in `bindings/mobile/src/builder_test.rs`). Field attributes:
  `#[builder(required)]`, `#[builder(optional)]`, `#[builder(default = "expr")]`, `#[builder(skip)]`. `build()` is always hand-written
  (`crates/xmtp_macro/src/builders.rs`).
- **Regeneration after any surface change**:
  - Mobile has **no `.udl`**; bindings come from the proc macros via `bindings/mobile/bindgen/bin.rs` (`[[bin]] ffi-uniffi-bindgen`,
    `required-features = ["uniffi/cli"]`), driven by `nix/lib/uniffiGenerate.nix` (`--language swift|kotlin` only). Run `just ios build` / `just android build`:
    these recipes run inside the SDK directories, where the `dev/bindings` scripts exist (`sdks/android/dev/bindings`, `sdks/ios/dev/bindings`, wired in
    `sdks/android/android.just:1`, `sdks/ios/ios.just:1`). There is no root-level `dev/bindings`. Nix targets `ios-xcframeworks` / `ios-xcframeworks-fast`
    (`flake.nix:108`) are an alternative.
  - Node: `just node build` (`yarn napi build --platform --esm`, then `bindings/node/node.just:_prepare-dist` moves output to `dist/`); `just node test` builds with
    `--features test-utils`. Run `just node lint` for any Node change.
  - Wasm: `just wasm build` (`nix build .#wasm-bindings`) runs `wasm-pack build --target web --out-dir ./dist` (`bindings/wasm/package.json`,
    `nix/package/wasm.nix:99`).
  - `dist/` output and `crates/xmtp_proto/src/gen/**` are build products. Never hand-edit them.

## 11. Anti-patterns

| Do not | Use instead |
| --- | --- |
| Hand-rolled `loop { .. sleep .. }` retry | `retry_async!(Retry::default(), (async { .. }))` |
| `reqwest::Client::new()` / `Client::builder()` | `xmtp_common::http::client()` / `client_builder()` |
| A hand-written `Store`/`Fetch` impl for a plain table | `impl_store!` / `impl_fetch!` / `impl_fetch_list!` |
| `#[xmtp_common::timeout]` in production code (it panics) | `xmtp_common::time::timeout(..)` and handle `Expired` |
| Building a rustls/TLS client before provider install | call `install_crypto_provider()` at the native entry point |
| Re-deriving an MLS message id | `utils::id::calculate_message_id(..)` |
| `std::time::SystemTime::now()` / `Instant::now()` in shared code | `xmtp_common::time::{now_ns, Instant}` |
| `tokio::spawn` in a shared crate | `xmtp_common::stream_handles::spawn(None, fut)`; keep the `StreamHandle` |
| Bare `#[tracing::instrument]` on an API/DB/MLS/FFI fn | `#[xmtp_common::rpc_span]` / `db_span` / `mls_span` / `err_span` |
| Logging a full id, key, or payload | `debug_hex`, `Snippet::snippet`, `ShortHex::short_hex`, and `skip_all` |
| A raw `conn.transaction(..)` | `XmtpMlsStorageProvider::transaction(\|tx\| .. )` → `Result<TransactionOutcome<T>, E>` |
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
