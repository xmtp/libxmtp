use diesel::prelude::*;

use super::{
    DbConnection,
    schema::readd_status::{self},
};
use crate::{ConnectionExt, impl_store};

#[derive(Identifiable, Queryable, Selectable, Insertable, Debug, Clone, PartialEq, Eq)]
#[diesel(table_name = readd_status)]
#[diesel(primary_key(group_id, inbox_id, installation_id))]
pub struct ReaddStatus {
    pub group_id: Vec<u8>,
    pub inbox_id: String,
    pub installation_id: Vec<u8>,
    pub requested_at_sequence_id: Option<i64>,
    pub responded_at_sequence_id: Option<i64>,
}

impl_store!(ReaddStatus, readd_status);

pub trait QueryReaddStatus {
    fn get_readd_status(
        &self,
        group_id: &[u8],
        inbox_id: &str,
        installation_id: &[u8],
    ) -> Result<Option<ReaddStatus>, crate::ConnectionError>;

    fn is_awaiting_readd(
        &self,
        group_id: &[u8],
        inbox_id: &str,
        installation_id: &[u8],
    ) -> Result<bool, crate::ConnectionError>;

    /// Update the requested_at_sequence_id for a given group_id, inbox_id, and installation_id,
    /// provided it is higher than the current value.
    /// Inserts the row if it doesn't exist.
    fn update_requested_at_sequence_id(
        &self,
        group_id: &[u8],
        inbox_id: &str,
        installation_id: &[u8],
        sequence_id: i64,
    ) -> Result<(), crate::ConnectionError>;

    /// Update the responded_at_sequence_id for a given group_id, inbox_id, and installation_id,
    /// provided it is higher than the current value.
    /// Inserts the row if it doesn't exist.
    fn update_responded_at_sequence_id(
        &self,
        group_id: &[u8],
        inbox_id: &str,
        installation_id: &[u8],
        sequence_id: i64,
    ) -> Result<(), crate::ConnectionError>;
}

impl<C: ConnectionExt> QueryReaddStatus for DbConnection<C> {
    fn get_readd_status(
        &self,
        group_id: &[u8],
        inbox_id: &str,
        installation_id: &[u8],
    ) -> Result<Option<ReaddStatus>, crate::ConnectionError> {
        use super::schema::readd_status::dsl as readd_dsl;
        use diesel::QueryDsl;

        self.raw_query_read(|conn| {
            readd_dsl::readd_status
                .filter(readd_dsl::group_id.eq(group_id))
                .filter(readd_dsl::inbox_id.eq(inbox_id))
                .filter(readd_dsl::installation_id.eq(installation_id))
                .first::<ReaddStatus>(conn)
                .optional()
        })
    }

