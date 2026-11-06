//! Commit log readd request tests: requests.

use super::*;

// verifies: FORK-041
#[xmtp_common::test(unwrap_try = true)]
async fn test_readd_installation_succeeds() {
    use xmtp_db::{prelude::QueryRefreshState, refresh_state::EntityKind};

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;

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
    b_group.update_consent_state(ConsentState::Allowed)?;

    let bo_installation_id = bo.context.installation_id();
    let prev_authenticator = a_group.epoch_authenticator().await.unwrap();
    assert_eq!(
        b_group.epoch_authenticator().await.unwrap(),
        prev_authenticator
    );
    bo.context
        .db()
        .set_group_commit_log_forked_status(&a_group.group_id, Some(true))?;
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
    assert_eq!(
        bo.context
            .db()
            .get_group_commit_log_forked_status(&a_group.group_id)?,
        Some(true)
    );
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
    assert_eq!(b_group.consent_state()?, ConsentState::Allowed);
    assert_eq!(
        bo.context
            .db()
            .get_group_commit_log_forked_status(&a_group.group_id)?,
        None
    );
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

// verifies: FORK-051, FORK-064
#[xmtp_common::test(unwrap_try = true)]
async fn test_readd_bookkeeping() {
    let alix = EngineTestPeer::new_with(|o| o.enable_fork_recovery_requests()).await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let devon = EngineTestPeer::new().await;
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
    // An admin is not a permitted readder.
    group
        .update_admin_list(UpdateAdminListType::Add, devon.inbox_id().to_string())
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

    let alix_installation = alix.context.installation_id();
    let bo_installation = bo.context.installation_id();
    let assert_awaiting = |expected: &[(&dyn QueryReaddStatus, &[u8], bool)]| {
        for (i, (conn, installation, awaiting)) in expected.iter().enumerate() {
            assert_eq!(
                conn.is_awaiting_readd(&group.group_id, installation)
                    .unwrap(),
                *awaiting,
                "row {i}"
            );
        }
    };
    // No readd requests yet
    assert_awaiting(&[
        (&a_conn, &alix_installation[..], false),
        (&b_conn, &alix_installation[..], false),
        (&c_conn, &alix_installation[..], false),
    ]);

    // Should trigger readd requests to be sent
    a_worker._tick().await.unwrap();
    // Should no-op
    a_worker._tick().await.unwrap();
    // Receive any oneshot messages
    alix.sync_welcomes().await.unwrap();
    bo.sync_welcomes().await.unwrap();
    caro.sync_welcomes().await.unwrap();
    devon.sync_welcomes().await.unwrap();

    // Only the requester and super admins record Alix's request; nobody records Bo
    assert_awaiting(&[
        (&a_conn, &alix_installation[..], true),
        (&b_conn, &alix_installation[..], true),
        (&c_conn, &alix_installation[..], true),
        (&d_conn, &alix_installation[..], false),
        (&a_conn, &bo_installation[..], false),
        (&b_conn, &bo_installation[..], false),
        (&c_conn, &bo_installation[..], false),
    ]);

    // Let's say Bo's worker processes it first
    let mut b_worker = CommitLogWorker::new(bo.context.clone());
    b_worker._tick().await.unwrap();

    caro.sync_all_welcomes_and_groups(None).await.unwrap();
    devon.sync_all_welcomes_and_groups(None).await.unwrap();
    // Apply the removal, but withhold the Welcome that completes our request.
    group.sync_with_conn().await?;
    assert!(!group.is_active()?);
    assert_awaiting(&[
        (&a_conn, &alix_installation[..], true),
        (&b_conn, &alix_installation[..], false),
        (&c_conn, &alix_installation[..], false),
        (&d_conn, &alix_installation[..], false),
    ]);
    a_worker._tick().await?; // Drain the removal's log upload.
    let published = alix.api_stats().publish.get_count();
    for _ in 0..2 {
        a_worker._tick().await?;
        assert_eq!(alix.api_stats().publish.get_count(), published);
    }
    alix.sync_welcomes().await?;
    assert!(group.is_active()?);
    assert_eq!(
        group.epoch_authenticator().await?,
        b_group.epoch_authenticator().await?
    );

    // Everyone should see that Alix is no longer awaiting readd
    assert_awaiting(&[
        (&a_conn, &alix_installation[..], false),
        (&b_conn, &alix_installation[..], false),
        (&c_conn, &alix_installation[..], false),
        (&d_conn, &alix_installation[..], false),
    ]);

    b_group.key_update().await?;
    group.sync_with_conn().await?;
    b_worker._tick().await?;
    a_worker._tick().await?;
    let published = alix.api_stats().publish.get_count();
    a_conn.set_group_commit_log_forked_status(&group.group_id, Some(true))?;
    a_worker._tick().await?;
    assert!(alix.api_stats().publish.get_count() > published);
    assert!(a_conn.is_awaiting_readd(&group.group_id, alix_installation.as_slice())?);
}

#[xmtp_common::test]
async fn test_request_readd_with_allowlisted_groups() {
    // Step 1: Bo creates a group
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;

    let group = bo
        .create_group_with_members(&[caro.inbox_id()], None, None)
        .await
        .unwrap();

    let group_id = group.group_id;
    let group_id_typed: GroupId = group_id;
    let group_id_hex = hex::encode(group_id);
    let unnormalized_group_id = "0x".to_owned() + &group_id_hex.to_uppercase();

    // Step 2: Create Alix with that group ID in the allowlist
    let alix = EngineTestPeer::new_with(|o| {
        o.enable_fork_recovery_requests_for(vec![unnormalized_group_id])
    })
    .await;

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
