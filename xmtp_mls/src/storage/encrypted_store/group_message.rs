use diesel::{
    backend::Backend,
    deserialize::{self, FromSql, FromSqlRow},
    expression::AsExpression,
    prelude::*,
    serialize::{self, IsNull, Output, ToSql},
    sql_types::Integer,
    sqlite::Sqlite,
};

use super::{schema::group_messages, DbConnection, EncryptedMessageStore};
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
    /// Group Message Kind Enum
    pub kind: GroupMessageKind,
    /// The ID of the App Installation this message was sent from.
    pub sender_installation_id: Vec<u8>,
    /// Network wallet address of the Sender
    pub sender_wallet_address: String,
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, AsExpression, FromSqlRow)]
#[diesel(sql_type = Integer)]
pub enum GroupMessageKind {
    Application = 1,
    MemberAdded = 2,
    MemberRemoved = 3,
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
            2 => Ok(GroupMessageKind::MemberAdded),
            3 => Ok(GroupMessageKind::MemberRemoved),
            x => Err(format!("Unrecognized variant {}", x).into()),
        }
    }
}

impl_fetch!(StoredGroupMessage, group_messages, Vec<u8>);
impl_store!(StoredGroupMessage, group_messages);

impl EncryptedMessageStore {
    /// Query for group messages
    pub fn get_group_messages<GroupId: AsRef<super::group::ID>>(
        &self,
        conn: &mut DbConnection,
        group_id: GroupId,
        sent_after: Option<i64>,
        sent_before: Option<i64>,
        kind: Option<GroupMessageKind>,
    ) -> Result<Vec<StoredGroupMessage>, StorageError> {
        use super::schema::group_messages::dsl;

        let mut query = dsl::group_messages
            .filter(dsl::group_id.eq(group_id.as_ref()))
            .into_boxed();

        if let Some(sent_after) = sent_after {
            query = query.filter(dsl::sent_at_ns.gt(sent_after));
        }

        if let Some(sent_before) = sent_before {
            query = query.filter(dsl::sent_at_ns.lt(sent_before));
        }

        if let Some(kind) = kind {
            query = query.filter(dsl::kind.eq(kind));
        }
        Ok(query.load::<StoredGroupMessage>(conn)?)
    }