    fn is_awaiting_readd(
        &self,
        group_id: &[u8],
        inbox_id: &str,
        installation_id: &[u8],
    ) -> Result<bool, crate::ConnectionError> {
        let readd_status = self.get_readd_status(group_id, inbox_id, installation_id)?;
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
        inbox_id: &str,
        installation_id: &[u8],
        sequence_id: i64,
    ) -> Result<(), crate::ConnectionError> {
        use super::schema::readd_status::dsl as readd_dsl;
        use diesel::query_dsl::methods::FilterDsl;

        let new_status = super::readd_status::ReaddStatus {
            group_id: group_id.to_vec(),
            inbox_id: inbox_id.to_string(),
            installation_id: installation_id.to_vec(),
            requested_at_sequence_id: Some(sequence_id),
            responded_at_sequence_id: None,
        };

        self.raw_query_write(|conn| {
            diesel::insert_into(readd_dsl::readd_status)
                .values(&new_status)
                .on_conflict((
                    readd_dsl::group_id,
                    readd_dsl::inbox_id,
                    readd_dsl::installation_id,
                ))
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
        inbox_id: &str,
        installation_id: &[u8],
        sequence_id: i64,
    ) -> Result<(), crate::ConnectionError> {
        use super::schema::readd_status::dsl as readd_dsl;
        use diesel::query_dsl::methods::FilterDsl;

        let new_status = super::readd_status::ReaddStatus {
            group_id: group_id.to_vec(),
            inbox_id: inbox_id.to_string(),
            installation_id: installation_id.to_vec(),
            requested_at_sequence_id: None,
            responded_at_sequence_id: Some(sequence_id),
        };

        self.raw_query_write(|conn| {
            diesel::insert_into(readd_dsl::readd_status)
                .values(&new_status)
                .on_conflict((
                    readd_dsl::group_id,
                    readd_dsl::inbox_id,
                    readd_dsl::installation_id,
                ))
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
}

impl<T> QueryReaddStatus for &T
where
    T: QueryReaddStatus,
{
    fn get_readd_status(
        &self,
        group_id: &[u8],
        inbox_id: &str,
        installation_id: &[u8],
    ) -> Result<Option<ReaddStatus>, crate::ConnectionError> {
        (**self).get_readd_status(group_id, inbox_id, installation_id)
    }

    fn is_awaiting_readd(
        &self,
        group_id: &[u8],
        inbox_id: &str,
        installation_id: &[u8],
    ) -> Result<bool, crate::ConnectionError> {
        (**self).is_awaiting_readd(group_id, inbox_id, installation_id)
    }

    fn update_requested_at_sequence_id(
        &self,
        group_id: &[u8],
        inbox_id: &str,
        installation_id: &[u8],
        sequence_id: i64,
    ) -> Result<(), crate::ConnectionError> {
        (**self).update_requested_at_sequence_id(group_id, inbox_id, installation_id, sequence_id)
    }

    fn update_responded_at_sequence_id(
        &self,
        group_id: &[u8],
        inbox_id: &str,
        installation_id: &[u8],
        sequence_id: i64,
    ) -> Result<(), crate::ConnectionError> {
        (**self).update_responded_at_sequence_id(group_id, inbox_id, installation_id, sequence_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Store, test_utils::with_connection};
    use xmtp_common::rand_string;

    #[xmtp_common::test]
    async fn test_get_readd_status_not_found() {
        with_connection(|conn| {
            let group_id = vec![1, 2, 3];
            let inbox_id = rand_string::<24>();
            let installation_id = vec![4, 5, 6];

            let result = conn
                .get_readd_status(&group_id, &inbox_id, &installation_id)
                .unwrap();
            assert!(result.is_none());
        })
        .await;
    }

    #[xmtp_common::test]
    async fn test_store_and_get_readd_status() {
        with_connection(|conn| {
            let group_id = vec![1, 2, 3];
            let inbox_id = rand_string::<24>();
            let installation_id = vec![4, 5, 6];

            let status = ReaddStatus {
                group_id: group_id.clone(),
                inbox_id: inbox_id.clone(),
                installation_id: installation_id.clone(),
                requested_at_sequence_id: Some(100),
                responded_at_sequence_id: Some(50),
            };

            // Store the status
            status.store(conn).unwrap();

            // Retrieve it
            let retrieved = conn
                .get_readd_status(&group_id, &inbox_id, &installation_id)
                .unwrap();
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
            let inbox_id = rand_string::<24>();
            let installation_id = vec![4, 5, 6];
            let sequence_id = 100;

            // Update on non-existing record should create it
            conn.update_requested_at_sequence_id(
                &group_id,
                &inbox_id,
                &installation_id,
                sequence_id,
            )
            .unwrap();

            // Verify it was created
            let status = conn
                .get_readd_status(&group_id, &inbox_id, &installation_id)
                .unwrap();
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
            let inbox_id = rand_string::<24>();
            let installation_id = vec![4, 5, 6];

            // Create initial status
            let initial_status = ReaddStatus {
                group_id: group_id.clone(),
                inbox_id: inbox_id.clone(),
                installation_id: installation_id.clone(),
                requested_at_sequence_id: Some(50),
                responded_at_sequence_id: Some(25),
            };
            initial_status.store(conn).unwrap();

            // Update with higher sequence_id
            conn.update_requested_at_sequence_id(&group_id, &inbox_id, &installation_id, 100)
                .unwrap();

            // Verify it was updated
            let status = conn
                .get_readd_status(&group_id, &inbox_id, &installation_id)
                .unwrap();
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
            let inbox_id = rand_string::<24>();
            let installation_id = vec![4, 5, 6];

            // Create initial status with high sequence_id
            let initial_status = ReaddStatus {
                group_id: group_id.clone(),
                inbox_id: inbox_id.clone(),
                installation_id: installation_id.clone(),
                requested_at_sequence_id: Some(100),
                responded_at_sequence_id: Some(50),
            };
            initial_status.store(conn).unwrap();

            // Try to update with lower sequence_id - this should be ignored
            conn.update_requested_at_sequence_id(&group_id, &inbox_id, &installation_id, 75)
                .unwrap();

            // Verify it was NOT updated (lower sequence_id should be ignored due to filter)
            let status = conn
                .get_readd_status(&group_id, &inbox_id, &installation_id)
                .unwrap();
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
            let inbox_id = rand_string::<24>();
            let installation_id = vec![4, 5, 6];

            // Create initial status with null requested_at_sequence_id
            let initial_status = ReaddStatus {
                group_id: group_id.clone(),
                inbox_id: inbox_id.clone(),
                installation_id: installation_id.clone(),
                requested_at_sequence_id: None,
                responded_at_sequence_id: Some(25),
            };
            initial_status.store(conn).unwrap();

            // Update with any sequence_id (should work since current is null)
            conn.update_requested_at_sequence_id(&group_id, &inbox_id, &installation_id, 50)
                .unwrap();

            // Verify it was updated
            let status = conn
                .get_readd_status(&group_id, &inbox_id, &installation_id)
                .unwrap();
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
            let inbox_id = rand_string::<24>();
            let installation_id = vec![4, 5, 6];
            let sequence_id = 100;

            // Update on non-existing record should create it
            conn.update_responded_at_sequence_id(
                &group_id,
                &inbox_id,
                &installation_id,
                sequence_id,
            )
            .unwrap();

            // Verify it was created
            let status = conn
                .get_readd_status(&group_id, &inbox_id, &installation_id)
                .unwrap();
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
            let inbox_id = rand_string::<24>();
            let installation_id = vec![4, 5, 6];

            // Create initial status with high responded_at_sequence_id
            let initial_status = ReaddStatus {
                group_id: group_id.clone(),
                inbox_id: inbox_id.clone(),
                installation_id: installation_id.clone(),
                requested_at_sequence_id: Some(50),
                responded_at_sequence_id: Some(100),
            };
            initial_status.store(conn).unwrap();

            // Try to update with lower sequence_id - this should be ignored
            conn.update_responded_at_sequence_id(&group_id, &inbox_id, &installation_id, 75)
                .unwrap();

            // Verify it was NOT updated (lower sequence_id should be ignored due to filter)
            let status = conn
                .get_readd_status(&group_id, &inbox_id, &installation_id)
                .unwrap();
            assert!(status.is_some());
            let status = status.unwrap();
            assert_eq!(status.responded_at_sequence_id, Some(100)); // Should remain unchanged
            assert_eq!(status.requested_at_sequence_id, Some(50)); // Should remain unchanged

            // Now update with a higher sequence_id - this should work
            conn.update_responded_at_sequence_id(&group_id, &inbox_id, &installation_id, 125)
                .unwrap();

            // Verify it was updated
            let status = conn
                .get_readd_status(&group_id, &inbox_id, &installation_id)
                .unwrap();
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
            let inbox_id = rand_string::<24>();
            let installation_id = vec![4, 5, 6];

            // Should return false when no readd status exists
            let result = conn
                .is_awaiting_readd(&group_id, &inbox_id, &installation_id)
                .unwrap();
            assert!(!result);
        })
        .await;
    }

    #[xmtp_common::test]
    async fn test_is_awaiting_readd_no_request() {
        with_connection(|conn| {
            let group_id = vec![1, 2, 3];
            let inbox_id = rand_string::<24>();
            let installation_id = vec![4, 5, 6];

            // Create a readd status without a requested_at_sequence_id
            ReaddStatus {
                group_id: group_id.clone(),
                inbox_id: inbox_id.clone(),
                installation_id: installation_id.clone(),
                requested_at_sequence_id: None,
                responded_at_sequence_id: Some(5),
            }
            .store(conn)
            .unwrap();

            // Should return false when no request has been made
            let result = conn
                .is_awaiting_readd(&group_id, &inbox_id, &installation_id)
                .unwrap();
            assert!(!result);
        })
        .await;
    }

    #[xmtp_common::test]
    async fn test_is_awaiting_readd_request_pending() {
        with_connection(|conn| {
            let group_id = vec![1, 2, 3];
            let inbox_id = rand_string::<24>();
            let installation_id = vec![4, 5, 6];

            // Create a readd status with requested_at > responded_at
            ReaddStatus {
                group_id: group_id.clone(),
                inbox_id: inbox_id.clone(),
                installation_id: installation_id.clone(),
                requested_at_sequence_id: Some(10),
                responded_at_sequence_id: Some(5),
            }
            .store(conn)
            .unwrap();

            // Should return true when request is pending
            let result = conn
                .is_awaiting_readd(&group_id, &inbox_id, &installation_id)
                .unwrap();
            assert!(result);
        })
        .await;
    }

    #[xmtp_common::test]
    async fn test_is_awaiting_readd_request_fulfilled() {
        with_connection(|conn| {
            let group_id = vec![1, 2, 3];
            let inbox_id = rand_string::<24>();
            let installation_id = vec![4, 5, 6];

            // Create a readd status with requested_at <= responded_at
            ReaddStatus {
                group_id: group_id.clone(),
                inbox_id: inbox_id.clone(),
                installation_id: installation_id.clone(),
                requested_at_sequence_id: Some(5),
                responded_at_sequence_id: Some(10),
            }
            .store(conn)
            .unwrap();

            // Should return false when request has been fulfilled
            let result = conn
                .is_awaiting_readd(&group_id, &inbox_id, &installation_id)
                .unwrap();
            assert!(!result);
        })
        .await;
    }

    #[xmtp_common::test]
    async fn test_is_awaiting_readd_equal_sequence_ids() {
        with_connection(|conn| {
            let group_id = vec![1, 2, 3];
            let inbox_id = rand_string::<24>();
            let installation_id = vec![4, 5, 6];

            // Create a readd status with requested_at == responded_at
            ReaddStatus {
                group_id: group_id.clone(),
                inbox_id: inbox_id.clone(),
                installation_id: installation_id.clone(),
                requested_at_sequence_id: Some(10),
                responded_at_sequence_id: Some(10),
            }
            .store(conn)
            .unwrap();

            // Should return true when sequence IDs are equal.
            // The response to a readd request will always add a commit, which increases the sequence ID.
            // It is possible that a readd request is subsequently issued at the same sequence ID.
            let result = conn
                .is_awaiting_readd(&group_id, &inbox_id, &installation_id)
                .unwrap();
            assert!(result);
        })
        .await;
    }

    #[xmtp_common::test]
    async fn test_is_awaiting_readd_no_responded_at() {
        with_connection(|conn| {
            let group_id = vec![1, 2, 3];
            let inbox_id = rand_string::<24>();
            let installation_id = vec![4, 5, 6];

            // Create a readd status with requested_at but no responded_at (defaults to 0)
            ReaddStatus {
                group_id: group_id.clone(),
                inbox_id: inbox_id.clone(),
                installation_id: installation_id.clone(),
                requested_at_sequence_id: Some(5),
                responded_at_sequence_id: None,
            }
            .store(conn)
            .unwrap();

            // Should return true when requested_at > 0 (default responded_at)
            let result = conn
                .is_awaiting_readd(&group_id, &inbox_id, &installation_id)
                .unwrap();
            assert!(result);
        })
        .await;
    }
}
