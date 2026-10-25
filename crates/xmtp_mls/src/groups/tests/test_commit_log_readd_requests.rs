use crate::{
    context::XmtpSharedContext,
    groups::{UpdateAdminListType, commit_log::CommitLogWorker, oneshot::Oneshot},
    tester,
};
use xmtp_db::{consent_record::ConsentState, group::QueryGroup, prelude::QueryReaddStatus};

use xmtp_proto::{
    types::GroupId,
    xmtp::mls::message_contents::{OneshotMessage, ReaddRequest, oneshot_message::MessageType},
};

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

#[xmtp_common::test(unwrap_try = true)]
async fn test_readd_installation_succeeds() {
    use xmtp_db::{prelude::QueryRefreshState, refresh_state::EntityKind};

    tester!(alix);
    tester!(bo);
    tester!(caro);

    let a_group = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await
        .unwrap();

    // Use the creation floor before recording the pre-removal authenticator.
    a_group
        .update_group_min_version(xmtp_configuration::PROPOSALS_MIN_PROTOCOL_VERSION)
        .await?;

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

    // Process the removal at the same anchor carried by the replacement Welcome.
    b_group.sync_with_conn().await?;
    assert!(!b_group.is_active()?);
    assert_eq!(b_group.epoch().await?, a_group.epoch().await?);
    let anchor = alix
        .context
        .db()
        .latest_cursor_for_id(a_group.group_id, &[EntityKind::ApplicationMessage])?;
    assert_eq!(
        bo.context
            .db()
            .latest_cursor_for_id(a_group.group_id, &[EntityKind::ApplicationMessage],)?,
        anchor
    );
    assert_eq!(b_group.epoch_authenticator().await?, prev_authenticator);

    // Model an older Welcome published after the replacement Welcome.
    // This fixture changes its stored ID; it does not delay network publication.
    let delayed_welcome_sequence = i64::MAX;
    let mut stored_group = bo.context.db().find_group(&a_group.group_id)?.unwrap();
    stored_group.sequence_id = Some(delayed_welcome_sequence);
    assert_eq!(
        bo.context
            .db()
            .insert_or_replace_group(stored_group)?
            .sequence_id,
        Some(delayed_welcome_sequence)
    );

    // Verify welcome was received and applied on B
    tracing::warn!("Syncing welcomes");
    bo.sync_welcomes().await.unwrap();
    assert_eq!(
        b_group.epoch_authenticator().await.unwrap(),
        new_authenticator
    );
    assert!(b_group.is_active()?);
    assert_eq!(b_group.epoch().await?, a_group.epoch().await?);
    b_group.with_group_snapshot(|mls_group| {
        assert!(
            mls_group.pending_proposals().next().is_none(),
            "a replacement Welcome must not restore old-epoch proposals"
        );
        Ok(())
    })?;
    assert_eq!(
        bo.context
            .db()
            .latest_cursor_for_id(a_group.group_id, &[EntityKind::ApplicationMessage])?,
        anchor
    );
    a_group.test_can_talk_with(&b_group).await?;
    b_group.test_can_talk_with(&a_group).await?;
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

    let group_id = group.group_id;
    let group_id_typed: GroupId = group_id;
    let group_id_hex = hex::encode(group_id);
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
            .is_awaiting_readd(&group_id_typed, alix.context.installation_id().as_slice(),)
            .unwrap()
    );
    assert!(
        !b_conn
            .is_awaiting_readd(&group_id_typed, alix.context.installation_id().as_slice(),)
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
            .is_awaiting_readd(&group_id_typed, alix.context.installation_id().as_slice(),)
            .unwrap()
    );
    // Bo is a superadmin so should have recorded the request
    assert!(
        b_conn
            .is_awaiting_readd(&group_id_typed, alix.context.installation_id().as_slice(),)
            .unwrap()
    );
    // Caro is not a superadmin so should not have received the request
    assert!(
        !c_conn
            .is_awaiting_readd(&group_id_typed, alix.context.installation_id().as_slice(),)
            .unwrap()
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_dictionary_native_readd_succeeds() {
    tester!(alix);
    tester!(bo);
    tester!(caro);
    let group = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await?;

    bo.sync_all_welcomes_and_groups(None).await?;
    caro.sync_all_welcomes_and_groups(None).await?;
    let bo_group = bo.group(&group.group_id)?;
    let caro_group = caro.group(&group.group_id)?;
    let before = group.epoch_authenticator().await?;
    group
        .readd_installations(vec![bo.context.installation_id().to_vec()])
        .await?;
    assert_ne!(group.epoch_authenticator().await?, before);
    caro_group.sync().await?;
    assert_eq!(
        caro_group.epoch_authenticator().await?,
        group.epoch_authenticator().await?
    );
    bo_group.sync_with_conn().await?;
    bo.sync_welcomes().await?;
    assert_eq!(
        bo_group.epoch_authenticator().await?,
        group.epoch_authenticator().await?
    );
}

// verifies: FORK-062
#[xmtp_common::test(unwrap_try = true)]
async fn a_readd_without_a_usable_key_package_waits() {
    use crate::utils::test_mocks_helpers::set_test_mode_upload_malformed_keypackage;
    use xmtp_db::{prelude::QueryRefreshState, refresh_state::EntityKind};
    use xmtp_mls_common::app_data::component_id::ComponentId;

    struct RestoreKeyPackages;
    impl Drop for RestoreKeyPackages {
        fn drop(&mut self) {
            set_test_mode_upload_malformed_keypackage(false, None);
        }
    }

    tester!(alix, disable_workers);
    tester!(caro, disable_workers);
    let group = alix
        .create_group_with_members(&[caro.inbox_id()], None, None)
        .await?;
    group
        .update_group_min_version(xmtp_configuration::PROPOSALS_MIN_PROTOCOL_VERSION)
        .await?;
    caro.sync_all_welcomes_and_groups(None).await?;
    let caro_group = caro.group(&group.group_id)?;
    let installation = caro.context.installation_id();
    let mut worker = CommitLogWorker::new(alix.context.clone());
    worker._tick().await?;
    let cursor = alix
        .context
        .db()
        .latest_cursor_for_id(group.group_id, &[EntityKind::ApplicationMessage])?
        .0;
    Oneshot::send_message(
        caro.context.clone(),
        vec![alix.inbox_id()],
        OneshotMessage {
            message_type: Some(MessageType::ReaddRequest(ReaddRequest {
                group_id: group.group_id.to_vec(),
                latest_commit_sequence_id: cursor,
            })),
        },
    )
    .await?;
    alix.sync_welcomes().await?;

    let before_epoch = group.epoch().await?;
    let before_membership = group.with_group_snapshot(|mls_group| {
        Ok(mls_group
            .extensions()
            .app_data_dictionary()
            .unwrap()
            .dictionary()
            .get(&ComponentId::GROUP_MEMBERSHIP.as_u16())
            .unwrap()
            .to_vec())
    })?;
    set_test_mode_upload_malformed_keypackage(true, Some(vec![installation.to_vec()]));
    let restore = RestoreKeyPackages;
    worker._tick().await?;
    assert_eq!(group.epoch().await?, before_epoch);
    group.with_group_snapshot(|mls_group| {
        assert_eq!(mls_group.members().count(), 2);
        assert!(
            mls_group
                .members()
                .any(|member| member.signature_key == installation.as_slice())
        );
        assert_eq!(
            mls_group
                .extensions()
                .app_data_dictionary()
                .unwrap()
                .dictionary()
                .get(&ComponentId::GROUP_MEMBERSHIP.as_u16())
                .unwrap(),
            before_membership.as_slice()
        );
        Ok(())
    })?;
    assert!(
        alix.context
            .db()
            .is_awaiting_readd(&group.group_id, installation.as_slice())?
    );
    assert_eq!(
        alix.context
            .db()
            .get_readd_status(&group.group_id, installation.as_slice())?
            .unwrap()
            .responded_at_sequence_id,
        None
    );

    drop(restore);
    caro.rotate_and_upload_key_package().await?;
    worker._tick().await?;
    assert_eq!(group.epoch().await?, before_epoch + 1);
    assert!(
        !alix
            .context
            .db()
            .is_awaiting_readd(&group.group_id, installation.as_slice())?
    );
    caro_group.sync_with_conn().await?;
    caro.sync_welcomes().await?;
    assert_eq!(
        caro_group.epoch_authenticator().await?,
        group.epoch_authenticator().await?
    );
    worker._tick().await?;
    assert_eq!(group.epoch().await?, before_epoch + 1);
}

// verifies: FORK-050, FORK-053, FORK-063, FORK-080, GMOD-052, FORK-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_dm_peer_answers_a_readd_request() {
    tester!(alix, disable_workers, enable_fork_recovery_requests);
    tester!(bo, disable_workers);
    let dm = alix
        .find_or_create_dm(bo.inbox_id().to_string(), None)
        .await?;
    bo.sync_all_welcomes_and_groups(None).await?;
    let bo_dm = bo.group(&dm.group_id)?;
    bo_dm.update_consent_state(ConsentState::Allowed)?;
    assert!(bo_dm.super_admin_list()?.is_empty());
    // Give both members a post-Welcome commit to compare before recovery.
    dm.key_update().await?;
    bo_dm.sync().await?;

    let mut alix_worker = CommitLogWorker::new(alix.context.clone());
    alix_worker._tick().await?;
    let alix_db = alix.context.db();
    let bo_db = bo.context.db();
    let installation = alix.context.installation_id();
    let epoch_before = bo_dm.epoch().await?;
    // Simulate fork detection without creating divergent MLS epoch secrets.
    alix_db.set_group_commit_log_forked_status(&dm.group_id, Some(true))?;
    alix_worker._tick().await?;
    bo.sync_welcomes().await?;
    assert!(bo_db.is_awaiting_readd(&dm.group_id, installation.as_slice())?);
    assert!(alix_db.is_awaiting_readd(&dm.group_id, installation.as_slice())?);

    let mut bo_worker = CommitLogWorker::new(bo.context.clone());
    for _ in 0..3 {
        bo_worker._tick().await?;
        if !bo_db.is_awaiting_readd(&dm.group_id, installation.as_slice())? {
            break;
        }
    }
    let status = bo_db
        .get_readd_status(&dm.group_id, installation.as_slice())?
        .expect("the response retains its bookkeeping row");
    assert!(status.responded_at_sequence_id.is_some());
    assert!(!bo_db.is_awaiting_readd(&dm.group_id, installation.as_slice())?);
    assert!(bo_dm.epoch().await? > epoch_before);
    dm.sync_with_conn().await?;
    assert!(!dm.is_active()?);
    alix.sync_welcomes().await?;
    assert!(dm.is_active()?);
    assert_eq!(
        dm.epoch_authenticator().await?,
        bo_dm.epoch_authenticator().await?
    );
    assert_eq!(
        alix_db.get_group_commit_log_forked_status(&dm.group_id)?,
        None
    );
    let repaired_epoch = dm.epoch().await?;
    alix_worker._tick().await?;
    bo_worker._tick().await?;
    assert_eq!(dm.epoch().await?, repaired_epoch);
    assert_eq!(bo_dm.epoch().await?, repaired_epoch);
}

