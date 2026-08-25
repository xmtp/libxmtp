#[cfg(feature = "sync")]
use super::ConnectionExt;
use super::group::StoredGroup;
#[cfg(feature = "sync")]
use super::{
    db_connection::DbConnection,
    schema::{
        consent_records::{self, dsl},
        groups::dsl as groups_dsl,
    },
};
#[cfg(feature = "sync")]
use crate::impl_store;
use crate::{DbQuery, StorageError};
#[cfg(feature = "sync")]
use diesel::{
    deserialize::FromSqlRow, expression::AsExpression, prelude::*, sql_types::Integer,
    upsert::excluded,
};
use serde::{Deserialize, Serialize};
use xmtp_common::time::now_ns;
use xmtp_proto::{
    ConversionError,
    xmtp::device_sync::consent_backup::{ConsentSave, ConsentStateSave, ConsentTypeSave},
};
mod convert;

/// StoredConsentRecord holds a serialized ConsentRecord
#[derive(Debug, Clone, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "sync", derive(Insertable, Queryable))]
#[cfg_attr(feature = "sync", diesel(table_name = consent_records))]
#[cfg_attr(feature = "sync", diesel(primary_key(entity_type, entity)))]
#[derive(xmtp_macro::PgModel)]
#[xmtp(table = "consent_records")]
pub struct StoredConsentRecord {
    /// Enum, [`ConsentType`] representing the type of consent (conversation_id inbox_id, etc..)
    pub entity_type: ConsentType,
    /// Enum, [`ConsentState`] representing the state of consent (allowed, denied, etc..)
    pub state: ConsentState,
    /// The entity of what was consented (0x00 etc..)
    pub entity: String,

    pub consented_at_ns: i64,
}

impl PartialEq for StoredConsentRecord {
    fn eq(&self, other: &Self) -> bool {
        self.entity == other.entity
            && self.entity_type == other.entity_type
            && self.state == other.state
    }
}

impl StoredConsentRecord {
    pub fn new(entity_type: ConsentType, state: ConsentState, entity: String) -> Self {
        Self {
            entity_type,
            state,
            entity,
            consented_at_ns: now_ns(),
        }
    }

    /// This function will perform some logic to see if a new group should be auto-consented
    /// or auto-denied based on past consent.
    pub async fn stitch_dm_consent(
        conn: &impl DbQuery,
        group: &StoredGroup,
    ) -> Result<(), StorageError> {
        if let Some(dm_id) = &group.dm_id {
            let mut past_consent = conn.find_consent_by_dm_id(dm_id).await?;
            let Some(last_consent) = past_consent.pop() else {
                return Ok(());
            };

            let cr = Self::new(
                ConsentType::ConversationId,
                last_consent.state,
                hex::encode(group.id),
            );
            conn.insert_newer_consent_record(cr).await?;
        }

        Ok(())
    }
}

#[cfg(feature = "sync")]
impl_store!(StoredConsentRecord, consent_records);

pub trait QueryConsentRecord {
    /// Returns the consent_records for the given entity up
    fn get_consent_record(
        &self,
        entity: String,
        entity_type: ConsentType,
    ) -> impl std::future::Future<
        Output = Result<Option<StoredConsentRecord>, crate::ConnectionError>,
    > + xmtp_common::MaybeSend;

