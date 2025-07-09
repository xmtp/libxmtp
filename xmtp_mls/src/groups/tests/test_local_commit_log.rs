use crate::{
    groups::{
        intents::{PermissionPolicyOption, PermissionUpdateType},
        UpdateAdminListType,
    },
    tester,
    utils::{ConcreteMlsGroup, FullXmtpClient},
};
use toxiproxy_rust::proxy::Proxy;
use xmtp_db::{
    local_commit_log::{CommitType, LocalCommitLog},
    remote_commit_log::CommitResult,
};

#[allow(dead_code)]
async fn print_commit_log(group: &ConcreteMlsGroup) {
    println!("{:?}\n", group.local_commit_log().await.unwrap());
}

async fn last_commit_log(group: &ConcreteMlsGroup) -> LocalCommitLog {
    group
        .local_commit_log()
        .await
        .unwrap()
        .last()
        .unwrap()
        .to_owned()
}

async fn last_commit_type_matches(
    group1: &ConcreteMlsGroup,
    group2: &ConcreteMlsGroup,
    expected: CommitType,
) -> bool {
    print_commit_log(group1).await;
    print_commit_log(group2).await;
    let log_1: LocalCommitLog = last_commit_log(group1).await;
    let log_2: LocalCommitLog = last_commit_log(group2).await;
    log_1.commit_result == CommitResult::Success
        && log_2.commit_result == CommitResult::Success
        && log_1.commit_type.unwrap() == expected.to_string()
        && log_2.commit_type.unwrap() == expected.to_string()
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_successful_commit_log_types() {
    tester!(alix);
    tester!(bo);
    tester!(caro);
    let a_client: &FullXmtpClient = &alix;
    let b_client: &FullXmtpClient = &bo;

    let a = a_client
        .create_group_with_inbox_ids(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await?;
    let b = b_client.sync_welcomes().await?.first()?.to_owned();
    b.sync().await?;
    assert_eq!(a.local_commit_log().await?.len(), 2);
    assert_eq!(
        a.local_commit_log().await?[0].commit_type,
        Some(CommitType::GroupCreation.to_string())
    );
    assert_eq!(
        last_commit_log(&a).await.commit_type,
        Some(CommitType::UpdateGroupMembership.to_string())
    );
    assert_eq!(b.local_commit_log().await?.len(), 1);
    assert_eq!(last_commit_log(&b).await.commit_type, None);

    a.key_update().await?;
    b.sync().await?;
    assert!(last_commit_type_matches(&a, &b, CommitType::KeyUpdate).await);

    a.remove_members_by_inbox_id(&[caro.inbox_id()]).await?;
    b.sync().await?;
    assert!(last_commit_type_matches(&a, &b, CommitType::UpdateGroupMembership).await);

    a.update_group_name("foo".to_string()).await?;
    b.sync().await?;
    assert!(last_commit_type_matches(&a, &b, CommitType::MetadataUpdate).await);

    tester!(_bo2, from: bo);
    a.update_installations().await?;
    b.sync().await?;
    assert!(last_commit_type_matches(&a, &b, CommitType::UpdateGroupMembership).await);

    a.update_admin_list(UpdateAdminListType::Add, bo.inbox_id().to_string())
        .await?;
    b.sync().await?;
    assert!(last_commit_type_matches(&a, &b, CommitType::UpdateAdminList).await);

    a.update_permission_policy(
        PermissionUpdateType::AddMember,
        PermissionPolicyOption::AdminOnly,
        None,
    )
    .await?;
    b.sync().await?;
    assert!(last_commit_type_matches(&a, &b, CommitType::UpdatePermission).await);

    assert_eq!(a.local_commit_log().await?.len(), 8);
    assert_eq!(b.local_commit_log().await?.len(), 7);
}

// TODO(rich): Fix intent publishing on bad network conditions
#[ignore]
#[xmtp_common::test(unwrap_try = true)]
async fn test_commit_log_retriable_error() {
    tester!(alix);
    tester!(bo, proxy);
    tester!(caro);
    let a_client: &FullXmtpClient = &alix;
    let b_client: &FullXmtpClient = &bo;
    let proxy: &Proxy = bo.proxy();

    let a = a_client
        .create_group_with_inbox_ids(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await?;
    let b = b_client.sync_welcomes().await?.first()?.to_owned();
    b.sync().await?;
    assert_eq!(a.local_commit_log().await?.len(), 1);
    assert_eq!(b.local_commit_log().await?.len(), 1);

    proxy.disable().await?;
    // Queues up a KeyUpdate intent followed by a SendMessage intent
    b.send_message(b"foo").await.unwrap_err();
    a.sync().await?;
    assert_eq!(a.local_commit_log().await?.len(), 1);
    assert_eq!(b.local_commit_log().await?.len(), 1);

    proxy.enable().await?;
    // This currently fails with error SyncFailedToWait, because the intent has been marked as 'published'
    // despite not being published. We need to fix the intent publishing flow for this test to work.
    b.sync_until_last_intent_resolved().await?;
    a.sync().await?;
    assert_eq!(a.local_commit_log().await?.len(), 2);
    assert_eq!(b.local_commit_log().await?.len(), 2);
    assert!(last_commit_type_matches(&a, &b, CommitType::KeyUpdate).await);
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
            &Some("GroupCreation".to_string()),
            &Some("UpdateGroupMembership".to_string()),
            &Some("MetadataUpdate".to_string()),
            &Some("KeyUpdate".to_string()),
            &Some("KeyUpdate".to_string()),
            &Some("KeyUpdate".to_string()),
            &Some("KeyUpdate".to_string()),
        ]
    );
    assert_eq!(get_result(&alix_logs), &[&CommitResult::Success; 7]);
}

fn get_type(logs: &[LocalCommitLog]) -> Vec<&Option<String>> {
    logs.iter().map(|l| &l.commit_type).collect()
}

fn get_result(logs: &[LocalCommitLog]) -> Vec<&CommitResult> {
    logs.iter().map(|l| &l.commit_result).collect()
}
