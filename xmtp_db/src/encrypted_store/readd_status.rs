use std::collections::HashSet;

use diesel::prelude::*;

use super::{
    DbConnection,
    schema::readd_status::{self},
};
use crate::{ConnectionExt, impl_store};

#[derive(Identifiable, Queryable, Selectable, Insertable, Debug, Clone, PartialEq, Eq)]
#[diesel(table_name = readd_status)]
#[diesel(primary_key(group_id, installation_id))]
pub struct ReaddStatus {
    pub group_id: Vec<u8>,
    pub installation_id: Vec<u8>,
    pub requested_at_sequence_id: Option<i64>,
    pub responded_at_sequence_id: Option<i64>,
}

impl_store!(ReaddStatus, readd_status);

pub trait QueryReaddStatus {
    fn get_readd_status(
        &self,
        group_id: &[u8],
        installation_id: &[u8],
    ) -> Result<Option<ReaddStatus>, crate::ConnectionError>;

    fn is_awaiting_readd(
        &self,
        group_id: &[u8],
        installation_id: &[u8],
    ) -> Result<bool, crate::ConnectionError>;

    /// Update the requested_at_sequence_id for a given group_id and installation_id,
    /// provided it is higher than the current value.
    /// Inserts the row if it doesn't exist.
    fn update_requested_at_sequence_id(
        &self,
        group_id: &[u8],
        installation_id: &[u8],
        sequence_id: i64,
    ) -> Result<(), crate::ConnectionError>;

    /// Update the responded_at_sequence_id for a given group_id and installation_id,
    /// provided it is higher than the current value.
    /// Inserts the row if it doesn't exist.
    fn update_responded_at_sequence_id(
        &self,
        group_id: &[u8],
        installation_id: &[u8],
        sequence_id: i64,
    ) -> Result<(), crate::ConnectionError>;

    fn delete_other_readd_statuses(
        &self,
        group_id: &[u8],
        self_installation_id: &[u8],
    ) -> Result<(), crate::ConnectionError>;

    fn delete_readd_statuses(
        &self,
        group_id: &[u8],
        installation_ids: HashSet<Vec<u8>>,
    ) -> Result<(), crate::ConnectionError>;

    fn get_readds_awaiting_response(
        &self,
        group_id: &[u8],
        self_installation_id: &[u8],
    ) -> Result<Vec<ReaddStatus>, crate::ConnectionError>;
}

impl<C: ConnectionExt> QueryReaddStatus for DbConnection<C> {
    fn get_readd_status(
        &self,
        group_id: &[u8],
        installation_id: &[u8],
    ) -> Result<Option<ReaddStatus>, crate::ConnectionError> {
        use super::schema::readd_status::dsl as readd_dsl;
        use diesel::QueryDsl;

        self.raw_query_read(|conn| {
            readd_dsl::readd_status
                .filter(readd_dsl::group_id.eq(group_id))
                .filter(readd_dsl::installation_id.eq(installation_id))
                .first::<ReaddStatus>(conn)
                .optional()
        })
    }

    fn is_awaiting_readd(
        &self,
        group_id: &[u8],
        installation_id: &[u8],
    ) -> Result<bool, crate::ConnectionError> {
        let readd_status = self.get_readd_status(group_id, installation_id)?;
        if let Some(readd_status) = readd_status
            && let Some(requested_at) = readd_status.requested_at_sequence_id
            && requested_at >= readd_status.responded_at_sequence_id.unwrap_or(0)
        {
            return Ok(true);
        }
        Ok(false)
    }

    fn update_requested_at_sequence_id(
        &self,
        group_id: &[u8],
        installation_id: &[u8],
        sequence_id: i64,
    ) -> Result<(), crate::ConnectionError> {
        use super::schema::readd_status::dsl as readd_dsl;
        use diesel::query_dsl::methods::FilterDsl;

        let new_status = super::readd_status::ReaddStatus {
            group_id: group_id.to_vec(),
            installation_id: installation_id.to_vec(),
            requested_at_sequence_id: Some(sequence_id),
            responded_at_sequence_id: None,
        };

        self.raw_query_write(|conn| {
            diesel::insert_into(readd_dsl::readd_status)
                .values(&new_status)
                .on_conflict((readd_dsl::group_id, readd_dsl::installation_id))
                .do_update()
                .set(readd_dsl::requested_at_sequence_id.eq(sequence_id))
                .filter(
                    readd_dsl::requested_at_sequence_id
                        .is_null()
                        .or(readd_dsl::requested_at_sequence_id.lt(sequence_id)),
                )
                .execute(conn)
        })?;

        Ok(())
    }

