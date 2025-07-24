use diesel::RunQueryDsl;
use std::collections::HashMap;

use crate::{
    ConnectionExt, DbConnection, impl_store, refresh_state::EntityKind, schema::remote_commit_log,
};
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
use xmtp_proto::xmtp::mls::message_contents::CommitResult as ProtoCommitResult;

#[derive(Insertable, Queryable, Debug, Clone)]
#[diesel(table_name = remote_commit_log)]
#[diesel(primary_key(sequence_id))]
pub struct RemoteCommitLog {
    // The sequence ID of the log entry on the server
    pub log_sequence_id: i64,
    // The group ID of the conversation
    pub group_id: Vec<u8>,
    // The sequence ID of the commit being referenced
    pub commit_sequence_id: i64,
    // Whether the commit was successfully applied or not
    // 1 = Applied, all other values are failures matching the protobuf enum
    pub commit_result: CommitResult,
    // The epoch number after the commit was applied, or the existing number otherwise
    pub applied_epoch_number: Option<i64>,
    // The state after the commit was applied, or the existing state otherwise
    pub applied_epoch_authenticator: Option<Vec<u8>>,
}

impl_store!(RemoteCommitLog, remote_commit_log);

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

impl From<ProtoCommitResult> for CommitResult {
    fn from(value: ProtoCommitResult) -> Self {
        match value {
            ProtoCommitResult::Applied => Self::Success,
            ProtoCommitResult::WrongEpoch => Self::WrongEpoch,
            ProtoCommitResult::Undecryptable => Self::Undecryptable,
            ProtoCommitResult::Invalid => Self::Invalid,
            ProtoCommitResult::Unspecified => Self::Unknown,
        }
    }
}

// the max page size for queries
pub const MAX_PAGE_SIZE: u32 = 100;

impl<C: ConnectionExt> DbConnection<C> {
    pub fn get_remote_log_cursors(
        &self,
        conversation_ids: &[Vec<u8>],
    ) -> Result<HashMap<Vec<u8>, i64>, crate::ConnectionError> {
        let mut cursor_map: HashMap<Vec<u8>, i64> = HashMap::new();
        for conversation_id in conversation_ids {
            let cursor = self
                .get_last_cursor_for_id(conversation_id, EntityKind::CommitLogDownload)
                .unwrap_or(0);
            cursor_map.insert(conversation_id.clone(), cursor);
        }
        Ok(cursor_map)
    }
}
