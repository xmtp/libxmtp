use std::borrow::Cow;
use std::fmt::Debug;
use std::fmt::Display;
use std::ops::Deref;

use diesel::expression::AsExpression;
use diesel::serialize;
use diesel::serialize::Output;
use diesel::serialize::ToSql;
use diesel::sql_types::Binary;
use diesel::sqlite::Sqlite;
use xmtp_proto::types::Cursor;
use xmtp_proto::types::GroupId;

#[derive(Debug, PartialEq, Clone)]
pub struct IntentDependency {
    pub cursor: Cursor,
    pub group_id: GroupId,
}

pub type PayloadHash = PayloadHashRef<'static>;

#[derive(Hash, Clone, Eq, PartialEq, AsExpression)]
#[diesel(sql_type = Binary)]
pub struct PayloadHashRef<'a>(Cow<'a, [u8]>);

impl Deref for PayloadHash {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> AsRef<T> for PayloadHash
where
    T: ?Sized,
    <PayloadHash as Deref>::Target: AsRef<T>,
{
    fn as_ref(&self) -> &T {
        self.deref().as_ref()
    }
}

impl ToSql<Binary, Sqlite> for PayloadHashRef<'_> {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        <Cow<'_, [u8]> as ToSql<Binary, Sqlite>>::to_sql(&self.0, out)
    }
}

impl<'a> Debug for PayloadHashRef<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(&self.0))
    }
}

impl<'a> Display for PayloadHashRef<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(&self.0))
    }
}

impl From<Vec<u8>> for PayloadHash {
    fn from(value: Vec<u8>) -> PayloadHash {
        PayloadHashRef(Cow::from(value))
    }
}

impl<'a> From<&'a [u8]> for PayloadHashRef<'a> {
    fn from(value: &'a [u8]) -> Self {
        PayloadHashRef(Cow::from(value))
    }
}
