use xmtp_db::local_commit_log::LocalCommitLog;

use crate::tester;

#[xmtp_common::test(unwrap_try = true)]
async fn test_local_commit_log_presence() {
    tester!(alix);
    tester!(bo);

    let g = alix
        .create_group_with_inbox_ids(&[bo.inbox_id()], None, None)
        .await?;

    let local_logs = LocalCommitLog::group_logs(alix.provider.db(), &g.group_id)?;
    assert_eq!(local_logs.len(), 1);
}
