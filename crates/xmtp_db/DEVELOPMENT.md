# Client storage development

Run commands from the root through `dev/nix-shell`. Model macros, query traits,
transaction examples, and errors are described in the
[database reference](../../.agents/skills/writing-rust/database.md).
Use `Store` for conflict errors and `StoreOrIgnore` for unique-conflict no-ops;
query traits live on connections. `IntoConnection` is infallible. Missing items
use `NotFound`; preserve typed errors with `#[from]`.

## Migrations

Existing migrations never change. Generate a new directory with Diesel, then
run `dev/nix-shell 'cargo update-schema'`. If models cannot yet compile, apply
all migrations to an empty SQLite file and run:

```sh
dev/nix-shell 'diesel print-schema --database-url <file> -e client_events > crates/xmtp_db/src/encrypted_store/schema_gen.rs'
```

The new migration must sort last by directory name and digits-only version
string. Diesel uses version-string order; `final_migration` uses directory
order. Different-length versions must not be compared numerically. For example,
`2026-09-28-000000_x` and `2026-09-28-000000-0000_y` sort oppositely by these rules.

`XmtpDb::init` accepts embedded versions as applied; a new migration needs no
init change. Cover baseline upgrades in
`upgrades_a_baseline_database_to_the_latest_migration`. Pre-transition formats
and formats without durable progress or `server_configuration` are rejected,
never deleted. Keep a backup before replacing an incompatible client database.

Before any migration, init also rejects an already-migrated database that lacks
`2026-10-20-000000_cbor_storage_format` and holds an `identity` or
`openmls_key_value` row (`StorageError::OldEncodingDatabase`). Those databases
contain bincode values. Without those rows, an interrupted first open may finish
its migrations. See [storage-format tests](src/encrypted_store/migrations/storage_format_tests.rs).

Init also rejects a database holding a 16-byte group id whose membership state
is not `Restored`, before any migration (`StorageError::LegacyGroupDatabase`).
A restored archive placeholder may open, but its history stays read-only: it
cannot become a current 32-byte group. See [legacy-group tests](src/encrypted_store/migrations/legacy_group_tests.rs)
and [archive limitations](../../docs/specs/ARCH-archive-format.md#known-limitations).

## Storage encoding and secrets

Read [the codec contract](src/sql_key_store/codec.rs) before changing stored
types. All serde key parts, values, and lists go through its `encode_key`,
`encode_value`, `decode_value`, `encode_list`, and `decode_list` helpers, never
a direct serde codec. Values use CBOR; keys retain the bincode 1.3 layout in
`codec/key.rs`. `db_serialize` uses the same value codec.

CBOR stores field and variant names: never rename them; new fields need serde
defaults. Byte fields of libxmtp-owned stored types use
`#[serde(with = "xmtp_db::byte_string")]` (or `byte_strings` /
`optional_byte_strings`); plain bytes under XMTP labels use `encode_bytes`.
OpenMLS types retain their own serde shapes. `XmtpInstallationCredential`
deliberately uses `SignatureKeyPair`'s integer-array keys so each reads the
other's rows; the golden column pins `.private[]:integer`.

`KEY_PACKAGE_REFERENCES` keys are raw TLS-encoded HPKE public keys (init or PQ
wrapper). Never pass them through `encode_key`: its length prefix hides existing
rows from Welcome lookup.

[Golden tests](src/sql_key_store/codec/golden_tests.rs) guard stored shapes.
Never edit frozen `codec/golden_rows_<release>.json` files. To add a new golden,
run `dev/nix-shell 'just test workspace -p xmtp_db write_golden_rows --run-ignored only'`
on a native host, then follow that file's instructions for `golden_rows_next.json`.

`SqlKeyStore` wipes every value buffer, regardless of label; `encode_value`
returns a wiping buffer. Spans may record storage keys only, never values.
Group contexts contain the secret group root and, for older groups, a private
commit-log signer. Wrapper keys go through
`write_wrapper_private_key` / `wrapper_private_key`. Secret columns use
`StoredSecret`: its BLOB matches `Vec<u8>`, but serde writes a CBOR byte string.
It wipes on drop and redacts Debug; containing rows keep their `Insertable`
derives because they need no custom Drop. Follow the shared
[secret rules](../../.agents/skills/writing-rust/types.md#secrets).

## Transactions

Transactions/savepoints belong to `XmtpMlsStorageProvider`. Handle
`TransactionOutcome`: Continue commits, Rollback rolls back without an error,
and Err rolls back and propagates. Use `TransactionalKeyStore::key_store` inside
transactions. `immediate_transaction` preserves SQLite's `BUSY_TIMEOUT` behavior.

## Durable streams and browser lifecycle

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
