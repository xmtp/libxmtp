use super::{ConnectionExt, db_connection::DbConnection, schema::tasks};
use crate::StorageError;
use derive_builder::Builder;
use diesel::prelude::*;
use prost::Message;
use xmtp_common::{NS_IN_DAY, NS_IN_SEC, time::now_ns};
use xmtp_proto::types::GroupId;
use xmtp_proto::xmtp::mls::database::{KpMaintenance, Task as TaskProto, task::Task as TaskKind};

/// The single constant `KpMaintenance` task payload (an empty message). Because
/// the payload is constant and empty, its `data_hash` is constant, so the
/// `UNIQUE(data_hash)` constraint on `tasks` makes insert-or-ignore a natural
/// singleton seed.
pub fn kp_maintenance_task_proto() -> TaskProto {
    TaskProto {
        task: Some(TaskKind::KpMaintenance(KpMaintenance {})),
    }
}

/// The constant `data_hash` of the singleton `KpMaintenance` task. Because the
/// payload is constant, this hash is constant, so it can be used to target the
/// singleton row directly without first loading it by id.
fn kp_maintenance_data_hash() -> Vec<u8> {
    xmtp_common::sha256_bytes(&kp_maintenance_task_proto().encode_to_vec())
}

#[derive(Queryable, Identifiable, Debug, Clone)]
#[diesel(table_name = tasks)]
#[diesel(primary_key(id))]
pub struct Task {
    pub id: i32,
    pub originating_message_sequence_id: i64,
    pub originating_message_originator_id: i32,
    pub created_at_ns: i64,
    pub expires_at_ns: i64,
    pub attempts: i32,
    pub max_attempts: i32,
    pub last_attempted_at_ns: i64,
    pub backoff_scaling_factor: f32,
    pub max_backoff_duration_ns: i64,
    pub initial_backoff_duration_ns: i64,
    pub next_attempt_at_ns: i64,
    pub data_hash: Vec<u8>,
    pub data: Vec<u8>,
}

#[derive(Insertable, Debug, PartialEq, Clone, Builder)]
#[diesel(table_name = tasks)]
#[builder(build_fn(skip))]
pub struct NewTask {
    pub originating_message_sequence_id: i64,
    pub originating_message_originator_id: i32,
    pub created_at_ns: i64,
    pub expires_at_ns: i64,
    pub attempts: i32,
    pub max_attempts: i32,
    pub last_attempted_at_ns: i64,
    pub backoff_scaling_factor: f32,
    pub max_backoff_duration_ns: i64,
    pub initial_backoff_duration_ns: i64,
    pub next_attempt_at_ns: i64,
    #[builder(setter(skip))]
    pub data_hash: Vec<u8>,
    #[builder(setter(skip))]
    pub data: Vec<u8>,
}

impl NewTask {
    pub fn builder() -> NewTaskBuilder {
        NewTaskBuilder::default()
    }
}

impl NewTaskBuilder {
    pub fn build(&mut self, task: TaskProto) -> Result<NewTask, StorageError> {
        use derive_builder::UninitializedFieldError;
        let err = |s: &'static str| UninitializedFieldError::new(s);
        let data = task.encode_to_vec();
        let data_hash = xmtp_common::sha256_bytes(&data);
        let new_task = NewTask {
            originating_message_sequence_id: self
                .originating_message_sequence_id
                .ok_or_else(|| err("originating_message_sequence_id"))?,
            originating_message_originator_id: self
                .originating_message_originator_id
                .ok_or_else(|| err("originating_message_originator_id"))?,
            created_at_ns: self.created_at_ns.unwrap_or_else(now_ns),
            expires_at_ns: self
                .expires_at_ns
                .unwrap_or_else(|| now_ns() + NS_IN_DAY * 3),
            attempts: self.attempts.unwrap_or(0),
            max_attempts: self.max_attempts.unwrap_or(20),
            last_attempted_at_ns: self.last_attempted_at_ns.unwrap_or_else(now_ns),
            backoff_scaling_factor: self.backoff_scaling_factor.unwrap_or(1.5),
            max_backoff_duration_ns: self.max_backoff_duration_ns.unwrap_or(60 * NS_IN_SEC),
            initial_backoff_duration_ns: self.initial_backoff_duration_ns.unwrap_or(2 * NS_IN_SEC),
            next_attempt_at_ns: self.next_attempt_at_ns.unwrap_or_else(now_ns),
            data_hash,
            data,
        };
        Ok(new_task)
    }
}

