use super::remote_commit_log::CommitResult;
#[cfg(feature = "sync")]
use super::{DbConnection, schema::local_commit_log::dsl};
#[cfg(feature = "sync")]
use crate::{ConnectionExt, impl_store, schema::local_commit_log};
#[cfg(feature = "sync")]
use diesel::{Insertable, Queryable, prelude::*};
use xmtp_common::snippet::Snippet;
use xmtp_proto::xmtp::mls::message_contents::PlaintextCommitLogEntry;

use xmtp_proto::types::GroupId;
pub enum CommitType {
    GroupCreation,
    BackupRestore,
    Welcome,
    KeyUpdate,
    MetadataUpdate,
    UpdateGroupMembership,
    UpdateAdminList,
    UpdatePermission,
    /// A commit (authored by anyone) that removed this installation's leaf
    /// from the group. The member merges only the public part of such a
    /// commit and cannot derive the new epoch's secrets, so the logged entry
    /// records the pre-commit epoch and authenticator.
    RemovedFromGroup,
}

impl std::fmt::Display for CommitType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let description = match self {
            CommitType::GroupCreation => "GroupCreation",
            CommitType::BackupRestore => "BackupRestore",
            CommitType::Welcome => "Welcome",
            CommitType::KeyUpdate => "KeyUpdate",
            CommitType::MetadataUpdate => "MetadataUpdate",
            CommitType::UpdateGroupMembership => "UpdateGroupMembership",
            CommitType::UpdateAdminList => "UpdateAdminList",
            CommitType::UpdatePermission => "UpdatePermission",
            CommitType::RemovedFromGroup => "RemovedFromGroup",
        };
        write!(f, "{}", description)
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "sync", derive(Insertable))]
#[cfg_attr(feature = "sync", diesel(table_name = local_commit_log))]
pub struct NewLocalCommitLog {
    pub group_id: GroupId,
    pub commit_sequence_id: i64,
    pub last_epoch_authenticator: Vec<u8>,
    pub commit_result: CommitResult,
    pub applied_epoch_number: i64,
    pub applied_epoch_authenticator: Vec<u8>,
    pub error_message: Option<String>,
    pub sender_inbox_id: Option<String>,
    pub sender_installation_id: Option<Vec<u8>>,
    pub commit_type: Option<String>,
}

#[derive(Clone)]
#[cfg_attr(feature = "sync", derive(Queryable))]
#[cfg_attr(feature = "sync", diesel(table_name = local_commit_log))]
#[cfg_attr(feature = "sync", diesel(primary_key(id)))]
pub struct LocalCommitLog {
    pub rowid: i32,
    pub group_id: GroupId,
    pub commit_sequence_id: i64,
    pub last_epoch_authenticator: Vec<u8>,
    pub commit_result: CommitResult,
    pub applied_epoch_number: i64,
    pub applied_epoch_authenticator: Vec<u8>,
    pub error_message: Option<String>,
    pub sender_inbox_id: Option<String>,
    pub sender_installation_id: Option<Vec<u8>>,
    pub commit_type: Option<String>,
}

impl From<&LocalCommitLog> for PlaintextCommitLogEntry {
    fn from(local_commit_log: &LocalCommitLog) -> Self {
        PlaintextCommitLogEntry {
            group_id: local_commit_log.group_id.to_vec(),
            commit_sequence_id: local_commit_log.commit_sequence_id as u64,
            last_epoch_authenticator: local_commit_log.last_epoch_authenticator.clone(),
            commit_result: local_commit_log.commit_result.into(),
            applied_epoch_number: local_commit_log.applied_epoch_number as u64,
            applied_epoch_authenticator: local_commit_log.applied_epoch_authenticator.clone(),
        }
    }
}

impl From<CommitResult> for i32 {
    fn from(commit_result: CommitResult) -> Self {
        match commit_result {
            CommitResult::Success => {
                xmtp_proto::xmtp::mls::message_contents::CommitResult::Applied as i32
            }
            CommitResult::WrongEpoch => {
                xmtp_proto::xmtp::mls::message_contents::CommitResult::WrongEpoch as i32
            }
            CommitResult::Undecryptable => {
                xmtp_proto::xmtp::mls::message_contents::CommitResult::Undecryptable as i32
            }
            CommitResult::Invalid => {
                xmtp_proto::xmtp::mls::message_contents::CommitResult::Invalid as i32
            }
            CommitResult::Unknown => {
                xmtp_proto::xmtp::mls::message_contents::CommitResult::Unspecified as i32
            }
        }
    }
}

#[cfg(feature = "sync")]
impl_store!(NewLocalCommitLog, local_commit_log);

impl std::fmt::Debug for LocalCommitLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "LocalCommitLog {{ rowid: {:?}, group_id {:?}, commit_sequence_id: {:?}, last_epoch_authenticator: {:?}, commit_result: {:?}, error_message: {:?}, applied_epoch_number: {:?}, applied_epoch_authenticator: {:?}, sender_inbox_id: {:?}, sender_installation_id: {:?}, commit_type: {:?} }}",
            self.rowid,
            self.group_id.as_slice().snippet(),
            self.commit_sequence_id,
            self.last_epoch_authenticator.snippet(),
            self.commit_result,
            self.error_message,
            self.applied_epoch_number,
            self.applied_epoch_authenticator.snippet(),
            self.sender_inbox_id.snippet(),
            self.sender_installation_id.snippet(),
            self.commit_type
        )
    }
}

