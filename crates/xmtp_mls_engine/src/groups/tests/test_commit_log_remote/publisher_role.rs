//! Remote commit log tests: publisher role.

use super::*;

// verifies: FORK-085
#[xmtp_common::test(unwrap_try = true)]
async fn the_publisher_flag_follows_the_super_admin_list() {
    use crate::groups::UpdateAdminListType::{AddSuper, RemoveSuper};
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let a = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    bo.sync_welcomes().await?;
    let b = bo.group(&a.group_id)?;
    assert!(
        !bo.context
            .db()
            .find_group(&a.group_id)??
            .should_publish_commit_log
    );
    a.update_admin_list(AddSuper, bo.inbox_id().to_string())
        .await?;
    b.sync().await?;
    assert!(
        bo.context
            .db()
            .find_group(&a.group_id)??
            .should_publish_commit_log
    );
    b.update_admin_list(RemoveSuper, alix.inbox_id().to_string())
        .await?;
    a.sync().await?;
    assert!(
        !alix
            .context
            .db()
            .find_group(&a.group_id)??
            .should_publish_commit_log
    );

    // Model the cached role left by an older client before replacing its MLS state.
    xmtp_mls_core::state_tx::state_write(alix.context.mls_storage(), |tx| {
        use xmtp_db::XmtpMlsStorageProvider;
        tx.storage()
            .db()
            .set_should_publish_commit_log(&a.group_id, true)?;
        Ok::<_, xmtp_db::ConnectionError>(xmtp_db::TransactionOutcome::Continue(()))
    })?;
    b.readd_installations(vec![alix.context.installation_id().to_vec()])
        .await?;
    a.sync_with_conn().await?;
    alix.sync_welcomes().await?;
    assert!(
        !alix
            .context
            .db()
            .find_group(&a.group_id)??
            .should_publish_commit_log
    );
}

// verifies: FORK-085
#[xmtp_common::test(unwrap_try = true)]
async fn a_readded_publisher_does_not_publish_its_earlier_membership() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let a = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    bo.sync_welcomes().await?;
    let b = bo.group(&a.group_id)?;
    b.update_consent_state(ConsentState::Allowed)?;
    a.update_admin_list(
        crate::groups::UpdateAdminListType::AddSuper,
        bo.inbox_id().to_string(),
    )
    .await?;
    a.update_group_name("unpublished before rejoin".into())
        .await?;
    b.sync().await?;
    let previous = b.local_commit_log().await?.last()?.commit_sequence_id as u64;
    a.readd_installations(vec![bo.context.installation_id().to_vec()])
        .await?;
    b.sync_with_conn().await?;
    bo.sync_welcomes().await?;
    a.update_group_name("after rejoin".into()).await?;
    b.sync().await?;
    publish_turn(&mut CommitLogWorker::new(bo.context.clone())).await;
    let entries = remote_entries(&bo, &b).await;
    assert!(!entries.is_empty());
    assert!(entries.iter().all(|entry| entry.author_installation_key
        == bo.context.installation_id().to_vec()
        && entry.commit_sequence_id > previous));
}