    fn consent_records(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<StoredConsentRecord>, crate::ConnectionError>>
    + xmtp_common::MaybeSend;

    fn consent_records_paged(
        &self,
        limit: i64,
        offset: i64,
    ) -> impl std::future::Future<Output = Result<Vec<StoredConsentRecord>, crate::ConnectionError>>
    + xmtp_common::MaybeSend;

    /// Returns true if newer
    fn insert_newer_consent_record(
        &self,
        record: StoredConsentRecord,
    ) -> impl std::future::Future<Output = Result<bool, crate::ConnectionError>> + xmtp_common::MaybeSend;

    /// Insert consent_records, and replace existing entries, returns records that are new or changed
    fn insert_or_replace_consent_records(
        &self,
        records: &[StoredConsentRecord],
    ) -> impl std::future::Future<Output = Result<Vec<StoredConsentRecord>, crate::ConnectionError>>
    + xmtp_common::MaybeSend;

    fn maybe_insert_consent_record_return_existing(
        &self,
        record: &StoredConsentRecord,
    ) -> impl std::future::Future<
        Output = Result<Option<StoredConsentRecord>, crate::ConnectionError>,
    > + xmtp_common::MaybeSend;

    fn find_consent_by_dm_id(
        &self,
        dm_id: &str,
    ) -> impl std::future::Future<Output = Result<Vec<StoredConsentRecord>, crate::ConnectionError>>
    + xmtp_common::MaybeSend;
}

#[cfg(feature = "sync")]
impl<C: ConnectionExt> QueryConsentRecord for DbConnection<C> {
    /// Returns the consent_records for the given entity up
    async fn get_consent_record(
        &self,
        entity: String,
        entity_type: ConsentType,
    ) -> Result<Option<StoredConsentRecord>, crate::ConnectionError> {
        self.raw_query(|conn| {
            dsl::consent_records
                .filter(dsl::entity.eq(entity))
                .filter(dsl::entity_type.eq(entity_type))
                .first(conn)
                .optional()
        })
    }

    async fn consent_records(&self) -> Result<Vec<StoredConsentRecord>, crate::ConnectionError> {
        self.raw_query(|conn| super::schema::consent_records::table.load(conn))
    }

