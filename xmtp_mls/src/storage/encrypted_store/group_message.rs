use diesel::{
    backend::Backend,
    deserialize::{self, FromSql, FromSqlRow},
    expression::AsExpression,
    prelude::*,
    serialize::{self, IsNull, Output, ToSql},
    sql_types::Integer,
};
use serde::{Deserialize, Serialize};

use super::{
    db_connection::DbConnection,
    schema::group_messages::{self, dsl},
    Sqlite,
};
use crate::{impl_fetch, impl_store, impl_store_or_ignore, StorageError};

#[derive(
    Debug, Clone, Serialize, Deserialize, Insertable, Identifiable, Queryable, Eq, PartialEq,
)]
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
    /// The Inbox ID of the Sender
    pub sender_inbox_id: String,
    /// We optimistically store messages before sending.
    pub delivery_status: DeliveryStatus,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

#[repr(i32)]
#[derive(Debug, Copy, Clone, Serialize, Deserialize, Eq, PartialEq, AsExpression, FromSqlRow)]
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
#[derive(Debug, Copy, Clone, Serialize, Deserialize, Eq, PartialEq, FromSqlRow, AsExpression)]
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
impl_store_or_ignore!(StoredGroupMessage, group_messages);

#[derive(Default)]
pub struct MsgQueryArgs {
    pub sent_after_ns: Option<i64>,
    pub sent_before_ns: Option<i64>,
    pub kind: Option<GroupMessageKind>,
    pub delivery_status: Option<DeliveryStatus>,
    pub limit: Option<i64>,
    pub direction: Option<SortDirection>,
}