    fn update_responded_at_sequence_id(
        &self,
        group_id: &[u8],
        installation_id: &[u8],
        sequence_id: i64,
    ) -> Result<(), crate::ConnectionError> {
        use super::schema::readd_status::dsl as readd_dsl;
        use diesel::query_dsl::methods::FilterDsl;

        let new_status = super::readd_status::ReaddStatus {
            group_id: group_id.to_vec(),
            installation_id: installation_id.to_vec(),
            requested_at_sequence_id: None,
            responded_at_sequence_id: Some(sequence_id),
        };

        self.raw_query_write(|conn| {
            diesel::insert_into(readd_dsl::readd_status)
                .values(&new_status)
                .on_conflict((readd_dsl::group_id, readd_dsl::installation_id))
                .do_update()
                .set(readd_dsl::responded_at_sequence_id.eq(sequence_id))
                .filter(
                    readd_dsl::responded_at_sequence_id
                        .is_null()
                        .or(readd_dsl::responded_at_sequence_id.lt(sequence_id)),
                )
                .execute(conn)
        })?;

        Ok(())
    }

    fn delete_other_readd_statuses(
        &self,
        group_id: &[u8],
        self_installation_id: &[u8],
    ) -> Result<(), crate::ConnectionError> {
        use super::schema::readd_status::dsl as readd_dsl;
        use diesel::{ExpressionMethods, QueryDsl};

        self.raw_query_write(|conn| {
            diesel::delete(
                readd_dsl::readd_status
                    .filter(readd_dsl::group_id.eq(group_id))
                    .filter(readd_dsl::installation_id.ne(self_installation_id)),
            )
            .execute(conn)?;
            Ok(())
        })
    }

    fn delete_readd_statuses(
        &self,
        group_id: &[u8],
        installation_ids: HashSet<Vec<u8>>,
    ) -> Result<(), crate::ConnectionError> {
        use super::schema::readd_status::dsl as readd_dsl;
        use diesel::{ExpressionMethods, QueryDsl};

        self.raw_query_write(|conn| {
            diesel::delete(
                readd_dsl::readd_status
                    .filter(readd_dsl::group_id.eq(group_id))
                    .filter(readd_dsl::installation_id.eq_any(installation_ids)),
            )
            .execute(conn)?;
            Ok(())
        })
    }

    fn get_readds_awaiting_response(
        &self,
        group_id: &[u8],
        self_installation_id: &[u8],
    ) -> Result<Vec<ReaddStatus>, crate::ConnectionError> {
        use super::schema::readd_status::dsl as readd_dsl;
        use diesel::{ExpressionMethods, QueryDsl};

        self.raw_query_read(|conn| {
            readd_dsl::readd_status
                .filter(readd_dsl::group_id.eq(group_id))
                .filter(readd_dsl::installation_id.ne(self_installation_id))
                .filter(readd_dsl::requested_at_sequence_id.is_not_null())
                .filter(
                    readd_dsl::requested_at_sequence_id
                        .ge(readd_dsl::responded_at_sequence_id)
                        .or(readd_dsl::responded_at_sequence_id.is_null()),
                )
                .load::<ReaddStatus>(conn)
        })
    }
}

