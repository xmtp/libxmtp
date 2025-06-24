use crate::tester;
use tokio::join;
use xmtp_db::{local_commit_log::LocalCommitLog, remote_commit_log::CommitResult};

#[xmtp_common::test(unwrap_try = true)]
async fn test_local_commit_log_presence() {
    tester!(alix);
    tester!(bo);
    tester!(caro);

    let alix_g = alix
        .create_group_with_inbox_ids(&[bo.inbox_id()], None, None)
        .await?;
    let alix_g2 = alix
        .create_group_with_inbox_ids(&[bo.inbox_id()], None, None)
        .await?;

    let local_logs = alix.provider.db().get_group_logs(&alix_g.group_id)?;
    assert_eq!(local_logs.len(), 1);

    bo.sync_welcomes().await?;
    let bo_g = bo.group(&alix_g.group_id)?;
    let bo_g2 = bo.group(&alix_g2.group_id)?;

    // This will not go out to the network

    let caro_inbox = caro.inbox_id();

    alix_g
        .add_members_by_inbox_id(vec![caro_inbox.to_string()])
        .await?;
    bo_g.add_members_by_inbox_id(vec![caro_inbox.to_string()])
        .await?;

    // This will go out to the network
    alix_g2
        .add_members_by_inbox_id(vec![caro_inbox.to_string()])
        .await?;

    bo_g2.add_members_by_inbox_id(&[caro.inbox_id()]).await?;

    alix_g.sync().await?;

    let get_results = |logs: &[LocalCommitLog]| {
        logs.iter()
            .map(|l| (l.commit_result, l.commit_type.clone()))
            .collect::<Vec<_>>()
    };

    let local_logs = alix.provider.db().get_group_logs(&alix_g.group_id)?;
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
            ),
            (CommitResult::WrongEpoch, None)
        ]
    );

    let local_logs2 = alix.provider.db().get_group_logs(&alix_g2.group_id)?;
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

#[xmtp_common::test(unwrap_try = true)]
async fn test_out_of_epoch() {
    tester!(alix);
    tester!(bo);
    tester!(caro);

    let alix_g = alix
        .create_group_with_inbox_ids(&[bo.inbox_id()], None, None)
        .await?;

    bo.sync_welcomes().await?;
    let bo_g = bo.group(&alix_g.group_id)?;

    for _ in 0..5 {
        alix_g.update_group_name("foo".to_string()).await?;
    }

    bo_g.add_members_by_inbox_id(&[caro.inbox_id()]).await?;

    let alix_logs = alix.provider.db().get_group_logs(&alix_g.group_id)?;
    let bo_logs = bo.provider.db().get_group_logs(&bo_g.group_id)?;

    assert_eq!(
        get_type(&bo_logs),
        &[
            &None,
            &Some("MetadataUpdate".to_string()),
            &Some("KeyUpdate".to_string()),
            &Some("KeyUpdate".to_string()),
            &Some("KeyUpdate".to_string()),
            &Some("KeyUpdate".to_string()),
            &Some("UpdateGroupMembership".to_string()),
        ]
    );
    assert_eq!(
        get_result(&bo_logs),
        &[
            &CommitResult::WrongEpoch,
            &CommitResult::Success,
            &CommitResult::Success,
            &CommitResult::Success,
            &CommitResult::Success,
            &CommitResult::Success,
            &CommitResult::Success
        ]
    );
    assert_eq!(
        get_type(&alix_logs),
        &[
            &Some("UpdateGroupMembership".to_string()),
            &Some("MetadataUpdate".to_string()),
            &Some("KeyUpdate".to_string()),
            &Some("KeyUpdate".to_string()),
            &Some("KeyUpdate".to_string()),
            &Some("KeyUpdate".to_string()),
        ]
    );
    assert_eq!(get_result(&alix_logs), &[&CommitResult::Success; 6]);
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_log_with_lag() {
    tester!(alix, proxy);
    tester!(bo);
    tester!(caro);

    let alix_g = alix
        .create_group_with_inbox_ids(&[bo.inbox_id()], None, None)
        .await?;
    bo.sync_welcomes().await?;
    let bo_g = bo.group(&alix_g.group_id)?;

    // Turn on latency for alix's connection
    alix.proxy()
        .with_latency("latency".to_string(), 1000, 0, 1.0)
        .await;
    // Both add caro at the same time.
    let _ = join!(
        alix_g.add_members_by_inbox_id(vec![caro.inbox_id().to_string()]),
        bo_g.add_members_by_inbox_id(vec![caro.inbox_id().to_string()])
    );

    let alix_logs = alix.provider.db().get_group_logs(&alix_g.group_id)?;
    let bo_logs = bo.provider.db().get_group_logs(&bo_g.group_id)?;

    assert_eq!(
        get_type(&alix_logs),
        &[
            &Some("UpdateGroupMembership".to_string()),
            &Some("UpdateGroupMembership".to_string()),
        ]
    );
    assert_eq!(get_result(&alix_logs), &[&CommitResult::Success; 2]);
    assert_eq!(
        get_type(&bo_logs),
        &[&None, &Some("UpdateGroupMembership".to_string()),]
    );
    assert_eq!(
        get_result(&bo_logs),
        &[&CommitResult::WrongEpoch, &CommitResult::Success]
    );
}

fn get_type<'a>(logs: &'a [LocalCommitLog]) -> Vec<&'a Option<String>> {
    logs.iter().map(|l| &l.commit_type).collect()
}

fn get_result<'a>(logs: &'a [LocalCommitLog]) -> Vec<&'a CommitResult> {
    logs.iter().map(|l| &l.commit_result).collect()
}
