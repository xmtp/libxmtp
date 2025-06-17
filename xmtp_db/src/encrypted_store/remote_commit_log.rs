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
    pub log_sequence_id: i64,
    pub group_id: Vec<u8>,
    pub commit_sequence_id: i64,
    pub commit_result: CommitResult,
    pub applied_epoch_number: Option<i64>,
    pub applied_epoch_authenticator: Option<Vec<u8>>,
}

#[repr(i32)]
#[derive(Copy, Clone, Serialize, Deserialize, Eq, PartialEq, AsExpression, FromSqlRow)]
#[diesel(sql_type = Integer)]
pub enum CommitResult {
    Unknown = 0,
    Success = 1,
    WrongEpoch = 2,
    Undecryptable = 3,
    Invalid = 4,
}

impl std::fmt::Debug for CommitResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            CommitResult::Unknown => "Unknown",
            CommitResult::Success => "Success",
            CommitResult::WrongEpoch => "WrongEpoch",
            CommitResult::Undecryptable => "Undecryptable",
            CommitResult::Invalid => "Invalid",
        };
        write!(f, "{}", s)
    }
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