// verifies: FORK-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_replacement_welcome_resets_the_fork_state() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    bo.sync_all_welcomes_and_groups(None).await?;
    let bo_group = bo.group(&group.group_id)?;
    bo.context
        .db()
        .set_group_commit_log_forked_status(&group.group_id, Some(true))?;
    group
        .readd_installations(vec![bo.context.installation_id().to_vec()])
        .await?;
    bo_group.sync_with_conn().await?;
    assert!(!bo_group.is_active()?);
    assert_eq!(
        bo.context
            .db()
            .get_group_commit_log_forked_status(&group.group_id)?,
        Some(true)
    );
    bo.sync_welcomes().await?;
    assert!(bo_group.is_active()?);
    assert_eq!(
        bo.context
            .db()
            .get_group_commit_log_forked_status(&group.group_id)?,
        None
    );
}

// verifies: FORK-050, FORK-080, FORK-064, GMOD-052
#[xmtp_common::test(unwrap_try = true)]
async fn a_dm_readd_is_answered_once_by_peer_or_sibling() {
    tester!(alix, disable_workers, enable_fork_recovery_requests);
    tester!(alix2, from: alix, disable_workers);
    tester!(bo, disable_workers);
    let dm = alix
        .find_or_create_dm(bo.inbox_id().to_string(), None)
        .await?;
    dm.update_group_min_version(xmtp_configuration::PROPOSALS_MIN_PROTOCOL_VERSION)
        .await?;
    alix2.sync_all_welcomes_and_groups(None).await?;
    bo.sync_all_welcomes_and_groups(None).await?;
    let sibling_dm = alix2.group(&dm.group_id)?;
    let bo_dm = bo.group(&dm.group_id)?;
    sibling_dm.update_consent_state(ConsentState::Allowed)?;
    bo_dm.update_consent_state(ConsentState::Allowed)?;
    dm.key_update().await?;
    sibling_dm.sync().await?;
    bo_dm.sync().await?;
    let mut alix_worker = CommitLogWorker::new(alix.context.clone());
    alix_worker._tick().await?;
    alix.context
        .db()
        .set_group_commit_log_forked_status(&dm.group_id, Some(true))?;
    alix_worker._tick().await?;
    alix2.sync_welcomes().await?;
    bo.sync_welcomes().await?;
    let installation = alix.context.installation_id();
    assert!(
        alix2
            .context
            .db()
            .is_awaiting_readd(&dm.group_id, installation.as_slice())?
    );
    assert!(
        bo.context
            .db()
            .is_awaiting_readd(&dm.group_id, installation.as_slice())?
    );
    let epoch_before = dm.epoch().await?;
    let mut sibling_worker = CommitLogWorker::new(alix2.context.clone());
    sibling_worker._tick().await?;
    assert_eq!(sibling_dm.epoch().await?, epoch_before + 1);
    assert!(
        !alix2
            .context
            .db()
            .is_awaiting_readd(&dm.group_id, installation.as_slice())?
    );
    let mut bo_worker = CommitLogWorker::new(bo.context.clone());
    bo_worker._tick().await?;
    assert_eq!(bo_dm.epoch().await?, epoch_before + 1);
    assert!(
        !bo.context
            .db()
            .is_awaiting_readd(&dm.group_id, installation.as_slice())?
    );
    assert!(
        bo.context
            .db()
            .get_readd_status(&dm.group_id, installation.as_slice())?
            .unwrap()
            .responded_at_sequence_id
            .is_some()
    );
    bo_worker._tick().await?;
    assert_eq!(bo_dm.epoch().await?, epoch_before + 1);
}

