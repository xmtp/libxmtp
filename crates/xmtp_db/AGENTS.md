# xmtp_db

Storage. Diesel over encrypted SQLite (SQLCipher).

## Commands

```bash
just check crate xmtp_db
dev/nix-shell 'dev/agent-run cargo bench -p xmtp_db --features bench --bench conversation_list'
# Set XMTP_BENCH_CONVERSATIONS=50000 for a larger data set.
just test crate xmtp_db
just test workspace -p xmtp_db --ignore-default-filter test_it_stores_group   # one test
just test workspace -p xmtp_db encrypted_store::group::   # one module
dev/nix-shell 'cargo update-schema'      # regenerate schema_gen.rs after a migration
```

## Gotchas

- Keep existing migrations unchanged. Add a new migration directory for each schema change. Pre-transition databases are rejected before migrations run.
- Initialization also rejects older self-hosted formats without durable stream progress or the `server_configuration` table. Keep a backup and create a new client database for those formats. Initialization never deletes old data.
- `XmtpDb::init()` accepts any embedded version as applied and rejects every other version as pre-transition, so a baseline database upgrades in place. A new migration needs no init change; cover the upgrade in `upgrades_a_baseline_database_to_the_latest_migration`.
- A new migration must sort after every existing one both by directory name and by its digits-only version string. Diesel applies pending migrations in version string order, but `final_migration` uses directory order; if they disagree, every open fails with `InvalidVersion`. Versions have different lengths, so compare them as strings, as Diesel does, never as numbers (`QueryMigrations::rollback_to_version` follows this rule). Example: `2026-09-28-000000_x` sorts before `2026-09-28-000000-0000_y` by version but after it by directory name.
- Regenerate `schema_gen.rs` with `cargo update-schema` through Nix. To generate before the models compile, apply all migrations to an empty SQLite file, then run `dev/nix-shell 'diesel print-schema --database-url <file> -e client_events > crates/xmtp_db/src/encrypted_store/schema_gen.rs'`.

## Conventions

Client persistence only. The backend picks its own database layer in spec 002.

- Rust client data paths are Diesel + encrypted SQLite. No `sqlx`, no `PgConnection`, no `diesel::pg` in this crate or below `xmtp_mls`. Root `Cargo.toml` pins `diesel = { version = "2.3", default-features = false }` on a SQLite-focused fork.
- Model traits (`src/traits.rs:22-64`). Which side implements each:
  - Model: `Store<C>` (`store(&self, into: &C) -> Result<Self::Output, StorageError>`, errors on conflict) and `StoreOrIgnore<C>` (silent no-op on a unique-constraint violation).
  - Connection or query type: `Fetch<Model>` (`fetch(&self, key: &Self::Key) -> Result<Option<Model>, _>`), `FetchList<Model>`, `FetchListWithKey<Model>` (takes `&[Self::Key]`), `Delete<Model>` (`delete(&self, key: Self::Key) -> Result<usize, _>`, key by value, returns the row count).
  - `IntoConnection` is plain: `into_connection(self) -> Self::Connection`, no `Result`.
- Standard table: use the macros in `src/encrypted_store/mod.rs`, not hand-written impls. `impl_fetch!(Model, table[, Key])` (`:376`), `impl_fetch_list!` (`:423`), `impl_store!` (`:440`), `impl_store_or_ignore!` (`:462`). Example: `src/encrypted_store/group.rs:150`.
- Query methods group into per-table `Query*` traits (`QueryGroup`, `QueryConsentRecord`, ...), aggregated by `src/traits.rs:DbQuery`. A new table needs a new `Query*` trait, a `pub use` in the `src/lib.rs` `prelude`, and a line in the `DbQuery` supertrait list. Callers take `impl DbQuery`.
- Connections: `encrypted_store/mod.rs:207 ConnectionExt` (`raw_query`, `disconnect`, `reconnect`) is the low-level contract, blanket-implemented for `&C`, `&mut C`, `Arc<C>`, and `encrypted_store/db_connection.rs:DbConnection<C>`. `XmtpDb` ties a `Connection` to a `DbQuery`. Converting a `DbConnection<C>` into `XmtpOpenMlsProvider<SqlKeyStore<C>>` consumes the wrapper and moves the inner `C` out (`db_connection.rs:54`).
- Transactions are on the storage provider, not the connection. There is no `DbConnection::transaction`. Use `src/xmtp_openmls_provider.rs:69 XmtpMlsStorageProvider::transaction` (`savepoint:81` when nested): `fn transaction<T, E, F>(&self, f: F) -> Result<TransactionOutcome<T>, E>` where `F: FnOnce(&mut Self::TxQuery) -> Result<TransactionOutcome<T>, E>`. `Ok(Continue(v))` commits, `Ok(Rollback)` rolls back with no error, `Err(e)` rolls back and propagates. The value is not unwrapped for you: match the outcome or call `into_continued` (`:34`). Implementation: `src/sql_key_store/transactions.rs:89` (`immediate_transaction`, so SQLite honours `BUSY_TIMEOUT`). Inside a transaction, reach the MLS key store via `src/traits.rs:TransactionalKeyStore::key_store`.
- Migrations: `migrations/<YYYY-MM-DD-HHMMSS>[-NNNN]_<snake_description>/{up.sql,down.sql}` (recent ones add the `-0000` suffix). Generate with `diesel migration generate <name>`, then the `update-schema` command above (`diesel.toml`). Embedded at `encrypted_store/mod.rs:66 MIGRATIONS`; runtime control via `encrypted_store/migrations.rs:QueryMigrations`.
- `encrypted_store/schema_gen.rs` is generated by Diesel CLI. Never hand-edit. `encrypted_store/schema.rs` is hand-written and re-exports `schema_gen::*`. The old `conversation_list` view is retired; the query builds its own CTE.
- Errors (`src/errors.rs`): `StorageError:14`, `NotFound:142` (the one enum for every "lost item" case), `DuplicateItem:230`, plus `ConnectionError` in `encrypted_store/mod.rs`. All derive `ErrorCode` and hand-implement `RetryableError`. Add a `NotFound` variant, not a bespoke enum. Wrap with `#[from]`, never strings.