    async fn consent_records_paged(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<StoredConsentRecord>, crate::ConnectionError> {
        let query = consent_records::table
            .order_by((consent_records::entity_type, consent_records::entity))
            .limit(limit)
            .offset(offset);

        self.raw_query(|conn| query.load::<StoredConsentRecord>(conn))
    }

    // returns true if newer
    async fn insert_newer_consent_record(
        &self,
        record: StoredConsentRecord,
    ) -> Result<bool, crate::ConnectionError> {
        self.raw_query(|conn| {
            let maybe_inserted_consent_record: Option<StoredConsentRecord> =
                diesel::insert_into(dsl::consent_records)
                    .values(&record)
                    .on_conflict_do_nothing()
                    .get_result(conn)
                    .optional()?;

            // if record was not inserted...
            if maybe_inserted_consent_record.is_none() {
                let old_record = dsl::consent_records
                    .find((&record.entity_type, &record.entity))
                    .first::<StoredConsentRecord>(conn)?;

                if old_record.eq(&record) {
                    return Ok(false);
                }

                let should_replace = old_record.consented_at_ns < record.consented_at_ns;
                if should_replace {
                    diesel::insert_into(dsl::consent_records)
                        .values(record)
                        .on_conflict((dsl::entity_type, dsl::entity))
                        .do_update()
                        .set(dsl::state.eq(excluded(dsl::state)))
                        .execute(conn)?;
                }
                return Ok(should_replace);
            }

            Ok(true)
        })
    }

    /// Insert consent_records, and replace existing entries, returns records that are new or changed
    async fn insert_or_replace_consent_records(
        &self,
        records: &[StoredConsentRecord],
    ) -> Result<Vec<StoredConsentRecord>, crate::ConnectionError> {
        let mut query = consent_records::table
            .into_boxed()
            .filter(false.into_sql::<diesel::sql_types::Bool>());
        let primary_keys: Vec<_> = records
            .iter()
            .map(|r| (&r.entity, &r.entity_type))
            .collect();
        for (entity, entity_type) in primary_keys {
            query = query.or_filter(
                consent_records::entity_type
                    .eq(entity_type)
                    .and(consent_records::entity.eq(entity)),
            );
        }

        let changed = self.raw_query(|conn| {
            let existing: Vec<StoredConsentRecord> = query.load(conn)?;
            let changed: Vec<_> = records
                .iter()
                .filter(|r| !existing.contains(r))
                .cloned()
                .collect();

            conn.transaction::<_, diesel::result::Error, _>(|conn| {
                for record in records.iter() {
                    diesel::insert_into(dsl::consent_records)
                        .values(record)
                        .on_conflict((dsl::entity_type, dsl::entity))
                        .do_update()
                        .set(dsl::state.eq(excluded(dsl::state)))
                        .execute(conn)?;
                }
                Ok(())
            })?;

            Ok(changed)
        })?;

        Ok(changed)
    }

    async fn maybe_insert_consent_record_return_existing(
        &self,
        record: &StoredConsentRecord,
    ) -> Result<Option<StoredConsentRecord>, crate::ConnectionError> {
        self.raw_query(|conn| {
            let maybe_inserted_consent_record: Option<StoredConsentRecord> =
                diesel::insert_into(dsl::consent_records)
                    .values(record)
                    .on_conflict_do_nothing()
                    .get_result(conn)
                    .optional()?;

            // if record was not inserted...
            if maybe_inserted_consent_record.is_none() {
                return dsl::consent_records
                    .find((&record.entity_type, &record.entity))
                    .first(conn)
                    .optional();
            }

            Ok(None)
        })
    }

    async fn find_consent_by_dm_id(
        &self,
        dm_id: &str,
    ) -> Result<Vec<StoredConsentRecord>, crate::ConnectionError> {
        self.raw_query(|conn| {
            // First, get all group IDs for this dm_id
            let group_ids: Vec<Vec<u8>> = groups_dsl::groups
                .filter(groups_dsl::dm_id.eq(dm_id))
                .select(groups_dsl::id)
                .load::<Vec<u8>>(conn)?;

            // Convert to hex strings
            let group_id_hexes: Vec<String> = group_ids.iter().map(hex::encode).collect();

            // Query consent records
            dsl::consent_records
                .filter(dsl::entity.eq_any(group_id_hexes))
                .filter(dsl::entity_type.eq(ConsentType::ConversationId))
                .order(dsl::consented_at_ns.desc())
                .load::<StoredConsentRecord>(conn)
        })
    }
}

impl<T: QueryConsentRecord + ?Sized + xmtp_common::MaybeSync> QueryConsentRecord for &T {
    async fn get_consent_record(
        &self,
        entity: String,
        entity_type: ConsentType,
    ) -> Result<Option<StoredConsentRecord>, crate::ConnectionError> {
        (**self).get_consent_record(entity, entity_type).await
    }

    async fn consent_records(&self) -> Result<Vec<StoredConsentRecord>, crate::ConnectionError> {
        (**self).consent_records().await
    }

    async fn consent_records_paged(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<StoredConsentRecord>, crate::ConnectionError> {
        (**self).consent_records_paged(limit, offset).await
    }

    async fn insert_newer_consent_record(
        &self,
        record: StoredConsentRecord,
    ) -> Result<bool, crate::ConnectionError> {
        (**self).insert_newer_consent_record(record).await
    }

    async fn insert_or_replace_consent_records(
        &self,
        records: &[StoredConsentRecord],
    ) -> Result<Vec<StoredConsentRecord>, crate::ConnectionError> {
        (**self).insert_or_replace_consent_records(records).await
    }

    async fn maybe_insert_consent_record_return_existing(
        &self,
        record: &StoredConsentRecord,
    ) -> Result<Option<StoredConsentRecord>, crate::ConnectionError> {
        (**self)
            .maybe_insert_consent_record_return_existing(record)
            .await
    }

    async fn find_consent_by_dm_id(
        &self,
        dm_id: &str,
    ) -> Result<Vec<StoredConsentRecord>, crate::ConnectionError> {
        (**self).find_consent_by_dm_id(dm_id).await
    }
}

#[repr(i32)]
#[derive(Debug, Copy, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "sync", derive(AsExpression, FromSqlRow))]
#[cfg_attr(feature = "sync", diesel(sql_type = Integer))]
/// Type of consent record stored
pub enum ConsentType {
    /// Consent is for a conversation
    ConversationId = 1,
    /// Consent is for an inbox
    InboxId = 2,
}

/// sqlx backend -- Postgres only. See the note on `QueryGroupVersion`'s impl for
/// why this is gated `not(feature = "sync")`.
#[cfg(all(feature = "async", not(feature = "sync"), not(target_arch = "wasm32")))]
mod pg_impl {
    use super::*;
    use crate::pg::{PgDb, PgModel};

