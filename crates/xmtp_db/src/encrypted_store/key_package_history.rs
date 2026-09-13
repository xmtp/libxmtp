use super::{
    ConnectionExt, StorageError, db_connection::DbConnection, schema::key_package_history,
};
use crate::{StoreOrIgnore, impl_store_or_ignore};
use diesel::prelude::*;
use xmtp_common::time::now_ns;
use xmtp_configuration::KEYS_EXPIRATION_INTERVAL_NS;
use xmtp_proto::types::Cursor;

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = key_package_history)]
pub struct NewKeyPackageHistoryEntry {
    pub key_package_hash_ref: Vec<u8>,
    pub post_quantum_public_key: Option<Vec<u8>>,
    pub created_at_ns: i64,
}

#[derive(Queryable, Selectable, Debug, Clone)]
#[diesel(table_name = key_package_history)]
pub struct StoredKeyPackageHistoryEntry {
    pub id: i32,
    pub key_package_hash_ref: Vec<u8>,
    pub created_at_ns: i64,
    pub delete_at_ns: Option<i64>,
    pub post_quantum_public_key: Option<Vec<u8>>,
    /// Highest confirmed publication receipt. Unknown publication never retires another key.
    pub published_sequence_id: Option<i64>,
}

impl_store_or_ignore!(NewKeyPackageHistoryEntry, key_package_history);

pub trait QueryKeyPackageHistory {
    fn store_key_package_history_entry(
        &self,
        key_package_hash_ref: Vec<u8>,
        post_quantum_public_key: Option<Vec<u8>>,
    ) -> Result<StoredKeyPackageHistoryEntry, StorageError>;

    fn find_key_package_history_entry_by_hash_ref(
        &self,
        hash_ref: Vec<u8>,
    ) -> Result<StoredKeyPackageHistoryEntry, StorageError>;

    fn find_key_package_history_entries_before_id(
        &self,
        id: i32,
    ) -> Result<Vec<StoredKeyPackageHistoryEntry>, StorageError>;

    /// Retire keys by confirmed publication order, not local creation order.
    /// The latest published key stays usable; duplicate receipts keep the first retirement deadline.
    fn record_key_package_publication(
        &self,
        history_id: i32,
        sequence: Cursor,
    ) -> Result<(), StorageError>;

    fn get_expired_key_packages(&self) -> Result<Vec<StoredKeyPackageHistoryEntry>, StorageError>;

    /// Soonest pending `delete_at_ns` across all key packages marked for deletion,
    /// or `None` if none are marked. The KpDeletion task's reschedule source.
    fn min_key_package_delete_at_ns(&self) -> Result<Option<i64>, StorageError>;

    fn delete_key_package_entry_with_id(&self, id: i32) -> Result<(), StorageError>;
}

impl<T> QueryKeyPackageHistory for &T
where
    T: QueryKeyPackageHistory,
{
    fn store_key_package_history_entry(
        &self,
        key_package_hash_ref: Vec<u8>,
        post_quantum_public_key: Option<Vec<u8>>,
    ) -> Result<StoredKeyPackageHistoryEntry, StorageError> {
        (**self).store_key_package_history_entry(key_package_hash_ref, post_quantum_public_key)
    }

    fn find_key_package_history_entry_by_hash_ref(
        &self,
        hash_ref: Vec<u8>,
    ) -> Result<StoredKeyPackageHistoryEntry, StorageError> {
        (**self).find_key_package_history_entry_by_hash_ref(hash_ref)
    }

    fn find_key_package_history_entries_before_id(
        &self,
        id: i32,
    ) -> Result<Vec<StoredKeyPackageHistoryEntry>, StorageError> {
        (**self).find_key_package_history_entries_before_id(id)
    }

    fn record_key_package_publication(
        &self,
        history_id: i32,
        sequence: Cursor,
    ) -> Result<(), StorageError> {
        (**self).record_key_package_publication(history_id, sequence)
    }

    fn get_expired_key_packages(&self) -> Result<Vec<StoredKeyPackageHistoryEntry>, StorageError> {
        (**self).get_expired_key_packages()
    }

    fn min_key_package_delete_at_ns(&self) -> Result<Option<i64>, StorageError> {
        (**self).min_key_package_delete_at_ns()
    }

    fn delete_key_package_entry_with_id(&self, id: i32) -> Result<(), StorageError> {
        (**self).delete_key_package_entry_with_id(id)
    }
}

impl<C: ConnectionExt> QueryKeyPackageHistory for DbConnection<C> {
    fn record_key_package_publication(
        &self,
        history_id: i32,
        sequence: Cursor,
    ) -> Result<(), StorageError> {
        use crate::schema::key_package_history::dsl;
        let sequence = i64::try_from(sequence.0)
            .ok()
            .filter(|sequence| *sequence > 0)
            .ok_or(crate::stream_storage::StreamStorageError::InvalidBatch)?;
        super::stream_storage::stream_transaction(self, |conn| {
            let previous = dsl::key_package_history
                .find(history_id)
                .select(dsl::published_sequence_id)
                .first::<Option<i64>>(conn)
                .optional()?
                .ok_or(crate::NotFound::KeyPackageHistory(history_id))?;
            diesel::update(dsl::key_package_history.find(history_id))
                .set(dsl::published_sequence_id.eq(previous.unwrap_or(0).max(sequence)))
                .execute(conn)?;
            let latest = dsl::key_package_history
                .select(diesel::dsl::max(dsl::published_sequence_id))
                .first::<Option<i64>>(conn)?
                .ok_or(StorageError::DbDeserialize)?;
            let delete_at = now_ns()
                .checked_add(KEYS_EXPIRATION_INTERVAL_NS)
                .ok_or(StorageError::DbSerialize)?;
            diesel::update(
                dsl::key_package_history
                    .filter(dsl::published_sequence_id.lt(latest))
                    .filter(dsl::delete_at_ns.is_null()),
            )
            .set(dsl::delete_at_ns.eq(delete_at))
            .execute(conn)?;
            diesel::update(
                dsl::key_package_history.filter(
                    dsl::published_sequence_id
                        .eq(latest)
                        .or(dsl::published_sequence_id.is_null()),
                ),
            )
            .set(dsl::delete_at_ns.eq(None::<i64>))
            .execute(conn)?;
            Ok(())
        })
    }

