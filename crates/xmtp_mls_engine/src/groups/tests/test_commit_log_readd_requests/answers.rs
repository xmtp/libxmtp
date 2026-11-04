//! Commit log readd request tests: answers.

use super::*;

// verifies: FORK-050, FORK-053, FORK-080, GMOD-052, FORK-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_dm_peer_answers_a_readd_request() {
    let alix = EngineTestPeer::new_with(|o| o.enable_fork_recovery_requests()).await;
    let bo = EngineTestPeer::new().await;
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
    assert!(!alix_db.is_awaiting_readd(&dm.group_id, installation.as_slice())?);
    assert!(!bo_db.is_awaiting_readd(&dm.group_id, installation.as_slice())?);
    // Simulate fork detection without creating divergent MLS epoch secrets.
    alix_db.set_group_commit_log_forked_status(&dm.group_id, Some(true))?;
    alix_worker._tick().await?;
    bo.sync_welcomes().await?;
    assert!(bo_db.is_awaiting_readd(&dm.group_id, installation.as_slice())?);
    assert!(alix_db.is_awaiting_readd(&dm.group_id, installation.as_slice())?);
    let requested_at = alix_db
        .get_readd_status(&dm.group_id, installation.as_slice())?
        .unwrap()
        .requested_at_sequence_id;
    assert!(requested_at.is_some());
    // A second requester tick keeps the original request while the peer has not answered.
    alix_worker._tick().await?;
    alix.sync_welcomes().await?;
    bo.sync_welcomes().await?;
    for db in [&alix_db, &bo_db] {
        assert_eq!(
            db.get_readd_status(&dm.group_id, installation.as_slice())?
                .unwrap()
                .requested_at_sequence_id,
            requested_at
        );
        assert!(!db.is_awaiting_readd(&dm.group_id, bo.context.installation_id().as_slice())?);
    }

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

// verifies: FORK-080
#[xmtp_common::test(unwrap_try = true)]
async fn a_readder_promoted_in_an_unsynced_commit_answers() {
    use xmtp_db::{
        prelude::{QueryGroupVersion, QueryRefreshState},
        refresh_state::EntityKind,
    };

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let group = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await?;
    bo.sync_all_welcomes_and_groups(None).await?;
    let bo_group = bo.group(&group.group_id)?;
    bo_group.update_consent_state(ConsentState::Allowed)?;
    // Bo has not processed its promotion when the request arrives.
    group
        .update_admin_list(UpdateAdminListType::AddSuper, bo.inbox_id().to_string())
        .await?;
    let cursor = alix
        .context
        .db()
        .latest_cursor_for_id(group.group_id, &[EntityKind::ApplicationMessage])?
        .0;
    Oneshot::send_message(
        caro.context.clone(),
        vec![bo.inbox_id()],
        OneshotMessage {
            message_type: Some(MessageType::ReaddRequest(ReaddRequest {
                group_id: group.group_id.to_vec(),
                latest_commit_sequence_id: cursor,
            })),
        },
    )
    .await?;
    bo.sync_welcomes().await?;
    let db = bo.context.db();
    let installation = caro.context.installation_id();
    assert!(db.is_awaiting_readd(&group.group_id, installation.as_slice())?);
    let mut worker = CommitLogWorker::new(bo.context.clone());
    // A failed sync must not judge permission from the unsynced state.
    db.set_group_paused(&group.group_id, "999.0.0")?;
    worker._tick().await?;
    assert!(db.is_awaiting_readd(&group.group_id, installation.as_slice())?);
    db.unpause_group(&group.group_id)?;
    let epoch_before = group.epoch().await?;
    for _ in 0..3 {
        worker._tick().await?;
        if !db.is_awaiting_readd(&group.group_id, installation.as_slice())? {
            break;
        }
    }
    assert_eq!(bo_group.epoch().await?, epoch_before + 1);
    assert!(!db.is_awaiting_readd(&group.group_id, installation.as_slice())?);
}

// verifies: FORK-050, FORK-080, FORK-064, GMOD-052
#[xmtp_common::test(unwrap_try = true)]
async fn a_dm_readd_is_answered_once_by_peer_or_sibling() {
    let alix = EngineTestPeer::new_with(|o| o.enable_fork_recovery_requests()).await;
    let alix2 = alix
        .new_installation_with(|o| o.enable_fork_recovery_requests())
        .await;
    let bo = EngineTestPeer::new().await;
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

// verifies: FORK-080
#[rstest::rstest]
#[case::signed_max(i64::MAX as u64)]
#[case::unsigned_max(u64::MAX)]
#[xmtp_common::test(unwrap_try = true)]
async fn a_future_readd_claim_publishes_no_commit(
    #[case] claimed: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let group = alix
        .find_or_create_dm(bo.inbox_id().to_string(), None)
        .await?;
    bo.sync_all_welcomes_and_groups(None).await?;
    let bo_group = bo.group(&group.group_id)?;
    assert!(group.super_admin_list()?.is_empty());
    group.key_update().await?;
    bo_group.sync().await?;
    let mut worker = CommitLogWorker::new(alix.context.clone());
    worker._tick().await?;
    Oneshot::send_message(
        bo.context.clone(),
        vec![alix.inbox_id()],
        OneshotMessage {
            message_type: Some(MessageType::ReaddRequest(ReaddRequest {
                group_id: group.group_id.to_vec(),
                latest_commit_sequence_id: claimed,
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
        assert_eq!(status.requested_at_sequence_id, Some(claimed as i64));
        assert_eq!(status.responded_at_sequence_id, None);
        assert!(
            alix.context
                .db()
                .get_readds_awaiting_response(
                    &group.group_id,
                    alix.context.installation_id().as_slice(),
                )?
                .iter()
                .any(|pending| pending.installation_id == installation.as_slice())
        );
    }
    Ok(())
}

// verifies: FORK-080, FORK-064
#[xmtp_common::test(unwrap_try = true)]
async fn a_claim_at_the_processed_cursor_is_answered_once() {
    use xmtp_db::{
        prelude::{QueryLocalCommitLog, QueryRefreshState},
        refresh_state::EntityKind,
    };

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
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
