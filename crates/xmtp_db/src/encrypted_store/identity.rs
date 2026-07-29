use crate::StorageError;
#[cfg(feature = "sync")]
use crate::encrypted_store::schema::identity;
#[cfg(feature = "sync")]
use crate::schema::identity::dsl;
#[cfg(feature = "sync")]
use crate::{ConnectionExt, DbConnection, impl_fetch, impl_store};
use derive_builder::Builder;
#[cfg(feature = "sync")]
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use xmtp_common::time::now_ns;
use xmtp_configuration::KEY_PACKAGE_QUEUE_INTERVAL_NS;

/// Identity of this installation
/// There can only be one.
#[derive(Debug, Clone, Builder, Serialize, Deserialize)]
#[cfg_attr(feature = "sync", derive(Insertable, Queryable))]
#[cfg_attr(feature = "sync", diesel(table_name = identity))]
#[builder(setter(into), build_fn(error = "crate::StorageError"))]
pub struct StoredIdentity {
    pub inbox_id: String,
    pub installation_keys: Vec<u8>,
    pub credential_bytes: Vec<u8>,
    #[builder(setter(skip))]
    rowid: Option<i32>,
    pub next_key_package_rotation_ns: Option<i64>,
    #[builder(default)]
    pub registration_cursor_originator_id: Option<i64>,
    #[builder(default)]
    pub registration_cursor_sequence_id: Option<i64>,
}

#[cfg(feature = "sync")]
impl_fetch!(StoredIdentity, identity);
#[cfg(feature = "sync")]
impl_store!(StoredIdentity, identity);

impl StoredIdentity {
    pub fn builder() -> StoredIdentityBuilder {
        StoredIdentityBuilder::default()
    }

    pub fn new(inbox_id: String, installation_keys: Vec<u8>, credential_bytes: Vec<u8>) -> Self {
        Self {
            inbox_id,
            installation_keys,
            credential_bytes,
            rowid: None,
            next_key_package_rotation_ns: None,
            registration_cursor_originator_id: None,
            registration_cursor_sequence_id: None,
        }
    }
}
#[maybe_async::maybe_async(AFIT)]
pub trait QueryIdentity {
    async fn queue_key_package_rotation(&self) -> Result<(), StorageError>;
    /// Atomically lower/initialize the rotation column (5s debounce) AND enqueue a
    /// `PullInDeadline` task targeting `rotation_task_hash` at the resulting column
    /// value — one transaction, so neither write can land without the other.
    /// `rotation_seed` is insert-or-ignored first so the pull-in always has a live
    /// target (commit-target-first), even if startup seeding never ran.
    /// Callers wake the TaskWorker AFTER this returns (never inside a tx).
    async fn queue_key_rotation_with_nudge(
        &self,
        rotation_task_hash: &crate::tasks::TaskDataHash,
        rotation_seed: crate::tasks::NewTask,
    ) -> Result<(), StorageError>;
    async fn reset_key_package_rotation_queue(
        &self,
        rotation_interval_ns: i64,
    ) -> Result<(), StorageError>;
    async fn is_identity_needs_rotation(&self) -> Result<bool, StorageError>;
    /// The identity's absolute rotation deadline (`next_key_package_rotation_ns`).
    /// `None` if NULL or if no identity row exists yet (indistinguishable to callers;
    /// treat as "no scheduled deadline").
    async fn next_key_package_rotation_ns(&self) -> Result<Option<i64>, StorageError>;
}