// impl_store_or_ignore!(Task, tasks);

pub trait QueryTasks {
    fn create_task(&self, task: NewTask) -> Result<Task, StorageError>;

    fn get_tasks(&self) -> Result<Vec<Task>, StorageError>;

    fn get_next_task(&self) -> Result<Option<Task>, StorageError>;

    /// Ensure exactly one live `ProcessPendingSelfRemove` task exists for
    /// `group_id`. Clears only dead rows (expired / attempts-exhausted) then
    /// insert-or-ignores, so a live retrying task keeps its backoff and is never
    /// deleted out from under the TaskRunner, while a stale dead row can't block
    /// a fresh retry via the `data_hash` unique constraint.
    fn upsert_pending_self_remove_task(
        &self,
        group_id: &GroupId,
        task: NewTask,
    ) -> Result<(), StorageError>;

    /// Ensure exactly one live `KpMaintenance` task exists. In one txn: delete any
    /// DEAD KP row (`expires_at_ns < now` OR `attempts >= max_attempts`), then
    /// insert-or-ignore a fresh one (`next_attempt_at_ns = now`,
    /// `expires_at_ns = now + NS_IN_DAY`). The constant empty payload's constant
    /// `data_hash` plus `UNIQUE(data_hash)` makes this a self-healing singleton
    /// seed: a live row is left untouched (dedup), a dead row is cleared+replaced.
    fn ensure_kp_maintenance_task(&self) -> Result<(), StorageError>;

    /// Re-arm the recurring KP task ATOMICALLY: in ONE transaction, re-read the live
    /// KP deadline (MIN of identity.next_key_package_rotation_ns and
    /// MIN(key_package_history.delete_at_ns)) and write
    /// next_attempt_at_ns = MIN(fallback_next_ns, fresh), plus attempts = 0 and
    /// renewed expires_at_ns. Closes the read->write lost-rotation race.
    fn reschedule_kp_task(
        &self,
        id: i32,
        fallback_next_ns: i64,
        expires_at_ns: i64,
    ) -> Result<Task, StorageError>;

    /// Nudge the KP maintenance task to run at `at` — only when it is NOT mid-backoff
    /// (attempts == 0). If no KP row exists (a prior seed failed), lazily ensure one.
    fn bump_kp_maintenance_task(&self, at: i64) -> Result<(), StorageError>;

    fn update_task(
        &self,
        id: i32,
        attempts: i32,
        last_attempted_at_ns: i64,
        next_attempt_at_ns: i64,
    ) -> Result<Task, StorageError>;

    fn delete_task(&self, id: i32) -> Result<bool, StorageError>;
}

impl<T: QueryTasks> QueryTasks for &'_ T {
    fn create_task(&self, task: NewTask) -> Result<Task, StorageError> {
        (**self).create_task(task)
    }

    fn get_tasks(&self) -> Result<Vec<Task>, StorageError> {
        (**self).get_tasks()
    }

    fn get_next_task(&self) -> Result<Option<Task>, StorageError> {
        (**self).get_next_task()
    }

    fn upsert_pending_self_remove_task(
        &self,
        group_id: &GroupId,
        task: NewTask,
    ) -> Result<(), StorageError> {
        (**self).upsert_pending_self_remove_task(group_id, task)
    }

    fn ensure_kp_maintenance_task(&self) -> Result<(), StorageError> {
        (**self).ensure_kp_maintenance_task()
    }

    fn reschedule_kp_task(
        &self,
        id: i32,
        fallback_next_ns: i64,
        expires_at_ns: i64,
    ) -> Result<Task, StorageError> {
        (**self).reschedule_kp_task(id, fallback_next_ns, expires_at_ns)
    }

    fn bump_kp_maintenance_task(&self, at: i64) -> Result<(), StorageError> {
        (**self).bump_kp_maintenance_task(at)
    }

    fn update_task(
        &self,
        id: i32,
        attempts: i32,
        last_attempted_at_ns: i64,
        next_attempt_at_ns: i64,
    ) -> Result<Task, StorageError> {
        (**self).update_task(id, attempts, last_attempted_at_ns, next_attempt_at_ns)
    }

    fn delete_task(&self, id: i32) -> Result<bool, StorageError> {
        (**self).delete_task(id)
    }
}

