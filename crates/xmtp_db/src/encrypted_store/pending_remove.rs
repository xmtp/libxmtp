#[cfg(feature = "sync")]
use super::ConnectionExt;
#[cfg(feature = "sync")]
use crate::schema::pending_remove::dsl;
#[cfg(feature = "sync")]
use crate::{DbConnection, impl_fetch, impl_store_or_ignore, schema::pending_remove};
#[cfg(feature = "sync")]
use diesel::dsl::exists;
#[cfg(feature = "sync")]
use diesel::prelude::*;
#[cfg(feature = "sync")]
use diesel::select;
use serde::{Deserialize, Serialize};

use xmtp_proto::types::GroupId;
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[cfg_attr(
    feature = "sync",
    derive(Insertable, Identifiable, Queryable, QueryableByName)
)]
#[cfg_attr(feature = "sync", diesel(table_name = pending_remove))]
#[cfg_attr(feature = "sync", diesel(primary_key(inbox_id, group_id)))]
pub struct PendingRemove {
    /// Id of the group this message is tied to.
    pub group_id: GroupId,
    /// Id of the inbox user want to leave the group.
    pub inbox_id: String,
    /// Id of the LeaveRequest message
    pub message_id: Vec<u8>,
}

#[cfg(feature = "sync")]
impl_store_or_ignore!(PendingRemove, pending_remove);
#[cfg(feature = "sync")]
impl_fetch!(PendingRemove, pending_remove);
pub trait QueryPendingRemove {
    fn get_pending_remove_users(
        &self,
        group_id: &GroupId,
    ) -> impl std::future::Future<Output = Result<Vec<String>, crate::ConnectionError>>
    + xmtp_common::MaybeSend;
    fn get_user_pending_remove_status(
        &self,
        group_id: &GroupId,
        inbox_id: &str,
    ) -> impl std::future::Future<Output = Result<bool, crate::ConnectionError>> + xmtp_common::MaybeSend;
    fn delete_pending_remove_users(
        &self,
        group_id: &GroupId,
        inbox_ids: Vec<String>,
    ) -> impl std::future::Future<Output = Result<usize, crate::ConnectionError>> + xmtp_common::MaybeSend;
}
impl<T> QueryPendingRemove for &T
where
    T: QueryPendingRemove + xmtp_common::MaybeSync,
{
    async fn get_pending_remove_users(
        &self,
        group_id: &GroupId,
    ) -> Result<Vec<String>, crate::ConnectionError> {
        (**self).get_pending_remove_users(group_id).await
    }
    async fn get_user_pending_remove_status(
        &self,
        group_id: &GroupId,
        inbox_id: &str,
    ) -> Result<bool, crate::ConnectionError> {
        (**self)
            .get_user_pending_remove_status(group_id, inbox_id)
            .await
    }
    async fn delete_pending_remove_users(
        &self,
        group_id: &GroupId,
        inbox_ids: Vec<String>,
    ) -> Result<usize, crate::ConnectionError> {
        (**self)
            .delete_pending_remove_users(group_id, inbox_ids)
            .await
    }
}
#[cfg(feature = "sync")]
impl<C: ConnectionExt> QueryPendingRemove for DbConnection<C> {
    async fn get_pending_remove_users(
        &self,
        group_id: &GroupId,
    ) -> Result<Vec<String>, crate::ConnectionError> {
        let result = self.raw_query(|conn| {
            dsl::pending_remove
                .filter(dsl::group_id.eq(group_id))
                .select(dsl::inbox_id)
                .load::<String>(conn)
        })?;

        Ok(result)
    }

    async fn get_user_pending_remove_status(
        &self,
        group_id: &GroupId,
        inbox_id: &str,
    ) -> Result<bool, crate::ConnectionError> {
        let result: bool = self.raw_query(|conn| {
            select(exists(dsl::pending_remove.filter(
                dsl::group_id.eq(group_id).and(dsl::inbox_id.eq(inbox_id)),
            )))
            .get_result::<bool>(conn)
        })?;
        Ok(result)
    }

