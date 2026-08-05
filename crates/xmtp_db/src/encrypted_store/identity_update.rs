use std::collections::HashMap;

use crate::StorageError;
#[cfg(feature = "sync")]
use crate::impl_store;

#[cfg(feature = "sync")]
use super::{
    ConnectionExt,
    db_connection::DbConnection,
    schema::identity_updates::{self, dsl},
};
use derive_builder::Builder;
#[cfg(feature = "sync")]
use diesel::{dsl::max, prelude::*};

/// StoredIdentityUpdate holds a serialized IdentityUpdate record
#[derive(Debug, Clone, PartialEq, Eq, Builder)]
#[cfg_attr(feature = "sync", derive(Insertable, Identifiable, Queryable))]
#[cfg_attr(feature = "sync", diesel(table_name = identity_updates))]
#[cfg_attr(feature = "sync", diesel(primary_key(inbox_id, sequence_id)))]
#[builder(setter(into), build_fn(error = "StorageError"))]
#[derive(xmtp_macro::PgModel)]
#[xmtp(table = "identity_updates")]
pub struct StoredIdentityUpdate {
    pub inbox_id: String,
    pub sequence_id: i64,
    pub server_timestamp_ns: i64,
    pub payload: Vec<u8>,
    pub originator_id: i32,
}

impl StoredIdentityUpdate {
    pub fn build() -> StoredIdentityUpdateBuilder {
        StoredIdentityUpdateBuilder::default()
    }

    pub fn new(
        inbox_id: String,
        sequence_id: i64,
        server_timestamp_ns: i64,
        payload: Vec<u8>,
        originator_id: i32,
    ) -> Self {
        Self {
            inbox_id,
            sequence_id,
            server_timestamp_ns,
            payload,
            originator_id,
        }
    }
}

#[cfg(feature = "sync")]
impl_store!(StoredIdentityUpdate, identity_updates);

pub trait QueryIdentityUpdates {
    /// Returns all identity updates for the given inbox ID up to the provided sequence_id.
    /// Returns updates greater than `from_sequence_id` and less than _or equal to_ `to_sequence_id`
    fn get_identity_updates(
        &self,
        inbox_id: &str,
        from_sequence_id: Option<i64>,
        to_sequence_id: Option<i64>,
    ) -> impl std::future::Future<Output = Result<Vec<StoredIdentityUpdate>, crate::ConnectionError>>
    + xmtp_common::MaybeSend;

    /// Batch insert identity updates, ignoring duplicates.
    fn insert_or_ignore_identity_updates(
        &self,
        updates: &[StoredIdentityUpdate],
    ) -> impl std::future::Future<Output = Result<(), crate::ConnectionError>> + xmtp_common::MaybeSend;

    fn get_latest_sequence_id_for_inbox(
        &self,
        inbox_id: &str,
    ) -> impl std::future::Future<Output = Result<i64, crate::ConnectionError>> + xmtp_common::MaybeSend;

    /// Given a list of inbox_ids return a HashMap of each inbox ID -> highest known sequence ID
    fn get_latest_sequence_id(
        &self,
        inbox_ids: &[&str],
    ) -> impl std::future::Future<Output = Result<HashMap<String, i64>, crate::ConnectionError>>
    + xmtp_common::MaybeSend;

    /// Returns the count of identity updates for inbox_ids
    fn count_inbox_updates(
        &self,
        inbox_ids: &[&str],
    ) -> impl std::future::Future<Output = Result<HashMap<String, i64>, crate::ConnectionError>>
    + xmtp_common::MaybeSend;
}

impl<T> QueryIdentityUpdates for &T
where
    T: QueryIdentityUpdates + xmtp_common::MaybeSync,
{
    async fn get_identity_updates(
        &self,
        inbox_id: &str,
        from_sequence_id: Option<i64>,
        to_sequence_id: Option<i64>,
    ) -> Result<Vec<StoredIdentityUpdate>, crate::ConnectionError> {
        (**self)
            .get_identity_updates(inbox_id, from_sequence_id, to_sequence_id)
            .await
    }

    async fn insert_or_ignore_identity_updates(
        &self,
        updates: &[StoredIdentityUpdate],
    ) -> Result<(), crate::ConnectionError> {
        (**self).insert_or_ignore_identity_updates(updates).await
    }

    async fn get_latest_sequence_id_for_inbox(
        &self,
        inbox_id: &str,
    ) -> Result<i64, crate::ConnectionError> {
        (**self).get_latest_sequence_id_for_inbox(inbox_id).await
    }

    async fn get_latest_sequence_id(
        &self,
        inbox_ids: &[&str],
    ) -> Result<HashMap<String, i64>, crate::ConnectionError> {
        (**self).get_latest_sequence_id(inbox_ids).await
    }

    async fn count_inbox_updates(
        &self,
        inbox_ids: &[&str],
    ) -> Result<HashMap<String, i64>, crate::ConnectionError> {
        (**self).count_inbox_updates(inbox_ids).await
    }
}