impl<C: ConnectionExt> QueryTasks for DbConnection<C> {
    fn create_task(&self, task: NewTask) -> Result<Task, StorageError> {
        self.raw_query(|conn| {
            diesel::insert_into(tasks::table)
                .values(task)
                .get_result::<Task>(conn)
        })
        .map_err(Into::into)
    }

    fn get_tasks(&self) -> Result<Vec<Task>, StorageError> {
        self.raw_query(|conn| tasks::table.load::<Task>(conn))
            .map_err(Into::into)
    }

    fn get_next_task(&self) -> Result<Option<Task>, StorageError> {
        self.raw_query(|conn| {
            tasks::table
                .order(tasks::next_attempt_at_ns)
                .first::<Task>(conn)
                .optional()
        })
        .map_err(Into::into)
    }

    fn upsert_pending_self_remove_task(
        &self,
        group_id: &GroupId,
        task: NewTask,
    ) -> Result<(), StorageError> {
        let now = now_ns();
        self.raw_query(|conn| {
            conn.transaction(|conn| {
                // Clear only DEAD rows for this group (expired or attempts
                // exhausted), then insert-or-ignore. We deliberately leave a LIVE
                // row untouched: deleting it would reset the TaskRunner's backoff
                // (resurrecting an intentionally-delayed task) and could race the
                // worker into calling update_task on a now-deleted id. The new
                // task carries the same data (group_id only), so the unique
                // data_hash constraint dedups it against any live row; clearing
                // dead rows first frees that hash so a fresh retry can take over.
                let rows: Vec<(i32, i32, i32, i64, Vec<u8>)> = tasks::table
                    .select((
                        tasks::id,
                        tasks::attempts,
                        tasks::max_attempts,
                        tasks::expires_at_ns,
                        tasks::data,
                    ))
                    .load(conn)?;
                for (id, attempts, max_attempts, expires_at_ns, data) in rows {
                    let is_self_remove = matches!(
                        TaskProto::decode(data.as_slice()).ok().and_then(|t| t.task),
                        Some(TaskKind::ProcessPendingSelfRemove(p)) if p.group_id == group_id.as_slice()
                    );
                    let is_dead = expires_at_ns < now || attempts >= max_attempts;
                    if is_self_remove && is_dead {
                        diesel::delete(tasks::table.filter(tasks::id.eq(id))).execute(conn)?;
                    }
                }
                diesel::insert_or_ignore_into(tasks::table)
                    .values(task)
                    .execute(conn)?;
                Ok(())
            })
        })
        .map_err(Into::into)
    }

    fn ensure_kp_maintenance_task(&self) -> Result<(), StorageError> {
        let now = now_ns();
        let task = NewTask::builder()
            .originating_message_sequence_id(0)
            .originating_message_originator_id(0)
            .next_attempt_at_ns(now)
            .expires_at_ns(now + NS_IN_DAY)
            .build(kp_maintenance_task_proto())?;
        self.raw_query(|conn| {
            conn.transaction(|conn| {
                // Clear only DEAD KpMaintenance rows (expired or attempts
                // exhausted), then insert-or-ignore a fresh one. A LIVE row is left
                // untouched so its TaskRunner backoff is never reset out from under
                // the worker; the constant empty payload shares the same data_hash,
                // so the unique constraint dedups against a live row, and clearing
                // dead rows first frees that hash for a fresh retry to take over.
                let rows: Vec<(i32, i32, i32, i64, Vec<u8>)> = tasks::table
                    .select((
                        tasks::id,
                        tasks::attempts,
                        tasks::max_attempts,
                        tasks::expires_at_ns,
                        tasks::data,
                    ))
                    .load(conn)?;
                for (id, attempts, max_attempts, expires_at_ns, data) in rows {
                    let is_kp_maintenance = matches!(
                        TaskProto::decode(data.as_slice()).ok().and_then(|t| t.task),
                        Some(TaskKind::KpMaintenance(_))
                    );
                    let is_dead = expires_at_ns < now || attempts >= max_attempts;
                    if is_kp_maintenance && is_dead {
                        diesel::delete(tasks::table.filter(tasks::id.eq(id))).execute(conn)?;
                    }
                }
                diesel::insert_or_ignore_into(tasks::table)
                    .values(task)
                    .execute(conn)?;
                Ok(())
            })
        })
        .map_err(Into::into)
    }

