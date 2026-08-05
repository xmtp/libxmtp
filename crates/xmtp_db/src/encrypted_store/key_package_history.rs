#[cfg(feature = "sync")]
use super::{ConnectionExt, db_connection::DbConnection, schema::key_package_history};
use crate::StorageError;
#[cfg(feature = "sync")]
use crate::{StoreOrIgnore, impl_store_or_ignore};
#[cfg(feature = "sync")]
use diesel::prelude::*;
use xmtp_common::time::now_ns;
use xmtp_configuration::KEYS_EXPIRATION_INTERVAL_NS;

#[derive(Debug, Clone)]
#[cfg_attr(feature = "sync", derive(Insertable))]
#[cfg_attr(feature = "sync", diesel(table_name = key_package_history))]
pub struct NewKeyPackageHistoryEntry {
    pub key_package_hash_ref: Vec<u8>,
    pub post_quantum_public_key: Option<Vec<u8>>,
    pub created_at_ns: i64,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "sync", derive(Queryable, Selectable))]
#[cfg_attr(feature = "sync", diesel(table_name = key_package_history))]
#[derive(xmtp_macro::PgModel)]
#[xmtp(table = "key_package_history")]
pub struct StoredKeyPackageHistoryEntry {
    pub id: i32,
    pub key_package_hash_ref: Vec<u8>,
    pub created_at_ns: i64,
    pub delete_at_ns: Option<i64>,
    pub post_quantum_public_key: Option<Vec<u8>>,
}

#[cfg(feature = "sync")]
impl_store_or_ignore!(NewKeyPackageHistoryEntry, key_package_history);

pub trait QueryKeyPackageHistory {
    fn store_key_package_history_entry(
        &self,
        key_package_hash_ref: Vec<u8>,
        post_quantum_public_key: Option<Vec<u8>>,
    ) -> impl std::future::Future<Output = Result<StoredKeyPackageHistoryEntry, StorageError>>
    + xmtp_common::MaybeSend;

    fn find_key_package_history_entry_by_hash_ref(
        &self,
        hash_ref: Vec<u8>,
    ) -> impl std::future::Future<Output = Result<StoredKeyPackageHistoryEntry, StorageError>>
    + xmtp_common::MaybeSend;

    fn find_key_package_history_entries_before_id(
        &self,
        id: i32,
    ) -> impl std::future::Future<Output = Result<Vec<StoredKeyPackageHistoryEntry>, StorageError>>
    + xmtp_common::MaybeSend;

    fn mark_key_package_before_id_to_be_deleted(
        &self,
        id: i32,
    ) -> impl std::future::Future<Output = Result<(), StorageError>> + xmtp_common::MaybeSend;

