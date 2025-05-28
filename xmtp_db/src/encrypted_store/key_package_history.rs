use super::{
    ConnectionExt, StorageError, db_connection::DbConnection, schema::key_package_history,
};
use crate::{StoreOrIgnore, impl_store_or_ignore};
use diesel::prelude::*;
use xmtp_common::NS_IN_DAY;
use xmtp_common::time::now_ns;

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = key_package_history)]
pub struct NewKeyPackageHistoryEntry {
    pub key_package_hash_ref: Vec<u8>,
    pub created_at_ns: i64,
}

#[derive(Queryable, Selectable, Debug, Clone)]
#[diesel(table_name = key_package_history)]
pub struct StoredKeyPackageHistoryEntry {
    pub id: i32,
    pub key_package_hash_ref: Vec<u8>,
    pub created_at_ns: i64,
    pub delete_in_ns: Option<i64>,
}

impl_store_or_ignore!(NewKeyPackageHistoryEntry, key_package_history);

impl<C: ConnectionExt> DbConnection<C> {
    pub fn store_key_package_history_entry(
        &self,
        key_package_hash_ref: Vec<u8>,
    ) -> Result<StoredKeyPackageHistoryEntry, StorageError> {
        let entry = NewKeyPackageHistoryEntry {
            key_package_hash_ref: key_package_hash_ref.clone(),
            created_at_ns: now_ns(),
        };
        entry.store_or_ignore(self)?;

        self.find_key_package_history_entry_by_hash_ref(key_package_hash_ref)
    }

    pub fn find_key_package_history_entry_by_hash_ref(
        &self,
        hash_ref: Vec<u8>,
    ) -> Result<StoredKeyPackageHistoryEntry, StorageError> {
        let result = self.raw_query_read(|conn| {
            key_package_history::dsl::key_package_history
                .filter(key_package_history::dsl::key_package_hash_ref.eq(hash_ref))
                .first::<StoredKeyPackageHistoryEntry>(conn)
        })?;

        Ok(result)
    }

    pub fn find_key_package_history_entries_before_id(
        &self,
        id: i32,
    ) -> Result<Vec<StoredKeyPackageHistoryEntry>, StorageError> {
        let result = self.raw_query_read(|conn| {
            key_package_history::dsl::key_package_history
                .filter(key_package_history::dsl::id.lt(id))
                .load::<StoredKeyPackageHistoryEntry>(conn)
        })?;

        Ok(result)
    }

    pub fn mark_key_package_before_id_to_be_deleted(&self, id: i32) -> Result<(), StorageError> {
        use crate::schema::key_package_history::dsl;
        let delete_in_24_hrs_ns = now_ns() + NS_IN_DAY;
        self.raw_query_write(|conn| {
            diesel::update(
                dsl::key_package_history
                    .filter(dsl::id.lt(id))
                    .filter(dsl::delete_in_ns.is_null()), // Only set if not already set
            )
            .set(dsl::delete_in_ns.eq(delete_in_24_hrs_ns))
            .execute(conn)
        })?;

        Ok(())
    }

    pub fn get_expired_key_packages(
        &self,
    ) -> Result<Vec<StoredKeyPackageHistoryEntry>, StorageError> {
        use crate::schema::key_package_history::dsl;
        self.raw_query_read(|conn| {
            dsl::key_package_history
                .filter(dsl::delete_in_ns.le(now_ns()))
                .load::<StoredKeyPackageHistoryEntry>(conn)
        })
        .map_err(StorageError::from) // convert ConnectionError into StorageError
    }

    pub fn delete_expired_key_packages(&self) -> Result<(), StorageError> {
        use crate::schema::key_package_history::dsl;

        self.raw_query_write(|conn| {
            diesel::delete(dsl::key_package_history.filter(dsl::delete_in_ns.le(now_ns())))
                .execute(conn)
        })?;

        Ok(())
    }

    pub fn delete_key_package_history_entries_before_id(
        &self,
        id: i32,
    ) -> Result<(), StorageError> {
        self.raw_query_write(|conn| {
            diesel::delete(
                key_package_history::dsl::key_package_history
                    .filter(key_package_history::dsl::id.lt(id)),
            )
            .execute(conn)
        })?;

        Ok(())
    }

    pub fn delete_key_package_entry_with_id(&self, id: i32) -> Result<(), StorageError> {
        self.raw_query_write(|conn| {
            diesel::delete(
                key_package_history::dsl::key_package_history
                    .filter(key_package_history::dsl::id.eq(id)),
            )
            .execute(conn)
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::test_utils::with_connection;
    use xmtp_common::rand_vec;
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    #[xmtp_common::test]
    async fn test_store_key_package_history_entry() {
        with_connection(|conn| {
            let hash_ref = rand_vec::<24>();
            let new_entry = conn
                .store_key_package_history_entry(hash_ref.clone())
                .unwrap();
            assert_eq!(new_entry.key_package_hash_ref, hash_ref);
            assert_eq!(new_entry.id, 1);

            // Now delete it
            conn.delete_key_package_entry_with_id(1).unwrap();
            let all_entries = conn
                .find_key_package_history_entries_before_id(100)
                .unwrap();
            assert!(all_entries.is_empty());
        })
        .await
    }

    #[xmtp_common::test]
    async fn test_store_multiple() {
        with_connection(|conn| {
            let hash_ref1 = rand_vec::<24>();
            let hash_ref2 = rand_vec::<24>();
            let hash_ref3 = rand_vec::<24>();

            conn.store_key_package_history_entry(hash_ref1.clone())
                .unwrap();
            conn.store_key_package_history_entry(hash_ref2.clone())
                .unwrap();
            let entry_3 = conn
                .store_key_package_history_entry(hash_ref3.clone())
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
        .await
    }
}
