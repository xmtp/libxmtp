use super::ConnectionExt;
use super::{
    db_connection::DbConnection,
    schema::fork_tester::{self, dsl},
};
use crate::impl_store;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};

/// StoredForkTester holds fork testing configuration for a group
#[derive(Insertable, Queryable, Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[diesel(table_name = fork_tester)]
#[diesel(primary_key(group_id))]
pub struct StoredForkTester {
    /// The group ID (BLOB)
    pub group_id: Vec<u8>,
    /// Whether to fork the next commit
    pub fork_next_commit: bool,
}

impl StoredForkTester {
    pub fn new(group_id: Vec<u8>, fork_next_commit: bool) -> Self {
        Self {
            group_id,
            fork_next_commit,
        }
    }
}

impl_store!(StoredForkTester, fork_tester);

pub trait QueryForkTester {
    /// Gets the fork tester record for the given group ID
    fn get_fork_tester(
        &self,
        group_id: &[u8],
    ) -> Result<Option<StoredForkTester>, crate::ConnectionError>;

    /// Insert or update fork tester record
    fn insert_or_update_fork_tester(
        &self,
        record: StoredForkTester,
    ) -> Result<(), crate::ConnectionError>;

    /// Delete fork tester record for the given group ID
    fn delete_fork_tester(&self, group_id: &[u8]) -> Result<bool, crate::ConnectionError>;

    /// Update the fork_next_commit flag for a group
    fn set_fork_next_commit(
        &self,
        group_id: &[u8],
        fork_next_commit: bool,
    ) -> Result<(), crate::ConnectionError>;
}

impl<C: ConnectionExt> QueryForkTester for DbConnection<C> {
    /// Gets the fork tester record for the given group ID
    fn get_fork_tester(
        &self,
        group_id: &[u8],
    ) -> Result<Option<StoredForkTester>, crate::ConnectionError> {
        self.raw_query_read(|conn| {
            dsl::fork_tester
                .filter(dsl::group_id.eq(group_id))
                .first(conn)
                .optional()
        })
    }

    /// Insert or update fork tester record
    fn insert_or_update_fork_tester(
        &self,
        record: StoredForkTester,
    ) -> Result<(), crate::ConnectionError> {
        self.raw_query_write(|conn| {
            diesel::insert_into(dsl::fork_tester)
                .values(&record)
                .on_conflict(dsl::group_id)
                .do_update()
                .set(dsl::fork_next_commit.eq(&record.fork_next_commit))
                .execute(conn)
        })?;
        Ok(())
    }

    /// Delete fork tester record for the given group ID
    fn delete_fork_tester(&self, group_id: &[u8]) -> Result<bool, crate::ConnectionError> {
        let num_deleted = self.raw_query_write(|conn| {
            diesel::delete(dsl::fork_tester.filter(dsl::group_id.eq(group_id))).execute(conn)
        })?;
        Ok(num_deleted > 0)
    }

    /// Update the fork_next_commit flag for a group
    fn set_fork_next_commit(
        &self,
        group_id: &[u8],
        fork_next_commit: bool,
    ) -> Result<(), crate::ConnectionError> {
        self.insert_or_update_fork_tester(StoredForkTester::new(
            group_id.to_vec(),
            fork_next_commit,
        ))?;
        Ok(())
    }
}

impl<T: QueryForkTester> QueryForkTester for &'_ T {
    fn get_fork_tester(
        &self,
        group_id: &[u8],
    ) -> Result<Option<StoredForkTester>, crate::ConnectionError> {
        (**self).get_fork_tester(group_id)
    }

    fn insert_or_update_fork_tester(
        &self,
        record: StoredForkTester,
    ) -> Result<(), crate::ConnectionError> {
        (**self).insert_or_update_fork_tester(record)
    }

    fn delete_fork_tester(&self, group_id: &[u8]) -> Result<bool, crate::ConnectionError> {
        (**self).delete_fork_tester(group_id)
    }

    fn set_fork_next_commit(
        &self,
        group_id: &[u8],
        fork_next_commit: bool,
    ) -> Result<(), crate::ConnectionError> {
        (**self).set_fork_next_commit(group_id, fork_next_commit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::with_connection;
    use xmtp_common::rand_vec;

    #[xmtp_common::test]
    async fn test_fork_tester_crud() {
        with_connection(|conn| {
            let group_id = rand_vec::<32>();

            // Test that no record exists initially
            let result = conn.get_fork_tester(&group_id).unwrap();
            assert!(result.is_none());

            // Test insert
            let fork_tester = StoredForkTester::new(group_id.clone(), true);
            conn.insert_or_update_fork_tester(fork_tester.clone())
                .unwrap();

            // Test get
            let result = conn.get_fork_tester(&group_id).unwrap();
            assert!(result.is_some());
            let stored = result.unwrap();
            assert_eq!(stored.group_id, group_id);
            assert!(stored.fork_next_commit);

            // Test update
            conn.set_fork_next_commit(&group_id, false).unwrap();
            let result = conn.get_fork_tester(&group_id).unwrap().unwrap();
            assert!(!result.fork_next_commit);

            // Test delete
            let deleted = conn.delete_fork_tester(&group_id).unwrap();
            assert!(deleted);

            // Verify deletion
            let result = conn.get_fork_tester(&group_id).unwrap();
            assert!(result.is_none());

            // Test delete non-existent
            let deleted = conn.delete_fork_tester(&group_id).unwrap();
            assert!(!deleted);
        })
        .await
    }
}