#[maybe_async::maybe_async(AFIT)]
impl<T> QueryIdentity for &T
where
    T: QueryIdentity,
{
    async fn queue_key_package_rotation(&self) -> Result<(), StorageError> {
        (**self).queue_key_package_rotation().await
    }

    async fn queue_key_rotation_with_nudge(
        &self,
        rotation_task_hash: &crate::tasks::TaskDataHash,
        rotation_seed: crate::tasks::NewTask,
    ) -> Result<(), StorageError> {
        (**self)
            .queue_key_rotation_with_nudge(rotation_task_hash, rotation_seed)
            .await
    }

    async fn reset_key_package_rotation_queue(
        &self,
        rotation_interval_ns: i64,
    ) -> Result<(), StorageError> {
        (**self)
            .reset_key_package_rotation_queue(rotation_interval_ns)
            .await
    }

    async fn is_identity_needs_rotation(&self) -> Result<bool, StorageError> {
        (**self).is_identity_needs_rotation().await
    }

    async fn next_key_package_rotation_ns(&self) -> Result<Option<i64>, StorageError> {
        (**self).next_key_package_rotation_ns().await
    }
}

#[cfg(feature = "sync")]
impl<C: ConnectionExt> QueryIdentity for DbConnection<C> {
    fn queue_key_package_rotation(&self) -> Result<(), StorageError> {
        self.raw_query(|conn| {
            let rotate_at_ns = now_ns() + KEY_PACKAGE_QUEUE_INTERVAL_NS;
            // NULL (migrated DBs) counts as unscheduled: initialize it here so the
            // 5s debounce applies and nudge payloads stay stable (coalescing).
            diesel::update(dsl::identity)
                .filter(
                    dsl::next_key_package_rotation_ns
                        .gt(rotate_at_ns)
                        .or(dsl::next_key_package_rotation_ns.is_null()),
                )
                .set(dsl::next_key_package_rotation_ns.eq(rotate_at_ns))
                .execute(conn)?;

            Ok(())
        })?;

        Ok(())
    }

    fn queue_key_rotation_with_nudge(
        &self,
        rotation_task_hash: &crate::tasks::TaskDataHash,
        rotation_seed: crate::tasks::NewTask,
    ) -> Result<(), StorageError> {
        use crate::schema::tasks;
        use diesel::Connection;
        use xmtp_proto::xmtp::mls::database::{PullInDeadline, Task as TaskProto, task::Task};

        let hash = rotation_task_hash.to_vec();
        self.raw_query(|conn| {
            conn.transaction::<_, diesel::result::Error, _>(|conn| {
                let rotate_at_ns = now_ns() + KEY_PACKAGE_QUEUE_INTERVAL_NS;
                diesel::update(dsl::identity)
                    .filter(
                        dsl::next_key_package_rotation_ns
                            .gt(rotate_at_ns)
                            .or(dsl::next_key_package_rotation_ns.is_null()),
                    )
                    .set(dsl::next_key_package_rotation_ns.eq(rotate_at_ns))
                    .execute(conn)?;

                // Read back inside the tx: the column is stable between rotations,
                // so repeat calls produce byte-identical pull-ins that coalesce.
                let deadline: Option<Option<i64>> = dsl::identity
                    .select(dsl::next_key_package_rotation_ns)
                    .first::<Option<i64>>(conn)
                    .optional()?;
                // Pre-registration (no identity row): match the old zero-rows-
                // matched no-op instead of erroring; nothing to rotate yet.
                let Some(deadline) = deadline else {
                    return Ok(());
                };

                // Ensure the pull-in's target exists (no-op when already seeded):
                // a client whose startup seeding never ran must not enqueue a
                // dropped-on-miss nudge.
                diesel::insert_or_ignore_into(tasks::table)
                    .values(rotation_seed)
                    .execute(conn)?;

                let pull_in = crate::tasks::NewTask::builder()
                    .originating_message_sequence_id(0)
                    .originating_message_originator_id(0)
                    .expires_at_ns(crate::tasks::NEVER_EXPIRES)
                    .max_attempts(i32::MAX)
                    .build(TaskProto {
                        task: Some(Task::PullInDeadline(PullInDeadline {
                            target_data_hash: hash,
                            not_later_than_ns: deadline.unwrap_or(rotate_at_ns),
                        })),
                    })
                    // All required builder fields are set above; unreachable.
                    .map_err(|_| diesel::result::Error::RollbackTransaction)?;
                diesel::insert_or_ignore_into(tasks::table)
                    .values(pull_in)
                    .execute(conn)?;
                Ok(())
            })
        })?;
        Ok(())
    }

