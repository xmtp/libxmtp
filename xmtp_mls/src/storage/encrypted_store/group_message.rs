use diesel::{
    backend::Backend,
    deserialize::{self, FromSql, FromSqlRow},
    expression::AsExpression,
    prelude::*,
    serialize::{self, IsNull, Output, ToSql},
    sql_types::Integer,
    sqlite::Sqlite,
};

use super::{
    db_connection::DbConnection,
    schema::{group_messages, group_messages::dsl},
};
use crate::{impl_fetch, impl_store, StorageError};

#[derive(Insertable, Identifiable, Queryable, Debug, Clone, PartialEq, Eq)]
#[diesel(table_name = group_messages)]
#[diesel(primary_key(id))]
/// Successfully processed messages to be returned to the User.
pub struct StoredGroupMessage {
    /// Id of the message.
    pub id: Vec<u8>,
    /// Id of the group this message is tied to.
    pub group_id: Vec<u8>,
    /// Contents of message after decryption.
    pub decrypted_message_bytes: Vec<u8>,
    /// Time in nanoseconds the message was sent.
    pub sent_at_ns: i64,
    /// Group Message Kind Enum: 1 = Application, 2 = MembershipChange
    pub kind: GroupMessageKind,
    /// The ID of the App Installation this message was sent from.
    pub sender_installation_id: Vec<u8>,
    /// Network wallet address of the Sender
    pub sender_account_address: String,
    /// We optimistically store messages before sending.
    pub delivery_status: DeliveryStatus,
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, AsExpression, FromSqlRow)]
#[diesel(sql_type = Integer)]
pub enum GroupMessageKind {
    Application = 1,
    MembershipChange = 2,
}

impl ToSql<Integer, Sqlite> for GroupMessageKind
where
    i32: ToSql<Integer, Sqlite>,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(*self as i32);
        Ok(IsNull::No)
    }
}

impl FromSql<Integer, Sqlite> for GroupMessageKind
where
    i32: FromSql<Integer, Sqlite>,
{
    fn from_sql(bytes: <Sqlite as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        match i32::from_sql(bytes)? {
            1 => Ok(GroupMessageKind::Application),
            2 => Ok(GroupMessageKind::MembershipChange),
            x => Err(format!("Unrecognized variant {}", x).into()),
        }
    }
}

#[repr(i32)]
#[derive(Debug, Copy, Clone, FromSqlRow, Eq, PartialEq, AsExpression)]
#[diesel(sql_type = Integer)]
pub enum DeliveryStatus {
    Unpublished = 1,
    Published = 2,
    Failed = 3,
}

impl ToSql<Integer, Sqlite> for DeliveryStatus
where
    i32: ToSql<Integer, Sqlite>,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(*self as i32);
        Ok(IsNull::No)
    }
}

impl FromSql<Integer, Sqlite> for DeliveryStatus
where
    i32: FromSql<Integer, Sqlite>,
{
    fn from_sql(bytes: <Sqlite as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        match i32::from_sql(bytes)? {
            1 => Ok(DeliveryStatus::Unpublished),
            2 => Ok(DeliveryStatus::Published),
            3 => Ok(DeliveryStatus::Failed),
            x => Err(format!("Unrecognized variant {}", x).into()),
        }
    }
}

impl_fetch!(StoredGroupMessage, group_messages, Vec<u8>);
impl_store!(StoredGroupMessage, group_messages);

impl DbConnection {
    /// Query for group messages
    pub fn get_group_messages<GroupId: AsRef<[u8]>>(
        &self,
        group_id: GroupId,
        sent_after_ns: Option<i64>,
        sent_before_ns: Option<i64>,
        kind: Option<GroupMessageKind>,
        delivery_status: Option<DeliveryStatus>,
        limit: Option<i64>,
    ) -> Result<Vec<StoredGroupMessage>, StorageError> {
        let mut query = dsl::group_messages
            .order(dsl::sent_at_ns.asc())
            .filter(dsl::group_id.eq(group_id.as_ref()))
            .into_boxed();

        if let Some(sent_after) = sent_after_ns {
            query = query.filter(dsl::sent_at_ns.gt(sent_after));
        }

        if let Some(sent_before) = sent_before_ns {
            query = query.filter(dsl::sent_at_ns.lt(sent_before));
        }

        if let Some(kind) = kind {
            query = query.filter(dsl::kind.eq(kind));
        }

        if let Some(status) = delivery_status {
            query = query.filter(dsl::delivery_status.eq(status));
        }

        if let Some(limit) = limit {
            query = query.limit(limit);
        }

        Ok(self.raw_query(|conn| query.load::<StoredGroupMessage>(conn))?)
    }

    /// Get a particular group message
    pub fn get_group_message<MessageId: AsRef<[u8]>>(
        &self,
        id: MessageId,
    ) -> Result<Option<StoredGroupMessage>, StorageError> {
        Ok(self.raw_query(|conn| {
            dsl::group_messages
                .filter(dsl::id.eq(id.as_ref()))
                .first(conn)
                .optional()
        })?)
    }

    pub fn get_group_message_by_timestamp<GroupId: AsRef<[u8]>>(
        &self,
        group_id: GroupId,
        timestamp: i64,
    ) -> Result<Option<StoredGroupMessage>, StorageError> {
        Ok(self.raw_query(|conn| {
            dsl::group_messages
                .filter(dsl::group_id.eq(group_id.as_ref()))
                .filter(dsl::sent_at_ns.eq(timestamp))
                .first(conn)
                .optional()
        })?)
    }