    fn get_expired_key_packages(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<StoredKeyPackageHistoryEntry>, StorageError>>
    + xmtp_common::MaybeSend;

    /// Soonest pending `delete_at_ns` across all key packages marked for deletion,
    /// or `None` if none are marked. The KpDeletion task's reschedule source.
    fn min_key_package_delete_at_ns(
        &self,
    ) -> impl std::future::Future<Output = Result<Option<i64>, StorageError>> + xmtp_common::MaybeSend;

    fn delete_key_package_history_up_to_id(
        &self,
        id: i32,
    ) -> impl std::future::Future<Output = Result<(), StorageError>> + xmtp_common::MaybeSend;

    fn delete_key_package_entry_with_id(
        &self,
        id: i32,
    ) -> impl std::future::Future<Output = Result<(), StorageError>> + xmtp_common::MaybeSend;
}

impl<T> QueryKeyPackageHistory for &T
where
    T: QueryKeyPackageHistory + xmtp_common::MaybeSync,
{
    async fn store_key_package_history_entry(
        &self,
        key_package_hash_ref: Vec<u8>,
        post_quantum_public_key: Option<Vec<u8>>,
    ) -> Result<StoredKeyPackageHistoryEntry, StorageError> {
        (**self)
            .store_key_package_history_entry(key_package_hash_ref, post_quantum_public_key)
            .await
    }

    async fn find_key_package_history_entry_by_hash_ref(
        &self,
        hash_ref: Vec<u8>,
    ) -> Result<StoredKeyPackageHistoryEntry, StorageError> {
        (**self)
            .find_key_package_history_entry_by_hash_ref(hash_ref)
            .await
    }

    async fn find_key_package_history_entries_before_id(
        &self,
        id: i32,
    ) -> Result<Vec<StoredKeyPackageHistoryEntry>, StorageError> {
        (**self)
            .find_key_package_history_entries_before_id(id)
            .await
    }

    async fn mark_key_package_before_id_to_be_deleted(&self, id: i32) -> Result<(), StorageError> {
        (**self).mark_key_package_before_id_to_be_deleted(id).await
    }

    async fn get_expired_key_packages(
        &self,
    ) -> Result<Vec<StoredKeyPackageHistoryEntry>, StorageError> {
        (**self).get_expired_key_packages().await
    }

    async fn min_key_package_delete_at_ns(&self) -> Result<Option<i64>, StorageError> {
        (**self).min_key_package_delete_at_ns().await
    }

    async fn delete_key_package_history_up_to_id(&self, id: i32) -> Result<(), StorageError> {
        (**self).delete_key_package_history_up_to_id(id).await
    }

    async fn delete_key_package_entry_with_id(&self, id: i32) -> Result<(), StorageError> {
        (**self).delete_key_package_entry_with_id(id).await
    }
}

#[cfg(feature = "sync")]
impl<C: ConnectionExt> QueryKeyPackageHistory for DbConnection<C> {
    async fn store_key_package_history_entry(
        &self,
        key_package_hash_ref: Vec<u8>,
        post_quantum_public_key: Option<Vec<u8>>,
    ) -> Result<StoredKeyPackageHistoryEntry, StorageError> {
        let entry = NewKeyPackageHistoryEntry {
            key_package_hash_ref: key_package_hash_ref.clone(),
            post_quantum_public_key: post_quantum_public_key.clone(),
            created_at_ns: now_ns(),
        };
        entry.store_or_ignore(self)?;

        self.find_key_package_history_entry_by_hash_ref(key_package_hash_ref)
            .await
    }

    async fn find_key_package_history_entry_by_hash_ref(
        &self,
        hash_ref: Vec<u8>,
    ) -> Result<StoredKeyPackageHistoryEntry, StorageError> {
        let result = self.raw_query(|conn| {
            key_package_history::dsl::key_package_history
                .filter(key_package_history::dsl::key_package_hash_ref.eq(hash_ref))
                .first::<StoredKeyPackageHistoryEntry>(conn)
        })?;

        Ok(result)
    }

    async fn find_key_package_history_entries_before_id(
        &self,
        id: i32,
    ) -> Result<Vec<StoredKeyPackageHistoryEntry>, StorageError> {
        let result = self.raw_query(|conn| {
            key_package_history::dsl::key_package_history
                .filter(key_package_history::dsl::id.lt(id))
                .load::<StoredKeyPackageHistoryEntry>(conn)
        })?;

        Ok(result)
    }

    async fn mark_key_package_before_id_to_be_deleted(&self, id: i32) -> Result<(), StorageError> {
        use crate::schema::key_package_history::dsl;
        let delete_at_24_hrs_ns = now_ns() + KEYS_EXPIRATION_INTERVAL_NS;
        self.raw_query(|conn| {
            diesel::update(
                dsl::key_package_history
                    .filter(dsl::id.lt(id))
                    .filter(dsl::delete_at_ns.is_null()), // Only set if not already set
            )
            .set(dsl::delete_at_ns.eq(delete_at_24_hrs_ns))
            .execute(conn)
        })?;

        Ok(())
    }

    async fn get_expired_key_packages(
        &self,
    ) -> Result<Vec<StoredKeyPackageHistoryEntry>, StorageError> {
        use crate::schema::key_package_history::dsl;
        self.raw_query(|conn| {
            dsl::key_package_history
                .filter(dsl::delete_at_ns.le(now_ns()))
                .load::<StoredKeyPackageHistoryEntry>(conn)
        })
        .map_err(StorageError::from) // convert ConnectionError into StorageError
    }

    async fn min_key_package_delete_at_ns(&self) -> Result<Option<i64>, StorageError> {
        use crate::schema::key_package_history::dsl;
        use diesel::dsl::min;
        let v: Option<i64> = self.raw_query(|conn| {
            dsl::key_package_history
                .filter(dsl::delete_at_ns.is_not_null())
                .select(min(dsl::delete_at_ns))
                .first::<Option<i64>>(conn)
        })?;
        Ok(v)
    }

