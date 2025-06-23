use xmtp_db::{local_commit_log::LocalCommitLog, remote_commit_log::CommitResult};

use crate::tester;

#[xmtp_common::test(unwrap_try = true)]
async fn test_local_commit_log_presence() {
    tester!(alix);
    tester!(bo);
    tester!(caro);

    let mut g = alix
        .create_group_with_inbox_ids(&[bo.inbox_id()], None, None)
        .await?;
    let g2 = alix
        .create_group_with_inbox_ids(&[bo.inbox_id()], None, None)
        .await?;

    let local_logs = alix.provider.db().get_group_logs(&g.group_id)?;
    assert_eq!(local_logs.len(), 1);

    bo.sync_welcomes().await?;
    let bo_g = bo.group(&g.group_id)?;
    let bo_g2 = bo.group(&g2.group_id)?;

    g.disable_network = true;
    // This will not go out to the network
    g.add_members_by_inbox_id(&[caro.inbox_id()]).await?;
    g.disable_network = false;
    // This will go out to the network
    g2.add_members_by_inbox_id(&[caro.inbox_id()]).await?;

    bo_g.add_members_by_inbox_id(&[caro.inbox_id()]).await?;
    bo_g2.add_members_by_inbox_id(&[caro.inbox_id()]).await?;

    g.sync().await?;

    let get_results = |logs: &[LocalCommitLog]| {
        logs.iter()
            .map(|l| (l.commit_result, l.commit_type.clone()))
            .collect::<Vec<_>>()
    };

    let local_logs = alix.provider.db().get_group_logs(&g.group_id)?;
    assert_eq!(
        get_results(&local_logs),
        &[
            (
                CommitResult::Success,
                Some("UpdateGroupMembership".to_string())
            ),
            (
                CommitResult::Success,
                Some("UpdateGroupMembership".to_string())
            )
        ]
    );

    let local_logs2 = alix.provider.db().get_group_logs(&g2.group_id)?;
    assert_eq!(
        get_results(&local_logs2),
        &[
            (
                CommitResult::Success,
                Some("UpdateGroupMembership".to_string())
            ),
            (
                CommitResult::Success,
                Some("UpdateGroupMembership".to_string())
            ),
            (CommitResult::WrongEpoch, None)
        ]
    );

    let bo_local_logs = bo.provider.db().get_group_logs(&bo_g.group_id)?;
    assert_eq!(
        get_results(&bo_local_logs),
        &[
            (CommitResult::WrongEpoch, None),
            (
                CommitResult::Success,
                Some("UpdateGroupMembership".to_string())
            ),
        ]
    );
}
