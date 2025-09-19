use diesel::dsl::exists;
use diesel::helper_types::select;
use super::ConnectionExt;
use crate::schema::pending_remove::dsl;
use crate::{DbConnection, impl_fetch, impl_store_or_ignore, schema::pending_remove};
use diesel::prelude::*;
use diesel::select;
use serde::{Deserialize, Serialize};

#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    Insertable,
    Identifiable,
    Queryable,
    Eq,
    PartialEq,
    QueryableByName,
)]
#[diesel(table_name = pending_remove)]
#[diesel(primary_key(inbox_id, group_id))]
pub struct PendingRemove {
    /// Id of the group this message is tied to.
    pub group_id: Vec<u8>,
    /// Id of the inbox user want to leave the group.
    pub inbox_id: String,
    /// Id of the LeaveRequest message
    pub message_id: Vec<u8>,
}

impl_store_or_ignore!(PendingRemove, pending_remove);
impl_fetch!(PendingRemove, pending_remove);
pub trait QueryPendingRemove {
    fn get_pending_remove_users(
        &self,
        group_id: &Vec<u8>,
    ) -> Result<Vec<String>, crate::ConnectionError>;
    fn get_user_pending_remove_status(
        &self,
        group_id: &Vec<u8>,
        inbox_id: &String,
    ) -> Result<bool, crate::ConnectionError>;
    fn delete_pending_remove_users(
        &self,
        group_id: &Vec<u8>,
        inbox_ids: Vec<String>,
    ) -> Result<usize, crate::ConnectionError>;
}
impl<T> QueryPendingRemove for &T
where
    T: QueryPendingRemove,
{
    fn get_pending_remove_users(
        &self,
        group_id: &Vec<u8>,
    ) -> Result<Vec<String>, crate::ConnectionError> {
        (**self).get_pending_remove_users(group_id)
    }
    fn get_user_pending_remove_status(
        &self,
        group_id: &Vec<u8>,
        inbox_id: &String,
    ) -> Result<bool, crate::ConnectionError>{
        (**self).get_user_pending_remove_status(group_id,inbox_id)
    }
    fn delete_pending_remove_users(
        &self,
        group_id: &Vec<u8>,
        inbox_ids: Vec<String>,
    ) -> Result<usize, crate::ConnectionError>{
        (**self).delete_pending_remove_users(group_id, inbox_ids)
    }
}
impl<C: ConnectionExt> QueryPendingRemove for DbConnection<C> {
    fn get_pending_remove_users(
        &self,
        group_id: &Vec<u8>,
    ) -> Result<Vec<String>, crate::ConnectionError> {
        let result = self.raw_query_read(|conn| {
            dsl::pending_remove
                .filter(dsl::group_id.eq(group_id))
                .select(dsl::inbox_id)
                .load::<String>(conn)
        })?;

        Ok(result)
    }

    fn get_user_pending_remove_status(&self, group_id: &Vec<u8>, inbox_id: &String) -> Result<bool, crate::ConnectionError> {
        let result: bool = self.raw_query_read(|conn| {
            select(exists(
                dsl::pending_remove.filter(
                    dsl::group_id.eq(group_id).and(dsl::inbox_id.eq(inbox_id))
                )
            ))
                .get_result::<bool>(conn)
        })?;
        Ok(result)
    }


    fn delete_pending_remove_users(
        &self,
        group_id: &Vec<u8>,
        inbox_ids: Vec<String>,
    ) -> Result<usize, crate::ConnectionError> {
        let result = self.raw_query_write(|conn| {
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
#[cfg(test)]
mod tests {
    use crate::encrypted_store::pending_remove::{PendingRemove, QueryPendingRemove};
    use crate::{StoreOrIgnore, with_connection};

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_add_pending_remove() {
        with_connection(|conn| {
            // Break the chain by unsetting the originator.
            PendingRemove {
                inbox_id: "123".to_string(),
                group_id: vec![1, 2, 3],
                message_id: vec![1, 2, 3],
            }
            .store_or_ignore(conn)?;
            let users = conn.get_pending_remove_users(&vec![1, 2, 3]).unwrap();
            assert_eq!(users.len(), 1);
            let users = conn.get_pending_remove_users(&vec![1]).unwrap();
            assert_eq!(users.len(), 0);
        })
        .await
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_delete_pending_remove_user() {
        with_connection(|conn| {
            // Break the chain by unsetting the originator.
            PendingRemove {
                inbox_id: "1".to_string(),
                group_id: vec![1, 2, 3],
                message_id: vec![1, 2, 3],
            }
            .store_or_ignore(conn)?;
            PendingRemove {
                inbox_id: "2".to_string(),
                group_id: vec![1, 2, 3],
                message_id: vec![1, 2, 3],
            }
            .store_or_ignore(conn)?;
            PendingRemove {
                inbox_id: "3".to_string(),
                group_id: vec![1, 2, 3],
                message_id: vec![1, 2, 3],
            }
            .store_or_ignore(conn)?;
            let users = conn.get_pending_remove_users(&vec![1, 2, 3]).unwrap();
            assert_eq!(users.len(), 3);
            let deleted_users = conn
                .delete_pending_remove_users(&vec![1, 2, 3], vec!["1".to_string(), "2".to_string()], )
                .unwrap();
            assert_eq!(deleted_users, 2usize);
            let users = conn.get_pending_remove_users(&vec![1, 2, 3]).unwrap();
            assert_eq!(users.len(), 1);
            let deleted_users = conn
                .delete_pending_remove_users(&vec![1],vec!["3".to_string()])
                .unwrap();
            assert_eq!(deleted_users, 0usize);
        })
        .await
    }
}
