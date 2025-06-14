use super::{DbConnection, remote_commit_log::CommitResult, schema::local_commit_log::dsl};
use crate::{impl_store, schema::local_commit_log};
use diesel::{Insertable, Queryable, prelude::*};

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = local_commit_log)]
pub struct NewLocalCommitLog {
    pub group_id: Vec<u8>,
    pub commit_sequence_id: i64,
    pub last_epoch_authenticator: Vec<u8>,
    pub commit_result: CommitResult,
    pub applied_epoch_number: Option<i64>,
    pub applied_epoch_authenticator: Option<Vec<u8>>,
    pub sender_inbox_id: Option<String>,
    pub sender_installation_id: Option<Vec<u8>>,
    pub commit_type: Option<i32>,
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
    pub applied_epoch_number: Option<i64>,
    pub applied_epoch_authenticator: Option<Vec<u8>>,
    pub sender_inbox_id: Option<String>,
    pub sender_installation_id: Option<Vec<u8>>,
    pub commit_type: Option<i32>,
}

impl_store!(NewLocalCommitLog, local_commit_log);

impl LocalCommitLog {
    pub fn group_logs(
        db: &DbConnection,
        group_id: &[u8],
    ) -> Result<Vec<Self>, crate::ConnectionError> {
        db.raw_query_read(|db| {
            dsl::local_commit_log
                .filter(dsl::group_id.eq(group_id))
                .order_by(dsl::rowid.asc())
                .load(db)
        })
    }
}