    fn reset_key_package_rotation_queue(
        &self,
        rotation_interval_ns: i64,
    ) -> Result<(), StorageError> {
        use crate::schema::identity::dsl;

        self.raw_query(|conn| {
            diesel::update(dsl::identity)
                .filter(
                    dsl::next_key_package_rotation_ns
                        .is_null()
                        .or(dsl::next_key_package_rotation_ns.le(now_ns())),
                )
                .set(dsl::next_key_package_rotation_ns.eq(Some(now_ns() + rotation_interval_ns)))
                .execute(conn)?;
            Ok(())
        })?;

        Ok(())
    }

    fn is_identity_needs_rotation(&self) -> Result<bool, StorageError> {
        use crate::schema::identity::dsl;

        let next_rotation_opt: Option<Option<i64>> = self.raw_query(|conn| {
            dsl::identity
                .select(dsl::next_key_package_rotation_ns)
                .first::<Option<i64>>(conn)
                .optional()
        })?;

        Ok(match next_rotation_opt {
            // No identity row (pre-registration): nothing to rotate yet.
            None => false,
            // NULL column on an existing row: rotation is due now.
            Some(None) => true,
            Some(Some(rotate_at)) => now_ns() >= rotate_at,
        })
    }

    fn next_key_package_rotation_ns(&self) -> Result<Option<i64>, StorageError> {
        use crate::schema::identity::dsl;
        // Use optional() so an empty table (pre-registration) returns Ok(None).
        let v: Option<Option<i64>> = self.raw_query(|conn| {
            dsl::identity
                .select(dsl::next_key_package_rotation_ns)
                .first::<Option<i64>>(conn)
                .optional()
        })?;
        Ok(v.flatten())
    }
}

/// sqlx backend -- Postgres only. See the note on `QueryGroupVersion`'s impl for
/// why this is gated `not(feature = "sync")`.
#[cfg(all(feature = "async", not(feature = "sync"), not(target_arch = "wasm32")))]
mod pg_impl {
    use super::*;
    use crate::pg::PgDb;
    use crate::tasks::NewTask;

    /// The body of `queue_key_rotation_with_nudge`, factored out so it can run
    /// either inside a transaction this method opens or inside one the caller
    /// already holds.
    async fn nudge(
        db: &PgDb,
        rotation_task_hash: &crate::tasks::TaskDataHash,
        rotation_seed: NewTask,
    ) -> Result<(), StorageError> {
        use sqlx::Row;
        use xmtp_proto::xmtp::mls::database::{PullInDeadline, Task as TaskProto, task::Task};

        let mut c = db.conn().await?;
        let rotate_at_ns = now_ns() + KEY_PACKAGE_QUEUE_INTERVAL_NS;

        sqlx::query(
            "UPDATE identity SET next_key_package_rotation_ns = $1 \
             WHERE next_key_package_rotation_ns > $1 OR next_key_package_rotation_ns IS NULL",
        )
        .bind(rotate_at_ns)
        .execute(&mut *c)
        .await
        .map_err(crate::ConnectionError::from)?;

        // Read back inside the tx: the column is stable between rotations, so
        // repeat calls produce byte-identical pull-ins that coalesce.
        let row = sqlx::query("SELECT next_key_package_rotation_ns FROM identity LIMIT 1")
            .fetch_optional(&mut *c)
            .await
            .map_err(crate::ConnectionError::from)?;

        // Pre-registration (no identity row): match the diesel path's
        // zero-rows-matched no-op instead of erroring; nothing to rotate yet.
        let Some(row) = row else {
            return Ok(());
        };
        let deadline: Option<i64> = row.try_get(0).map_err(crate::ConnectionError::from)?;

        // Ensure the pull-in's target exists (no-op when already seeded): a
        // client whose startup seeding never ran must not enqueue a
        // dropped-on-miss nudge.
        crate::tasks::pg::insert_or_ignore(&mut c, &rotation_seed).await?;

        let pull_in = NewTask::builder()
            .originating_message_sequence_id(0)
            .originating_message_originator_id(0)
            .expires_at_ns(crate::tasks::NEVER_EXPIRES)
            .max_attempts(i32::MAX)
            .build(TaskProto {
                task: Some(Task::PullInDeadline(PullInDeadline {
                    target_data_hash: rotation_task_hash.to_vec(),
                    not_later_than_ns: deadline.unwrap_or(rotate_at_ns),
                })),
            })?;
        crate::tasks::pg::insert_or_ignore(&mut c, &pull_in).await?;
        Ok(())
    }

