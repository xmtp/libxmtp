use crate::StorageError;
#[cfg(feature = "sqlite")]
use diesel::prelude::*;

#[cfg(feature = "sqlite")]
use super::{ConnectionExt, db_connection::DbConnection, schema::openmls_key_store};
#[cfg(feature = "sqlite")]
use crate::{Delete, impl_fetch, impl_store};

#[derive(Debug, Clone)]
#[cfg_attr(feature = "sqlite", derive(Insertable, Queryable))]
#[cfg_attr(feature = "sqlite", diesel(table_name = openmls_key_store))]
#[cfg_attr(feature = "sqlite", diesel(primary_key(key_bytes)))]
pub struct StoredKeyStoreEntry {
    pub key_bytes: Vec<u8>,
    pub value_bytes: Vec<u8>,
}

#[cfg(feature = "sqlite")]
impl_fetch!(StoredKeyStoreEntry, openmls_key_store, Vec<u8>);
#[cfg(feature = "sqlite")]
impl_store!(StoredKeyStoreEntry, openmls_key_store);

#[cfg(feature = "sqlite")]
impl<C: ConnectionExt> Delete<StoredKeyStoreEntry> for DbConnection<C> {
    type Key = Vec<u8>;
    fn delete(&self, key: Vec<u8>) -> Result<usize, StorageError> where {
        use super::schema::openmls_key_store::dsl::*;
        Ok(self.raw_query(|conn| {
            diesel::delete(openmls_key_store.filter(key_bytes.eq(key))).execute(conn)
        })?)
    }
}

/// Query traits carry the `maybe_async` attribute so one definition serves both
/// storage tracks: `sync` collapses it to the blocking diesel shape (the only
/// option on wasm), `async` keeps it awaitable for the sqlx backend.
pub trait QueryKeyStoreEntry {
    fn insert_or_update_key_store_entry(
        &self,
        key: Vec<u8>,
        value: Vec<u8>,
    ) -> impl std::future::Future<Output = Result<(), StorageError>> + xmtp_common::MaybeSend;
}

// No `Sync` bound, matching the other 17 `&T` forwarding impls. This trait was
// the first one converted to maybe_async and kept a `Sync` left over from that
// spike; on wasm the connection is an `Rc<RefCell<_>>`, so `DbConnection<C>` is
// never `Sync` and the bound made `&DbConnection<C>: DbQuery` unsatisfiable --
// breaking the wasm build at `cleanup_duplicate_updates.rs`.
impl<T> QueryKeyStoreEntry for &T
where
    T: QueryKeyStoreEntry + xmtp_common::MaybeSync,
{
    async fn insert_or_update_key_store_entry(
        &self,
        key: Vec<u8>,
        value: Vec<u8>,
    ) -> Result<(), StorageError> {
        (**self).insert_or_update_key_store_entry(key, value).await
    }
}

/// Diesel backend. `raw_query` hands out a blocking connection, so this impl
/// only exists on the SQLite backend; nothing in it ever awaits.
#[cfg(feature = "sqlite")]
impl<C: ConnectionExt> QueryKeyStoreEntry for DbConnection<C> {
    async fn insert_or_update_key_store_entry(
        &self,
        key: Vec<u8>,
        value: Vec<u8>,
    ) -> Result<(), StorageError> {
        use super::schema::openmls_key_store::dsl::*;
        let entry = StoredKeyStoreEntry {
            key_bytes: key,
            value_bytes: value,
        };

        self.raw_query(|conn| {
            diesel::replace_into(openmls_key_store)
                .values(entry)
                .execute(conn)
        })?;
        Ok(())
    }
}

/// sqlx backend — Postgres only (see the `sqlx` dependency note in Cargo.toml:
/// SQLite cannot be reached through sqlx in this workspace).
// `not(feature = "sqlite")`: the two backends are single-choice but not hard-exclusive
// -- cargo feature unification can hand a graph both (`--all-features` does), and
// `sqlite` is dominant, so when both are on the SQLite backend is selected and this
// impl is compiled out.
#[cfg(all(feature = "sqlx", not(feature = "sqlite"), not(target_arch = "wasm32")))]
impl QueryKeyStoreEntry for crate::pg::PgDb {
    /// Postgres has no `REPLACE INTO`; `ON CONFLICT DO UPDATE` on the primary
    /// key is the equivalent upsert.
    async fn insert_or_update_key_store_entry(
        &self,
        key: Vec<u8>,
        value: Vec<u8>,
    ) -> Result<(), StorageError> {
        let mut c = self.conn().await?;
        sqlx::query(
            "INSERT INTO openmls_key_store (key_bytes, value_bytes) VALUES ($1, $2) \
             ON CONFLICT (key_bytes) DO UPDATE SET value_bytes = excluded.value_bytes",
        )
        .bind(key)
        .bind(value)
        .execute(&mut *c)
        .await
        .map_err(crate::ConnectionError::from)?;
        Ok(())
    }
}
