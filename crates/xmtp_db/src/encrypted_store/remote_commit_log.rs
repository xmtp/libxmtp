#[cfg(feature = "sync")]
use diesel::RunQueryDsl;

#[cfg(feature = "sync")]
use crate::{
    ConnectionExt, DbConnection, impl_store, schema::remote_commit_log,
    schema::remote_commit_log::dsl,
};
#[cfg(feature = "sync")]
use diesel::{
    Insertable, Queryable, deserialize::FromSqlRow, expression::AsExpression, prelude::*,
    sql_types::Integer,
};

use serde::{Deserialize, Serialize};
use xmtp_common::snippet::Snippet;
use xmtp_proto::xmtp::mls::message_contents::CommitResult as ProtoCommitResult;

use xmtp_proto::types::GroupId;
#[derive(Debug, Clone)]
#[cfg_attr(feature = "sync", derive(Insertable))]
#[cfg_attr(feature = "sync", diesel(table_name = remote_commit_log))]
pub struct NewRemoteCommitLog {
    pub log_sequence_id: i64,
    pub group_id: GroupId,
    pub commit_sequence_id: i64,
    pub commit_result: CommitResult,
    pub applied_epoch_number: i64,
    pub applied_epoch_authenticator: Vec<u8>,
}

#[cfg(feature = "sync")]
impl_store!(NewRemoteCommitLog, remote_commit_log);

#[derive(Clone)]
#[cfg_attr(feature = "sync", derive(Insertable, Queryable))]
#[cfg_attr(feature = "sync", diesel(table_name = remote_commit_log))]
#[cfg_attr(feature = "sync", diesel(primary_key(rowid)))]
#[derive(xmtp_macro::PgModel)]
#[xmtp(table = "remote_commit_log")]
pub struct RemoteCommitLog {
    pub rowid: i32,
    // The sequence ID of the log entry on the server
    pub log_sequence_id: i64,
    // The group ID of the conversation
    pub group_id: GroupId,
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

#[cfg(feature = "sync")]
impl_store!(RemoteCommitLog, remote_commit_log);

#[repr(i32)]
#[derive(Copy, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "sync", derive(AsExpression, FromSqlRow))]
#[cfg_attr(feature = "sync", diesel(sql_type = Integer))]
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
            self.group_id.as_slice().snippet(),
            self.commit_sequence_id,
            self.commit_result,
            self.applied_epoch_number,
            self.applied_epoch_authenticator.snippet()
        )
    }
}

crate::impl_sql_int_enum!(CommitResult {
    Unknown = 0,
    Success = 1,
    WrongEpoch = 2,
    Undecryptable = 3,
    Invalid = 4,
});

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

pub enum RemoteCommitLogOrder {
    AscendingByRowid,
    DescendingByRowid,
}

#[maybe_async::maybe_async(AFIT)]
pub trait QueryRemoteCommitLog {
    async fn get_latest_remote_log_for_group(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<RemoteCommitLog>, crate::ConnectionError>;

    async fn get_remote_commit_log_after_cursor(
        &self,
        group_id: &GroupId,
        after_cursor: i64,
        order_by: RemoteCommitLogOrder,
    ) -> Result<Vec<RemoteCommitLog>, crate::ConnectionError>;
}

#[maybe_async::maybe_async(AFIT)]
impl<T> QueryRemoteCommitLog for &T
where
    T: QueryRemoteCommitLog,
{
    async fn get_latest_remote_log_for_group(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<RemoteCommitLog>, crate::ConnectionError> {
        (**self).get_latest_remote_log_for_group(group_id).await
    }

    async fn get_remote_commit_log_after_cursor(
        &self,
        group_id: &GroupId,
        after_cursor: i64,
        order_by: RemoteCommitLogOrder,
    ) -> Result<Vec<RemoteCommitLog>, crate::ConnectionError> {
        (**self)
            .get_remote_commit_log_after_cursor(group_id, after_cursor, order_by)
            .await
    }
}

#[cfg(feature = "sync")]
impl<C: ConnectionExt> QueryRemoteCommitLog for DbConnection<C> {
    fn get_latest_remote_log_for_group(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<RemoteCommitLog>, crate::ConnectionError> {
        self.raw_query(|db| {
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
        group_id: &GroupId,
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

        self.raw_query(|db| match order {
            RemoteCommitLogOrder::AscendingByRowid => query.order_by(dsl::rowid.asc()).load(db),
            RemoteCommitLogOrder::DescendingByRowid => query.order_by(dsl::rowid.desc()).load(db),
        })
    }
}

/// sqlx backend -- Postgres only. See the note on `QueryGroupVersion`'s impl for
/// why this is gated `not(feature = "sync")`.
#[cfg(all(feature = "async", not(feature = "sync"), not(target_arch = "wasm32")))]
mod pg_impl {
    use super::*;
    use crate::pg::{PgDb, PgModel};
    use sqlx::Row;

    /// Decode via the `FromRow` that `#[derive(PgModel)]` emits: by column
    /// name, from the same fields the column list comes from.
    fn log(row: &sqlx::postgres::PgRow) -> Result<RemoteCommitLog, crate::ConnectionError> {
        use sqlx::FromRow;
        Ok(RemoteCommitLog::from_row(row)?)
    }

    impl QueryRemoteCommitLog for PgDb {
        async fn get_latest_remote_log_for_group(
            &self,
            group_id: &GroupId,
        ) -> Result<Option<RemoteCommitLog>, crate::ConnectionError> {
            let mut c = self.conn().await?;
            let row = sqlx::query(&format!(
                "SELECT {} FROM remote_commit_log WHERE group_id = $1 \
                 ORDER BY log_sequence_id DESC LIMIT 1",
                RemoteCommitLog::select_columns()
            ))
            .bind(group_id)
            .fetch_optional(&mut *c)
            .await?;
            row.as_ref().map(log).transpose()
        }

        async fn get_remote_commit_log_after_cursor(
            &self,
            group_id: &GroupId,
            after_cursor: i64,
            order: RemoteCommitLogOrder,
        ) -> Result<Vec<RemoteCommitLog>, crate::ConnectionError> {
            // `rowid` is a 32-bit serial, so a cursor past i32::MAX cannot name a
            // real row. The sync track reports this as a query-builder error;
            // there is no diesel error type here, so it surfaces as InvalidQuery.
            if after_cursor > i32::MAX as i64 {
                return Err(crate::ConnectionError::InvalidQuery(
                    "Cursor value exceeds i32::MAX".into(),
                ));
            }
            let after_cursor = after_cursor as i32;

            // The two orderings are separate literals rather than an interpolated
            // direction: the sort key never comes from a caller-supplied string.
            let sql = format!(
                "SELECT {} FROM remote_commit_log \
                 WHERE group_id = $1 AND rowid > $2 AND commit_sequence_id <> 0 ORDER BY rowid {}",
                RemoteCommitLog::select_columns(),
                match order {
                    RemoteCommitLogOrder::AscendingByRowid => "ASC",
                    RemoteCommitLogOrder::DescendingByRowid => "DESC",
                }
            );

            let mut c = self.conn().await?;
            let rows = sqlx::query(&sql)
                .bind(group_id)
                .bind(after_cursor)
                .fetch_all(&mut *c)
                .await?;
            rows.iter().map(log).collect()
        }
    }
}