    impl QueryIdentity for PgDb {
        async fn queue_key_package_rotation(&self) -> Result<(), StorageError> {
            let rotate_at_ns = now_ns() + KEY_PACKAGE_QUEUE_INTERVAL_NS;
            let mut c = self.conn().await?;
            // NULL (migrated DBs) counts as unscheduled: initialize it here so
            // the debounce applies and nudge payloads stay stable (coalescing).
            sqlx::query(
                "UPDATE identity SET next_key_package_rotation_ns = $1 \
                 WHERE next_key_package_rotation_ns > $1 OR next_key_package_rotation_ns IS NULL",
            )
            .bind(rotate_at_ns)
            .execute(&mut *c)
            .await
            .map_err(crate::ConnectionError::from)?;
            Ok(())
        }

        async fn queue_key_rotation_with_nudge(
            &self,
            rotation_task_hash: &crate::tasks::TaskDataHash,
            rotation_seed: NewTask,
        ) -> Result<(), StorageError> {
            // The column update and both task inserts must land together.
            self.atomic(async |db| nudge(db, rotation_task_hash, rotation_seed).await)
                .await
        }

        async fn reset_key_package_rotation_queue(
            &self,
            rotation_interval_ns: i64,
        ) -> Result<(), StorageError> {
            let now = now_ns();
            let mut c = self.conn().await?;
            sqlx::query(
                "UPDATE identity SET next_key_package_rotation_ns = $1 \
                 WHERE next_key_package_rotation_ns IS NULL \
                    OR next_key_package_rotation_ns <= $2",
            )
            .bind(now + rotation_interval_ns)
            .bind(now)
            .execute(&mut *c)
            .await
            .map_err(crate::ConnectionError::from)?;
            Ok(())
        }

        async fn is_identity_needs_rotation(&self) -> Result<bool, StorageError> {
            Ok(match self.rotation_column().await? {
                // No identity row (pre-registration): nothing to rotate yet.
                None => false,
                // NULL column on an existing row: rotation is due now.
                Some(None) => true,
                Some(Some(rotate_at)) => now_ns() >= rotate_at,
            })
        }

        async fn next_key_package_rotation_ns(&self) -> Result<Option<i64>, StorageError> {
            Ok(self.rotation_column().await?.flatten())
        }
    }