    fn reschedule_kp_task(
        &self,
        id: i32,
        fallback_next_ns: i64,
        expires_at_ns: i64,
    ) -> Result<Task, StorageError> {
        let now = now_ns();
        self.raw_query(|conn| {
            conn.transaction(|conn| {
                // Re-read the LIVE KP deadline INSIDE the same txn as the write.
                // We deliberately use inline diesel against this txn's `conn`
                // rather than the public readers (next_key_package_rotation_ns /
                // min_key_package_delete_at_ns): those re-enter `raw_query` and
                // grab a SECOND pooled connection, which would deadlock and read
                // outside this transaction's isolation. Doing the read here means
                // a rotation queued just before this txn is included in `fresh`
                // and cannot be clobbered to the far fallback deadline.
                use crate::schema::identity::dsl as id_dsl;
                use crate::schema::key_package_history::dsl as kph_dsl;
                use diesel::dsl::min;

                // The identity table may have zero rows in some states; `.optional()`
                // maps the empty-table NotFound to Ok(None) and `.flatten()` collapses
                // a present-but-NULL column, so `rotation` is None (never an error).
                let rotation: Option<i64> = id_dsl::identity
                    .select(id_dsl::next_key_package_rotation_ns)
                    .first::<Option<i64>>(conn)
                    .optional()?
                    .flatten();
                let delete: Option<i64> = kph_dsl::key_package_history
                    .filter(kph_dsl::delete_at_ns.is_not_null())
                    .select(min(kph_dsl::delete_at_ns))
                    .first::<Option<i64>>(conn)?;

                let fresh = [rotation, delete]
                    .into_iter()
                    .flatten()
                    .min()
                    .unwrap_or(fallback_next_ns);
                let next = fallback_next_ns.min(fresh);

                diesel::update(tasks::table.filter(tasks::id.eq(id)))
                    .set((
                        tasks::attempts.eq(0),
                        tasks::last_attempted_at_ns.eq(now),
                        tasks::expires_at_ns.eq(expires_at_ns),
                        tasks::next_attempt_at_ns.eq(next),
                    ))
                    .get_result::<Task>(conn)
            })
        })
        .map_err(Into::into)
    }

    fn bump_kp_maintenance_task(&self, at: i64) -> Result<(), StorageError> {
        let now = now_ns();
        let data_hash = kp_maintenance_data_hash();
        // Build the lazy-seed task up front so its StorageError-typed `?` happens
        // here, not inside the diesel txn closure (whose error is diesel::Error).
        let seed = NewTask::builder()
            .originating_message_sequence_id(0)
            .originating_message_originator_id(0)
            .next_attempt_at_ns(at)
            .expires_at_ns(now + NS_IN_DAY)
            .build(kp_maintenance_task_proto())?;
        self.raw_query(|conn| {
            conn.transaction(|conn| {
                // Find the singleton KP row by its constant data_hash. Read its
                // current (id, attempts, next_attempt_at_ns) so we can decide
                // whether to pull the deadline in, all inside this txn so a
                // concurrent backoff write can't slip between read and write.
                let existing: Option<(i32, i32, i64)> = tasks::table
                    .filter(tasks::data_hash.eq(&data_hash))
                    .select((tasks::id, tasks::attempts, tasks::next_attempt_at_ns))
                    .first::<(i32, i32, i64)>(conn)
                    .optional()?;
                match existing {
                    // Live row, idle: pull next_attempt in to MIN(existing, at).
                    // We never push it OUT, so a sooner deadline is preserved.
                    Some((id, 0, next)) => {
                        diesel::update(tasks::table.filter(tasks::id.eq(id)))
                            .set(tasks::next_attempt_at_ns.eq(next.min(at)))
                            .execute(conn)?;
                    }
                    // Row exists but is mid-backoff (attempts > 0): no-op, so a
                    // flood of welcomes can't trample an upload-outage schedule.
                    Some((_id, _attempts, _next)) => {}
                    // Missing (a prior startup seed failed): lazily insert a fresh
                    // singleton scheduled at `at`. Inlined here (not via
                    // ensure_kp_maintenance_task) to avoid nesting a second
                    // raw_query/connection inside this transaction. The fresh row
                    // has attempts == 0, so the same idle pull-in semantics apply.
                    None => {
                        diesel::insert_or_ignore_into(tasks::table)
                            .values(&seed)
                            .execute(conn)?;
                    }
                }
                Ok(())
            })
        })
        .map_err(Into::into)
    }