    fn store_key_package_history_entry(
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
    }

    fn find_key_package_history_entry_by_hash_ref(
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

    fn find_key_package_history_entries_before_id(
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

    fn get_expired_key_packages(&self) -> Result<Vec<StoredKeyPackageHistoryEntry>, StorageError> {
        use crate::schema::key_package_history::dsl;
        self.raw_query(|conn| {
            dsl::key_package_history
                .filter(dsl::delete_at_ns.le(now_ns()))
                .load::<StoredKeyPackageHistoryEntry>(conn)
        })
        .map_err(StorageError::from) // convert ConnectionError into StorageError
    }

    fn min_key_package_delete_at_ns(&self) -> Result<Option<i64>, StorageError> {
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

    fn delete_key_package_entry_with_id(&self, id: i32) -> Result<(), StorageError> {
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

#[cfg(test)]
mod tests {
    #[xmtp_common::test(unwrap_try = true)]
    async fn duplicate_publication_preserves_the_first_retirement_deadline() {
        use crate::{TestDb, XmtpTestDb};
        use xmtp_proto::types::Cursor;
        let store = TestDb::create_persistent_store(None).await;
        let db = store.db();
        let old = db.store_key_package_history_entry(vec![1], None)?;
        let latest = db.store_key_package_history_entry(vec![2], None)?;
        db.record_key_package_publication(old.id, Cursor(10))?;
        db.record_key_package_publication(latest.id, Cursor(20))?;
        let retired = db.find_key_package_history_entry_by_hash_ref(vec![1])?;
        assert!(retired.delete_at_ns.is_some());
        db.record_key_package_publication(old.id, Cursor(5))?;
        db.record_key_package_publication(latest.id, Cursor(20))?;
        let repeated = db.find_key_package_history_entry_by_hash_ref(vec![1])?;
        assert_eq!(repeated.published_sequence_id, Some(10));
        assert_eq!(repeated.delete_at_ns, retired.delete_at_ns);
        assert_eq!(db.min_key_package_delete_at_ns()?, retired.delete_at_ns);
        assert!(db.get_expired_key_packages()?.is_empty());
        assert!(
            db.find_key_package_history_entry_by_hash_ref(vec![2])?
                .delete_at_ns
                .is_none()
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn failed_publication_receipt_rolls_back_retirement() {
        use crate::{ConnectionExt, TestDb, XmtpTestDb};
        use diesel::connection::SimpleConnection;
        use xmtp_proto::types::Cursor;
        let store = TestDb::create_persistent_store(None).await;
        let db = store.db();
        let old = db.store_key_package_history_entry(vec![1], None)?;
        let next = db.store_key_package_history_entry(vec![2], None)?;
        db.record_key_package_publication(old.id, Cursor(10))?;
        db.raw_query(|conn| conn.batch_execute("CREATE TEMP TRIGGER fail_key_retirement BEFORE UPDATE OF delete_at_ns ON key_package_history WHEN NEW.delete_at_ns IS NOT NULL BEGIN SELECT RAISE(ABORT, 'injected retirement failure'); END;"))?;
        assert!(
            db.record_key_package_publication(next.id, Cursor(20))
                .is_err()
        );
        let old = db.find_key_package_history_entry_by_hash_ref(vec![1])?;
        let next = db.find_key_package_history_entry_by_hash_ref(vec![2])?;
        assert_eq!(old.published_sequence_id, Some(10));
        assert!(old.delete_at_ns.is_none());
        assert!(next.published_sequence_id.is_none());
        assert!(next.delete_at_ns.is_none());
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn publication_order_preserves_the_last_advertised_key() {
        use crate::{TestDb, XmtpTestDb};
        use xmtp_proto::types::Cursor;
        let store = TestDb::create_persistent_store(None).await;
        let db = store.db();
        let first = db.store_key_package_history_entry(vec![1], None)?;
        let second = db.store_key_package_history_entry(vec![2], None)?;
        let unknown = db.store_key_package_history_entry(vec![3], None)?;
        db.record_key_package_publication(second.id, Cursor(10))?;
        db.record_key_package_publication(first.id, Cursor(20))?;
        db.record_key_package_publication(second.id, Cursor(10))?;
        assert!(
            db.find_key_package_history_entry_by_hash_ref(first.key_package_hash_ref.clone())?
                .delete_at_ns
                .is_none()
        );
        assert!(
            db.find_key_package_history_entry_by_hash_ref(second.key_package_hash_ref.clone())?
                .delete_at_ns
                .is_some()
        );
        assert!(
            db.find_key_package_history_entry_by_hash_ref(unknown.key_package_hash_ref)?
                .delete_at_ns
                .is_none()
        );
        db.record_key_package_publication(second.id, Cursor(30))?;
        assert!(
            db.find_key_package_history_entry_by_hash_ref(first.key_package_hash_ref)?
                .delete_at_ns
                .is_some()
        );
        assert!(
            db.find_key_package_history_entry_by_hash_ref(second.key_package_hash_ref)?
                .delete_at_ns
                .is_none()
        );
    }

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
