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
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
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
    let b_group = bo.group(&group.group_id).unwrap();
    b_group.update_consent_state(ConsentState::Allowed).unwrap();
    let c_group = caro.group(&group.group_id).unwrap();
    c_group.update_consent_state(ConsentState::Allowed).unwrap();

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
            .is_awaiting_readd(&group.group_id, alix.context.installation_id().as_slice(),)
            .unwrap()
    );
    assert!(
        !b_conn
            .is_awaiting_readd(&group.group_id, alix.context.installation_id().as_slice(),)
            .unwrap()
    );
    assert!(
        !c_conn
            .is_awaiting_readd(&group.group_id, alix.context.installation_id().as_slice(),)
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
            .is_awaiting_readd(&group.group_id, alix.context.installation_id().as_slice(),)
            .unwrap()
    );
    assert!(
        b_conn
            .is_awaiting_readd(&group.group_id, alix.context.installation_id().as_slice(),)
            .unwrap()
    );
    assert!(
        !c_conn
            .is_awaiting_readd(&group.group_id, alix.context.installation_id().as_slice(),)
            .unwrap()
    );
    assert!(
        !a_conn
            .is_awaiting_readd(&group.group_id, bo.context.installation_id().as_slice(),)
            .unwrap()
    );
    assert!(
        !b_conn
            .is_awaiting_readd(&group.group_id, bo.context.installation_id().as_slice(),)
            .unwrap()
    );
    assert!(
        !c_conn
            .is_awaiting_readd(&group.group_id, bo.context.installation_id().as_slice(),)
            .unwrap()
    );
}

#[xmtp_common::test]
async fn test_request_readd_dm() {
    tester!(alix, enable_fork_recovery_requests);
    tester!(bo);
    let dm = alix
        .find_or_create_dm(bo.inbox_id().to_string(), None)
        .await
        .unwrap();
    bo.sync_all_welcomes_and_groups(None).await.unwrap();
    let bo_dm = bo.group(&dm.group_id).unwrap();
    bo_dm.update_consent_state(ConsentState::Allowed).unwrap();

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
            .is_awaiting_readd(&dm.group_id, alix.context.installation_id().as_slice(),)
            .unwrap()
    );
    assert!(
        !b_conn
            .is_awaiting_readd(&dm.group_id, alix.context.installation_id().as_slice(),)
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
            .is_awaiting_readd(&dm.group_id, alix.context.installation_id().as_slice(),)
            .unwrap()
    );
    assert!(
        b_conn
            .is_awaiting_readd(&dm.group_id, alix.context.installation_id().as_slice(),)
            .unwrap()
    );
    assert!(
        !a_conn
            .is_awaiting_readd(&dm.group_id, bo.context.installation_id().as_slice(),)
            .unwrap()
    );
    assert!(
        !b_conn
            .is_awaiting_readd(&dm.group_id, bo.context.installation_id().as_slice(),)
            .unwrap()
    );
}

#[xmtp_common::test]
async fn test_readd_installation_succeeds() {
    tester!(alix);
    tester!(bo);
    tester!(caro);

    let a_group = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await
        .unwrap();

    bo.sync_all_welcomes_and_groups(None).await.unwrap();
    caro.sync_all_welcomes_and_groups(None).await.unwrap();
    let b_group = bo.group(&a_group.group_id).unwrap();
    let c_group = caro.group(&a_group.group_id).unwrap();

    let bo_installation_id = bo.context.installation_id();
    let prev_authenticator = a_group.epoch_authenticator().await.unwrap();
    assert_eq!(
        b_group.epoch_authenticator().await.unwrap(),
        prev_authenticator
    );
    a_group
        .readd_installations(vec![bo_installation_id.to_vec()])
        .await
        .unwrap();
    // Verify the commit was applied without erroring on A and C
    let new_authenticator = a_group.epoch_authenticator().await.unwrap();
    assert_ne!(prev_authenticator, new_authenticator);
    c_group.sync().await.unwrap();
    let c_group_authenticator = c_group.epoch_authenticator().await.unwrap();
    assert_eq!(c_group_authenticator, new_authenticator);

    // Verify welcome was received and applied on B
    tracing::warn!("Syncing welcomes");
    bo.sync_welcomes().await.unwrap();
    assert_eq!(
        b_group.epoch_authenticator().await.unwrap(),
        new_authenticator
    );
}

