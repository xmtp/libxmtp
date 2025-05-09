use std::collections::HashMap;

use crate::impl_store;

use super::{
    ConnectionExt,
    db_connection::DbConnection,
    schema::identity_updates::{self, dsl},
};
use diesel::{dsl::max, prelude::*};

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

impl<C: ConnectionExt> DbConnection<C> {
    /// Returns all identity updates for the given inbox ID up to the provided sequence_id.
    /// Returns updates greater than `from_sequence_id` and less than _or equal to_ `to_sequence_id`
    pub fn get_identity_updates<InboxId: AsRef<str>>(
        &self,
        inbox_id: InboxId,
        from_sequence_id: Option<i64>,
        to_sequence_id: Option<i64>,
    ) -> Result<Vec<StoredIdentityUpdate>, crate::ConnectionError> {
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

        self.raw_query_read(|conn| query.load::<StoredIdentityUpdate>(conn))
    }

    /// Batch insert identity updates, ignoring duplicates.
    #[tracing::instrument(level = "trace", skip(updates))]
    pub fn insert_or_ignore_identity_updates(
        &self,
        updates: &[StoredIdentityUpdate],
    ) -> Result<(), crate::ConnectionError> {
        self.raw_query_write(|conn| {
            diesel::insert_or_ignore_into(dsl::identity_updates)
                .values(updates)
                .execute(conn)
        })?;
        Ok(())
    }

    pub fn get_latest_sequence_id_for_inbox(
        &self,
        inbox_id: &str,
    ) -> Result<i64, crate::ConnectionError> {
        let query = dsl::identity_updates
            .select(dsl::sequence_id)
            .order(dsl::sequence_id.desc())
            .limit(1)
            .filter(dsl::inbox_id.eq(inbox_id))
            .into_boxed();

        self.raw_query_read(|conn| query.first::<i64>(conn))
    }

    /// Given a list of inbox_ids return a HashMap of each inbox ID -> highest known sequence ID
    #[tracing::instrument(level = "trace", skip_all)]
    pub fn get_latest_sequence_id(
        &self,
        inbox_ids: &[&str],
    ) -> Result<HashMap<String, i64>, crate::ConnectionError> {
        // Query IdentityUpdates grouped by inbox_id, getting the max sequence_id
        let query = dsl::identity_updates
            .group_by(dsl::inbox_id)
            .select((dsl::inbox_id, max(dsl::sequence_id)))
            .filter(dsl::inbox_id.eq_any(inbox_ids));

        // Get the results as a Vec of (inbox_id, sequence_id) tuples
        let result_tuples: Vec<(String, i64)> = self
            .raw_query_read(|conn| query.load::<(String, Option<i64>)>(conn))?
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

    pub fn filter_inbox_ids_needing_updates<'a>(
        &self,
        filters: &[(&'a str, i64)],
    ) -> Result<Vec<&'a str>, crate::ConnectionError> {
        let existing_sequence_ids =
            self.get_latest_sequence_id(&filters.iter().map(|f| f.0).collect::<Vec<&str>>())?;

        let needs_update = filters
            .iter()
            .filter_map(|filter| {
                let existing_sequence_id = existing_sequence_ids.get(filter.0);
                if let Some(sequence_id) = existing_sequence_id {
                    if sequence_id.ge(&filter.1) {
                        return None;
                    }
                }

                Some(filter.0)
            })
            .collect::<Vec<&str>>();

        Ok(needs_update)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use crate::{Store, test_utils::with_connection};
    use xmtp_common::{rand_time, rand_vec};

    use super::*;

    fn build_update(inbox_id: &str, sequence_id: i64) -> StoredIdentityUpdate {
        StoredIdentityUpdate::new(
            inbox_id.to_string(),
            sequence_id,
            rand_time(),
            rand_vec::<24>(),
        )
    }

    #[xmtp_common::test]
    async fn insert_and_read() {
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
        })
        .await
    }

    #[xmtp_common::test]
    async fn test_filter() {
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
        .await
    }

    #[xmtp_common::test]
    async fn test_get_latest_sequence_id() {
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
                .get_latest_sequence_id(&[inbox_1, inbox_2])
                .expect("query should work");

            assert_eq!(latest_sequence_ids.get(inbox_1), Some(&3));
            assert_eq!(latest_sequence_ids.get(inbox_2), Some(&6));

            let latest_sequence_ids_with_missing_member = conn
                .get_latest_sequence_id(&[inbox_1, "missing_inbox"])
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
        .await
    }

    #[xmtp_common::test]
    async fn get_single_sequence_id() {
        with_connection(|conn| {
            let inbox_id = "inbox_1";
            let update = build_update(inbox_id, 1);
            let update_2 = build_update(inbox_id, 2);
            update.store(conn).expect("should store without error");
            update_2.store(conn).expect("should store without error");

            let sequence_id = conn
                .get_latest_sequence_id_for_inbox(inbox_id)
                .expect("query should work");
            assert_eq!(sequence_id, 2);
        })
        .await
    }
}
