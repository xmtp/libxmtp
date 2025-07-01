use super::{DbConnection, remote_commit_log::CommitResult, schema::local_commit_log::dsl};
use crate::{ConnectionExt, impl_store, schema::local_commit_log};
use diesel::{Insertable, Queryable, prelude::*};

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = local_commit_log)]
pub struct NewLocalCommitLog {
    pub group_id: Vec<u8>,
    pub commit_sequence_id: i64,
    pub last_epoch_authenticator: Vec<u8>,
    pub commit_result: CommitResult,
    pub error_message: Option<String>,
    pub applied_epoch_number: Option<i64>,
    pub applied_epoch_authenticator: Option<Vec<u8>>,
    pub sender_inbox_id: Option<String>,
    pub sender_installation_id: Option<Vec<u8>>,
    pub commit_type: Option<String>,
}

#[derive(Queryable, Debug, Clone)]
#[diesel(table_name = local_commit_log)]
#[diesel(primary_key(id))]
pub struct LocalCommitLog {
    pub rowid: i32,
    pub group_id: Vec<u8>,
    pub commit_sequence_id: i64,
    pub last_epoch_authenticator: Vec<u8>,
    pub commit_result: CommitResult,
    pub error_message: Option<String>,
    pub applied_epoch_number: Option<i64>,
    pub applied_epoch_authenticator: Option<Vec<u8>>,
    pub sender_inbox_id: Option<String>,
    pub sender_installation_id: Option<Vec<u8>>,
    pub commit_type: Option<String>,
}

impl_store!(NewLocalCommitLog, local_commit_log);

impl std::fmt::Display for LocalCommitLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "LocalCommitLog {{ rowid: {:?}, group_id {:?}, commit_sequence_id: {:?}, last_epoch_authenticator: {:?}, commit_result: {:?}, error_message: {:?}, applied_epoch_number: {:?}, applied_epoch_authenticator: {:?}, sender_inbox_id: {:?}, sender_installation_id: {:?}, commit_type: {:?} }}",
            self.rowid,
            hex::encode(&self.group_id),
            self.commit_sequence_id,
            hex::encode(&self.last_epoch_authenticator),
            self.commit_result,
            self.error_message,
            self.applied_epoch_number,
            hex::encode(self.applied_epoch_authenticator.as_ref().unwrap_or(&vec![])),
            self.sender_inbox_id,
            hex::encode(self.sender_installation_id.as_ref().unwrap_or(&vec![])),
            self.commit_type
        )
    }
}

impl<C: ConnectionExt> DbConnection<C> {
    pub fn get_group_logs(
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