    async fn delete_pending_remove_users(
        &self,
        group_id: &GroupId,
        inbox_ids: Vec<String>,
    ) -> Result<usize, crate::ConnectionError> {
        let result = self.raw_query(|conn| {
            diesel::delete(
                dsl::pending_remove.filter(
                    dsl::inbox_id
                        .eq_any(inbox_ids)
                        .and(dsl::group_id.eq(group_id)),
                ),
            )
            .execute(conn)
        })?;
        Ok(result)
    }
}
/// sqlx backend -- Postgres only. See the note on `QueryGroupVersion`'s impl for
/// why this is gated `not(feature = "sync")`.
#[cfg(all(feature = "async", not(feature = "sync"), not(target_arch = "wasm32")))]
impl QueryPendingRemove for crate::pg::PgDb {
    async fn get_pending_remove_users(
        &self,
        group_id: &GroupId,
    ) -> Result<Vec<String>, crate::ConnectionError> {
        use sqlx::Row;
        let mut c = self.conn().await?;
        let rows = sqlx::query("SELECT inbox_id FROM pending_remove WHERE group_id = $1")
            .bind(group_id)
            .fetch_all(&mut *c)
            .await?;
        rows.into_iter()
            .map(|row| row.try_get(0).map_err(Into::into))
            .collect()
    }

    async fn get_user_pending_remove_status(
        &self,
        group_id: &GroupId,
        inbox_id: &str,
    ) -> Result<bool, crate::ConnectionError> {
        use sqlx::Row;
        let mut c = self.conn().await?;
        let row = sqlx::query(
            "SELECT EXISTS(SELECT 1 FROM pending_remove WHERE group_id = $1 AND inbox_id = $2)",
        )
        .bind(group_id)
        .bind(inbox_id)
        .fetch_one(&mut *c)
        .await?;
        Ok(row.try_get(0)?)
    }

    async fn delete_pending_remove_users(
        &self,
        group_id: &GroupId,
        inbox_ids: Vec<String>,
    ) -> Result<usize, crate::ConnectionError> {
        let mut c = self.conn().await?;
        // `= ANY($2)` rather than an `IN` list built by hand: one prepared
        // statement regardless of how many ids are passed, and an empty slice
        // correctly matches nothing.
        let deleted =
            sqlx::query("DELETE FROM pending_remove WHERE group_id = $1 AND inbox_id = ANY($2)")
                .bind(group_id)
                .bind(&inbox_ids)
                .execute(&mut *c)
                .await?
                .rows_affected();
        Ok(deleted as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::GroupId;
    use crate::encrypted_store::pending_remove::{PendingRemove, QueryPendingRemove};
    use crate::{StoreOrIgnore, with_connection};

    #[xmtp_common::test(unwrap_try = true)]
    fn test_add_pending_remove() {
        with_connection(|conn| {
            // Break the chain by unsetting the originator.
            PendingRemove {
                inbox_id: "123".to_string(),
                group_id: GroupId::ONE,
                message_id: vec![1, 2, 3],
            }
            .store_or_ignore(conn)?;
            let users = conn.get_pending_remove_users(&GroupId::ONE).unwrap();
            assert_eq!(users.len(), 1);
            let users = conn.get_pending_remove_users(&GroupId::TWO).unwrap();
            assert_eq!(users.len(), 0);
        })
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_delete_pending_remove_user() {
        with_connection(|conn| {
            // Break the chain by unsetting the originator.
            PendingRemove {
                inbox_id: "1".to_string(),
                group_id: GroupId::ONE,
                message_id: vec![1, 2, 3],
            }
            .store_or_ignore(conn)?;
            PendingRemove {
                inbox_id: "2".to_string(),
                group_id: GroupId::ONE,
                message_id: vec![1, 2, 3],
            }
            .store_or_ignore(conn)?;
            PendingRemove {
                inbox_id: "3".to_string(),
                group_id: GroupId::ONE,
                message_id: vec![1, 2, 3],
            }
            .store_or_ignore(conn)?;
            let group_id = GroupId::ONE;
            let users = conn.get_pending_remove_users(&group_id).unwrap();
            assert_eq!(users.len(), 3);
            let deleted_users = conn
                .delete_pending_remove_users(&group_id, vec!["1".to_string(), "2".to_string()])
                .unwrap();
            assert_eq!(deleted_users, 2usize);
            let users = conn.get_pending_remove_users(&group_id).unwrap();
            assert_eq!(users.len(), 1);
            let deleted_users = conn
                .delete_pending_remove_users(&GroupId::TWO, vec!["3".to_string()])
                .unwrap();
            assert_eq!(deleted_users, 0usize);
        })
    }
}