#[cfg(feature = "sync")]
impl<C: ConnectionExt> QueryIdentityUpdates for DbConnection<C> {
    /// Returns all identity updates for the given inbox ID up to the provided sequence_id.
    /// Returns updates greater than `from_sequence_id` and less than _or equal to_ `to_sequence_id`
    async fn get_identity_updates(
        &self,
        inbox_id: &str,
        from_sequence_id: Option<i64>,
        to_sequence_id: Option<i64>,
    ) -> Result<Vec<StoredIdentityUpdate>, crate::ConnectionError> {
        let mut query = dsl::identity_updates
            .order(dsl::sequence_id.asc())
            .filter(dsl::inbox_id.eq(inbox_id))
            .into_boxed();

        if let Some(sequence_id) = from_sequence_id {
            query = query.filter(dsl::sequence_id.gt(sequence_id));
        }

        if let Some(sequence_id) = to_sequence_id {
            query = query.filter(dsl::sequence_id.le(sequence_id));
        }

        self.raw_query(|conn| query.load::<StoredIdentityUpdate>(conn))
    }

    /// Batch insert identity updates, ignoring duplicates.
    #[tracing::instrument(level = "trace", skip(updates))]
    async fn insert_or_ignore_identity_updates(
        &self,
        updates: &[StoredIdentityUpdate],
    ) -> Result<(), crate::ConnectionError> {
        self.raw_query(|conn| {
            diesel::insert_or_ignore_into(dsl::identity_updates)
                .values(updates)
                .execute(conn)
        })?;
        Ok(())
    }

    async fn get_latest_sequence_id_for_inbox(
        &self,
        inbox_id: &str,
    ) -> Result<i64, crate::ConnectionError> {
        let query = dsl::identity_updates
            .select(dsl::sequence_id)
            .order(dsl::sequence_id.desc())
            .limit(1)
            .filter(dsl::inbox_id.eq(inbox_id))
            .into_boxed();

        self.raw_query(|conn| query.first::<i64>(conn))
    }