#[xmtp_common::test]
async fn test_readd_bookkeeping() {
    tester!(alix, enable_fork_recovery_requests);
    tester!(bo);
    tester!(caro);
    tester!(devon);
    let group = alix
        .create_group_with_members(
            &[bo.inbox_id(), caro.inbox_id(), devon.inbox_id()],
            None,
            None,
        )
        .await
        .unwrap();
    group
        .update_admin_list(UpdateAdminListType::AddSuper, bo.inbox_id().to_string())
        .await
        .unwrap();
    group
        .update_admin_list(UpdateAdminListType::AddSuper, caro.inbox_id().to_string())
        .await
        .unwrap();
    bo.sync_all_welcomes_and_groups(None).await.unwrap();
    caro.sync_all_welcomes_and_groups(None).await.unwrap();
    devon.sync_all_welcomes_and_groups(None).await.unwrap();
    let b_group = bo.group(&group.group_id).unwrap();
    b_group.update_consent_state(ConsentState::Allowed).unwrap();
    let c_group = caro.group(&group.group_id).unwrap();
    c_group.update_consent_state(ConsentState::Allowed).unwrap();
    let d_group = devon.group(&group.group_id).unwrap();
    d_group.update_consent_state(ConsentState::Allowed).unwrap();

    let mut a_worker = CommitLogWorker::new(alix.context.clone());
    // Publishes remote commit log (needed for readd request to be sent)
    a_worker._tick().await.unwrap();

    let a_conn = alix.context.db();
    let b_conn = bo.context.db();
    let c_conn = caro.context.db();
    let d_conn = devon.context.db();
    // Simulate a fork
    a_conn
        .set_group_commit_log_forked_status(&group.group_id, Some(true))
        .unwrap();

    // No readd requests yet
    assert!(
        !a_conn
            .is_awaiting_readd(&group.group_id, alix.context.installation_id().as_slice(),)
            .unwrap()
    );
    assert!(
        !b_conn
            .is_awaiting_readd(&group.group_id, alix.context.installation_id().as_slice(),)
            .unwrap()
    );
    assert!(
        !c_conn
            .is_awaiting_readd(&group.group_id, alix.context.installation_id().as_slice(),)
            .unwrap()
    );

    // Should trigger readd requests to be sent
    a_worker._tick().await.unwrap();
    // Receive any oneshot messages
    alix.sync_welcomes().await.unwrap();
    bo.sync_welcomes().await.unwrap();
    caro.sync_welcomes().await.unwrap();
    devon.sync_welcomes().await.unwrap();

    // Everyone except Devon (non-superadmin) sees Alix as awaiting readd
    assert!(
        a_conn
            .is_awaiting_readd(&group.group_id, alix.context.installation_id().as_slice(),)
            .unwrap()
    );
    assert!(
        b_conn
            .is_awaiting_readd(&group.group_id, alix.context.installation_id().as_slice(),)
            .unwrap()
    );
    assert!(
        c_conn
            .is_awaiting_readd(&group.group_id, alix.context.installation_id().as_slice(),)
            .unwrap()
    );
    assert!(
        !d_conn
            .is_awaiting_readd(&group.group_id, alix.context.installation_id().as_slice(),)
            .unwrap()
    );

    // Let's say Bo's worker processes it first
    let mut b_worker = CommitLogWorker::new(bo.context.clone());
    b_worker._tick().await.unwrap();

    caro.sync_all_welcomes_and_groups(None).await.unwrap();
    devon.sync_all_welcomes_and_groups(None).await.unwrap();

    // Everyone should see that Alix is no longer awaiting readd
    assert!(
        !b_conn
            .is_awaiting_readd(&group.group_id, alix.context.installation_id().as_slice(),)
            .unwrap()
    );
    assert!(
        !c_conn
            .is_awaiting_readd(&group.group_id, alix.context.installation_id().as_slice(),)
            .unwrap()
    );
    assert!(
        !d_conn
            .is_awaiting_readd(&group.group_id, alix.context.installation_id().as_slice(),)
            .unwrap()
    );

    alix.sync_welcomes().await.unwrap();
    assert!(
        !a_conn
            .is_awaiting_readd(&group.group_id, alix.context.installation_id().as_slice(),)
            .unwrap()
    );
}

#[xmtp_common::test]
async fn test_request_readd_with_allowlisted_groups() {
    // Step 1: Bo creates a group
    tester!(bo);
    tester!(caro);

    let group = bo
        .create_group_with_members(&[caro.inbox_id()], None, None)
        .await
        .unwrap();

    let group_id = group.group_id.clone();
    let group_id_hex = hex::encode(&group_id);
    let unnormalized_group_id = "0x".to_owned() + &group_id_hex.to_uppercase();

    // Step 2: Create Alix with that group ID in the allowlist
    tester!(alix, enable_fork_recovery_requests_for: vec![unnormalized_group_id]);

    // Step 3: Bo adds Alix to the group
    group
        .add_members(&[alix.inbox_id().to_string()])
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
            .is_awaiting_readd(&group_id, alix.context.installation_id().as_slice(),)
            .unwrap()
    );
    assert!(
        !b_conn
            .is_awaiting_readd(&group_id, alix.context.installation_id().as_slice(),)
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
            .is_awaiting_readd(&group_id, alix.context.installation_id().as_slice(),)
            .unwrap()
    );
    // Bo is a superadmin so should have recorded the request
    assert!(
        b_conn
            .is_awaiting_readd(&group_id, alix.context.installation_id().as_slice(),)
            .unwrap()
    );
    // Caro is not a superadmin so should not have received the request
    assert!(
        !c_conn
            .is_awaiting_readd(&group_id, alix.context.installation_id().as_slice(),)
            .unwrap()
    );
}