    /// Get a particular group message
    pub fn get_group_message<MessageId: AsRef<[u8]>>(
        &self,
        id: MessageId,
        conn: &mut DbConnection,
    ) -> Result<Option<StoredGroupMessage>, StorageError> {
        use super::schema::group_messages::dsl;
        Ok(dsl::group_messages
            .filter(dsl::id.eq(id.as_ref()))
            .first(conn)
            .optional()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        assert_err, assert_ok,
        storage::encrypted_store::{
            group::tests::generate_group,
            tests::{rand_bytes, rand_time, with_store},
        },
        Store,
    };

    fn generate_message(
        kind: Option<GroupMessageKind>,
        group_id: Option<&[u8]>,
        sent_at_ns: Option<i64>,
    ) -> StoredGroupMessage {
        StoredGroupMessage {
            id: rand_bytes(32),
            group_id: group_id.map(<[u8]>::to_vec).unwrap_or(rand_bytes(32)),
            decrypted_message_bytes: rand_bytes(600),
            sent_at_ns: sent_at_ns.unwrap_or(rand_time()),
            sender_installation_id: rand_bytes(64),
            sender_wallet_address: "0x0".to_string(),
            kind: kind.unwrap_or(GroupMessageKind::Application),
        }
    }

    #[test]
    fn it_does_not_error_on_empty_messages() {
        with_store(|store, mut conn| {
            let id = vec![0x0];
            assert_ok!(store.get_group_message(&id, &mut conn), None);
        })
    }

    #[test]
    fn it_gets_messages() {
        with_store(|store, mut conn| {
            let group = generate_group(None);
            let message = generate_message(None, Some(&group.id), None);
            group.store(&mut conn).unwrap();
            let id = message.id.clone();

            message.store(&mut conn).unwrap();

            let stored_message = store.get_group_message(&id, &mut conn);
            assert_ok!(stored_message, Some(message));
        })
    }

    #[test]
    fn it_cannot_insert_message_without_group() {
        use diesel::result::{DatabaseErrorKind::ForeignKeyViolation, Error::DatabaseError};

        with_store(|_, mut conn| {
            let message = generate_message(None, None, None);
            assert_err!(
                message.store(&mut conn),
                StorageError::DieselResult(DatabaseError(ForeignKeyViolation, _))
            );
        })
    }

    #[test]
    fn it_gets_many_messages() {
        use crate::storage::encrypted_store::schema::group_messages::dsl;

        with_store(|store, mut conn| {
            let group = generate_group(None);
            group.store(&mut conn).unwrap();

            for _ in 0..4_000 {
                let msg = generate_message(None, Some(&group.id), None);
                assert_ok!(msg.store(&mut conn));
            }

            let count: i64 = dsl::group_messages
                .select(diesel::dsl::count_star())
                .first(&mut conn)
                .unwrap();
            assert_eq!(count, 4_000);

            let messages = store
                .get_group_messages(&mut conn, &group.id, None, None, None)
                .unwrap();
            assert_eq!(messages.len(), 4_000);
        })
    }

    #[test]
    fn it_gets_messages_by_time() {
        with_store(|store, mut conn| {
            let group = generate_group(None);
            group.store(&mut conn).unwrap();

            let messages = vec![
                generate_message(None, Some(&group.id), Some(1_000)),
                generate_message(None, Some(&group.id), Some(10_000)),
                generate_message(None, Some(&group.id), Some(100_000)),
                generate_message(None, Some(&group.id), Some(1_000_000)),
            ];
            assert_ok!(messages.store(&mut conn));
            let message = store
                .get_group_messages(&mut conn, &group.id, Some(1_000), Some(100_000), None)
                .unwrap();
            assert_eq!(message.len(), 1);
            assert_eq!(message.first().unwrap().sent_at_ns, 10_000);
        })
    }

    #[test]
    fn it_gets_messages_by_kind() {
        with_store(|store, mut conn| {
            let group = generate_group(None);
            group.store(&mut conn).unwrap();

            // just a bunch of random messages so we have something to filter through
            for i in 0..4_000 {
                match i % 4 {
                    0 | 1 => {
                        let msg = generate_message(
                            Some(GroupMessageKind::Application),
                            Some(&group.id),
                            None,
                        );
                        msg.store(&mut conn).unwrap();
                    }
                    2 => {
                        let msg = generate_message(
                            Some(GroupMessageKind::MemberRemoved),
                            Some(&group.id),
                            None,
                        );
                        msg.store(&mut conn).unwrap();
                    }
                    3 | _ => {
                        let msg = generate_message(
                            Some(GroupMessageKind::MemberAdded),
                            Some(&group.id),
                            None,
                        );
                        msg.store(&mut conn).unwrap();
                    }
                }
            }

            let application_messages = store
                .get_group_messages(
                    &mut conn,
                    &group.id,
                    None,
                    None,
                    Some(GroupMessageKind::Application),
                )
                .unwrap();
            assert_eq!(application_messages.len(), 2_000);

            let member_removed = store
                .get_group_messages(
                    &mut conn,
                    &group.id,
                    None,
                    None,
                    Some(GroupMessageKind::MemberAdded),
                )
                .unwrap();
            assert_eq!(member_removed.len(), 1_000);

            let member_added = store
                .get_group_messages(
                    &mut conn,
                    &group.id,
                    None,
                    None,
                    Some(GroupMessageKind::MemberRemoved),
                )
                .unwrap();
            assert_eq!(member_added.len(), 1_000);
        })
    }
}
