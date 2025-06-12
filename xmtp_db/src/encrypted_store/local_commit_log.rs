use super::remote_commit_log::CommitResult;
use crate::schema::local_commit_log;
use diesel::{Insertable, Queryable};

#[derive(Insertable, Queryable, Debug, Clone)]
#[diesel(table_name = local_commit_log)]
#[diesel(primary_key(sequence_id))]
pub struct LocalCommitLog {
    pub sequence_id: i64,
    pub group_id: Option<Vec<u8>>,
    pub epoch_authenticator: Vec<u8>,
    pub result: CommitResult,
    pub epoch_number: Option<i64>,
}
