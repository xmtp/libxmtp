use super::{group, schema::group_intents};
use crate::{impl_fetch, impl_store};

use diesel::prelude::*;
use diesel::{
    backend::Backend,
    deserialize::{self, FromSql, FromSqlRow},
    expression::AsExpression,
    serialize::{self, IsNull, Output, ToSql},
    sql_types::Integer,
    sqlite::Sqlite,
};

pub type ID = i32;

#[derive(Queryable, Identifiable, Debug, Clone)]
#[diesel(table_name = group_intents)]
#[diesel(primary_key(id))]
pub struct StoredGroupIntent {
    pub id: ID,
    pub kind: IntentKind,
    pub group_id: group::ID,
    pub data: Vec<u8>,
    pub state: IntentState,
    pub payload_hash: Option<Vec<u8>>,
    pub post_commit_data: Option<Vec<u8>>,
}

impl_fetch!(StoredGroupIntent, group_intents, ID);

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = group_intents)]
pub struct NewGroupIntent {
    pub kind: i32,
    pub group_id: Vec<u8>,
    pub data: Vec<u8>,
    pub state: i32,
}

impl_store!(NewGroupIntent, group_intents);

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, AsExpression, FromSqlRow)]
#[diesel(sql_type = Integer)]
/// Status of membership in a group, once a user sends a request to join
pub enum IntentKind {
    SendMessage = 1,
    AddMembers = 2,
    RemoveMembers = 3,
    KeyUpdate = 4,
}

impl ToSql<Integer, Sqlite> for IntentKind
where
    i32: ToSql<Integer, Sqlite>,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(*self as i32);
        Ok(IsNull::No)
    }
}

impl FromSql<Integer, Sqlite> for IntentKind
where
    i32: FromSql<Integer, Sqlite>,
{
    fn from_sql(bytes: <Sqlite as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        match i32::from_sql(bytes)? {
            1 => Ok(IntentKind::SendMessage),
            2 => Ok(IntentKind::AddMembers),
            3 => Ok(IntentKind::RemoveMembers),
            4 => Ok(IntentKind::KeyUpdate),
            x => Err(format!("Unrecognized variant {}", x).into()),
        }
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, AsExpression, FromSqlRow)]
#[diesel(sql_type = Integer)]
/// Status of membership in a group, once a user sends a request to join
pub enum IntentState {
    ToSend = 1,
    Published = 2,
    Committed = 3,
}

impl ToSql<Integer, Sqlite> for IntentState
where
    i32: ToSql<Integer, Sqlite>,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(*self as i32);
        Ok(IsNull::No)
    }
}

impl FromSql<Integer, Sqlite> for IntentState
where
    i32: FromSql<Integer, Sqlite>,
{
    fn from_sql(bytes: <Sqlite as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        match i32::from_sql(bytes)? {
            1 => Ok(IntentState::ToSend),
            2 => Ok(IntentState::Published),
            3 => Ok(IntentState::Committed),
            x => Err(format!("Unrecognized variant {}", x).into()),
        }
    }
}
