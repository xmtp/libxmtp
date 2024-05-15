use std::collections::HashMap;

use crate::{impl_store, storage::StorageError};

use super::{
    db_connection::DbConnection,
    schema::identity_updates::{self, dsl},
};
use diesel::{dsl::max, prelude::*};
use xmtp_id::associations::{AssociationError, IdentityUpdate};

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

impl TryFrom<StoredIdentityUpdate> for IdentityUpdate {
    type Error = AssociationError;

    fn try_from(update: StoredIdentityUpdate) -> Result<Self, Self::Error> {
        Ok(IdentityUpdate::try_from(update.payload)?)
    }
}

impl_store!(StoredIdentityUpdate, identity_updates);

impl DbConnection {
    /// Returns all identity updates for the given inbox ID up to the provided sequence_id.
    /// Returns updates greater than `from_sequence_id` and less than _or equal to_ `to_sequence_id`
    pub fn get_identity_updates<InboxId: AsRef<str>>(
        &self,
        inbox_id: InboxId,
        from_sequence_id: Option<i64>,
        to_sequence_id: Option<i64>,
    ) -> Result<Vec<StoredIdentityUpdate>, StorageError> {
        let mut query = dsl::identity_updates
            .order(dsl::sequence_id.asc())
            .filter(dsl::inbox_id.eq(inbox_id.as_ref()))
            .into_boxed();

        if let Some(sequence_id) = from_sequence_id {
            query = query.filter(dsl::sequence_id.gt(sequence_id));
        }

        if let Some(sequence_id) = to_sequence_id {
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

    /// Given a list of inbox_ids return a hashamp of each inbox ID -> highest known sequence ID
    pub fn get_latest_sequence_id(
        &self,
        inbox_ids: &[String],
    ) -> Result<HashMap<String, i64>, StorageError> {
        // Query IdentityUpdates grouped by inbox_id, getting the max sequence_id
        let query = dsl::identity_updates
            .group_by(dsl::inbox_id)
            .select((dsl::inbox_id, max(dsl::sequence_id)))
            .filter(dsl::inbox_id.eq_any(inbox_ids));

        // Get the results as a Vec of (inbox_id, sequence_id) tuples
        let result_tuples: Vec<(String, i64)> = self
            .raw_query(|conn| query.load::<(String, Option<i64>)>(conn))?
            .into_iter()
            // Diesel needs an Option type for aggregations like max(sequence_id), so we
            // unwrap the option here
            .filter_map(|(inbox_id, sequence_id_opt)| {
                sequence_id_opt.map(|sequence_id| (inbox_id, sequence_id))
            })
            .collect();

        // Convert the Vec to a HashMap
        Ok(HashMap::from_iter(result_tuples))
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

            update_1.store(conn).expect("should store without error");
            update_2.store(conn).expect("should store without error");

            let all_updates = conn
                .get_identity_updates(inbox_id, None, None)
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
                .get_identity_updates(inbox_id, None, Some(2))
                .expect("query should work");

            assert_eq!(update_1_and_2.len(), 2);

            let all_updates = conn
                .get_identity_updates(inbox_id, None, None)
                .expect("query should work");

            assert_eq!(all_updates.len(), 3);

            let only_update_2 = conn
                .get_identity_updates(inbox_id, Some(1), Some(2))
                .expect("query should work");

            assert_eq!(only_update_2.len(), 1);
            assert_eq!(only_update_2[0].sequence_id, 2);
        })
    }

    #[test]
    fn test_get_latest_sequence_id() {
        with_connection(|conn| {
            let inbox_1 = "inbox_1";
            let inbox_2 = "inbox_2";
            let update_1 = build_update(inbox_1, 1);
            let update_2 = build_update(inbox_1, 3);
            let update_3 = build_update(inbox_2, 5);
            let update_4 = build_update(inbox_2, 6);

            conn.insert_or_ignore_identity_updates(&[update_1, update_2, update_3, update_4])
                .expect("insert should succeed");

            let latest_sequence_ids = conn
                .get_latest_sequence_id(&[inbox_1.to_string(), inbox_2.to_string()])
                .expect("query should work");

            assert_eq!(latest_sequence_ids.get(inbox_1), Some(&3));
            assert_eq!(latest_sequence_ids.get(inbox_2), Some(&6));

            let latest_sequence_ids_with_missing_member = conn
                .get_latest_sequence_id(&[inbox_1.to_string(), "missing_inbox".to_string()])
                .expect("should still succeed");

            assert_eq!(
                latest_sequence_ids_with_missing_member.get(inbox_1),
                Some(&3)
            );
            assert_eq!(
                latest_sequence_ids_with_missing_member.get("missing_inbox"),
                None
            );
        })
    }
}