// verifies: FORK-087, FORK-083, FORK-085
#[xmtp_common::test(unwrap_try = true)]
async fn a_publisher_handover_keeps_the_log_readable() {
    use crate::groups::UpdateAdminListType::{AddSuper, RemoveSuper};
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let dan = EngineTestPeer::new().await;
    let a = alix
        .create_group_with_members(
            &[bo.inbox_id(), caro.inbox_id(), dan.inbox_id()],
            None,
            None,
        )
        .await?;
    bo.sync_welcomes().await?;
    caro.sync_welcomes().await?;
    dan.sync_welcomes().await?;
    let b = bo.group(&a.group_id)?;
    let c = caro.group(&a.group_id)?;
    let d = dan.group(&a.group_id)?;
    for group in [&b, &c, &d] {
        group.update_consent_state(ConsentState::Allowed)?;
    }
    a.update_group_name("accepted old publisher prefix".into())
        .await?;
    c.sync().await?;
    publish_turn(&mut CommitLogWorker::new(alix.context.clone())).await;
    let mut reader = CommitLogWorker::new(caro.context.clone());
    reader
        .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
        .await?;
    let last_old = caro
        .context
        .db()
        .get_latest_remote_log_for_group(&a.group_id)??
        .commit_sequence_id;
    // This unpublished entry leaves a gap before Bo's promotion.
    a.update_group_name("unpublished gap".into()).await?;
    a.update_admin_list(AddSuper, bo.inbox_id().to_string())
        .await?;
    b.sync().await?;
    let promotion = b.local_commit_log().await?.last()?.commit_sequence_id as u64;
    b.update_admin_list(RemoveSuper, alix.inbox_id().to_string())
        .await?;
    a.sync().await?;
    c.sync().await?;
    d.sync().await?;
    let mut demoted = PlaintextCommitLogEntry::from(a.local_commit_log().await?.last()?);
    let keys = keys_of(&a);
    sign_entry_author(
        &keys,
        &alix.context.identity().installation_keys,
        &mut demoted,
    )?;
    let mut stored = Vec::new();
    alix.context
        .api()
        .publish_commit_log(vec![seal_entry(&keys, &demoted)?], &mut stored)
        .await?;
    publish_turn(&mut CommitLogWorker::new(bo.context.clone())).await;
    let published = remote_entries(&bo, &b).await;
    let bo_entries = published
        .iter()
        .filter(|entry| entry.author_installation_key == bo.context.installation_id().to_vec())
        .collect::<Vec<_>>();
    assert!(
        !bo_entries.is_empty(),
        "the promoted publisher must publish"
    );
    assert!(
        bo_entries
            .iter()
            .all(|entry| entry.commit_sequence_id >= promotion),
        "the new publisher must not publish its earlier history"
    );
    assert!(
        bo_entries
            .iter()
            .any(|entry| entry.commit_sequence_id == promotion),
        "the promotion commit must be published"
    );
    for mut reader in [reader, CommitLogWorker::new(dan.context.clone())] {
        reader
            .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
            .await?;
        reader
            .run_test(CommitLogTestFunction::CheckForkedState, None)
            .await?;
    }
    for client in [&caro, &dan] {
        let entries = client.context.db().get_remote_commit_log_after_cursor(
            &a.group_id,
            0,
            RemoteCommitLogOrder::AscendingByRowid,
        )?;
        assert!(entries.iter().any(|entry| entry.author_installation_key == bo.context.installation_id().to_vec()));
        assert!(entries.iter().filter(|entry| entry.commit_sequence_id > last_old)
            .all(|entry| entry.author_installation_key == bo.context.installation_id().to_vec()));
        assert_eq!(
            client
                .context
                .db()
                .get_group_commit_log_forked_status(&a.group_id)?,
            Some(false)
        );
    }
}

enum PublisherReturn {
    Role,
    Readd,
    Continuous,
}