    impl PgDb {
        /// `next_key_package_rotation_ns` as stored: the outer `Option` is "does
        /// an identity row exist", the inner one is the nullable column. Callers
        /// that collapse the two lose the pre-registration case.
        async fn rotation_column(&self) -> Result<Option<Option<i64>>, StorageError> {
            use sqlx::Row;
            let mut c = self.conn().await?;
            let row = sqlx::query("SELECT next_key_package_rotation_ns FROM identity LIMIT 1")
                .fetch_optional(&mut *c)
                .await
                .map_err(crate::ConnectionError::from)?;
            row.map(|r| r.try_get(0))
                .transpose()
                .map_err(crate::ConnectionError::from)
                .map_err(Into::into)
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::StoredIdentity;
    use crate::{Store, XmtpTestDb};
    use xmtp_common::rand_vec;

    /// A stand-in rotation seed for exercising `queue_key_rotation_with_nudge`
    /// (the real seed payload lives in xmtp_mls).
    fn test_rotation_seed() -> crate::tasks::NewTask {
        use xmtp_proto::xmtp::mls::database::{KpRotation, Task as TaskProto, task::Task};
        crate::tasks::NewTask::builder()
            .originating_message_sequence_id(0)
            .originating_message_originator_id(0)
            .expires_at_ns(crate::tasks::NEVER_EXPIRES)
            .max_attempts(i32::MAX)
            .next_attempt_at_ns(0)
            .build(TaskProto {
                task: Some(Task::KpRotation(KpRotation {})),
            })
            .unwrap()
    }

    #[xmtp_common::test]
    fn queue_with_nudge_is_noop_before_registration() {
        use crate::prelude::{QueryIdentity, QueryTasks};
        use crate::test_utils::with_connection;
        with_connection(|conn| {
            // Empty identity table (pre-registration): must be a no-op like the
            // old column-only path, not a NotFound error. The seed must NOT be
            // inserted either — pre-registration means zero writes.
            let hash = crate::tasks::TaskDataHash::try_from([0x11u8; 32].as_slice()).unwrap();
            conn.queue_key_rotation_with_nudge(&hash, test_rotation_seed())
                .unwrap();
            assert!(
                conn.get_tasks().unwrap().is_empty(),
                "no pull-in without an identity row"
            );
        })
    }

    #[xmtp_common::test]
    fn queue_with_nudge_selfheals_missing_seed() {
        use crate::prelude::{QueryIdentity, QueryTasks};
        use crate::test_utils::with_connection;
        with_connection(|conn| {
            StoredIdentity::new("".to_string(), rand_vec::<24>(), rand_vec::<24>())
                .store(conn)
                .unwrap();
            let seed = test_rotation_seed();
            let hash = crate::tasks::TaskDataHash::try_from(seed.data_hash.as_slice()).unwrap();
            conn.queue_key_rotation_with_nudge(&hash, seed).unwrap();
            let tasks = conn.get_tasks().unwrap();
            assert!(
                tasks.iter().any(|t| t.data_hash == hash.as_ref()),
                "nudge must insert the missing rotation seed (pull-in target)"
            );
            assert_eq!(tasks.len(), 2, "seed + pull-in");
        })
    }

    #[xmtp_common::test]
    fn queue_initializes_null_rotation_column() {
        use crate::prelude::QueryIdentity;
        use crate::test_utils::with_connection;
        use xmtp_configuration::KEY_PACKAGE_QUEUE_INTERVAL_NS;
        with_connection(|conn| {
            StoredIdentity::new("".to_string(), rand_vec::<24>(), rand_vec::<24>())
                .store(conn)
                .unwrap();

            // Migrated DBs have NULL here; queueing must initialize it (5s
            // debounce) rather than skip the row.
            conn.queue_key_package_rotation().unwrap();
            let v = conn
                .next_key_package_rotation_ns()
                .unwrap()
                .expect("NULL column must be initialized");
            let now = xmtp_common::time::now_ns();
            assert!(v > now && v <= now + KEY_PACKAGE_QUEUE_INTERVAL_NS);

            // Lower-only: a later queue call never raises the deadline.
            conn.queue_key_package_rotation().unwrap();
            assert_eq!(conn.next_key_package_rotation_ns().unwrap().unwrap(), v);
        })
    }

    #[xmtp_common::test]
    async fn can_only_store_one_identity() {
        let store = crate::TestDb::create_ephemeral_store().await;
        let conn = &store.conn();

        StoredIdentity::new("".to_string(), rand_vec::<24>(), rand_vec::<24>())
            .store(conn)
            .unwrap();

        let duplicate_insertion =
            StoredIdentity::new("".to_string(), rand_vec::<24>(), rand_vec::<24>()).store(conn);
        assert!(duplicate_insertion.is_err());
    }
}
