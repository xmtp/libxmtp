use diesel::prelude::*;

use super::{StorageError, db_connection::DbConnection, schema::key_package_history};
use crate::{StoreOrIgnore, impl_store_or_ignore};
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
}

impl_store_or_ignore!(NewKeyPackageHistoryEntry, key_package_history);

impl DbConnection {
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
    use crate::storage::encrypted_store::tests::with_connection;
    use xmtp_common::rand_vec;
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
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

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
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