## Durable streams

- `QueryIncomingEnvelope` commits ordered envelopes and received progress together. Adapters validate wire metadata and the supplied hash shape first. The client never recomputes the backend envelope hash. Use `install_group_anchor` only inside the transaction that installs a validated welcome.
- `record_welcome_discovery` records the first successful Welcome in that installation transaction. Rejoin never changes it. Do not record local creation or history import. Fixed catch-up targets use `group_ids_discovered_through`.
- Record terminal rejection codes before deleting pending work in the same state transaction. Only the last rejection per topic is retained. No message payload enters this diagnostic.
- `QueryReceivedProposal` is the durable evidence that the ordered prefix delivered a proposal. Record it in the state transaction that stores the proposal in the MLS proposal store. Forget it in the transaction that removes the proposal for a protocol reason: an eviction, a merged commit, or a replacing Welcome. A commit naming a proposal missing from the store is rejected only when this evidence shows the prefix never delivered it.
- Complete a pending group or identity head in the same state transaction as its MLS or identity writes. Welcome completion is independent and checks unresolved rows through a fixed target.
- Published message inserts and publication transitions allocate immutable local delivery numbers. Unpublished optimistic rows have no number. Message-history imports use the normal message store API.
- `QueryDelivery` fences both acknowledgements and filtered scans with one default owner token. Call `check_delivery_owner` before each callback or iterator handoff. A candidate batch is not an acknowledgement.
- Whole-database restore must have exclusive lifecycle access, fence open handles, and call `rotate_stream_database_id` before accepting readers or cursors. Opening an existing database does not rotate its identity.
- `WasmDb::new` keeps the legacy browser behavior: if this worker cannot install or resume the OPFS pool, it logs the error and opens the path on SQLite's default VFS. That legacy open and the legacy `database::init_sqlite` do not fence the pool, so a later call in the same worker can install once another owner releases it. The new SDK uses `WasmDb::new_strict`, which returns the typed pool error. The strict rules below apply to `new_strict` and `WasmDbConnection::new`.
- Strict persistent OPFS connections require a plain path; `file:` and `sqlite://` URI forms return `InvalidDatabasePath` before initialization. Only one live connection can use a path. A second open or reconnect returns `DatabaseInUse` until that connection closes. Clones share one connection; different paths can remain open together.
- Browser utility reads use `list_opfs_databases`, `opfs_database_count`, `opfs_pool_capacity`, and `opfs_database_exists`. Export uses `export_opfs_database` and requires a closed target. All use the shared OPFS admission and transition guards. Do not add callers of the raw `get_sqlite` utility.
- If `opfs_requires_worker_restart()` is true, terminate the worker while its OPFS Web Lock is still held. A failed or cancelled VFS transition can leave partial access handles. Do not retry in that worker or clear only its initialization promise.
- Browser whole-database restore uses `import_opfs_database`. It validates and rotates an in-memory copy before creating an absent destination. It does not overwrite an existing file. OPFS delete/clear helpers require closed persistent handles and fence old objects against reconnect. Copying database files outside these lifecycle APIs is unsupported.
- `QueryPreparedEnvelope` stores exact prepared bytes in existing intents. A late reply must compare its original prepared bytes before it updates the attempt.
- Key retirement follows confirmed backend publication order through `record_key_package_publication`, not local key creation order.

- Legacy browser migration uses OpfsWorkingCopy under the shared lifecycle guard.
  It reads source VFS bytes in 64 KiB chunks and opens SQLite only on a private
  OPFS copy. The private VFS is cleared on initialization and after conversion.