    /// Upsert that only moves `state`, matching the sync track's
    /// `do_update().set(state.eq(excluded(state)))` — `consented_at_ns` on an
    /// existing row is left alone.
    const UPSERT: &str = "INSERT INTO consent_records (entity_type, state, entity, consented_at_ns) \
                          VALUES ($1, $2, $3, $4) \
                          ON CONFLICT (entity_type, entity) DO UPDATE SET state = excluded.state";

    /// Decode via the `FromRow` that `#[derive(PgModel)]` emits: by column
    /// name, from the same fields the column list comes from.
    fn record(row: &sqlx::postgres::PgRow) -> Result<StoredConsentRecord, crate::ConnectionError> {
        use sqlx::FromRow;
        Ok(StoredConsentRecord::from_row(row)?)
    }

    async fn get(
        db: &PgDb,
        entity: &str,
        entity_type: ConsentType,
    ) -> Result<Option<StoredConsentRecord>, crate::ConnectionError> {
        let mut c = db.conn().await?;
        let row = sqlx::query(&format!(
            "SELECT {} FROM consent_records WHERE entity = $1 AND entity_type = $2",
            StoredConsentRecord::select_columns()
        ))
        .bind(entity)
        .bind(entity_type)
        .fetch_optional(&mut *c)
        .await?;
        row.as_ref().map(record).transpose()
    }

    impl QueryConsentRecord for PgDb {
        async fn get_consent_record(
            &self,
            entity: String,
            entity_type: ConsentType,
        ) -> Result<Option<StoredConsentRecord>, crate::ConnectionError> {
            get(self, &entity, entity_type).await
        }

        async fn consent_records(
            &self,
        ) -> Result<Vec<StoredConsentRecord>, crate::ConnectionError> {
            let mut c = self.conn().await?;
            let rows = sqlx::query(&format!(
                "SELECT {} FROM consent_records",
                StoredConsentRecord::select_columns()
            ))
            .fetch_all(&mut *c)
            .await?;
            rows.iter().map(record).collect()
        }

        async fn consent_records_paged(
            &self,
            limit: i64,
            offset: i64,
        ) -> Result<Vec<StoredConsentRecord>, crate::ConnectionError> {
            let mut c = self.conn().await?;
            let rows = sqlx::query(&format!(
                "SELECT {} FROM consent_records \
                 ORDER BY entity_type, entity LIMIT $1 OFFSET $2",
                StoredConsentRecord::select_columns()
            ))
            .bind(limit)
            .bind(offset)
            .fetch_all(&mut *c)
            .await?;
            rows.iter().map(record).collect()
        }

        /// Returns whether the store now reflects `record` — true if it was
        /// inserted, or replaced an older one; false if an equal or newer record
        /// was already there.
        ///
        /// Runs atomically, unlike the sync track: the read-back and conditional
        /// replace are separate statements, and on a server two clients can race
        /// between them.
        async fn insert_newer_consent_record(
            &self,
            new: StoredConsentRecord,
        ) -> Result<bool, crate::ConnectionError> {
            self.atomic(async |db| {
                let existing = get(db, &new.entity, new.entity_type).await?;
                let Some(old) = existing else {
                    let mut c = db.conn().await?;
                    sqlx::query(UPSERT)
                        .bind(new.entity_type)
                        .bind(new.state)
                        .bind(&new.entity)
                        .bind(new.consented_at_ns)
                        .execute(&mut *c)
                        .await?;
                    return Ok(true);
                };

                // `PartialEq` here ignores `consented_at_ns` on purpose: an
                // identical decision restated later is not a change.
                if old == new {
                    return Ok(false);
                }

                let should_replace = old.consented_at_ns < new.consented_at_ns;
                if should_replace {
                    let mut c = db.conn().await?;
                    sqlx::query(UPSERT)
                        .bind(new.entity_type)
                        .bind(new.state)
                        .bind(&new.entity)
                        .bind(new.consented_at_ns)
                        .execute(&mut *c)
                        .await?;
                }
                Ok(should_replace)
            })
            .await
        }