// verifies: FORK-080, FORK-061
#[xmtp_common::test(unwrap_try = true)]
async fn a_future_readd_claim_publishes_no_commit() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    bo.sync_all_welcomes_and_groups(None).await?;
    let mut worker = CommitLogWorker::new(alix.context.clone());
    worker._tick().await?;
    Oneshot::send_message(
        bo.context.clone(),
        vec![alix.inbox_id()],
        OneshotMessage {
            message_type: Some(MessageType::ReaddRequest(ReaddRequest {
                group_id: group.group_id.to_vec(),
                latest_commit_sequence_id: i64::MAX as u64,
            })),
        },
    )
    .await?;
    alix.sync_welcomes().await?;
    let installation = bo.context.installation_id();
    let epoch_before = group.epoch().await?;
    for _ in 0..3 {
        worker._tick().await?;
        assert_eq!(group.epoch().await?, epoch_before);
        let status = alix
            .context
            .db()
            .get_readd_status(&group.group_id, installation.as_slice())?
            .expect("the responder must retain a future request");
        assert_eq!(status.requested_at_sequence_id, Some(i64::MAX));
        assert_eq!(status.responded_at_sequence_id, None);
        assert!(
            alix.context
                .db()
                .is_awaiting_readd(&group.group_id, installation.as_slice())?
        );
    }
}