impl<T> QueryReaddStatus for &T
where
    T: QueryReaddStatus,
{
    fn get_readd_status(
        &self,
        group_id: &[u8],
        installation_id: &[u8],
    ) -> Result<Option<ReaddStatus>, crate::ConnectionError> {
        (**self).get_readd_status(group_id, installation_id)
    }

    fn is_awaiting_readd(
        &self,
        group_id: &[u8],
        installation_id: &[u8],
    ) -> Result<bool, crate::ConnectionError> {
        (**self).is_awaiting_readd(group_id, installation_id)
    }

    fn update_requested_at_sequence_id(
        &self,
        group_id: &[u8],
        installation_id: &[u8],
        sequence_id: i64,
    ) -> Result<(), crate::ConnectionError> {
        (**self).update_requested_at_sequence_id(group_id, installation_id, sequence_id)
    }

    fn update_responded_at_sequence_id(
        &self,
        group_id: &[u8],
        installation_id: &[u8],
        sequence_id: i64,
    ) -> Result<(), crate::ConnectionError> {
        (**self).update_responded_at_sequence_id(group_id, installation_id, sequence_id)
    }

    fn delete_other_readd_statuses(
        &self,
        group_id: &[u8],
        self_installation_id: &[u8],
    ) -> Result<(), crate::ConnectionError> {
        (**self).delete_other_readd_statuses(group_id, self_installation_id)
    }

    fn delete_readd_statuses(
        &self,
        group_id: &[u8],
        installation_ids: HashSet<Vec<u8>>,
    ) -> Result<(), crate::ConnectionError> {
        (**self).delete_readd_statuses(group_id, installation_ids)
    }

    fn get_readds_awaiting_response(
        &self,
        group_id: &[u8],
        self_installation_id: &[u8],
    ) -> Result<Vec<ReaddStatus>, crate::ConnectionError> {
        (**self).get_readds_awaiting_response(group_id, self_installation_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Store, test_utils::with_connection};

    #[xmtp_common::test]
    async fn test_get_readd_status_not_found() {
        with_connection(|conn| {
            let group_id = vec![1, 2, 3];
            let installation_id = vec![4, 5, 6];

            let result = conn.get_readd_status(&group_id, &installation_id).unwrap();
            assert!(result.is_none());
        })
        .await;
    }

    #[xmtp_common::test]
    async fn test_store_and_get_readd_status() {
        with_connection(|conn| {
            let group_id = vec![1, 2, 3];
            let installation_id = vec![4, 5, 6];

            let status = ReaddStatus {
                group_id: group_id.clone(),
                installation_id: installation_id.clone(),
                requested_at_sequence_id: Some(100),
                responded_at_sequence_id: Some(50),
            };

            // Store the status
            status.store(conn).unwrap();

            // Retrieve it
            let retrieved = conn.get_readd_status(&group_id, &installation_id).unwrap();
            assert!(retrieved.is_some());
            let retrieved_status = retrieved.unwrap();
            assert_eq!(retrieved_status.requested_at_sequence_id, Some(100));
            assert_eq!(retrieved_status.responded_at_sequence_id, Some(50));
        })
        .await;
    }

    #[xmtp_common::test]
    async fn test_update_requested_at_sequence_id_creates_new() {
        with_connection(|conn| {
            let group_id = vec![1, 2, 3];
            let installation_id = vec![4, 5, 6];
            let sequence_id = 100;

            // Update on non-existing record should create it
            conn.update_requested_at_sequence_id(&group_id, &installation_id, sequence_id)
                .unwrap();

            // Verify it was created
            let status = conn.get_readd_status(&group_id, &installation_id).unwrap();
            assert!(status.is_some());
            let status = status.unwrap();
            assert_eq!(status.requested_at_sequence_id, Some(sequence_id));
            assert_eq!(status.responded_at_sequence_id, None);
        })
        .await;
    }

    #[xmtp_common::test]
    async fn test_update_requested_at_sequence_id_updates_existing() {
        with_connection(|conn| {
            let group_id = vec![1, 2, 3];
            let installation_id = vec![4, 5, 6];

            // Create initial status
            let initial_status = ReaddStatus {
                group_id: group_id.clone(),
                installation_id: installation_id.clone(),
                requested_at_sequence_id: Some(50),
                responded_at_sequence_id: Some(25),
            };
            initial_status.store(conn).unwrap();

            // Update with higher sequence_id
            conn.update_requested_at_sequence_id(&group_id, &installation_id, 100)
                .unwrap();

            // Verify it was updated
            let status = conn.get_readd_status(&group_id, &installation_id).unwrap();
            assert!(status.is_some());
            let status = status.unwrap();
            assert_eq!(status.requested_at_sequence_id, Some(100));
            assert_eq!(status.responded_at_sequence_id, Some(25)); // This is preserved by the UPDATE
        })
        .await;
    }

    #[xmtp_common::test]
    async fn test_update_requested_at_sequence_id_only_updates_if_higher() {
        with_connection(|conn| {
            let group_id = vec![1, 2, 3];
            let installation_id = vec![4, 5, 6];

            // Create initial status with high sequence_id
            let initial_status = ReaddStatus {
                group_id: group_id.clone(),
                installation_id: installation_id.clone(),
                requested_at_sequence_id: Some(100),
                responded_at_sequence_id: Some(50),
            };
            initial_status.store(conn).unwrap();

            // Try to update with lower sequence_id - this should be ignored
            conn.update_requested_at_sequence_id(&group_id, &installation_id, 75)
                .unwrap();

            // Verify it was NOT updated (lower sequence_id should be ignored due to filter)
            let status = conn.get_readd_status(&group_id, &installation_id).unwrap();
            assert!(status.is_some());
            let status = status.unwrap();
            assert_eq!(status.requested_at_sequence_id, Some(100)); // Should remain unchanged
            assert_eq!(status.responded_at_sequence_id, Some(50)); // Should remain unchanged
        })
        .await;
    }

    #[xmtp_common::test]
    async fn test_update_requested_at_sequence_id_updates_from_null() {
        with_connection(|conn| {
            let group_id = vec![1, 2, 3];
            let installation_id = vec![4, 5, 6];

            // Create initial status with null requested_at_sequence_id
            let initial_status = ReaddStatus {
                group_id: group_id.clone(),
                installation_id: installation_id.clone(),
                requested_at_sequence_id: None,
                responded_at_sequence_id: Some(25),
            };
            initial_status.store(conn).unwrap();

            // Update with any sequence_id (should work since current is null)
            conn.update_requested_at_sequence_id(&group_id, &installation_id, 50)
                .unwrap();

            // Verify it was updated
            let status = conn.get_readd_status(&group_id, &installation_id).unwrap();
            assert!(status.is_some());
            let status = status.unwrap();
            assert_eq!(status.requested_at_sequence_id, Some(50));
            assert_eq!(status.responded_at_sequence_id, Some(25)); // This is preserved by the UPDATE
        })
        .await;
    }

    #[xmtp_common::test]
    async fn test_update_responded_at_sequence_id_creates_new() {
        with_connection(|conn| {
            let group_id = vec![1, 2, 3];
            let installation_id = vec![4, 5, 6];
            let sequence_id = 100;

            // Update on non-existing record should create it
            conn.update_responded_at_sequence_id(&group_id, &installation_id, sequence_id)
                .unwrap();

            // Verify it was created
            let status = conn.get_readd_status(&group_id, &installation_id).unwrap();
            assert!(status.is_some());
            let status = status.unwrap();
            assert_eq!(status.responded_at_sequence_id, Some(sequence_id));
            assert_eq!(status.requested_at_sequence_id, None);
        })
        .await;
    }

    #[xmtp_common::test]
    async fn test_update_responded_at_sequence_id_only_updates_if_higher() {
        with_connection(|conn| {
            let group_id = vec![1, 2, 3];
            let installation_id = vec![4, 5, 6];

            // Create initial status with high responded_at_sequence_id
            let initial_status = ReaddStatus {
                group_id: group_id.clone(),
                installation_id: installation_id.clone(),
                requested_at_sequence_id: Some(50),
                responded_at_sequence_id: Some(100),
            };
            initial_status.store(conn).unwrap();

            // Try to update with lower sequence_id - this should be ignored
            conn.update_responded_at_sequence_id(&group_id, &installation_id, 75)
                .unwrap();

            // Verify it was NOT updated (lower sequence_id should be ignored due to filter)
            let status = conn.get_readd_status(&group_id, &installation_id).unwrap();
            assert!(status.is_some());
            let status = status.unwrap();
            assert_eq!(status.responded_at_sequence_id, Some(100)); // Should remain unchanged
            assert_eq!(status.requested_at_sequence_id, Some(50)); // Should remain unchanged

            // Now update with a higher sequence_id - this should work
            conn.update_responded_at_sequence_id(&group_id, &installation_id, 125)
                .unwrap();

            // Verify it was updated
            let status = conn.get_readd_status(&group_id, &installation_id).unwrap();
            assert!(status.is_some());
            let status = status.unwrap();
            assert_eq!(status.responded_at_sequence_id, Some(125)); // Should be updated
            assert_eq!(status.requested_at_sequence_id, Some(50)); // Should remain unchanged
        })
        .await;
    }

    #[xmtp_common::test]
    async fn test_is_awaiting_readd_no_status() {
        with_connection(|conn| {
            let group_id = vec![1, 2, 3];
            let installation_id = vec![4, 5, 6];

            // Should return false when no readd status exists
            let result = conn.is_awaiting_readd(&group_id, &installation_id).unwrap();
            assert!(!result);
        })
        .await;
    }

    #[xmtp_common::test]
    async fn test_is_awaiting_readd_no_request() {
        with_connection(|conn| {
            let group_id = vec![1, 2, 3];
            let installation_id = vec![4, 5, 6];

            // Create a readd status without a requested_at_sequence_id
            ReaddStatus {
                group_id: group_id.clone(),
                installation_id: installation_id.clone(),
                requested_at_sequence_id: None,
                responded_at_sequence_id: Some(5),
            }
            .store(conn)
            .unwrap();

            // Should return false when no request has been made
            let result = conn.is_awaiting_readd(&group_id, &installation_id).unwrap();
            assert!(!result);
        })
        .await;
    }

    #[xmtp_common::test]
    async fn test_is_awaiting_readd_request_pending() {
        with_connection(|conn| {
            let group_id = vec![1, 2, 3];
            let installation_id = vec![4, 5, 6];

            // Create a readd status with requested_at > responded_at
            ReaddStatus {
                group_id: group_id.clone(),
                installation_id: installation_id.clone(),
                requested_at_sequence_id: Some(10),
                responded_at_sequence_id: Some(5),
            }
            .store(conn)
            .unwrap();

            // Should return true when request is pending
            let result = conn.is_awaiting_readd(&group_id, &installation_id).unwrap();
            assert!(result);
        })
        .await;
    }

    #[xmtp_common::test]
    async fn test_is_awaiting_readd_request_fulfilled() {
        with_connection(|conn| {
            let group_id = vec![1, 2, 3];
            let installation_id = vec![4, 5, 6];

            // Create a readd status with requested_at <= responded_at
            ReaddStatus {
                group_id: group_id.clone(),
                installation_id: installation_id.clone(),
                requested_at_sequence_id: Some(5),
                responded_at_sequence_id: Some(10),
            }
            .store(conn)
            .unwrap();

            // Should return false when request has been fulfilled
            let result = conn.is_awaiting_readd(&group_id, &installation_id).unwrap();
            assert!(!result);
        })
        .await;
    }

    #[xmtp_common::test]
    async fn test_is_awaiting_readd_equal_sequence_ids() {
        with_connection(|conn| {
            let group_id = vec![1, 2, 3];
            let installation_id = vec![4, 5, 6];

            // Create a readd status with requested_at == responded_at
            ReaddStatus {
                group_id: group_id.clone(),
                installation_id: installation_id.clone(),
                requested_at_sequence_id: Some(10),
                responded_at_sequence_id: Some(10),
            }
            .store(conn)
            .unwrap();

            // Should return true when sequence IDs are equal.
            // The response to a readd request will always add a commit, which increases the sequence ID.
            // It is possible that a readd request is subsequently issued at the same sequence ID.
            let result = conn.is_awaiting_readd(&group_id, &installation_id).unwrap();
            assert!(result);
        })
        .await;
    }

    #[xmtp_common::test]
    async fn test_is_awaiting_readd_no_responded_at() {
        with_connection(|conn| {
            let group_id = vec![1, 2, 3];
            let installation_id = vec![4, 5, 6];

            // Create a readd status with requested_at but no responded_at (defaults to 0)
            ReaddStatus {
                group_id: group_id.clone(),
                installation_id: installation_id.clone(),
                requested_at_sequence_id: Some(5),
                responded_at_sequence_id: None,
            }
            .store(conn)
            .unwrap();

            // Should return true when requested_at > 0 (default responded_at)
            let result = conn.is_awaiting_readd(&group_id, &installation_id).unwrap();
            assert!(result);
        })
        .await;
    }

    #[xmtp_common::test]
    async fn test_delete_other_readd_statuses() {
        with_connection(|conn| {
            let group_id = vec![1, 2, 3];
            let keep_installation_id = vec![10, 11, 12];
            let delete_installation_id_1 = vec![20, 21, 22];
            let delete_installation_id_2 = vec![30, 31, 32];

            // Create readd statuses for the same group with different installation IDs
            let status_to_keep = ReaddStatus {
                group_id: group_id.clone(),
                installation_id: keep_installation_id.clone(),
                requested_at_sequence_id: Some(10),
                responded_at_sequence_id: Some(5),
            };
            status_to_keep.store(conn).unwrap();

            let status_to_delete_1 = ReaddStatus {
                group_id: group_id.clone(),
                installation_id: delete_installation_id_1.clone(),
                requested_at_sequence_id: Some(15),
                responded_at_sequence_id: Some(8),
            };
            status_to_delete_1.store(conn).unwrap();

            let status_to_delete_2 = ReaddStatus {
                group_id: group_id.clone(),
                installation_id: delete_installation_id_2.clone(),
                requested_at_sequence_id: Some(20),
                responded_at_sequence_id: None,
            };
            status_to_delete_2.store(conn).unwrap();

            // Create a status for a different group (should not be affected)
            let different_group_status = ReaddStatus {
                group_id: vec![4, 5, 6],
                installation_id: vec![40, 41, 42],
                requested_at_sequence_id: Some(25),
                responded_at_sequence_id: Some(12),
            };
            different_group_status.store(conn).unwrap();

            // Delete other readd statuses for the group
            conn.delete_other_readd_statuses(&group_id, &keep_installation_id)
                .unwrap();

            // Verify the status we wanted to keep is still there
            let kept_status = conn
                .get_readd_status(&group_id, &keep_installation_id)
                .unwrap();
            assert!(kept_status.is_some());

            // Verify the other statuses in the same group were deleted
            let deleted_status_1 = conn
                .get_readd_status(&group_id, &delete_installation_id_1)
                .unwrap();
            assert!(deleted_status_1.is_none());

            let deleted_status_2 = conn
                .get_readd_status(&group_id, &delete_installation_id_2)
                .unwrap();
            assert!(deleted_status_2.is_none());

            // Verify the status in the different group was not affected
            let different_group_check = conn.get_readd_status(&[4, 5, 6], &[40, 41, 42]).unwrap();
            assert!(different_group_check.is_some());
        })
        .await;
    }

    #[xmtp_common::test]
    async fn test_get_readds_awaiting_response() {
        with_connection(|conn| {
            let group_id = vec![1, 2, 3];
            let self_installation_id = vec![10, 11, 12];
            let other_installation_id_1 = vec![20, 21, 22];
            let other_installation_id_2 = vec![30, 31, 32];

            // Create readd statuses with various states

            // Case 1: Pending readd from other installation (should be included)
            let pending_status_1 = ReaddStatus {
                group_id: group_id.clone(),
                installation_id: other_installation_id_1.clone(),
                requested_at_sequence_id: Some(10),
                responded_at_sequence_id: Some(5),
            };
            pending_status_1.store(conn).unwrap();

            // Case 2: Pending readd from other installation with null responded_at (should be included)
            let pending_status_2 = ReaddStatus {
                group_id: group_id.clone(),
                installation_id: other_installation_id_2.clone(),
                requested_at_sequence_id: Some(15),
                responded_at_sequence_id: None,
            };
            pending_status_2.store(conn).unwrap();

            // Case 3: Not pending readd from other installation (should be excluded)
            let fulfilled_status = ReaddStatus {
                group_id: group_id.clone(),
                installation_id: vec![40, 41, 42],
                requested_at_sequence_id: Some(8),
                responded_at_sequence_id: Some(12),
            };
            fulfilled_status.store(conn).unwrap();

            // Case 4: Pending readd from self installation (should be excluded)
            let self_status = ReaddStatus {
                group_id: group_id.clone(),
                installation_id: self_installation_id.clone(),
                requested_at_sequence_id: Some(20),
                responded_at_sequence_id: Some(10),
            };
            self_status.store(conn).unwrap();

            // Case 5: No requested_at_sequence_id (should be excluded)
            let no_request_status = ReaddStatus {
                group_id: group_id.clone(),
                installation_id: vec![50, 51, 52],
                requested_at_sequence_id: None,
                responded_at_sequence_id: Some(5),
            };
            no_request_status.store(conn).unwrap();

            // Case 6: Different group (should be excluded)
            let different_group_status = ReaddStatus {
                group_id: vec![4, 5, 6],
                installation_id: vec![60, 61, 62],
                requested_at_sequence_id: Some(25),
                responded_at_sequence_id: Some(15),
            };
            different_group_status.store(conn).unwrap();

            // Call the method under test
            let result = conn
                .get_readds_awaiting_response(&group_id, &self_installation_id)
                .unwrap();

            // Should return 2 pending readds from other installations
            assert_eq!(result.len(), 2);

            // Verify the correct statuses are returned
            let returned_installations: Vec<Vec<u8>> =
                result.iter().map(|r| r.installation_id.clone()).collect();
            assert!(returned_installations.contains(&other_installation_id_1));
            assert!(returned_installations.contains(&other_installation_id_2));

            // Verify the details of the returned statuses
            for status in result {
                assert_eq!(status.group_id, group_id);
                assert_ne!(status.installation_id, self_installation_id);
                assert!(status.requested_at_sequence_id.is_some());

                // Check that the awaiting response condition is met
                let requested_at = status.requested_at_sequence_id.unwrap();
                let responded_at = status.responded_at_sequence_id.unwrap_or(0);
                assert!(requested_at >= responded_at);
            }
        })
        .await;
    }
}
