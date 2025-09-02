use crate::{
    groups::{UpdateAdminListType, commit_log::CommitLogWorker},
    tester,
};
use xmtp_db::{group::QueryGroup, prelude::QueryReaddStatus};

#[xmtp_common::test]
async fn test_request_readd() {
    tester!(alix);
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
    tester!(alix);
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
