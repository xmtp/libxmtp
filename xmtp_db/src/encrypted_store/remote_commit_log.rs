use crate::schema::remote_commit_log;
use diesel::{
    Insertable, Queryable, deserialize::FromSqlRow, expression::AsExpression, sql_types::Integer,
};
use serde::{Deserialize, Serialize};

#[derive(Insertable, Queryable, Debug, Clone)]
#[diesel(table_name = remote_commit_log)]
#[diesel(primary_key(created_at_ns))]
pub struct RemoteCommitLog {
    pub created_at_ns: i64,
    pub group_id: Option<Vec<u8>>,
    pub last_state_hash: Option<Vec<u8>>,
    pub epoch_authenticator: Vec<u8>,
    pub result: CommitResult,
    pub state_hash: Option<Vec<u8>>,
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