    fn update_task(
        &self,
        id: i32,
        attempts: i32,
        last_attempted_at_ns: i64,
        next_attempt_at_ns: i64,
    ) -> Result<Task, StorageError> {
        self.raw_query(|conn| {
            diesel::update(tasks::table.filter(tasks::id.eq(id)))
                .set((
                    tasks::attempts.eq(attempts),
                    tasks::last_attempted_at_ns.eq(last_attempted_at_ns),
                    tasks::next_attempt_at_ns.eq(next_attempt_at_ns),
                ))
                .get_result::<Task>(conn)
        })
        .map_err(Into::into)
    }

    fn delete_task(&self, id: i32) -> Result<bool, StorageError> {
        let num_deleted = self.raw_query(|conn| {
            diesel::delete(tasks::table.filter(tasks::id.eq(id))).execute(conn)
        })?;
        Ok(num_deleted == 1)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::test_utils::with_connection;

    #[xmtp_common::test]
    fn get_tasks_returns_empty_list_initially() {
        with_connection(|conn| {
            let tasks = conn.get_tasks().unwrap();
            assert!(tasks.is_empty());
        })
    }

    #[xmtp_common::test]
    fn update_task_returns_error_when_not_found() {
        with_connection(|conn| {
            // Try to update a task that doesn't exist
            let result = conn.update_task(999, 5, 1000, 2000);
            // The update should fail when the task doesn't exist
            assert!(result.is_err());
        })
    }

    #[xmtp_common::test]
    fn delete_task_returns_false_when_not_found() {
        with_connection(|conn| {
            let deleted = conn.delete_task(999).unwrap();
            assert!(!deleted);
        })
    }

    // Generate a random task data for testing to ensure that the hashes are unique
    fn gen_task_data() -> TaskProto {
        TaskProto {
            task: Some(
                xmtp_proto::xmtp::mls::database::task::Task::ProcessWelcomePointer(
                    xmtp_proto::xmtp::mls::message_contents::WelcomePointer {
                        version: Some(xmtp_proto::xmtp::mls::message_contents::welcome_pointer::Version::WelcomeV1Pointer(xmtp_proto::xmtp::mls::message_contents::welcome_pointer::WelcomeV1Pointer {
                            destination: xmtp_common::rand_vec::<32>(),
                            aead_type: xmtp_proto::xmtp::mls::message_contents::WelcomePointeeEncryptionAeadType::Chacha20Poly1305.into(),
                            encryption_key: xmtp_common::rand_vec::<32>(),
                            data_nonce: xmtp_common::rand_vec::<12>(),
                            welcome_metadata_nonce: xmtp_common::rand_vec::<12>(),
                        })),
                    },
                ),
            ),
        }
    }

    #[xmtp_common::test]
    fn all_task_operations_work_together() {
        with_connection(|conn| {
            let now = xmtp_common::time::now_ns();

            // 1. Create first task (should be next to run)
            let task1 = NewTaskBuilder::default()
                .originating_message_sequence_id(1)
                .originating_message_originator_id(1)
                .created_at_ns(now)
                .expires_at_ns(now + 3_600_000_000_000)
                .attempts(0)
                .max_attempts(5)
                .last_attempted_at_ns(0)
                .backoff_scaling_factor(1.5)
                .max_backoff_duration_ns(600_000_000_000)
                .initial_backoff_duration_ns(2_000_000_000)
                .next_attempt_at_ns(now + 1000) // Later attempt time
                .build(gen_task_data())
                .unwrap();

            // 2. Create second task (should be first to run)
            let task2 = NewTaskBuilder::default()
                .originating_message_sequence_id(2)
                .originating_message_originator_id(1)
                .created_at_ns(now)
                .expires_at_ns(now + 7_200_000_000_000) // 2 hours from now
                .attempts(0)
                .max_attempts(3)
                .last_attempted_at_ns(0)
                .backoff_scaling_factor(2.0)
                .max_backoff_duration_ns(300_000_000_000)
                .initial_backoff_duration_ns(1_000_000_000)
                .next_attempt_at_ns(now + 500) // Earlier attempt time - should be next
                .build(gen_task_data())
                .unwrap();

            // 3. Verify no tasks initially
            assert!(conn.get_next_task().unwrap().is_none());
            assert!(conn.get_tasks().unwrap().is_empty());

            // 4. Create both tasks
            let created_task1 = conn.create_task(task1).unwrap();
            let created_task2 = conn.create_task(task2).unwrap();

            let task1_id = created_task1.id;
            let task2_id = created_task2.id;
            assert!(task1_id >= 0, "task1_id: {task1_id}");
            assert!(task2_id >= 0, "task2_id: {task2_id}");
            assert_ne!(task1_id, task2_id);

            // 5. Verify both tasks appear in get_tasks
            let all_tasks = conn.get_tasks().unwrap();
            assert_eq!(all_tasks.len(), 2);

            // 6. Verify get_next_task returns the task with earlier next_attempt_at_ns (task2)
            let next_task = conn.get_next_task().unwrap();
            assert!(next_task.is_some());
            let next_task = next_task.unwrap();
            assert_eq!(next_task.id, task2_id);
            assert_eq!(next_task.next_attempt_at_ns, now + 500);

            // 7. Update task1 to have an even earlier next_attempt_at_ns
            let updated_task1 = conn
                .update_task(
                    task1_id,
                    1,          // attempts
                    now + 2000, // last_attempted_at_ns
                    now + 200,  // next_attempt_at_ns - now earliest
                )
                .unwrap();

            // Verify the update
            assert_eq!(updated_task1.id, task1_id);
            assert_eq!(updated_task1.attempts, 1);
            assert_eq!(updated_task1.next_attempt_at_ns, now + 200);

            // 8. Verify get_next_task now returns task1 (earliest next_attempt_at_ns)
            let next_task = conn.get_next_task().unwrap();
            assert!(next_task.is_some());
            let next_task = next_task.unwrap();
            assert_eq!(next_task.id, task1_id);
            assert_eq!(next_task.next_attempt_at_ns, now + 200);

            // 9. Verify both tasks appear in get_tasks with correct data
            let all_tasks_after_update = conn.get_tasks().unwrap();
            assert_eq!(all_tasks_after_update.len(), 2);

            // Find each task by ID
            let updated_task1_in_list = all_tasks_after_update
                .iter()
                .find(|t| t.id == task1_id)
                .unwrap();
            let task2_in_list = all_tasks_after_update
                .iter()
                .find(|t| t.id == task2_id)
                .unwrap();

            assert_eq!(updated_task1_in_list.attempts, 1);
            assert_eq!(updated_task1_in_list.next_attempt_at_ns, now + 200);
            assert_eq!(task2_in_list.attempts, 0);
            assert_eq!(task2_in_list.next_attempt_at_ns, now + 500);

            // 10. Delete task1
            let deleted = conn.delete_task(task1_id).unwrap();
            assert!(deleted);

            // 11. Verify get_next_task now returns task2
            let next_task = conn.get_next_task().unwrap();
            assert!(next_task.is_some());
            let next_task = next_task.unwrap();
            assert_eq!(next_task.id, task2_id);

            // 12. Verify only task2 remains in get_tasks
            let remaining_tasks = conn.get_tasks().unwrap();
            assert_eq!(remaining_tasks.len(), 1);
            assert_eq!(remaining_tasks[0].id, task2_id);

            // 13. Delete task2
            let deleted = conn.delete_task(task2_id).unwrap();
            assert!(deleted);

            // 14. Verify no tasks remain
            let all_tasks_after_delete = conn.get_tasks().unwrap();
            assert!(all_tasks_after_delete.is_empty());
            assert!(conn.get_next_task().unwrap().is_none());

            // 15. Verify delete returns false for non-existent task
            let deleted_again = conn.delete_task(task1_id).unwrap();
            assert!(!deleted_again);
        })
    }

    #[xmtp_common::test]
    fn ensure_kp_maintenance_task_is_singleton_and_self_heals() {
        use crate::test_utils::with_connection;
        with_connection(|conn| {
            conn.ensure_kp_maintenance_task().unwrap();
            conn.ensure_kp_maintenance_task().unwrap();
            assert_eq!(conn.get_tasks().unwrap().len(), 1); // idempotent singleton

            // Self-heal: force the single live row to be DEAD by expiring it in the
            // past, then re-ensure. The dead row must be cleared+replaced (not
            // ignored), leaving exactly one fresh, live row.
            conn.raw_query(|conn| {
                diesel::update(tasks::table)
                    .set(tasks::expires_at_ns.eq(now_ns() - 1))
                    .execute(conn)
            })
            .unwrap();

            conn.ensure_kp_maintenance_task().unwrap();
            let tasks = conn.get_tasks().unwrap();
            assert_eq!(tasks.len(), 1);
            assert!(
                tasks[0].expires_at_ns > now_ns(),
                "dead row should have been replaced by a fresh live one"
            );
        })
    }

    #[xmtp_common::test]
    fn reschedule_kp_task_floors_to_in_txn_deadline() {
        use crate::Store;
        use crate::identity::QueryIdentity;
        use crate::key_package_history::QueryKeyPackageHistory;
        use crate::test_utils::with_connection;
        use xmtp_common::time::now_ns;

        with_connection(|conn| {
            conn.ensure_kp_maintenance_task().unwrap();
            let id = conn.get_next_task().unwrap().unwrap().id;
            let now = now_ns();
            let far = now + 30 * xmtp_common::NS_IN_DAY;

            // (i) nothing pending -> stored next_attempt == far (advances).
            // There is no identity row and nothing queued for deletion, so the
            // live deadline is empty and `next` falls back to `far`. This proves
            // the re-arm ADVANCES instead of getting stuck on a past value.
            conn.reschedule_kp_task(id, far, far + xmtp_common::NS_IN_DAY)
                .unwrap();
            assert_eq!(
                conn.get_next_task().unwrap().unwrap().next_attempt_at_ns,
                far
            );

            // (ii) rotation pending sooner -> stored == near rotation (MIN(far, near)).
            // Seed the identity row exactly as `next_key_package_rotation_ns_reads_queue`
            // does (far-future deadline), then queue a rotation so the in-txn read
            // sees a NEAR deadline. The reschedule must floor to it.
            let near = now + xmtp_common::NS_IN_DAY; // sooner than `far`
            crate::identity::StoredIdentity::builder()
                .inbox_id(String::new())
                .installation_keys(xmtp_common::rand_vec::<24>())
                .credential_bytes(xmtp_common::rand_vec::<24>())
                .next_key_package_rotation_ns(Some(near))
                .build()
                .unwrap()
                .store(conn)
                .unwrap();
            assert_eq!(
                conn.next_key_package_rotation_ns().unwrap(),
                Some(near),
                "sanity: identity row seeded with near rotation deadline"
            );

            let task = conn
                .reschedule_kp_task(id, far, far + xmtp_common::NS_IN_DAY)
                .unwrap();
            assert_eq!(
                task.next_attempt_at_ns, near,
                "reschedule must floor to the live rotation deadline read in-txn"
            );
            assert_ne!(
                task.next_attempt_at_ns, far,
                "must NOT store the far fallback when a sooner rotation is live"
            );
            // attempts reset + expires renewed.
            assert_eq!(task.attempts, 0);
            assert_eq!(task.expires_at_ns, far + xmtp_common::NS_IN_DAY);

            // (iii) delete pending sooner than rotation -> stored == delete_at.
            // Store key packages and mark them for deletion; their `delete_at_ns`
            // is ~24h out. Re-seed the rotation deadline far in the future so the
            // delete deadline is the soonest, and assert the reschedule floors to
            // exactly that delete deadline.
            conn.raw_query(|conn| {
                use crate::schema::identity::dsl;
                diesel::update(dsl::identity)
                    .set(dsl::next_key_package_rotation_ns.eq(Some(far)))
                    .execute(conn)
            })
            .unwrap();
            conn.store_key_package_history_entry(xmtp_common::rand_vec::<24>(), None)
                .unwrap();
            conn.store_key_package_history_entry(xmtp_common::rand_vec::<24>(), None)
                .unwrap();
            // High id marks all existing entries; sets `delete_at_ns` ~24h out.
            conn.mark_key_package_before_id_to_be_deleted(i32::MAX)
                .unwrap();
            let delete_at = conn.min_key_package_delete_at_ns().unwrap().unwrap();
            assert!(
                delete_at < far,
                "sanity: delete deadline must be sooner than the far fallback/rotation"
            );

            let task = conn
                .reschedule_kp_task(id, far, far + xmtp_common::NS_IN_DAY)
                .unwrap();
            assert_eq!(
                task.next_attempt_at_ns, delete_at,
                "reschedule must floor to the soonest live deadline (the delete deadline)"
            );
        })
    }

    #[xmtp_common::test]
    fn bump_kp_maintenance_task_pulls_in_only_when_idle_and_lazy_seeds() {
        use crate::test_utils::with_connection;
        use xmtp_common::time::now_ns;
        with_connection(|conn| {
            let at = now_ns() + 5 * xmtp_common::NS_IN_SEC;
            // No row yet -> lazy seed creates exactly one.
            conn.bump_kp_maintenance_task(at).unwrap();
            assert_eq!(conn.get_tasks().unwrap().len(), 1);
            // attempts == 0 -> pulls next_attempt in to <= at.
            let t = conn.get_next_task().unwrap().unwrap();
            assert!(t.next_attempt_at_ns <= at);
            // Mid-backoff (attempts > 0, far next) -> bump is a no-op.
            let far = now_ns() + xmtp_common::NS_IN_DAY;
            conn.update_task(t.id, 3, now_ns(), far).unwrap();
            conn.bump_kp_maintenance_task(now_ns() + 5 * xmtp_common::NS_IN_SEC)
                .unwrap();
            assert_eq!(
                conn.get_next_task().unwrap().unwrap().next_attempt_at_ns,
                far
            );
        })
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn upsert_pending_self_remove_dedups_per_group() {
        use xmtp_proto::xmtp::mls::database::ProcessPendingSelfRemove;
        let build = |gid: &GroupId| {
            let proto = TaskProto {
                task: Some(TaskKind::ProcessPendingSelfRemove(
                    ProcessPendingSelfRemove {
                        group_id: gid.to_vec(),
                    },
                )),
            };
            NewTask::builder()
                .originating_message_sequence_id(0)
                .originating_message_originator_id(0)
                .build(proto)
                .unwrap()
        };
        with_connection(|conn| {
            // First upsert inserts; a second for the same group dedups, not piles up.
            conn.upsert_pending_self_remove_task(&GroupId::ONE, build(&GroupId::ONE))?;
            conn.upsert_pending_self_remove_task(&GroupId::ONE, build(&GroupId::ONE))?;
            assert_eq!(conn.get_tasks()?.len(), 1);

            // A different group gets its own task.
            conn.upsert_pending_self_remove_task(&GroupId::TWO, build(&GroupId::TWO))?;
            assert_eq!(conn.get_tasks()?.len(), 2);
        })
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn upsert_preserves_live_task_but_replaces_dead_one() {
        use xmtp_proto::xmtp::mls::database::ProcessPendingSelfRemove;
        let proto = |gid: &GroupId| TaskProto {
            task: Some(TaskKind::ProcessPendingSelfRemove(
                ProcessPendingSelfRemove {
                    group_id: gid.to_vec(),
                },
            )),
        };
        with_connection(|conn| {
            // A live task that has already retried twice and backed off.
            let now = now_ns();
            let live = NewTask::builder()
                .originating_message_sequence_id(0)
                .originating_message_originator_id(0)
                .attempts(2)
                .next_attempt_at_ns(now + NS_IN_DAY)
                .build(proto(&GroupId::ONE))?;
            conn.create_task(live)?;

            // Re-upsert must NOT reset its backoff: the live row is left in place.
            conn.upsert_pending_self_remove_task(&GroupId::ONE, {
                NewTask::builder()
                    .originating_message_sequence_id(0)
                    .originating_message_originator_id(0)
                    .next_attempt_at_ns(now)
                    .build(proto(&GroupId::ONE))?
            })?;
            let tasks = conn.get_tasks()?;
            assert_eq!(tasks.len(), 1);
            assert_eq!(tasks[0].attempts, 2);
            assert_eq!(tasks[0].next_attempt_at_ns, now + NS_IN_DAY);

            // A dead task (attempts exhausted) IS replaced with a fresh retry.
            let dead = NewTask::builder()
                .originating_message_sequence_id(0)
                .originating_message_originator_id(0)
                .attempts(20)
                .max_attempts(20)
                .build(proto(&GroupId::TWO))?;
            conn.create_task(dead)?;
            conn.upsert_pending_self_remove_task(&GroupId::TWO, {
                NewTask::builder()
                    .originating_message_sequence_id(0)
                    .originating_message_originator_id(0)
                    .attempts(0)
                    .build(proto(&GroupId::TWO))?
            })?;
            let two: Vec<_> = conn
                .get_tasks()?
                .into_iter()
                .filter(|t| {
                    matches!(
                        TaskProto::decode(t.data.as_slice()).ok().and_then(|p| p.task),
                        Some(TaskKind::ProcessPendingSelfRemove(p)) if p.group_id == GroupId::TWO.as_slice()
                    )
                })
                .collect();
            assert_eq!(two.len(), 1);
            assert_eq!(two[0].attempts, 0);
        })
    }
}