        /// Upserts every record and returns the subset that was new or changed.
        async fn insert_or_replace_consent_records(
            &self,
            records: &[StoredConsentRecord],
        ) -> Result<Vec<StoredConsentRecord>, crate::ConnectionError> {
            if records.is_empty() {
                return Ok(vec![]);
            }

            self.atomic(async |db| {
                // Which of these already exist, matched on the (entity_type,
                // entity) primary key as a pair rather than column-wise.
                let (types, entities): (Vec<ConsentType>, Vec<&str>) = records
                    .iter()
                    .map(|r| (r.entity_type, r.entity.as_str()))
                    .unzip();
                let types: Vec<i32> = types.iter().map(|t| *t as i32).collect();

                let existing: Vec<StoredConsentRecord> = {
                    let mut c = db.conn().await?;
                    let rows = sqlx::query(&format!(
                        "SELECT {} FROM consent_records \
                         WHERE (entity_type, entity) IN \
                               (SELECT * FROM UNNEST($1::int4[], $2::text[]))",
                        StoredConsentRecord::select_columns()
                    ))
                    .bind(&types)
                    .bind(&entities)
                    .fetch_all(&mut *c)
                    .await?;
                    rows.iter().map(record).collect::<Result<_, _>>()?
                };

                let changed: Vec<_> = records
                    .iter()
                    .filter(|r| !existing.contains(r))
                    .cloned()
                    .collect();

                let mut c = db.conn().await?;
                for r in records {
                    sqlx::query(UPSERT)
                        .bind(r.entity_type)
                        .bind(r.state)
                        .bind(&r.entity)
                        .bind(r.consented_at_ns)
                        .execute(&mut *c)
                        .await?;
                }
                Ok(changed)
            })
            .await
        }

        /// `None` when the record was inserted; `Some(existing)` when one was
        /// already there. The caller uses the distinction to detect a conflict,
        /// so the insert and the read-back must not race.
        async fn maybe_insert_consent_record_return_existing(
            &self,
            new: &StoredConsentRecord,
        ) -> Result<Option<StoredConsentRecord>, crate::ConnectionError> {
            self.atomic(async |db| {
                let inserted = {
                    let mut c = db.conn().await?;
                    sqlx::query(
                        "INSERT INTO consent_records \
                         (entity_type, state, entity, consented_at_ns) VALUES ($1, $2, $3, $4) \
                         ON CONFLICT DO NOTHING",
                    )
                    .bind(new.entity_type)
                    .bind(new.state)
                    .bind(&new.entity)
                    .bind(new.consented_at_ns)
                    .execute(&mut *c)
                    .await?
                    .rows_affected()
                        > 0
                };

                if inserted {
                    return Ok(None);
                }
                get(db, &new.entity, new.entity_type).await
            })
            .await
        }