    /// Given a list of inbox_ids return a HashMap of each inbox ID -> highest known sequence ID
    #[tracing::instrument(level = "trace", skip_all)]
    async fn get_latest_sequence_id(
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

    async fn count_inbox_updates(
        &self,
        inbox_ids: &[&str],
    ) -> Result<HashMap<String, i64>, crate::ConnectionError> {
        use diesel::dsl::count_star;
        let query = dsl::identity_updates
            .group_by(dsl::inbox_id)
            .select((dsl::inbox_id, count_star()))
            .filter(dsl::inbox_id.eq_any(inbox_ids));
        self.raw_query(|conn| {
            query
                .load_iter::<(String, i64), _>(conn)?
                .collect::<Result<HashMap<_, _>, _>>()
        })
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
    fn update(row: &sqlx::postgres::PgRow) -> Result<StoredIdentityUpdate, crate::ConnectionError> {
        use sqlx::FromRow;
        Ok(StoredIdentityUpdate::from_row(row)?)
    }

    impl QueryIdentityUpdates for PgDb {
        /// Exclusive on `from_sequence_id`, inclusive on `to_sequence_id`.
        ///
        /// The bounds are optional, so they are applied as `$n IS NULL OR ...`
        /// rather than by assembling the SQL conditionally: one statement text,
        /// one prepared plan, whichever bounds the caller supplies.
        async fn get_identity_updates(
            &self,
            inbox_id: &str,
            from_sequence_id: Option<i64>,
            to_sequence_id: Option<i64>,
        ) -> Result<Vec<StoredIdentityUpdate>, crate::ConnectionError> {
            let mut c = self.conn().await?;
            let rows = sqlx::query(&format!(
                "SELECT {} FROM identity_updates WHERE inbox_id = $1 \
                   AND ($2::int8 IS NULL OR sequence_id > $2) \
                   AND ($3::int8 IS NULL OR sequence_id <= $3) \
                 ORDER BY sequence_id ASC",
                StoredIdentityUpdate::select_columns()
            ))
            .bind(inbox_id)
            .bind(from_sequence_id)
            .bind(to_sequence_id)
            .fetch_all(&mut *c)
            .await?;
            rows.iter().map(update).collect()
        }

        /// Batch insert, ignoring rows already present.
        ///
        /// The whole batch goes over as five parallel arrays zipped by `UNNEST`,
        /// so it stays one statement and one round trip no matter how many
        /// updates there are — the sync track's multi-row `VALUES` would hit
        /// Postgres' 65535-parameter ceiling at ~13k updates.
        async fn insert_or_ignore_identity_updates(
            &self,
            updates: &[StoredIdentityUpdate],
        ) -> Result<(), crate::ConnectionError> {
            if updates.is_empty() {
                return Ok(());
            }
            let mut inbox_ids = Vec::with_capacity(updates.len());
            let mut sequence_ids = Vec::with_capacity(updates.len());
            let mut timestamps = Vec::with_capacity(updates.len());
            let mut payloads = Vec::with_capacity(updates.len());
            let mut originators = Vec::with_capacity(updates.len());
            for u in updates {
                inbox_ids.push(u.inbox_id.as_str());
                sequence_ids.push(u.sequence_id);
                timestamps.push(u.server_timestamp_ns);
                payloads.push(u.payload.as_slice());
                originators.push(u.originator_id);
            }

            let mut c = self.conn().await?;
            sqlx::query(
                "INSERT INTO identity_updates \
                 (inbox_id, sequence_id, server_timestamp_ns, payload, originator_id) \
                 SELECT * FROM UNNEST($1::text[], $2::int8[], $3::int8[], $4::bytea[], $5::int4[]) \
                 ON CONFLICT DO NOTHING",
            )
            .bind(&inbox_ids)
            .bind(&sequence_ids)
            .bind(&timestamps)
            .bind(&payloads)
            .bind(&originators)
            .execute(&mut *c)
            .await?;
            Ok(())
        }

        /// An inbox with no updates is an error, matching the diesel impl's
        /// `first()` — callers treat "unknown inbox" as distinct from sequence 0.
        async fn get_latest_sequence_id_for_inbox(
            &self,
            inbox_id: &str,
        ) -> Result<i64, crate::ConnectionError> {
            let mut c = self.conn().await?;
            let row = sqlx::query(
                "SELECT sequence_id FROM identity_updates WHERE inbox_id = $1 \
                 ORDER BY sequence_id DESC LIMIT 1",
            )
            .bind(inbox_id)
            .fetch_one(&mut *c)
            .await?;
            Ok(row.try_get(0)?)
        }

        async fn get_latest_sequence_id(
            &self,
            inbox_ids: &[&str],
        ) -> Result<HashMap<String, i64>, crate::ConnectionError> {
            if inbox_ids.is_empty() {
                return Ok(HashMap::new());
            }
            let mut c = self.conn().await?;
            let rows = sqlx::query(
                "SELECT inbox_id, MAX(sequence_id) FROM identity_updates \
                 WHERE inbox_id = ANY($1) GROUP BY inbox_id",
            )
            .bind(inbox_ids)
            .fetch_all(&mut *c)
            .await?;

            let pairs: Vec<(String, Option<i64>)> = rows
                .iter()
                .map(|r| Ok((r.try_get(0)?, r.try_get(1)?)))
                .collect::<Result<_, crate::ConnectionError>>()?;
            // An aggregate over a present group is never NULL here, but the type
            // is nullable; drop rather than coerce.
            Ok(pairs
                .into_iter()
                .filter_map(|(id, seq)| seq.map(|s| (id, s)))
                .collect())
        }

        async fn count_inbox_updates(
            &self,
            inbox_ids: &[&str],
        ) -> Result<HashMap<String, i64>, crate::ConnectionError> {
            if inbox_ids.is_empty() {
                return Ok(HashMap::new());
            }
            let mut c = self.conn().await?;
            let rows = sqlx::query(
                "SELECT inbox_id, COUNT(*) FROM identity_updates \
                 WHERE inbox_id = ANY($1) GROUP BY inbox_id",
            )
            .bind(inbox_ids)
            .fetch_all(&mut *c)
            .await?;
            rows.iter()
                .map(|r| Ok((r.try_get(0)?, r.try_get(1)?)))
                .collect()
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::{Store, test_utils::with_connection};
    use xmtp_common::{rand_time, rand_vec};

    use super::*;

    fn build_update(inbox_id: &str, sequence_id: i64) -> StoredIdentityUpdate {
        StoredIdentityUpdate::new(
            inbox_id.to_string(),
            sequence_id,
            rand_time(),
            rand_vec::<24>(),
            1,
        )
    }

    #[xmtp_common::test]
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
        })
    }

    #[xmtp_common::test]
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

    #[xmtp_common::test]
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
    }

    #[xmtp_common::test]
    fn get_single_sequence_id() {
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
    }

    #[xmtp_common::test]
    fn test_count_inbox_updates() {
        with_connection(|conn| {
            let inbox_1 = "inbox_1";
            let inbox_2 = "inbox_2";
            conn.insert_or_ignore_identity_updates(&[
                build_update(inbox_1, 1),
                build_update(inbox_1, 2),
                build_update(inbox_2, 1),
            ])
            .unwrap();
            let counts = conn
                .count_inbox_updates(&[inbox_1, inbox_2, "missing"])
                .unwrap();
            assert_eq!(counts.get(inbox_1), Some(&2));
            assert_eq!(counts.get(inbox_2), Some(&1));
            assert_eq!(counts.get("missing"), None);
        })
    }
}
