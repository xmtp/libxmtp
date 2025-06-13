use super::{DbConnection, remote_commit_log::CommitResult, schema::local_commit_log::dsl};
use crate::{impl_store, schema::local_commit_log};
use diesel::{Insertable, Queryable, prelude::*};

#[derive(Insertable, Queryable, Debug, Clone)]
#[diesel(table_name = local_commit_log)]
#[diesel(primary_key(timestamp_ns))]
pub struct LocalCommitLog {
    pub timestamp_ns: i64,
    pub epoch_authenticator: Option<Vec<u8>>,
    pub group_id: Vec<u8>,
    pub result: CommitResult,
    pub epoch_number: Option<i64>,
    pub sender_inbox_id: Option<String>,
    pub sender_installation_id: Option<Vec<u8>>,
}

impl_store!(LocalCommitLog, local_commit_log);

impl LocalCommitLog {
    pub fn group_logs(
        db: &DbConnection,
        group_id: &[u8],
    ) -> Result<Vec<Self>, crate::ConnectionError> {
        db.raw_query_read(|db| {
            dsl::local_commit_log
                .filter(dsl::group_id.eq(group_id))
                .order_by(dsl::timestamp_ns.asc())
                .load(db)
        })
    }
}
