use super::{DbConnection, remote_commit_log::CommitResult, schema::local_commit_log::dsl};
use crate::{ConnectionExt, impl_store, schema::local_commit_log};
use diesel::{Insertable, Queryable, prelude::*};
use xmtp_common::snippet::Snippet;

pub enum CommitType {
    GroupCreation,
    BackupRestore,
    Welcome,
    KeyUpdate,
    MetadataUpdate,
    UpdateGroupMembership,
    UpdateAdminList,
    UpdatePermission,
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
        };
        write!(f, "{}", description)
    }
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = local_commit_log)]
pub struct NewLocalCommitLog {
    pub group_id: Vec<u8>,
    pub commit_sequence_id: i64,
    pub last_epoch_authenticator: Vec<u8>,
    pub commit_result: CommitResult,
    pub error_message: Option<String>,
    pub applied_epoch_number: i64,
    pub applied_epoch_authenticator: Vec<u8>,
    pub sender_inbox_id: Option<String>,
    pub sender_installation_id: Option<Vec<u8>>,
    pub commit_type: Option<String>,
}

#[derive(Queryable, Clone)]
#[diesel(table_name = local_commit_log)]
#[diesel(primary_key(id))]
pub struct LocalCommitLog {
    pub rowid: i32,
    pub group_id: Vec<u8>,
    pub commit_sequence_id: i64,
    pub last_epoch_authenticator: Vec<u8>,
    pub commit_result: CommitResult,
    pub error_message: Option<String>,
    pub applied_epoch_number: i64,
    pub applied_epoch_authenticator: Vec<u8>,
    pub sender_inbox_id: Option<String>,
    pub sender_installation_id: Option<Vec<u8>>,
    pub commit_type: Option<String>,
}

impl_store!(NewLocalCommitLog, local_commit_log);

impl std::fmt::Debug for LocalCommitLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "LocalCommitLog {{ rowid: {:?}, group_id {:?}, commit_sequence_id: {:?}, last_epoch_authenticator: {:?}, commit_result: {:?}, error_message: {:?}, applied_epoch_number: {:?}, applied_epoch_authenticator: {:?}, sender_inbox_id: {:?}, sender_installation_id: {:?}, commit_type: {:?} }}",
            self.rowid,
            &self.group_id.snippet(),
            self.commit_sequence_id,
            &self.last_epoch_authenticator.snippet(),
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

pub trait QueryLocalCommitLog<C: ConnectionExt> {
    fn get_group_logs(
        &self,
        group_id: &[u8],
    ) -> Result<Vec<LocalCommitLog>, crate::ConnectionError>;

    // Local commit log entries are returned sorted in ascending order of `rowid`
    // Entries with `commit_sequence_id` = 0 should not be published to the remote commit log
    fn get_group_logs_for_publishing(
        &self,
        group_id: &[u8],
        after_cursor: i64,
    ) -> Result<Vec<LocalCommitLog>, crate::ConnectionError>;

    fn get_latest_log_for_group(
        &self,
        group_id: &[u8],
    ) -> Result<Option<LocalCommitLog>, crate::ConnectionError>;

    fn get_local_commit_log_cursor(
        &self,
        group_id: &[u8],
    ) -> Result<Option<i32>, crate::ConnectionError>;
}

impl<C: ConnectionExt> QueryLocalCommitLog<C> for DbConnection<C> {
    fn get_group_logs(
        &self,
        group_id: &[u8],
    ) -> Result<Vec<LocalCommitLog>, crate::ConnectionError> {
        self.raw_query_read(|db| {
            dsl::local_commit_log
                .filter(dsl::group_id.eq(group_id))
                .order_by(dsl::rowid.asc())
                .load(db)
        })
    }

    pub fn get_latest_log_for_group(
        &self,
        group_id: &[u8],
    ) -> Result<Option<LocalCommitLog>, crate::ConnectionError> {
        self.raw_query_read(|db| {
            dsl::local_commit_log
                .filter(dsl::group_id.eq(group_id))
                .order_by(dsl::rowid.desc())
                .limit(1)
                .first(db)
                .optional()
        })
    }
}