    pub fn set_delivery_status_to_published<MessageId: AsRef<[u8]>>(
        &self,
        msg_id: &MessageId,
        timestamp: u64,
    ) -> Result<usize, StorageError> {
        Ok(self.raw_query(|conn| {
            diesel::update(dsl::group_messages)
                .filter(dsl::id.eq(msg_id.as_ref()))
                .set((
                    dsl::delivery_status.eq(DeliveryStatus::Published),
                    dsl::sent_at_ns.eq(timestamp as i64),
                ))
                .execute(conn)
        })?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        assert_err, assert_ok,
        storage::encrypted_store::{group::tests::generate_group, tests::with_connection},
        utils::test::{rand_time, rand_vec},
        Store,
    };

    fn generate_message(
        kind: Option<GroupMessageKind>,
        group_id: Option<&[u8]>,
        sent_at_ns: Option<i64>,
    ) -> StoredGroupMessage {
        StoredGroupMessage {
            id: rand_vec(),
            group_id: group_id.map(<[u8]>::to_vec).unwrap_or(rand_vec()),
            decrypted_message_bytes: rand_vec(),
            sent_at_ns: sent_at_ns.unwrap_or(rand_time()),
            sender_installation_id: rand_vec(),
            sender_account_address: "0x0".to_string(),
            kind: kind.unwrap_or(GroupMessageKind::Application),
            delivery_status: DeliveryStatus::Unpublished,
        }
    }

    #[test]
    fn it_does_not_error_on_empty_messages() {
        with_connection(|conn| {
            let id = vec![0x0];
            assert_eq!(conn.get_group_message(id).unwrap(), None);
        })
    }

    #[test]
    fn it_gets_messages() {
        with_connection(|conn| {
            let group = generate_group(None);
            let message = generate_message(None, Some(&group.id), None);
            group.store(conn).unwrap();
            let id = message.id.clone();

            message.store(conn).unwrap();

            let stored_message = conn.get_group_message(id);
            assert_eq!(stored_message.unwrap(), Some(message));
        })
    }

    #[test]
    fn it_cannot_insert_message_without_group() {
        use diesel::result::{DatabaseErrorKind::ForeignKeyViolation, Error::DatabaseError};

        with_connection(|conn| {
            let message = generate_message(None, None, None);
            assert_err!(
                message.store(conn),
                StorageError::DieselResult(DatabaseError(ForeignKeyViolation, _))
            );
        })
    }

    #[test]
    fn it_gets_many_messages() {
        use crate::storage::encrypted_store::schema::group_messages::dsl;

        with_connection(|conn| {
            let group = generate_group(None);
            group.store(conn).unwrap();

            for idx in 0..50 {
                let msg = generate_message(None, Some(&group.id), Some(idx));
                assert_ok!(msg.store(conn));
            }

            let count: i64 = conn
                .raw_query(|raw_conn| {
                    dsl::group_messages
                        .select(diesel::dsl::count_star())
                        .first(raw_conn)
                })
                .unwrap();
            assert_eq!(count, 50);

            let messages = conn
                .get_group_messages(&group.id, None, None, None, None, None)
                .unwrap();

            assert_eq!(messages.len(), 50);
            messages.iter().fold(0, |acc, msg| {
                assert!(msg.sent_at_ns >= acc);
                msg.sent_at_ns
            });
        })
    }

    #[test]
    fn it_gets_messages_by_time() {
        with_connection(|conn| {
            let group = generate_group(None);
            group.store(conn).unwrap();

            let messages = vec![
                generate_message(None, Some(&group.id), Some(1_000)),
                generate_message(None, Some(&group.id), Some(100_000)),
                generate_message(None, Some(&group.id), Some(10_000)),
                generate_message(None, Some(&group.id), Some(1_000_000)),
            ];
            assert_ok!(messages.store(conn));
            let message = conn
                .get_group_messages(&group.id, Some(1_000), Some(100_000), None, None, None)
                .unwrap();
            assert_eq!(message.len(), 1);
            assert_eq!(message.first().unwrap().sent_at_ns, 10_000);

            let messages = conn
                .get_group_messages(&group.id, None, Some(100_000), None, None, None)
                .unwrap();
            assert_eq!(messages.len(), 2);

            let messages = conn
                .get_group_messages(&group.id, Some(10_000), None, None, None, None)
                .unwrap();
            assert_eq!(messages.len(), 2);
        })
    }

    #[test]
    fn it_gets_messages_by_kind() {
        with_connection(|conn| {
            let group = generate_group(None);
            group.store(conn).unwrap();

            // just a bunch of random messages so we have something to filter through
            for i in 0..30 {
                match i % 2 {
                    0 => {
                        let msg = generate_message(
                            Some(GroupMessageKind::Application),
                            Some(&group.id),
                            None,
                        );
                        msg.store(conn).unwrap();
                    }
                    _ => {
                        let msg = generate_message(
                            Some(GroupMessageKind::MembershipChange),
                            Some(&group.id),
                            None,
                        );
                        msg.store(conn).unwrap();
                    }
                }
            }

            let application_messages = conn
                .get_group_messages(
                    &group.id,
                    None,
                    None,
                    Some(GroupMessageKind::Application),
                    None,
                    None,
                )
                .unwrap();
            assert_eq!(application_messages.len(), 15);

            let membership_changes = conn
                .get_group_messages(
                    &group.id,
                    None,
                    None,
                    Some(GroupMessageKind::MembershipChange),
                    None,
                    None,
                )
                .unwrap();
            assert_eq!(membership_changes.len(), 15);
        })
    }
}