impl DbConnection {
    /// Query for group messages
    pub fn get_group_messages(
        &self,
        group_id: &[u8],
        args: &MsgQueryArgs,
    ) -> Result<Vec<StoredGroupMessage>, StorageError> {
        let mut query = dsl::group_messages
            .filter(dsl::group_id.eq(group_id))
            .into_boxed();

        if let Some(sent_after) = args.sent_after_ns {
            query = query.filter(dsl::sent_at_ns.gt(sent_after));
        }

        if let Some(sent_before) = args.sent_before_ns {
            query = query.filter(dsl::sent_at_ns.lt(sent_before));
        }

        if let Some(kind) = args.kind {
            query = query.filter(dsl::kind.eq(kind));
        }

        if let Some(status) = args.delivery_status {
            query = query.filter(dsl::delivery_status.eq(status));
        }

        query = match args.direction.as_ref().unwrap_or(&SortDirection::Ascending) {
            SortDirection::Ascending => query.order(dsl::sent_at_ns.asc()),
            SortDirection::Descending => query.order(dsl::sent_at_ns.desc()),
        };

        if let Some(limit) = args.limit {
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

    pub fn set_delivery_status_to_failed<MessageId: AsRef<[u8]>>(
        &self,
        msg_id: &MessageId,
    ) -> Result<usize, StorageError> {
        Ok(self.raw_query(|conn| {
            diesel::update(dsl::group_messages)
                .filter(dsl::id.eq(msg_id.as_ref()))
                .set((dsl::delivery_status.eq(DeliveryStatus::Failed),))
                .execute(conn)
        })?)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use super::*;
    use crate::{
        storage::encrypted_store::{group::tests::generate_group, tests::with_connection},
        Store,
    };
    use wasm_bindgen_test::wasm_bindgen_test;
    use xmtp_common::{assert_err, assert_ok, rand_time, rand_vec};

    fn generate_message(
        kind: Option<GroupMessageKind>,
        group_id: Option<&[u8]>,
        sent_at_ns: Option<i64>,
    ) -> StoredGroupMessage {
        StoredGroupMessage {
            id: rand_vec::<24>(),
            group_id: group_id.map(<[u8]>::to_vec).unwrap_or(rand_vec::<24>()),
            decrypted_message_bytes: rand_vec::<24>(),
            sent_at_ns: sent_at_ns.unwrap_or(rand_time()),
            sender_installation_id: rand_vec::<24>(),
            sender_inbox_id: "0x0".to_string(),
            kind: kind.unwrap_or(GroupMessageKind::Application),
            delivery_status: DeliveryStatus::Unpublished,
        }
    }

    #[wasm_bindgen_test(unsupported = tokio::test)]
    async fn it_does_not_error_on_empty_messages() {
        with_connection(|conn| {
            let id = vec![0x0];
            assert_eq!(conn.get_group_message(id).unwrap(), None);
        })
        .await
    }

    #[wasm_bindgen_test(unsupported = tokio::test)]
    async fn it_gets_messages() {
        with_connection(|conn| {
            let group = generate_group(None);
            let message = generate_message(None, Some(&group.id), None);
            group.store(conn).unwrap();
            let id = message.id.clone();

            message.store(conn).unwrap();

            let stored_message = conn.get_group_message(id);
            assert_eq!(stored_message.unwrap(), Some(message));
        })
        .await
    }

    #[wasm_bindgen_test(unsupported = tokio::test)]
    async fn it_cannot_insert_message_without_group() {
        use diesel::result::{DatabaseErrorKind::ForeignKeyViolation, Error::DatabaseError};

        with_connection(|conn| {
            let message = generate_message(None, None, None);
            assert_err!(
                message.store(conn),
                StorageError::DieselResult(DatabaseError(ForeignKeyViolation, _))
            );
        })
        .await
    }

    #[wasm_bindgen_test(unsupported = tokio::test)]
    async fn it_gets_many_messages() {
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
                .get_group_messages(&group.id, &MsgQueryArgs::default())
                .unwrap();

            assert_eq!(messages.len(), 50);
            messages.iter().fold(0, |acc, msg| {
                assert!(msg.sent_at_ns >= acc);
                msg.sent_at_ns
            });
        })
        .await
    }

    #[wasm_bindgen_test(unsupported = tokio::test)]
    async fn it_gets_messages_by_time() {
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
                .get_group_messages(
                    &group.id,
                    &MsgQueryArgs {
                        sent_after_ns: Some(1_000),
                        sent_before_ns: Some(100_000),
                        ..Default::default()
                    },
                )
                .unwrap();
            assert_eq!(message.len(), 1);
            assert_eq!(message.first().unwrap().sent_at_ns, 10_000);

            let messages = conn
                .get_group_messages(
                    &group.id,
                    &MsgQueryArgs {
                        sent_before_ns: Some(100_000),
                        ..Default::default()
                    },
                )
                .unwrap();
            assert_eq!(messages.len(), 2);

            let messages = conn
                .get_group_messages(
                    &group.id,
                    &MsgQueryArgs {
                        sent_after_ns: Some(10_000),
                        ..Default::default()
                    },
                )
                .unwrap();
            assert_eq!(messages.len(), 2);
        })
        .await
    }

    #[wasm_bindgen_test(unsupported = tokio::test)]
    async fn it_gets_messages_by_kind() {
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
                    &MsgQueryArgs {
                        kind: Some(GroupMessageKind::Application),
                        ..Default::default()
                    },
                )
                .unwrap();
            assert_eq!(application_messages.len(), 15);

            let membership_changes = conn
                .get_group_messages(
                    &group.id,
                    &MsgQueryArgs {
                        kind: Some(GroupMessageKind::MembershipChange),
                        ..Default::default()
                    },
                )
                .unwrap();
            assert_eq!(membership_changes.len(), 15);
        })
        .await
    }

    #[wasm_bindgen_test(unsupported = tokio::test)]
    async fn it_orders_messages_by_sent() {
        with_connection(|conn| {
            let group = generate_group(None);
            group.store(conn).unwrap();

            let messages = vec![
                generate_message(None, Some(&group.id), Some(10_000)),
                generate_message(None, Some(&group.id), Some(1_000)),
                generate_message(None, Some(&group.id), Some(100_000)),
                generate_message(None, Some(&group.id), Some(1_000_000)),
            ];

            assert_ok!(messages.store(conn));

            let messages_asc = conn
                .get_group_messages(
                    &group.id,
                    &MsgQueryArgs {
                        direction: Some(SortDirection::Ascending),
                        ..Default::default()
                    },
                )
                .unwrap();
            assert_eq!(messages_asc.len(), 4);
            assert_eq!(messages_asc[0].sent_at_ns, 1_000);
            assert_eq!(messages_asc[1].sent_at_ns, 10_000);
            assert_eq!(messages_asc[2].sent_at_ns, 100_000);
            assert_eq!(messages_asc[3].sent_at_ns, 1_000_000);

            let messages_desc = conn
                .get_group_messages(
                    &group.id,
                    &MsgQueryArgs {
                        direction: Some(SortDirection::Descending),
                        ..Default::default()
                    },
                )
                .unwrap();
            assert_eq!(messages_desc.len(), 4);
            assert_eq!(messages_desc[0].sent_at_ns, 1_000_000);
            assert_eq!(messages_desc[1].sent_at_ns, 100_000);
            assert_eq!(messages_desc[2].sent_at_ns, 10_000);
            assert_eq!(messages_desc[3].sent_at_ns, 1_000);
        })
        .await
    }
}
