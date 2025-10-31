use openmls::group::MlsGroup as OpenMlsGroup;
use openmls::prelude::GroupId;
use xmtp_db::{consent_record::ConsentState, prelude::QueryReaddStatus};

use crate::{
    context::XmtpSharedContext,
    groups::{
        MlsGroup, build_group_membership_extension, commit_log::CommitLogWorker,
        group_membership::GroupMembership,
    },
    tester,
};

async fn is_forked<Context>(group_1: &MlsGroup<Context>, group_2: &MlsGroup<Context>) -> bool
where
    Context: XmtpSharedContext,
{
    group_1.sync().await.unwrap();
    group_2.sync().await.unwrap();
    let initial_group_name = group_1.group_name().unwrap_or("".to_string());
    let target_group_name = initial_group_name + "x";
    // Test via sending a commit, rather than a message, to sidestep any confusion around MAX_PAST_EPOCHS
    group_1
        .update_group_name(target_group_name.clone())
        .await
        .unwrap();
    group_2.sync().await.unwrap();
    group_2.group_name().unwrap() != target_group_name
}

fn fork_group<Context>(context: &Context, group: &MlsGroup<Context>)
where
    Context: XmtpSharedContext,
{
    let provider = &context.mls_provider();
    let mut mls_group = OpenMlsGroup::load(
        group.context.mls_storage(),
        &GroupId::from_slice(&group.group_id),
    )
    .unwrap()
    .unwrap();

    // Unilaterally merge a commit that others in the group do not see, causing you to be forked
    let mut existing_extensions = mls_group.extensions().clone();
    let mut group_membership = GroupMembership::new();
    group_membership.add("deadbeef".to_string(), 1);
    existing_extensions.add_or_replace(build_group_membership_extension(&group_membership));

    mls_group
        .update_group_context_extensions(
            provider,
            existing_extensions.clone(),
            &context.identity().installation_keys,
        )
        .unwrap();
    mls_group.merge_pending_commit(provider).unwrap();
}

#[xmtp_common::test]
async fn test_fork_recovery_e2e() {
    tester!(alix);
    tester!(bo, enable_fork_recovery_requests);

    tracing::warn!("Creating group and adding Bo to it");
    let a_group = alix
        .create_group_with_inbox_ids(&[bo.inbox_id()], None, None)
        .await
        .unwrap();
    bo.sync_all_welcomes_and_groups(None).await.unwrap();
    let b_group = bo.group(&a_group.group_id).unwrap();
    // Must be consented to enable fork recovery
    b_group.update_consent_state(ConsentState::Allowed).unwrap();
    assert!(!is_forked(&a_group, &b_group).await);

    tracing::warn!("Forking group");
    fork_group(&bo.context, &b_group);
    assert!(is_forked(&a_group, &b_group).await);
    // Today, we need a published commit to happen to detect the fork (fork_group() does not publish any commit)
    // In future we could detect forks by checking that your epoch authenticator still matches your local commit log
    a_group
        .update_group_name("new name".to_string())
        .await
        .unwrap();
    b_group.sync().await.unwrap();

    // Detecting fork
    let mut a_worker = CommitLogWorker::new(alix.context.clone());
    let mut b_worker = CommitLogWorker::new(bo.context.clone());
    tracing::warn!("A uploads their commit result to the remote log");
    a_worker._tick().await.unwrap();
    tracing::warn!(
        "B downloads the commit result from the remote log, then detects a fork, then sends a readd request"
    );
    b_worker._tick().await.unwrap();

    tracing::warn!(
        "(Unrelated) Adding an extra commit to test the worker's behavior after Bo's group is recovered"
    );
    a_group
        .update_group_name("new name 2".to_string())
        .await
        .unwrap();
    tracing::warn!("(Unrelated) Alix publishes the extra commit to the remote log");
    a_worker._tick().await.unwrap();
    tracing::warn!(
        "(Unrelated) B reads the extra commit, fails, and stores the result in their local log for later"
    );
    b_group.sync().await.unwrap();

    // Fork recovery
    tracing::warn!("Alix receives the readd request as a oneshot message");
    alix.sync_welcomes().await.unwrap();
    tracing::warn!("Alix sends the readd welcome");
    a_worker._tick().await.unwrap();
    tracing::warn!("Bo receives the readd welcome, which reinitializes their group");
    bo.sync_welcomes().await.unwrap();

    let a_conn = alix.context.db();
    tracing::warn!("Nobody should think B is forked anymore");
    assert!(
        !b_group
            .debug_info()
            .await
            .unwrap()
            .is_commit_log_forked
            .is_some_and(|is_forked| is_forked)
    );
    assert!(
        !a_conn
            .is_awaiting_readd(&a_group.group_id, bo.context.installation_id().as_slice(),)
            .unwrap()
    );

    tracing::warn!(
        "B should not compare commits from before they were readded, and should not conclude that they are forked based on old commits"
    );
    b_worker._tick().await.unwrap();
    assert!(
        !b_group
            .debug_info()
            .await
            .unwrap()
            .is_commit_log_forked
            .is_some_and(|is_forked| is_forked)
    );
    tracing::warn!("Alix should not receive any more readd requests");
    assert!(alix.sync_welcomes().await.unwrap().is_empty());
    assert!(
        !a_conn
            .is_awaiting_readd(&a_group.group_id, bo.context.installation_id().as_slice(),)
            .unwrap()
    );

    tracing::warn!("Confirm B is not forked");
    assert!(!is_forked(&a_group, &b_group).await);
}