        /// One round trip instead of the sync track's two: the group-id lookup
        /// becomes a subquery. `encode(id, 'hex')` matches Rust's `hex::encode`
        /// (lowercase, unpadded), which is what the entity column holds.
        async fn find_consent_by_dm_id(
            &self,
            dm_id: &str,
        ) -> Result<Vec<StoredConsentRecord>, crate::ConnectionError> {
            let mut c = self.conn().await?;
            let rows = sqlx::query(&format!(
                "SELECT {} FROM consent_records \
                 WHERE entity_type = $1 \
                   AND entity IN (SELECT encode(id, 'hex') FROM groups WHERE dm_id = $2) \
                 ORDER BY consented_at_ns DESC",
                StoredConsentRecord::select_columns()
            ))
            .bind(ConsentType::ConversationId)
            .bind(dm_id)
            .fetch_all(&mut *c)
            .await?;
            rows.iter().map(record).collect()
        }
    }
}

crate::impl_sql_int_enum!(ConsentType {
    ConversationId = 1,
    InboxId = 2,
});

#[repr(i32)]
#[derive(Debug, Copy, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "sync", derive(AsExpression, FromSqlRow))]
#[cfg_attr(feature = "sync", diesel(sql_type = Integer))]
/// The state of the consent
pub enum ConsentState {
    /// Consent is unknown
    Unknown = 0,
    /// Consent is allowed
    Allowed = 1,
    /// Consent is denied
    Denied = 2,
}

crate::impl_sql_int_enum!(ConsentState {
    Unknown = 0,
    Allowed = 1,
    Denied = 2,
});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Store, group::tests::generate_group, test_utils::with_connection};

    fn generate_consent_record(
        entity_type: ConsentType,
        state: ConsentState,
        entity: String,
    ) -> StoredConsentRecord {
        StoredConsentRecord {
            entity_type,
            state,
            entity,
            consented_at_ns: now_ns(),
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn find_consent_by_dm_id() {
        with_connection(async |conn| {
            let mut g = generate_group(None);
            g.dm_id = Some("dm:alpha:beta".to_string());
            g.store(conn).await?;

            let cr = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Allowed,
                hex::encode(g.id),
            );
            cr.store(conn).await?;

            let mut records = conn.find_consent_by_dm_id("dm:alpha:beta").await?;

            assert_eq!(records.len(), 1);
            assert_eq!(records.pop()?, cr);
        })
        .await
    }

    #[xmtp_common::test]
    async fn insert_and_read() {
        with_connection(async |conn| {
            let inbox_id = "inbox_1";
            let consent_record = generate_consent_record(
                ConsentType::InboxId,
                ConsentState::Allowed,
                inbox_id.to_string(),
            );
            let consent_record_entity = consent_record.entity.clone();

            // Insert the record
            let result = conn
                .insert_or_replace_consent_records(std::slice::from_ref(&consent_record))
                .await
                .expect("should store without error");
            // One record was inserted
            assert_eq!(result.len(), 1);

            // Insert it again
            let result = conn
                .insert_or_replace_consent_records(std::slice::from_ref(&consent_record))
                .await
                .expect("should store without error");
            // Nothing should change
            assert_eq!(result.len(), 0);

            // Insert it again, this time with a Denied state
            let result = conn
                .insert_or_replace_consent_records(&[StoredConsentRecord {
                    state: ConsentState::Denied,
                    ..consent_record
                }])
                .await
                .expect("should store without error");
            // Should change
            assert_eq!(result.len(), 1);

            let consent_record = conn
                .get_consent_record(inbox_id.to_owned(), ConsentType::InboxId)
                .await
                .expect("query should work");

            assert_eq!(consent_record.unwrap().entity, consent_record_entity);

            let conflict = generate_consent_record(
                ConsentType::InboxId,
                ConsentState::Allowed,
                inbox_id.to_string(),
            );

            let existing = conn
                .maybe_insert_consent_record_return_existing(&conflict)
                .await
                .unwrap();
            assert!(existing.is_some());
            let existing = existing.unwrap();
            // we want the old record to be returned.
            assert_eq!(existing.state, ConsentState::Denied);

            let db_cr = conn
                .get_consent_record(existing.entity, existing.entity_type)
                .await
                .unwrap()
                .unwrap();
            // ensure the db matches the state of what was returned
            assert_eq!(db_cr.state, existing.state);
        })
        .await
    }
}
