use crate::{
    context::XmtpSharedContext,
    groups::{UpdateAdminListType, commit_log::CommitLogWorker},
    tester,
};
use xmtp_db::{consent_record::ConsentState, group::QueryGroup, prelude::QueryReaddStatus};

#[xmtp_common::test]
async fn test_request_readd() {
    tester!(alix, enable_fork_recovery_requests);
    tester!(bo);
    tester!(caro);
    let group = alix
        .create_group_with_inbox_ids(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await
        .unwrap();
    group
        .update_admin_list(UpdateAdminListType::AddSuper, bo.inbox_id().to_string())
        .await
        .unwrap();
    group
        .update_admin_list(UpdateAdminListType::Add, caro.inbox_id().to_string())
        .await
        .unwrap();
    bo.sync_all_welcomes_and_groups(None).await.unwrap();
    caro.sync_all_welcomes_and_groups(None).await.unwrap();

    let mut a_worker = CommitLogWorker::new(alix.context.clone());
    a_worker._tick().await.unwrap();

    let a_conn = alix.context.db();
    let b_conn = bo.context.db();
    let c_conn = caro.context.db();
    // Simulate a fork
    a_conn
        .set_group_commit_log_forked_status(&group.group_id, Some(true))
        .unwrap();

    // No readd requests yet
    assert!(
        !a_conn
            .is_awaiting_readd(
                &group.group_id,
                alix.inbox_id(),
                alix.context.installation_id().as_slice(),
            )
            .unwrap()
    );
    assert!(
        !b_conn
            .is_awaiting_readd(
                &group.group_id,
                alix.inbox_id(),
                alix.context.installation_id().as_slice(),
            )
            .unwrap()
    );
    assert!(
        !c_conn
            .is_awaiting_readd(
                &group.group_id,
                alix.inbox_id(),
                alix.context.installation_id().as_slice(),
            )
            .unwrap()
    );

    // Should trigger readd requests to be sent
    a_worker._tick().await.unwrap();
    // Should no-op
    a_worker._tick().await.unwrap();
    // Receive any oneshot messages
    alix.sync_welcomes().await.unwrap();
    bo.sync_welcomes().await.unwrap();
    caro.sync_welcomes().await.unwrap();

    // Only Alix and Bo are superadmins, so only they should have recorded a readd request
    assert!(
        a_conn
            .is_awaiting_readd(
                &group.group_id,
                alix.inbox_id(),
                alix.context.installation_id().as_slice(),
            )
            .unwrap()
    );
    assert!(
        b_conn
            .is_awaiting_readd(
                &group.group_id,
                alix.inbox_id(),
                alix.context.installation_id().as_slice(),
            )
            .unwrap()
    );
    assert!(
        !c_conn
            .is_awaiting_readd(
                &group.group_id,
                alix.inbox_id(),
                alix.context.installation_id().as_slice(),
            )
            .unwrap()
    );
    assert!(
        !a_conn
            .is_awaiting_readd(
                &group.group_id,
                bo.inbox_id(),
                bo.context.installation_id().as_slice(),
            )
            .unwrap()
    );
    assert!(
        !b_conn
            .is_awaiting_readd(
                &group.group_id,
                bo.inbox_id(),
                bo.context.installation_id().as_slice(),
            )
            .unwrap()
    );
    assert!(
        !c_conn
            .is_awaiting_readd(
                &group.group_id,
                bo.inbox_id(),
                bo.context.installation_id().as_slice(),
            )
            .unwrap()
    );
}

#[xmtp_common::test]
async fn test_request_readd_dm() {
    tester!(alix, enable_fork_recovery_requests);
    tester!(bo);
    let dm = alix
        .find_or_create_dm_by_inbox_id(bo.inbox_id().to_string(), None)
        .await
        .unwrap();
    bo.sync_all_welcomes_and_groups(None).await.unwrap();

    let mut a_worker = CommitLogWorker::new(alix.context.clone());
    a_worker._tick().await.unwrap();

    let a_conn = alix.context.db();
    let b_conn = bo.context.db();
    // Simulate a fork
    a_conn
        .set_group_commit_log_forked_status(&dm.group_id, Some(true))
        .unwrap();

    // No readd requests yet
    assert!(
        !a_conn
            .is_awaiting_readd(
                &dm.group_id,
                alix.inbox_id(),
                alix.context.installation_id().as_slice(),
            )
            .unwrap()
    );
    assert!(
        !b_conn
            .is_awaiting_readd(
                &dm.group_id,
                alix.inbox_id(),
                alix.context.installation_id().as_slice(),
            )
            .unwrap()
    );

    // Should trigger readd requests to be sent
    a_worker._tick().await.unwrap();
    // Should no-op
    a_worker._tick().await.unwrap();
    // Receive any oneshot messages
    alix.sync_welcomes().await.unwrap();
    bo.sync_welcomes().await.unwrap();

    assert!(
        a_conn
            .is_awaiting_readd(
                &dm.group_id,
                alix.inbox_id(),
                alix.context.installation_id().as_slice(),
            )
            .unwrap()
    );
    assert!(
        b_conn
            .is_awaiting_readd(
                &dm.group_id,
                alix.inbox_id(),
                alix.context.installation_id().as_slice(),
            )
            .unwrap()
    );
    assert!(
        !a_conn
            .is_awaiting_readd(
                &dm.group_id,
                bo.inbox_id(),
                bo.context.installation_id().as_slice(),
            )
            .unwrap()
    );
    assert!(
        !b_conn
            .is_awaiting_readd(
                &dm.group_id,
                bo.inbox_id(),
                bo.context.installation_id().as_slice(),
            )
            .unwrap()
    );
}

#[xmtp_common::test]
async fn test_request_readd_with_allowlisted_groups() {
    // Step 1: Bo creates a group
    tester!(bo);
    tester!(caro);

    let group = bo
        .create_group_with_inbox_ids(&[caro.inbox_id()], None, None)
        .await
        .unwrap();

    let group_id = group.group_id.clone();
    let group_id_hex = hex::encode(&group_id);

    // Step 2: Create Alix with that group ID in the allowlist
    tester!(alix, enable_fork_recovery_requests_for: vec![group_id_hex]);

    // Step 3: Bo adds Alix to the group
    group
        .add_members_by_inbox_id(&[alix.inbox_id().to_string()])
        .await
        .unwrap();

    alix.sync_all_welcomes_and_groups(None).await.unwrap();
    caro.sync_all_welcomes_and_groups(None).await.unwrap();

    // Fork detection and recovery does not operate on non-consented groups
    let a_group = alix.group(&group_id).unwrap();
    a_group.update_consent_state(ConsentState::Allowed).unwrap();
    let c_group = caro.group(&group_id).unwrap();
    c_group.update_consent_state(ConsentState::Allowed).unwrap();

    // Upload remote commit log on Bo's end
    let mut b_worker = CommitLogWorker::new(bo.context.clone());
    b_worker._tick().await.unwrap();
    // Download remote commit log on Alix's end - we need the latest remote commit sequence ID for readd request to be sent
    let mut a_worker = CommitLogWorker::new(alix.context.clone());
    a_worker._tick().await.unwrap();

    let a_conn = alix.context.db();
    let b_conn = bo.context.db();
    let c_conn = caro.context.db();

    // Simulate a fork
    a_conn
        .set_group_commit_log_forked_status(&group_id, Some(true))
        .unwrap();

    // No readd requests yet
    assert!(
        !a_conn
            .is_awaiting_readd(
                &group_id,
                alix.inbox_id(),
                alix.context.installation_id().as_slice(),
            )
            .unwrap()
    );
    assert!(
        !b_conn
            .is_awaiting_readd(
                &group_id,
                alix.inbox_id(),
                alix.context.installation_id().as_slice(),
            )
            .unwrap()
    );

    // Should trigger readd requests to be sent for this allowlisted group
    a_worker._tick().await.unwrap();
    // Receive any oneshot messages
    bo.sync_welcomes().await.unwrap();
    caro.sync_welcomes().await.unwrap();

    // Alix should have recorded a readd request since the group is allowlisted
    assert!(
        a_conn
            .is_awaiting_readd(
                &group_id,
                alix.inbox_id(),
                alix.context.installation_id().as_slice(),
            )
            .unwrap()
    );
    // Bo is a superadmin so should have recorded the request
    assert!(
        b_conn
            .is_awaiting_readd(
                &group_id,
                alix.inbox_id(),
                alix.context.installation_id().as_slice(),
            )
            .unwrap()
    );
    // Caro is not a superadmin so should not have received the request
    assert!(
        !c_conn
            .is_awaiting_readd(
                &group_id,
                alix.inbox_id(),
                alix.context.installation_id().as_slice(),
            )
            .unwrap()
    );
}
