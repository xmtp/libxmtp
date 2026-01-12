use super::{ConnectionExt, db_connection::DbConnection, schema::tasks};
use crate::StorageError;
use derive_builder::Builder;
use diesel::prelude::*;
use prost::Message;
use xmtp_common::{NS_IN_DAY, NS_IN_MIN, NS_IN_SEC, time::now_ns};
use xmtp_proto::xmtp::mls::database::Task as TaskProto;

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
            created_at_ns: self.created_at_ns.unwrap_or_else(xmtp_common::time::now_ns),
            expires_at_ns: self
                .expires_at_ns
                .unwrap_or_else(|| now_ns() + NS_IN_DAY * 3),
            attempts: self.attempts.unwrap_or(0),
            max_attempts: self.max_attempts.unwrap_or(20),
            last_attempted_at_ns: self
                .last_attempted_at_ns
                .unwrap_or_else(xmtp_common::time::now_ns),
            backoff_scaling_factor: self.backoff_scaling_factor.unwrap_or(1.5),
            max_backoff_duration_ns: self.max_backoff_duration_ns.unwrap_or(60 * NS_IN_SEC),
            initial_backoff_duration_ns: self.initial_backoff_duration_ns.unwrap_or(2 * NS_IN_SEC),
            next_attempt_at_ns: self
                .next_attempt_at_ns
                .unwrap_or_else(|| now_ns() + NS_IN_MIN * 5),
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
        self.raw_query_write(|conn| {
            diesel::insert_into(tasks::table)
                .values(task)
                .get_result::<Task>(conn)
        })
        .map_err(Into::into)
    }

    fn get_tasks(&self) -> Result<Vec<Task>, StorageError> {
        self.raw_query_read(|conn| tasks::table.load::<Task>(conn))
            .map_err(Into::into)
    }

    fn get_next_task(&self) -> Result<Option<Task>, StorageError> {
        self.raw_query_read(|conn| {
            tasks::table
                .order(tasks::next_attempt_at_ns)
                .first::<Task>(conn)
                .optional()
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
        self.raw_query_write(|conn| {
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
        let num_deleted = self.raw_query_write(|conn| {
            diesel::delete(tasks::table.filter(tasks::id.eq(id))).execute(conn)
        })?;
        Ok(num_deleted == 1)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

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
}
