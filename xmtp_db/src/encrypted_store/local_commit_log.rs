use super::remote_commit_log::CommitResult;
use crate::{impl_store, schema::local_commit_log};
use diesel::{Insertable, Queryable, prelude::*};

#[derive(Insertable, Queryable, Debug, Clone)]
#[diesel(table_name = local_commit_log)]
#[diesel(primary_key(timestamp_ns))]
pub struct LocalCommitLog {
    pub timestamp_ns: i64,
    pub group_id: Option<Vec<u8>>,
    pub epoch_authenticator: Vec<u8>,
    pub result: CommitResult,
    pub epoch_number: Option<i64>,
    pub sender_inbox_id: String,
    pub sender_installation_id: Vec<u8>,
}

impl_store!(LocalCommitLog, local_commit_log);