async fn assert_returning_publisher_progresses(change: PublisherReturn) {
    use crate::groups::UpdateAdminListType::{AddSuper, RemoveSuper};
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    #[cfg(not(target_arch = "wasm32"))]
    let caro = EngineTestPeer::persistent().await;
    #[cfg(target_arch = "wasm32")]
    let caro = EngineTestPeer::new().await;
    let dan = EngineTestPeer::new().await;
    let eve = EngineTestPeer::new().await;
    let a = alix
        .create_group_with_members(
            &[bo.inbox_id(), caro.inbox_id(), dan.inbox_id()],
            None,
            None,
        )
        .await
        .unwrap();
    bo.sync_welcomes().await.unwrap();
    caro.sync_welcomes().await.unwrap();
    dan.sync_welcomes().await.unwrap();
    let b = bo.group(&a.group_id).unwrap();
    let c = caro.group(&a.group_id).unwrap();
    let d = dan.group(&a.group_id).unwrap();
    for group in [&b, &c, &d] {
        group.update_consent_state(ConsentState::Allowed).unwrap();
    }
    a.update_group_name("accepted prefix before publisher return".into())
        .await
        .unwrap();
    c.sync().await.unwrap();
    publish_turn(&mut CommitLogWorker::new(alix.context.clone())).await;
    CommitLogWorker::new(caro.context.clone())
        .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
        .await
        .unwrap();
    let prefix = caro
        .context
        .db()
        .get_latest_remote_log_for_group(&a.group_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        prefix.author_installation_key,
        alix.context.installation_id().to_vec()
    );
    a.update_group_name("unpublished gap before publisher return".into())
        .await
        .unwrap();
    let gap = a
        .local_commit_log()
        .await
        .unwrap()
        .last()
        .unwrap()
        .commit_sequence_id;
    a.update_admin_list(AddSuper, bo.inbox_id().to_string())
        .await
        .unwrap();
    b.sync().await.unwrap();
    match change {
        PublisherReturn::Readd => {
            b.readd_installations(vec![alix.context.installation_id().to_vec()])
                .await
                .unwrap();
            a.sync_with_conn().await.unwrap();
            alix.sync_welcomes().await.unwrap();
        }
        PublisherReturn::Role => {
            b.update_admin_list(RemoveSuper, alix.inbox_id().to_string())
                .await
                .unwrap();
            b.update_admin_list(AddSuper, alix.inbox_id().to_string())
                .await
                .unwrap();
        }
        PublisherReturn::Continuous => {
            a.update_admin_list(RemoveSuper, bo.inbox_id().to_string())
                .await
                .unwrap();
            a.update_admin_list(AddSuper, bo.inbox_id().to_string())
                .await
                .unwrap();
            b.sync().await.unwrap();
        }
    }
    let return_commit = b
        .local_commit_log()
        .await
        .unwrap()
        .last()
        .unwrap()
        .commit_sequence_id;
    // Eve joins after the transitions; her Welcome is her only boundary evidence.
    b.add_members(&[eve.inbox_id()]).await.unwrap();
    let join_commit = b
        .local_commit_log()
        .await
        .unwrap()
        .last()
        .unwrap()
        .commit_sequence_id;
    eve.sync_welcomes().await.unwrap();
    let e = eve.group(&a.group_id).unwrap();
    e.update_consent_state(ConsentState::Allowed).unwrap();
    b.update_group_name("latest commit after publisher return".into())
        .await
        .unwrap();
    a.sync().await.unwrap();
    c.sync().await.unwrap();
    d.sync().await.unwrap();
    e.sync().await.unwrap();
    let latest = c
        .local_commit_log()
        .await
        .unwrap()
        .last()
        .unwrap()
        .commit_sequence_id;
    publish_turn(&mut CommitLogWorker::new(bo.context.clone())).await;
    publish_turn(&mut CommitLogWorker::new(alix.context.clone())).await;
    if !matches!(change, PublisherReturn::Continuous) {
        assert!(
            remote_entries(&bo, &b)
                .await
                .iter()
                .all(|entry| entry.commit_sequence_id != gap as u64)
        );
    }
    // Reopen Caro's complete database, including the old accepted prefix, before
    // reading. A process-local boundary would be lost here.
    #[cfg(not(target_arch = "wasm32"))]
    let caro = {
        let path = match caro.context.store().opts() {
            xmtp_db::StorageOption::Persistent(path) => path.clone(),
            xmtp_db::StorageOption::Ephemeral => panic!("restart test needs a persistent database"),
        };
        let owner = caro.owner.clone();
        drop((c, caro));
        super::super::test_commit_log_upgrade::reopen_commit_log_client(owner, &path).await
    };
    // Caro retains Alix's prefix; Dan downloads it after the return; Eve's
    // Welcome follows the return. All three must reach the same local tip.
    for (label, client) in [("returning", &caro), ("delayed", &dan), ("joined", &eve)] {
        let mut reader = CommitLogWorker::new(client.context.clone());
        reader
            .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
            .await
            .unwrap();
        let tip = client
            .context
            .db()
            .get_latest_remote_log_for_group(&a.group_id)
            .unwrap()
            .unwrap();
        assert_eq!(
            tip.commit_sequence_id, latest,
            "{label}: publisher return must not strand the accepted chain"
        );
        let accepted = client
            .context
            .db()
            .get_remote_commit_log_after_cursor(
                &a.group_id,
                0,
                RemoteCommitLogOrder::AscendingByRowid,
            )
            .unwrap();
        if client.context.installation_id() == eve.context.installation_id() {
            assert!(
                accepted
                    .iter()
                    .any(|entry| entry.commit_sequence_id == join_commit),
                "a fresh reader must accept the entry at its Welcome's group-message anchor"
            );
        }
        if !matches!(change, PublisherReturn::Continuous) {
            assert!(
                accepted.iter().all(|entry| {
                    entry.commit_sequence_id <= prefix.commit_sequence_id
                        || entry.commit_sequence_id >= return_commit
                }),
                "a pre-boundary candidate must not restart the old chain"
            );
        } else if client.context.installation_id() != eve.context.installation_id() {
            assert!(
                accepted.iter().any(|entry| entry.commit_sequence_id == gap),
                "unrelated publisher churn must not reset a continuous author's chain"
            );
        }
        reader
            .run_test(CommitLogTestFunction::CheckForkedState, None)
            .await
            .unwrap();
        assert_eq!(
            client
                .context
                .db()
                .get_group_commit_log_forked_status(&a.group_id)
                .unwrap(),
            Some(false)
        );
    }
}

// verifies: FORK-083, FORK-089
#[xmtp_common::test(unwrap_try = true)]
async fn a_repromoted_publisher_keeps_old_and_new_readers_progressing() {
    assert_returning_publisher_progresses(PublisherReturn::Role).await;
}

// verifies: FORK-083, FORK-089
#[xmtp_common::test(unwrap_try = true)]
async fn a_readded_publisher_keeps_old_and_new_readers_progressing() {
    assert_returning_publisher_progresses(PublisherReturn::Readd).await;
}

// verifies: FORK-083
#[xmtp_common::test(unwrap_try = true)]
async fn a_continuous_publisher_keeps_its_chain_across_unrelated_role_changes() {
    assert_returning_publisher_progresses(PublisherReturn::Continuous).await;
}
