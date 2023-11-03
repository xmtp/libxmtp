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
    pub fn get_group_messages(
        &self,
        conn: &mut DbConnection,
        group_id: &[u8],
        sent_after: Option<i64>,
        sent_before: Option<i64>,
        kind: Option<i32>,
    ) -> Result<Vec<StoredGroupMessage>, StorageError> {
        use super::schema::group_messages::dsl;

        let mut query = dsl::group_messages
            .filter(dsl::group_id.eq(group_id))
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
    pub fn get_group_message(
        &self,
        id: &[u8],
        conn: &mut DbConnection,
    ) -> Result<Option<StoredGroupMessage>, StorageError> {
        use super::schema::group_messages::dsl;
        Ok(dsl::group_messages
            .filter(dsl::id.eq(id))
            .first(conn)
            .optional()?)
    }
}

#[cfg(test)]
mod tests {
    use rand::Rng;

    use super::*;
    use crate::{
        storage::encrypted_store::{schema::groups::dsl::groups, tests::with_store},
        Fetch, Store,
    };

    fn rand_bytes(length: usize) -> Vec<u8> {
        (0..length).map(|_| rand::random::<u8>()).collect()
    }

    fn generate_message(kind: Option<GroupMessageKind>) -> StoredGroupMessage {
        let mut rng = rand::thread_rng();

        StoredGroupMessage {
            id: rand_bytes(32),
            group_id: rand_bytes(32),
            decrypted_message_bytes: rand_bytes(600),
            sent_at_ns: rng.gen(),
            sender_installation_id: rand_bytes(64),
            sender_wallet_address: "0x0".to_string(),
            kind: kind.unwrap_or(GroupMessageKind::Application),
        }
    }

    #[test]
    fn no_error_on_empty_messages() {
        with_store(|store, conn| {
            let id = vec![0x0];
            let mut conn = store.conn().unwrap();
            // TODO: could replace w/ something like an assert_ok macro in tokio-test. not sure
            // it's worth pulling the whole library for that
            assert!(matches!(store.get_group_message(&id, &mut conn), Ok(_)));
        })
    }

    #[test]
    fn it_gets_messages() {
        with_store(|store, conn| {
            let message = generate_message(None);
            let id = message.id.clone();
            message.store(conn).unwrap();
            let stored_message = store.get_group_message(&id, conn).ok().flatten().unwrap();
            assert_eq!(message, stored_message);
        })
    }
}