    async fn delete_key_package_history_up_to_id(&self, id: i32) -> Result<(), StorageError> {
        self.raw_query(|conn| {
            diesel::delete(
                key_package_history::dsl::key_package_history
                    .filter(key_package_history::dsl::id.le(id)),
            )
            .execute(conn)
        })?;

        Ok(())
    }

    async fn delete_key_package_entry_with_id(&self, id: i32) -> Result<(), StorageError> {
        self.raw_query(|conn| {
            diesel::delete(
                key_package_history::dsl::key_package_history
                    .filter(key_package_history::dsl::id.eq(id)),
            )
            .execute(conn)
        })?;

        Ok(())
    }
}

/// sqlx backend -- Postgres only. See the note on `QueryGroupVersion`'s impl for
/// why this is gated `not(feature = "sync")`.
#[cfg(all(feature = "async", not(feature = "sync"), not(target_arch = "wasm32")))]
mod pg_impl {
    use super::*;
    use crate::pg::{PgDb, PgModel};
    use sqlx::Row;

    /// Decode via the `FromRow` that `#[derive(PgModel)]` emits: by column
    /// name, from the same fields the column list comes from.
    fn entry(row: &sqlx::postgres::PgRow) -> Result<StoredKeyPackageHistoryEntry, StorageError> {
        use sqlx::FromRow;
        Ok(StoredKeyPackageHistoryEntry::from_row(row).map_err(crate::ConnectionError::from)?)
    }

    impl QueryKeyPackageHistory for PgDb {
        async fn store_key_package_history_entry(
            &self,
            key_package_hash_ref: Vec<u8>,
            post_quantum_public_key: Option<Vec<u8>>,
        ) -> Result<StoredKeyPackageHistoryEntry, StorageError> {
            // Insert and read-back are two statements on two pooled connections
            // unless they share a transaction; without one, a concurrent delete
            // between them turns this into a spurious "not found".
            self.atomic(async |db| {
                {
                    let mut c = db.conn().await?;
                    sqlx::query(
                        "INSERT INTO key_package_history \
                         (key_package_hash_ref, post_quantum_public_key, created_at_ns) \
                         VALUES ($1, $2, $3) ON CONFLICT (key_package_hash_ref) DO NOTHING",
                    )
                    .bind(&key_package_hash_ref)
                    .bind(&post_quantum_public_key)
                    .bind(now_ns())
                    .execute(&mut *c)
                    .await
                    .map_err(crate::ConnectionError::from)?;
                }
                // Read back rather than using RETURNING: on a conflict the insert
                // yields no row, and the sync track's contract is to return the
                // *existing* entry in that case.
                db.find_key_package_history_entry_by_hash_ref(key_package_hash_ref)
                    .await
            })
            .await
        }

        /// A missing entry is an error, matching the diesel impl's `first()`.
        async fn find_key_package_history_entry_by_hash_ref(
            &self,
            hash_ref: Vec<u8>,
        ) -> Result<StoredKeyPackageHistoryEntry, StorageError> {
            let mut c = self.conn().await?;
            let row = sqlx::query(&format!(
                "SELECT {} FROM key_package_history WHERE key_package_hash_ref = $1",
                StoredKeyPackageHistoryEntry::select_columns()
            ))
            .bind(hash_ref)
            .fetch_one(&mut *c)
            .await
            .map_err(crate::ConnectionError::from)?;
            entry(&row)
        }

        async fn find_key_package_history_entries_before_id(
            &self,
            id: i32,
        ) -> Result<Vec<StoredKeyPackageHistoryEntry>, StorageError> {
            let mut c = self.conn().await?;
            let rows = sqlx::query(&format!(
                "SELECT {} FROM key_package_history WHERE id < $1",
                StoredKeyPackageHistoryEntry::select_columns()
            ))
            .bind(id)
            .fetch_all(&mut *c)
            .await
            .map_err(crate::ConnectionError::from)?;
            rows.iter().map(entry).collect()
        }

        async fn mark_key_package_before_id_to_be_deleted(
            &self,
            id: i32,
        ) -> Result<(), StorageError> {
            let mut c = self.conn().await?;
            // `delete_at_ns IS NULL` keeps this idempotent: an entry already
            // marked keeps its original deadline instead of being pushed out.
            sqlx::query(
                "UPDATE key_package_history SET delete_at_ns = $1 \
                 WHERE id < $2 AND delete_at_ns IS NULL",
            )
            .bind(now_ns() + KEYS_EXPIRATION_INTERVAL_NS)
            .bind(id)
            .execute(&mut *c)
            .await
            .map_err(crate::ConnectionError::from)?;
            Ok(())
        }