pub enum LocalCommitLogOrder {
    AscendingByRowid,
    DescendingByRowid,
}

#[maybe_async::maybe_async(AFIT)]
pub trait QueryLocalCommitLog {
    async fn get_group_logs(
        &self,
        group_id: &GroupId,
    ) -> Result<Vec<LocalCommitLog>, crate::ConnectionError>;

    // Local commit log entries are returned sorted in ascending order of `rowid`
    // Entries with `commit_sequence_id` = 0 should not be published to the remote commit log
    async fn get_local_commit_log_after_cursor(
        &self,
        group_id: &GroupId,
        after_cursor: i64,
        order_by: LocalCommitLogOrder,
    ) -> Result<Vec<LocalCommitLog>, crate::ConnectionError>;

    async fn get_latest_log_for_group(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<LocalCommitLog>, crate::ConnectionError>;

    async fn get_local_commit_log_cursor(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<i32>, crate::ConnectionError>;

    /// Rowid of the most recent chain-start entry for this group, if any.
    /// Chain-start entries have `commit_sequence_id == 0` (Welcome /
    /// GroupCreation / BackupRestore) and mark the beginning of the member's
    /// current membership session.
    async fn get_latest_chain_start_rowid(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<i32>, crate::ConnectionError>;
}

#[maybe_async::maybe_async(AFIT)]
impl<T> QueryLocalCommitLog for &T
where
    T: QueryLocalCommitLog,
{
    async fn get_group_logs(
        &self,
        group_id: &GroupId,
    ) -> Result<Vec<LocalCommitLog>, crate::ConnectionError> {
        (**self).get_group_logs(group_id).await
    }

    async fn get_local_commit_log_after_cursor(
        &self,
        group_id: &GroupId,
        after_cursor: i64,
        order_by: LocalCommitLogOrder,
    ) -> Result<Vec<LocalCommitLog>, crate::ConnectionError> {
        (**self)
            .get_local_commit_log_after_cursor(group_id, after_cursor, order_by)
            .await
    }

    async fn get_latest_log_for_group(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<LocalCommitLog>, crate::ConnectionError> {
        (**self).get_latest_log_for_group(group_id).await
    }

    async fn get_local_commit_log_cursor(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<i32>, crate::ConnectionError> {
        (**self).get_local_commit_log_cursor(group_id).await
    }

    async fn get_latest_chain_start_rowid(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<i32>, crate::ConnectionError> {
        (**self).get_latest_chain_start_rowid(group_id).await
    }
}

#[cfg(feature = "sync")]
impl<C: ConnectionExt> QueryLocalCommitLog for DbConnection<C> {
    fn get_group_logs(
        &self,
        group_id: &GroupId,
    ) -> Result<Vec<LocalCommitLog>, crate::ConnectionError> {
        self.raw_query(|db| {
            dsl::local_commit_log
                .filter(dsl::group_id.eq(group_id))
                .order_by(dsl::rowid.asc())
                .load(db)
        })
    }

    // Local commit log entries are sorted by `rowid`
    // Entries with `commit_sequence_id` = 0 should not be published to the remote commit log
    fn get_local_commit_log_after_cursor(
        &self,
        group_id: &GroupId,
        after_cursor: i64,
        order: LocalCommitLogOrder,
    ) -> Result<Vec<LocalCommitLog>, crate::ConnectionError> {
        // i64 cursor is populated by i32 local_commit_log rowid value, so we should never hit this error
        if after_cursor > i32::MAX as i64 {
            return Err(crate::ConnectionError::Database(
                diesel::result::Error::QueryBuilderError("Cursor value exceeds i32::MAX".into()),
            ));
        }
        let after_cursor = after_cursor as i32;

        let query = dsl::local_commit_log
            .filter(dsl::group_id.eq(group_id))
            .filter(dsl::rowid.gt(after_cursor))
            .filter(dsl::commit_sequence_id.ne(0));

        self.raw_query(|db| match order {
            LocalCommitLogOrder::AscendingByRowid => query.order_by(dsl::rowid.asc()).load(db),
            LocalCommitLogOrder::DescendingByRowid => query.order_by(dsl::rowid.desc()).load(db),
        })
    }

    fn get_latest_log_for_group(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<LocalCommitLog>, crate::ConnectionError> {
        self.raw_query(|db| {
            dsl::local_commit_log
                .filter(dsl::group_id.eq(group_id))
                .order_by(dsl::rowid.desc())
                .limit(1)
                .first(db)
                .optional()
        })
    }

    fn get_local_commit_log_cursor(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<i32>, crate::ConnectionError> {
        let query = dsl::local_commit_log
            .filter(dsl::group_id.eq(group_id))
            .select(dsl::rowid)
            .order(dsl::rowid.desc())
            .limit(1);

        self.raw_query(|conn| query.first::<i32>(conn).optional())
    }

    fn get_latest_chain_start_rowid(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<i32>, crate::ConnectionError> {
        let query = dsl::local_commit_log
            .filter(dsl::group_id.eq(group_id))
            .filter(dsl::commit_sequence_id.eq(0))
            .select(dsl::rowid)
            .order(dsl::rowid.desc())
            .limit(1);

        self.raw_query(|conn| query.first::<i32>(conn).optional())
    }
}

/// sqlx backend -- Postgres only. See the note on `QueryGroupVersion`'s impl for
/// why this is gated `not(feature = "sync")`.
#[cfg(all(feature = "async", not(feature = "sync"), not(target_arch = "wasm32")))]
mod pg_impl {
    use super::*;
    use crate::pg::PgDb;
    use sqlx::Row;

    const COLUMNS: &str = "rowid, group_id, commit_sequence_id, last_epoch_authenticator, \
                           commit_result, applied_epoch_number, applied_epoch_authenticator, \
                           error_message, sender_inbox_id, sender_installation_id, commit_type";

    fn log(row: &sqlx::postgres::PgRow) -> Result<LocalCommitLog, crate::ConnectionError> {
        Ok(LocalCommitLog {
            rowid: row.try_get(0)?,
            group_id: row.try_get(1)?,
            commit_sequence_id: row.try_get(2)?,
            last_epoch_authenticator: row.try_get(3)?,
            commit_result: row.try_get(4)?,
            applied_epoch_number: row.try_get(5)?,
            applied_epoch_authenticator: row.try_get(6)?,
            error_message: row.try_get(7)?,
            sender_inbox_id: row.try_get(8)?,
            sender_installation_id: row.try_get(9)?,
            commit_type: row.try_get(10)?,
        })
    }

    impl QueryLocalCommitLog for PgDb {
        async fn get_group_logs(
            &self,
            group_id: &GroupId,
        ) -> Result<Vec<LocalCommitLog>, crate::ConnectionError> {
            let mut c = self.conn().await?;
            let rows = sqlx::query(&format!(
                "SELECT {COLUMNS} FROM local_commit_log WHERE group_id = $1 ORDER BY rowid ASC"
            ))
            .bind(group_id)
            .fetch_all(&mut *c)
            .await?;
            rows.iter().map(log).collect()
        }

        /// Entries with `commit_sequence_id = 0` are chain starts and are never
        /// published to the remote commit log, so they are excluded here.
        async fn get_local_commit_log_after_cursor(
            &self,
            group_id: &GroupId,
            after_cursor: i64,
            order: LocalCommitLogOrder,
        ) -> Result<Vec<LocalCommitLog>, crate::ConnectionError> {
            // The cursor is populated from an i32 rowid, so this is unreachable
            // in practice; the sync track reports it as a query-builder error,
            // which has no equivalent here.
            if after_cursor > i32::MAX as i64 {
                return Err(crate::ConnectionError::InvalidQuery(
                    "Cursor value exceeds i32::MAX".into(),
                ));
            }
            let after_cursor = after_cursor as i32;

            let sql = format!(
                "SELECT {COLUMNS} FROM local_commit_log \
                 WHERE group_id = $1 AND rowid > $2 AND commit_sequence_id <> 0 ORDER BY rowid {}",
                match order {
                    LocalCommitLogOrder::AscendingByRowid => "ASC",
                    LocalCommitLogOrder::DescendingByRowid => "DESC",
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

        async fn get_latest_log_for_group(
            &self,
            group_id: &GroupId,
        ) -> Result<Option<LocalCommitLog>, crate::ConnectionError> {
            let mut c = self.conn().await?;
            let row = sqlx::query(&format!(
                "SELECT {COLUMNS} FROM local_commit_log WHERE group_id = $1 \
                 ORDER BY rowid DESC LIMIT 1"
            ))
            .bind(group_id)
            .fetch_optional(&mut *c)
            .await?;
            row.as_ref().map(log).transpose()
        }

        async fn get_local_commit_log_cursor(
            &self,
            group_id: &GroupId,
        ) -> Result<Option<i32>, crate::ConnectionError> {
            let mut c = self.conn().await?;
            let row = sqlx::query(
                "SELECT rowid FROM local_commit_log WHERE group_id = $1 ORDER BY rowid DESC LIMIT 1",
            )
            .bind(group_id)
            .fetch_optional(&mut *c)
            .await?;
            row.map(|r| r.try_get(0)).transpose().map_err(Into::into)
        }

        async fn get_latest_chain_start_rowid(
            &self,
            group_id: &GroupId,
        ) -> Result<Option<i32>, crate::ConnectionError> {
            let mut c = self.conn().await?;
            let row = sqlx::query(
                "SELECT rowid FROM local_commit_log \
                 WHERE group_id = $1 AND commit_sequence_id = 0 ORDER BY rowid DESC LIMIT 1",
            )
            .bind(group_id)
            .fetch_optional(&mut *c)
            .await?;
            row.map(|r| r.try_get(0)).transpose().map_err(Into::into)
        }
    }
}