// verifies: FORK-080, FORK-064
#[xmtp_common::test(unwrap_try = true)]
async fn a_claim_at_the_processed_cursor_is_answered_once() {
    use xmtp_db::{
        prelude::{QueryLocalCommitLog, QueryRefreshState},
        refresh_state::EntityKind,
    };

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    group
        .update_group_min_version(xmtp_configuration::PROPOSALS_MIN_PROTOCOL_VERSION)
        .await?;
    bo.sync_all_welcomes_and_groups(None).await?;
    group
        .send_message(b"advance the processed cursor", Default::default())
        .await?;
    let db = alix.context.db();
    let cursor = db
        .latest_cursor_for_id(group.group_id, &[EntityKind::ApplicationMessage])?
        .0;
    assert!(
        cursor
            > db.get_latest_log_for_group(&group.group_id)?
                .unwrap()
                .commit_sequence_id as u64
    );
    let mut worker = CommitLogWorker::new(alix.context.clone());
    worker._tick().await?;
    Oneshot::send_message(
        bo.context.clone(),
        vec![alix.inbox_id()],
        OneshotMessage {
            message_type: Some(MessageType::ReaddRequest(ReaddRequest {
                group_id: group.group_id.to_vec(),
                latest_commit_sequence_id: cursor,
            })),
        },
    )
    .await?;
    alix.sync_welcomes().await?;
    let epoch_before = group.epoch().await?;
    worker._tick().await?;
    assert_eq!(group.epoch().await?, epoch_before + 1);
    let installation = bo.context.installation_id();
    assert!(!db.is_awaiting_readd(&group.group_id, installation.as_slice())?);
    assert!(
        db.get_readd_status(&group.group_id, installation.as_slice())?
            .unwrap()
            .responded_at_sequence_id
            .unwrap()
            > cursor as i64
    );
    for _ in 0..2 {
        worker._tick().await?;
        assert_eq!(group.epoch().await?, epoch_before + 1);
    }
}