        async fn get_expired_key_packages(
            &self,
        ) -> Result<Vec<StoredKeyPackageHistoryEntry>, StorageError> {
            let mut c = self.conn().await?;
            let rows = sqlx::query(&format!(
                "SELECT {} FROM key_package_history WHERE delete_at_ns <= $1",
                StoredKeyPackageHistoryEntry::select_columns()
            ))
            .bind(now_ns())
            .fetch_all(&mut *c)
            .await
            .map_err(crate::ConnectionError::from)?;
            rows.iter().map(entry).collect()
        }

        async fn min_key_package_delete_at_ns(&self) -> Result<Option<i64>, StorageError> {
            let mut c = self.conn().await?;
            // An aggregate with no GROUP BY always returns exactly one row, so
            // `fetch_one` is right even when nothing is marked — the value is
            // then NULL, which is the `None` callers expect.
            let row = sqlx::query(
                "SELECT MIN(delete_at_ns) FROM key_package_history WHERE delete_at_ns IS NOT NULL",
            )
            .fetch_one(&mut *c)
            .await
            .map_err(crate::ConnectionError::from)?;
            Ok(row.try_get(0).map_err(crate::ConnectionError::from)?)
        }

        async fn delete_key_package_history_up_to_id(&self, id: i32) -> Result<(), StorageError> {
            let mut c = self.conn().await?;
            sqlx::query("DELETE FROM key_package_history WHERE id <= $1")
                .bind(id)
                .execute(&mut *c)
                .await
                .map_err(crate::ConnectionError::from)?;
            Ok(())
        }

        async fn delete_key_package_entry_with_id(&self, id: i32) -> Result<(), StorageError> {
            let mut c = self.conn().await?;
            sqlx::query("DELETE FROM key_package_history WHERE id = $1")
                .bind(id)
                .execute(&mut *c)
                .await
                .map_err(crate::ConnectionError::from)?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::prelude::*;
    use crate::test_utils::with_connection;
    use xmtp_common::rand_vec;

    #[xmtp_common::test]
    fn min_key_package_delete_at_ns_none_when_empty() {
        with_connection(|conn| {
            // Aggregate MIN over an empty/unmarked table is NULL -> None.
            assert_eq!(conn.min_key_package_delete_at_ns().unwrap(), None);
        })
    }

    #[xmtp_common::test]
    fn test_store_key_package_history_entry() {
        with_connection(|conn| {
            let hash_ref = rand_vec::<24>();
            let post_quantum_public_key = rand_vec::<32>();
            let new_entry = conn
                .store_key_package_history_entry(
                    hash_ref.clone(),
                    Some(post_quantum_public_key.clone()),
                )
                .unwrap();
            assert_eq!(new_entry.key_package_hash_ref, hash_ref);
            assert_eq!(
                new_entry.post_quantum_public_key,
                Some(post_quantum_public_key)
            );
            assert_eq!(new_entry.id, 1);

            // Now delete it
            conn.delete_key_package_entry_with_id(1).unwrap();
            let all_entries = conn
                .find_key_package_history_entries_before_id(100)
                .unwrap();
            assert!(all_entries.is_empty());
        })
    }

    #[xmtp_common::test]
    fn test_store_multiple() {
        with_connection(|conn| {
            let post_quantum_public_key = rand_vec::<32>();
            let hash_ref1 = rand_vec::<24>();
            let hash_ref2 = rand_vec::<24>();
            let hash_ref3 = rand_vec::<24>();

            conn.store_key_package_history_entry(
                hash_ref1.clone(),
                Some(post_quantum_public_key.clone()),
            )
            .unwrap();
            conn.store_key_package_history_entry(
                hash_ref2.clone(),
                Some(post_quantum_public_key.clone()),
            )
            .unwrap();
            let entry_3 = conn
                .store_key_package_history_entry(hash_ref3.clone(), None)
                .unwrap();

            let all_entries = conn
                .find_key_package_history_entries_before_id(100)
                .unwrap();

            assert_eq!(all_entries.len(), 3);

            let earlier_entries = conn
                .find_key_package_history_entries_before_id(entry_3.id)
                .unwrap();
            assert_eq!(earlier_entries.len(), 2);
        })
    }
}
