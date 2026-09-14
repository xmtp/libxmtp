# Database

## Client: `xmtp_db`

Diesel over encrypted SQLite. No `sqlx`, no Postgres in a client crate or below
`xmtp_mls`. Model traits, migrations, and errors: `crates/xmtp_db/AGENTS.md`.

### Transactions

Transactions live on the storage provider, not the connection. The closure and
the method both return `Result<TransactionOutcome<T>, E>`.

```rust
// crates/xmtp_mls/src/groups/intents/queue.rs
use xmtp_db::{TransactionOutcome, TransactionOutcome::Continue, prelude::*};

crate::state_tx::state_write(context.mls_storage(), |tx| {
    let db = tx.storage().db();
    self.queue_in(&db, group).map(Continue)      // Ok(Continue(v)) commits
})                                                // Ok(Rollback) rolls back, no error
.map(TransactionOutcome::into_continued)          // Err(e) rolls back and propagates
```

In `xmtp_mls`, `state_tx::state_write` is the usual entry; it wraps
`XmtpMlsStorageProvider::transaction`. `into_continued` panics on `Rollback`,
so match the outcome when a closure can roll back. Never call
`conn.transaction(..)`.

### Tables

A standard table gets the macros, not hand-written trait impls:

```rust
// crates/xmtp_db/src/encrypted_store/group.rs
impl_fetch!(StoredGroup, groups, GroupId);
impl_store!(StoredGroup, groups);             // insert; errors on conflict
impl_store_or_ignore!(StoredGroup, groups);   // insert or ignore
impl_fetch_list!(StoredGroup, groups);
```

Query methods group into a per-table `Query*` trait implemented for
`DbConnection<C: ConnectionExt>`, with a forwarding impl for `&T`
(`crates/xmtp_db/src/encrypted_store/group/version.rs`). A new table needs a
new `Query*` trait, a `pub use` in the `prelude`, and a line in the `DbQuery`
supertrait list in `src/traits.rs`. Callers take `impl DbQuery`.

Missing rows use the one `NotFound` enum: `.ok_or(NotFound::GroupById(id))?`.
Add a variant there, not a new error type.

### Migrations

`dev/nix-shell 'diesel migration generate <name>'` inside `crates/xmtp_db`,
then `dev/nix-shell 'cargo update-schema'`. `schema_gen.rs` is generated;
`schema.rs` is hand-written. Never edit `schema_gen.rs`.

## Backend: `apps/backend`

SQLx over PostgreSQL, per spec 002. Client conventions do not apply.

- Helpers on `Store` take and return the records in
  `apps/backend/src/db/model.rs`, never protobuf messages or `tonic::Status`.
  Stored payload bytes are opaque.
- Every helper carries `#[xmtp_common::db_span]`.
- Errors stay typed (`apps/backend/src/error.rs`). One `From<Error> for Status`
  in `apps/backend/src/service/error.rs` maps them to transport codes.
- Validate and normalize request collections before the database call. The API
  layer owns protobuf encoding and decoding.

```rust
// apps/backend/src/db/identity.rs
#[xmtp_common::db_span]
pub(crate) async fn history(&self, topic: &[u8]) -> Result<History, Error> { .. }
```

Tests need the database: `just backend db-up`, then `just backend test`. After
changing a query, `just backend sql-prepare` refreshes the checked SQLx
metadata; `just backend sql-check` verifies it.
