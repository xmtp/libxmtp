use diesel::RunQueryDsl;

use crate::{
    ConnectionExt, DbConnection, impl_store, schema::remote_commit_log,
    schema::remote_commit_log::dsl,
};
use diesel::{
    Insertable, Queryable,
    backend::Backend,
    deserialize::{self, FromSql, FromSqlRow},
    expression::AsExpression,
    prelude::*,
    serialize::{self, IsNull, Output, ToSql},
    sql_types::Integer,
    sqlite::Sqlite,
};

use serde::{Deserialize, Serialize};
use xmtp_common::snippet::Snippet;
use xmtp_proto::xmtp::mls::message_contents::CommitResult as ProtoCommitResult;

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = remote_commit_log)]
pub struct NewRemoteCommitLog {
    pub log_sequence_id: i64,
    pub group_id: Vec<u8>,
    pub commit_sequence_id: i64,
    pub commit_result: CommitResult,
    pub applied_epoch_number: i64,
    pub applied_epoch_authenticator: Vec<u8>,
}

impl_store!(NewRemoteCommitLog, remote_commit_log);

#[derive(Insertable, Queryable, Clone)]
#[diesel(table_name = remote_commit_log)]
#[diesel(primary_key(rowid))]
pub struct RemoteCommitLog {
    pub rowid: i32,
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
    pub applied_epoch_number: i64,
    // The state after the commit was applied, or the existing state otherwise
    pub applied_epoch_authenticator: Vec<u8>,
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

impl std::fmt::Debug for RemoteCommitLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "RemoteCommitLog {{ rowid: {:?}, log_sequence_id: {:?}, group_id {:?}, commit_sequence_id: {:?}, commit_result: {:?}, applied_epoch_number: {:?}, applied_epoch_authenticator: {:?} }}",
            self.rowid,
            self.log_sequence_id,
            &self.group_id.snippet(),
            self.commit_sequence_id,
            self.commit_result,
            self.applied_epoch_number,
            &self.applied_epoch_authenticator.snippet()
        )
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

pub enum RemoteCommitLogOrder {
    AscendingByRowid,
    DescendingByRowid,
}

pub trait QueryRemoteCommitLog {
    fn get_latest_remote_log_for_group(
        &self,
        group_id: &[u8],
    ) -> Result<Option<RemoteCommitLog>, crate::ConnectionError>;

    fn get_remote_commit_log_after_cursor(
        &self,
        group_id: &[u8],
        after_cursor: i64,
        order_by: RemoteCommitLogOrder,
    ) -> Result<Vec<RemoteCommitLog>, crate::ConnectionError>;
}

impl<T> QueryRemoteCommitLog for &T
where
    T: QueryRemoteCommitLog,
{
    fn get_latest_remote_log_for_group(
        &self,
        group_id: &[u8],
    ) -> Result<Option<RemoteCommitLog>, crate::ConnectionError> {
        (**self).get_latest_remote_log_for_group(group_id)
    }

    fn get_remote_commit_log_after_cursor(
        &self,
        group_id: &[u8],
        after_cursor: i64,
        order_by: RemoteCommitLogOrder,
    ) -> Result<Vec<RemoteCommitLog>, crate::ConnectionError> {
        (**self).get_remote_commit_log_after_cursor(group_id, after_cursor, order_by)
    }
}

impl<C: ConnectionExt> QueryRemoteCommitLog for DbConnection<C> {
    fn get_latest_remote_log_for_group(
        &self,
        group_id: &[u8],
    ) -> Result<Option<RemoteCommitLog>, crate::ConnectionError> {
        self.raw_query_read(|db| {
            dsl::remote_commit_log
                .filter(remote_commit_log::group_id.eq(group_id))
                .order(remote_commit_log::log_sequence_id.desc())
                .limit(1)
                .first(db)
                .optional()
        })
    }

    fn get_remote_commit_log_after_cursor(
        &self,
        group_id: &[u8],
        after_cursor: i64,
        order: RemoteCommitLogOrder,
    ) -> Result<Vec<RemoteCommitLog>, crate::ConnectionError> {
        // If a group hits more than 2^31 entries on the remote commit log rowid, we will hit this error
        // If we want to address this we can make a new sqlite cursor table/row that stores u64 values
        if after_cursor > i32::MAX as i64 {
            return Err(crate::ConnectionError::Database(
                diesel::result::Error::QueryBuilderError("Cursor value exceeds i32::MAX".into()),
            ));
        }
        let after_cursor: i32 = after_cursor as i32;

        let query = dsl::remote_commit_log
            .filter(dsl::group_id.eq(group_id))
            .filter(dsl::rowid.gt(after_cursor))
            .filter(dsl::commit_sequence_id.ne(0));

        self.raw_query_read(|db| match order {
            RemoteCommitLogOrder::AscendingByRowid => query.order_by(dsl::rowid.asc()).load(db),
            RemoteCommitLogOrder::DescendingByRowid => query.order_by(dsl::rowid.desc()).load(db),
        })
    }
}
