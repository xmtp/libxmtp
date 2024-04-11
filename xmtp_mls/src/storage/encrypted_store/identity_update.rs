use crate::{impl_store, storage::StorageError};

use super::{
    db_connection::DbConnection,
    schema::identity_updates::{self, dsl},
};
use diesel::prelude::*;

/// StoredIdentityUpdate holds a serialized IdentityUpdate record
#[derive(Insertable, Identifiable, Queryable, Debug, Clone, PartialEq, Eq)]
#[diesel(table_name = identity_updates)]
#[diesel(primary_key(inbox_id, sequence_id))]
pub struct StoredIdentityUpdate {
    pub inbox_id: String,
    pub sequence_id: i64,
    pub server_timestamp_ns: i64,
    pub payload: Vec<u8>,
}

impl StoredIdentityUpdate {
    pub fn new(
        inbox_id: String,
        sequence_id: i64,
        server_timestamp_ns: i64,
        payload: Vec<u8>,
    ) -> Self {
        Self {
            inbox_id,
            sequence_id,
            server_timestamp_ns,
            payload,
        }
    }
}

impl_store!(StoredIdentityUpdate, identity_updates);

impl DbConnection<'_> {
    /// Returns all identity updates for the given inbox ID up to the provided sequence_id.
    /// If no sequence_id is provided, return all updates
    pub fn get_identity_updates<InboxId: AsRef<str>>(
        &self,
        inbox_id: InboxId,
        sequence_id: Option<i64>,
    ) -> Result<Vec<StoredIdentityUpdate>, StorageError> {
        let mut query = dsl::identity_updates
            .order(dsl::sequence_id.asc())
            .filter(dsl::inbox_id.eq(inbox_id.as_ref()))
            .into_boxed();

        if let Some(sequence_id) = sequence_id {
            query = query.filter(dsl::sequence_id.le(sequence_id));
        }

        Ok(self.raw_query(|conn| query.load::<StoredIdentityUpdate>(conn))?)
    }

    /// Batch insert identity updates, ignoring duplicates.
    pub fn insert_or_ignore_identity_updates(
        &self,
        updates: &[StoredIdentityUpdate],
    ) -> Result<(), StorageError> {
        Ok(self.raw_query(|conn| {
            diesel::insert_or_ignore_into(dsl::identity_updates)
                .values(updates)
                .execute(conn)?;

            Ok(())
        })?)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        storage::encrypted_store::tests::with_connection,
        utils::test::{rand_time, rand_vec},
        Store,
    };

    use super::*;

    fn build_update(inbox_id: &str, sequence_id: i64) -> StoredIdentityUpdate {
        StoredIdentityUpdate::new(inbox_id.to_string(), sequence_id, rand_time(), rand_vec())
    }

    #[test]
    fn insert_and_read() {
        with_connection(|conn| {
            let inbox_id = "inbox_1";
            let update_1 = build_update(inbox_id, 1);
            let update_1_payload = update_1.payload.clone();
            let update_2 = build_update(inbox_id, 2);
            let update_2_payload = update_2.payload.clone();

            update_1.store(&conn).expect("should store without error");
            update_2.store(&conn).expect("should store without error");

            let all_updates = conn
                .get_identity_updates(inbox_id, None)
                .expect("query should work");

            assert_eq!(all_updates.len(), 2);
            let first_update = all_updates.first().unwrap();
            assert_eq!(first_update.payload, update_1_payload);
            let second_update = all_updates.last().unwrap();
            assert_eq!(second_update.payload, update_2_payload);
        });
    }

    #[test]
    fn test_filter() {
        with_connection(|conn| {
            let inbox_id = "inbox_1";
            let update_1 = build_update(inbox_id, 1);
            let update_2 = build_update(inbox_id, 2);
            let update_3 = build_update(inbox_id, 3);

            conn.insert_or_ignore_identity_updates(&[update_1, update_2, update_3])
                .expect("insert should succeed");

            let update_1_and_2 = conn
                .get_identity_updates(inbox_id, Some(2))
                .expect("query should work");

            assert_eq!(update_1_and_2.len(), 2);

            let all_updates = conn
                .get_identity_updates(inbox_id, None)
                .expect("query should work");

            assert_eq!(all_updates.len(), 3);
        })
    }
}
