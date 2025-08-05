use crate::{
    groups::{
        UpdateAdminListType,
        intents::{PermissionPolicyOption, PermissionUpdateType},
    },
    tester,
    utils::{FullXmtpClient, TestMlsGroup},
};
use toxiproxy_rust::proxy::Proxy;
use xmtp_db::{
    local_commit_log::{CommitType, LocalCommitLog},
    remote_commit_log::CommitResult,
};

#[allow(dead_code)]
async fn print_commit_log(group: &TestMlsGroup) {
    println!("{:?}\n", group.local_commit_log().await.unwrap());
}

async fn last_commit_log(group: &TestMlsGroup) -> LocalCommitLog {
    group
        .local_commit_log()
        .await
        .unwrap()
        .last()
        .unwrap()
        .to_owned()
}

async fn last_commit_type_matches(
    group1: &TestMlsGroup,
    group2: &TestMlsGroup,
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
    assert_eq!(
        get_type(&a.local_commit_log().await?),
        &[
            &Some(CommitType::GroupCreation.to_string()),
            &Some(CommitType::UpdateGroupMembership.to_string()),
        ]
    );

    let b = b_client.sync_welcomes().await?.first()?.to_owned();
    b.sync().await?;
    assert_eq!(
        b.local_commit_log().await?[0].commit_type,
        Some(CommitType::Welcome.to_string())
    );

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

#[xmtp_common::test(unwrap_try = true)]
async fn test_welcome_commit_log() {
    tester!(alix);
    tester!(bo);
    tester!(caro);

    let a = alix
        .create_group_with_inbox_ids(&[caro.inbox_id()], None, None)
        .await?;
    a.add_members_by_inbox_id(&[bo.inbox_id()]).await?;
    a.update_group_name("foo".to_string()).await?;
    assert_eq!(
        get_type(&a.local_commit_log().await?),
        &[
            &Some(CommitType::GroupCreation.to_string()),
            &Some(CommitType::UpdateGroupMembership.to_string()),
            &Some(CommitType::UpdateGroupMembership.to_string()),
            &Some(CommitType::MetadataUpdate.to_string()),
        ]
    );

    let b = bo.sync_welcomes().await?.first()?.to_owned();
    b.sync().await?;
    // Commits before the welcome should not be logged
    assert_eq!(
        get_type(&b.local_commit_log().await?),
        &[
            &Some(CommitType::Welcome.to_string()),
            &Some(CommitType::MetadataUpdate.to_string()),
        ]
    );
    // Welcome metadata should be set correctly
    assert_eq!(
        b.local_commit_log().await?[0].sender_inbox_id,
        Some(alix.inbox_id().to_string())
    );
    assert_eq!(
        b.local_commit_log().await?[0].sender_installation_id,
        Some(alix.installation_public_key().to_vec())
    );
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
    assert_eq!(a.local_commit_log().await?.len(), 2); // GroupCreation + UpdateGroupMembership
    assert_eq!(b.local_commit_log().await?.len(), 1); // Welcome

    proxy.disable().await?;
    // Queues up a KeyUpdate intent followed by a SendMessage intent
    b.send_message(b"foo").await.unwrap_err();
    a.sync().await?;
    // A doesn't receive anything because the payloads failed to send
    assert_eq!(a.local_commit_log().await?.len(), 2);
    // B should not log any errors because they are retriable
    assert_eq!(b.local_commit_log().await?.len(), 1);

    proxy.enable().await?;
    // This currently fails with error SyncFailedToWait, because the intent has been marked as 'published'
    // despite not being published. We need to fix the intent publishing flow for this test to work.
    b.sync_until_last_intent_resolved().await?;
    a.sync().await?;
    // KeyUpdate should have been added to the commit log (SendMessage is not logged because it is not a commit)
    assert_eq!(a.local_commit_log().await?.len(), 3);
    assert_eq!(b.local_commit_log().await?.len(), 2);
    assert!(last_commit_type_matches(&a, &b, CommitType::KeyUpdate).await);
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_commit_log_non_retriable_error() {
    tester!(alix);
    tester!(bo);

    let a_client: &FullXmtpClient = &alix;
    let b_client: &FullXmtpClient = &bo;

    let a = a_client
        .create_group_with_inbox_ids(&[bo.inbox_id()], None, None)
        .await?;
    let b = b_client.sync_welcomes().await?.first()?.to_owned();
    assert_eq!(
        get_type(&a.local_commit_log().await?),
        &[
            &Some("GroupCreation".to_string()),
            &Some("UpdateGroupMembership".to_string()),
        ]
    );
    assert_eq!(
        get_type(&b.local_commit_log().await?),
        &[&Some("Welcome".to_string())]
    );

    // Should successfully publish a MetadataUpdate commit
    a.update_group_name("foo".to_string()).await?;
    // B has not synced, so will publish a commit one epoch behind
    // When syncing, the commit should be marked as failed with a non-retriable epoch error
    // Then the commit should be re-published in the correct epoch
    b.update_group_name("bar".to_string()).await?;
    a.sync().await?;
    b.sync().await?;
    assert_eq!(
        get_type(&a.local_commit_log().await?),
        &[
            &Some("GroupCreation".to_string()),
            &Some("UpdateGroupMembership".to_string()),
            &Some("MetadataUpdate".to_string()),
            &None,
            &Some("MetadataUpdate".to_string()),
        ]
    );
    assert_eq!(
        get_result(&a.local_commit_log().await?),
        &[
            &CommitResult::Success,
            &CommitResult::Success,
            &CommitResult::Success,
            &CommitResult::WrongEpoch,
            &CommitResult::Success
        ]
    );
    assert_eq!(
        get_type(&b.local_commit_log().await?),
        &[
            &Some("Welcome".to_string()),
            &Some("MetadataUpdate".to_string()),
            &None,
            &Some("MetadataUpdate".to_string()),
        ]
    );
    assert_eq!(
        get_result(&b.local_commit_log().await?),
        &[
            &CommitResult::Success,
            &CommitResult::Success,
            &CommitResult::WrongEpoch,
            &CommitResult::Success
        ]
    )
}

fn get_type(logs: &[LocalCommitLog]) -> Vec<&Option<String>> {
    logs.iter().map(|l| &l.commit_type).collect()
}

fn get_result(logs: &[LocalCommitLog]) -> Vec<&CommitResult> {
    logs.iter().map(|l| &l.commit_result).collect()
}
