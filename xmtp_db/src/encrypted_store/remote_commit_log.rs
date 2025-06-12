use crate::schema::remote_commit_log;
use diesel::{
    Insertable, Queryable,
    backend::Backend,
    deserialize::{self, FromSql, FromSqlRow},
    expression::AsExpression,
    serialize::{self, IsNull, Output, ToSql},
    sql_types::Integer,
    sqlite::Sqlite,
};
use serde::{Deserialize, Serialize};

#[derive(Insertable, Queryable, Debug, Clone)]
#[diesel(table_name = remote_commit_log)]
#[diesel(primary_key(sequence_id))]
pub struct RemoteCommitLog {
    pub sequence_id: i64,
    pub group_id: Option<Vec<u8>>,
    pub epoch_authenticator: Vec<u8>,
    pub last_epoch_authenticator: Option<Vec<u8>>,
    pub result: CommitResult,
    pub epoch_number: Option<i64>,
}

#[repr(i32)]
#[derive(Debug, Copy, Clone, Serialize, Deserialize, Eq, PartialEq, AsExpression, FromSqlRow)]
#[diesel(sql_type = Integer)]
pub enum CommitResult {
    Unknown = 0,
    Success = 1,
    WrongEpoch = 2,
    Undecryptable = 3,
    Invalid = 4,
}

impl ToSql<Integer, Sqlite> for CommitResult
where
    i32: ToSql<Integer, Sqlite>,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(*self as i32);
        Ok(IsNull::No)
    }
}

impl FromSql<Integer, Sqlite> for CommitResult
where
    i32: FromSql<Integer, Sqlite>,
{
    fn from_sql(bytes: <Sqlite as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        match i32::from_sql(bytes)? {
            0 => Ok(Self::Unknown),
            1 => Ok(Self::Success),
            2 => Ok(Self::WrongEpoch),
            3 => Ok(Self::Undecryptable),
            4 => Ok(Self::Invalid),
            x => Err(format!("Unrecognized variant {}", x).into()),
        }
    }
}
