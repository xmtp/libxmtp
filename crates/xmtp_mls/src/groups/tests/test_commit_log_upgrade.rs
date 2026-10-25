//! The author migration must keep an existing handover able to publish.
#![cfg(not(target_arch = "wasm32"))]

use std::sync::Arc;

use diesel::RunQueryDsl;
use diesel::sql_types::{Binary, Bool, Integer};
use xmtp_db::consent_record::ConsentState;
use xmtp_db::prelude::*;
use xmtp_db::refresh_state::EntityKind;
use xmtp_db::remote_commit_log::RemoteCommitLogOrder;
use xmtp_db::{ConnectionExt, StorageOption, Store, TestDb, XmtpTestDb};

use crate::context::XmtpSharedContext;
use crate::groups::UpdateAdminListType::{AddSuper, RemoveSuper};
use crate::groups::commit_log::{CommitLogTestFunction, CommitLogWorker};
use crate::{
    Client,
    builder::DeviceSyncMode,
    identity::IdentityStrategy,
    tester,
    utils::{DefaultTestClientCreator, FullXmtpClient},
};
use xmtp_id::associations::test_utils::MockSmartContractSignatureVerifier;
use xmtp_proto::{api_client::ApiBuilder, prelude::XmtpTestClient};

// Snapshot Tester restoration intentionally resets network progress. Reopen the
// original file and cached identity to preserve the state a real restart keeps.
pub(super) async fn reopen_commit_log_client(database: &str) -> FullXmtpClient {
    reopen_commit_log_client_with_logging(database, None).await
}

async fn reopen_commit_log_client_with_logging(
    database: &str,
    logging: Option<bool>,
) -> FullXmtpClient {
    let store = TestDb::create_persistent_store(Some(database.into())).await;
    let api = Arc::new(DefaultTestClientCreator::create().build().unwrap());
    let mut builder = Client::builder(IdentityStrategy::CachedOnly)
        .store(store)
        .api_client(api)
        .default_mls_store()
        .unwrap()
        .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
        .with_disable_workers(true)
        .with_commit_log_worker(false)
        .with_device_sync_worker_mode(Some(DeviceSyncMode::Disabled));
    if let Some(logging) = logging {
        builder =
            builder.config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
                |configuration| configuration.mls.commit_log_enabled = Some(logging),
            )));
    }
    builder.build().await.unwrap()
}

// verifies: FORK-085, FORK-086
#[rstest::rstest]
#[case::idle(false, false, false)]
#[case::first_merge(true, false, false)]
#[case::cleared_history(false, true, false)]
#[case::imported_history(false, true, true)]
#[xmtp_common::test(unwrap_try = true)]
async fn legacy_publisher_recovery_preserves_the_eligible_backlog(
    #[case] first_merge: bool,
    #[case] clear_history: bool,
    #[case] imported_history: bool,
) {
    tester!(alix, persistent_db, disable_workers);
    tester!(bo, persistent_db, disable_workers);
    tester!(caro, persistent_db, disable_workers);
    let a = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await
        .unwrap();
    bo.sync_welcomes().await.unwrap();
    caro.sync_welcomes().await.unwrap();
    let b = bo.group(&a.group_id).unwrap();
    let c = caro.group(&a.group_id).unwrap();
    b.update_consent_state(ConsentState::Allowed).unwrap();
    c.update_consent_state(ConsentState::Allowed).unwrap();
    a.update_group_name("before promotion".into())
        .await
        .unwrap();
    b.sync().await.unwrap();
    let prepromotion = b
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
    let promotion = b
        .local_commit_log()
        .await
        .unwrap()
        .last()
        .unwrap()
        .commit_sequence_id;
    b.update_group_name("pending before upgrade".into())
        .await
        .unwrap();
    let pending = b
        .local_commit_log()
        .await
        .unwrap()
        .last()
        .unwrap()
        .commit_sequence_id;
    b.update_admin_list(RemoveSuper, alix.inbox_id().to_string())
        .await
        .unwrap();
    a.sync().await.unwrap();
    c.sync().await.unwrap();
    let last_role_change = b
        .local_commit_log()
        .await
        .unwrap()
        .last()
        .unwrap()
        .commit_sequence_id;
    let group_id = a.group_id;
    if clear_history {
        let db = bo.context.db();
        let retained = db
            .get_group_messages(&group_id, &Default::default())
            .unwrap();
        // The supported db_tools clear removes genuine role transcripts but
        // retains the local commit records needed to prove a safe suffix.
        db.clear_messages(Some(&[group_id]), None).unwrap();
        if imported_history {
            // Real archive conversion retains content and sequence positions,
            // but imported transcripts cannot prove this client's applied role.
            for message in retained {
                let saved: xmtp_proto::xmtp::device_sync::message_backup::GroupMessageSave =
                    message.into();
                let restored = xmtp_db::group_message::StoredGroupMessage::try_from(saved).unwrap();
                restored.store(&db).unwrap();
            }
        }
    }

    // The prior build kept the creator's flag and never enabled the promoted
    // member. Write that input directly, without the new setter's cutoff.
    for (client, old_flag) in [(&alix, true), (&bo, false)] {
        client
            .context
            .db()
            .raw_query(|conn| {
                diesel::sql_query("UPDATE groups SET should_publish_commit_log = ? WHERE id = ?")
                    .bind::<Bool, _>(old_flag)
                    .bind::<Binary, _>(group_id.as_slice())
                    .execute(conn)
                    .unwrap();
                diesel::sql_query(
                    "DELETE FROM refresh_state WHERE entity_id = ? AND entity_kind = ?",
                )
                .bind::<Binary, _>(group_id.as_slice())
                .bind::<Integer, _>(EntityKind::CommitLogUpload as i32)
                .execute(conn)
            })
            .unwrap();
    }
    // Run the real down migration and remove its tracking row. Reopening each
    // file runs pending migrations before opening its cached identity.
    for client in [&alix, &bo, &caro] {
        // The prior build had no publisher-boundary records.
        client
            .context
            .db()
            .raw_query(|conn| {
                diesel::sql_query("DELETE FROM refresh_state WHERE entity_kind IN (?, ?)")
                    .bind::<Integer, _>(EntityKind::CommitLogPublisherBoundary as i32)
                    .bind::<Integer, _>(EntityKind::CommitLogPublisherRecovery as i32)
                    .execute(conn)
            })
            .unwrap();
        client
            .context
            .db()
            .rollback_to_version("20261025000000")
            .unwrap();
    }
    let paths = [&alix, &bo, &caro].map(|client| match client.context.store().opts() {
        StorageOption::Persistent(path) => path.clone(),
        StorageOption::Ephemeral => panic!("upgrade test needs a persistent database"),
    });
    drop((a, b, c, alix, bo, caro));

    let alix = reopen_commit_log_client(&paths[0]).await;
    let bo = reopen_commit_log_client(&paths[1]).await;
    let caro = reopen_commit_log_client(&paths[2]).await;
    assert!(
        alix.context
            .db()
            .find_group(&group_id)
            .unwrap()
            .unwrap()
            .should_publish_commit_log
    );
    assert!(
        !bo.context
            .db()
            .find_group(&group_id)
            .unwrap()
            .unwrap()
            .should_publish_commit_log
    );
    let a = alix.group(&group_id).unwrap();
    let b = bo.group(&group_id).unwrap();
    let c = caro.group(&group_id).unwrap();
    if first_merge {
        b.update_group_name("ordinary commit after upgrade".into())
            .await
            .unwrap();
        a.sync().await.unwrap();
        c.sync().await.unwrap();
    }
    let latest_commit = b
        .local_commit_log()
        .await
        .unwrap()
        .last()
        .unwrap()
        .commit_sequence_id;

    let bo = if !first_merge && !clear_history {
        // Recovery must survive a failed publish and a real restart. Clearing
        // transcripts afterward must not discard the already proven backlog.
        CommitLogWorker::new(bo.context.clone())
            .with_seal_fault(Arc::new(|_, _| {
                Err(xmtp_mls_common::group_keys::SealError::SelfCheck)
            }))
            .run_test(CommitLogTestFunction::PublishCommitLogsToRemote, None)
            .await
            .unwrap();
        bo.context
            .db()
            .clear_messages(Some(&[group_id]), None)
            .unwrap();
        drop((b, bo));
        reopen_commit_log_client(&paths[1]).await
    } else {
        drop(b);
        bo
    };

    for client in [&alix, &bo] {
        CommitLogWorker::new(client.context.clone())
            .run_test(CommitLogTestFunction::PublishCommitLogsToRemote, None)
            .await
            .unwrap();
    }
    assert!(
        !alix
            .context
            .db()
            .find_group(&group_id)
            .unwrap()
            .unwrap()
            .should_publish_commit_log,
        "recovery must disable the demoted publisher after upgrade"
    );
    assert!(
        bo.context
            .db()
            .find_group(&group_id)
            .unwrap()
            .unwrap()
            .should_publish_commit_log,
        "recovery must enable the current publisher after upgrade"
    );
    let mut reader = CommitLogWorker::new(caro.context.clone());
    reader
        .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
        .await
        .unwrap();
    reader
        .run_test(CommitLogTestFunction::CheckForkedState, None)
        .await
        .unwrap();
    let accepted = caro
        .context
        .db()
        .get_remote_commit_log_after_cursor(&group_id, 0, RemoteCommitLogOrder::AscendingByRowid)
        .unwrap();
    assert!(
        accepted.iter().any(|entry| {
            entry.author_installation_key == bo.context.installation_id().to_vec()
                && entry.commit_sequence_id == latest_commit
        }),
        "the current publisher must publish the ordinary commit after upgrade"
    );
    let bo_entries: Vec<_> = accepted
        .iter()
        .filter(|entry| entry.author_installation_key == bo.context.installation_id().to_vec())
        .map(|entry| entry.commit_sequence_id)
        .collect();
    assert!(
        !bo_entries.contains(&prepromotion),
        "prepromotion history must not publish"
    );
    assert!(
        bo_entries.contains(&last_role_change),
        "eligible pending history must survive upgrade"
    );
    for sequence in [promotion, pending] {
        assert_eq!(
            bo_entries.contains(&sequence),
            !clear_history,
            "intact evidence recovers the promotion and backlog; cleared history omits its uncertain prefix"
        );
    }
    assert_eq!(
        caro.context
            .db()
            .get_group_commit_log_forked_status(&group_id)
            .unwrap(),
        Some(false)
    );
}

// verifies: FORK-085, FORK-086
#[xmtp_common::test(unwrap_try = true)]
async fn legacy_dm_keeps_pending_entries_after_an_ordinary_merge() {
    tester!(alix, persistent_db, disable_workers);
    tester!(bo, disable_workers);
    let a = alix.find_or_create_dm(bo.inbox_id(), None).await?;
    bo.sync_welcomes().await?;
    let b = bo.group(&a.group_id)?;
    b.update_consent_state(ConsentState::Allowed)?;
    a.key_update().await?;
    let pending = a.local_commit_log().await?.last()?.commit_sequence_id;
    let group_id = a.group_id;
    alix.context.db().raw_query(|conn| {
        diesel::sql_query("UPDATE groups SET should_publish_commit_log = 0 WHERE id = ?")
            .bind::<Binary, _>(group_id.as_slice())
            .execute(conn)?;
        diesel::sql_query("DELETE FROM refresh_state WHERE entity_id = ? AND entity_kind IN (?, ?)")
            .bind::<Binary, _>(group_id.as_slice())
            .bind::<Integer, _>(EntityKind::CommitLogUpload as i32)
            .bind::<Integer, _>(EntityKind::CommitLogPublisherRecovery as i32)
            .execute(conn)
    })?;
    alix.context.db().rollback_to_version("20261025000000")?;
    let path = match alix.context.store().opts() {
        StorageOption::Persistent(path) => path.clone(),
        StorageOption::Ephemeral => panic!("upgrade test needs persistent storage"),
    };
    drop((a, alix));
    let alix = reopen_commit_log_client(&path).await;
    let a = alix.group(&group_id)?;
    a.key_update().await?;
    b.sync().await?;
    CommitLogWorker::new(alix.context.clone())
        .run_test(CommitLogTestFunction::PublishCommitLogsToRemote, None)
        .await?;
    let mut reader = CommitLogWorker::new(bo.context.clone());
    reader
        .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
        .await?;
    let accepted = bo.context.db().get_remote_commit_log_after_cursor(
        &group_id,
        0,
        RemoteCommitLogOrder::AscendingByRowid,
    )?;
    assert!(
        accepted
            .iter()
            .any(|entry| entry.commit_sequence_id == pending),
        "a continuing DM publisher must keep its preupgrade pending entry"
    );
}

// verifies: FORK-085, FORK-086
#[rstest::rstest]
#[case::promotion(false)]
#[case::replacement_welcome(true)]
#[xmtp_common::test(unwrap_try = true)]
async fn legacy_recovery_excludes_history_before_an_unlogged_transition(#[case] replacement: bool) {
    tester!(alix, disable_workers);
    tester!(bo, persistent_db, disable_workers);
    tester!(caro, disable_workers);
    let a = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await
        .unwrap();
    bo.sync_welcomes().await.unwrap();
    caro.sync_welcomes().await.unwrap();
    let b = bo.group(&a.group_id).unwrap();
    let c = caro.group(&a.group_id).unwrap();
    b.update_consent_state(ConsentState::Allowed).unwrap();
    c.update_consent_state(ConsentState::Allowed).unwrap();
    if replacement {
        a.update_admin_list(AddSuper, bo.inbox_id().to_string())
            .await
            .unwrap();
        b.sync().await.unwrap();
    }
    a.key_update().await.unwrap();
    b.sync().await.unwrap();
    let old_entry = b
        .local_commit_log()
        .await
        .unwrap()
        .last()
        .unwrap()
        .commit_sequence_id;
    let old_count = b.local_commit_log().await.unwrap().len();
    let group_id = a.group_id;
    let bo_inbox = bo.inbox_id().to_string();
    let path = match bo.context.store().opts() {
        StorageOption::Persistent(path) => path.clone(),
        StorageOption::Ephemeral => panic!("upgrade test needs persistent storage"),
    };
    drop((b, bo));
    let bo = reopen_commit_log_client_with_logging(&path, Some(false)).await;
    let b = bo.group(&group_id).unwrap();
    if replacement {
        a.readd_installations(vec![bo.context.installation_id().to_vec()])
            .await
            .unwrap();
        b.sync().await.unwrap();
        bo.sync_welcomes().await.unwrap();
    } else {
        a.update_admin_list(AddSuper, bo_inbox).await.unwrap();
        b.sync().await.unwrap();
    }
    b.update_admin_list(RemoveSuper, alix.inbox_id().to_string())
        .await
        .unwrap();
    a.sync().await.unwrap();
    assert_eq!(
        b.local_commit_log().await.unwrap().len(),
        old_count,
        "disabled logging must retain only the old membership/role history"
    );
    drop((b, bo));
    let bo = reopen_commit_log_client_with_logging(&path, Some(true)).await;
    let b = bo.group(&group_id).unwrap();
    b.key_update().await.unwrap();
    let connected_entry = b
        .local_commit_log()
        .await
        .unwrap()
        .last()
        .unwrap()
        .commit_sequence_id;
    c.sync().await.unwrap();
    // Model the old build: no recovery marker or author migration. The real
    // administrative clear also erases transcripts from the unlogged interval.
    bo.context
        .db()
        .clear_messages(Some(&[group_id]), None)
        .unwrap();
    bo.context
        .db()
        .raw_query(|conn| {
            diesel::sql_query("UPDATE groups SET should_publish_commit_log = 0 WHERE id = ?")
                .bind::<Binary, _>(group_id.as_slice())
                .execute(conn)?;
            diesel::sql_query(
                "DELETE FROM refresh_state WHERE entity_id = ? AND entity_kind IN (?, ?)",
            )
            .bind::<Binary, _>(group_id.as_slice())
            .bind::<Integer, _>(EntityKind::CommitLogUpload as i32)
            .bind::<Integer, _>(EntityKind::CommitLogPublisherRecovery as i32)
            .execute(conn)
        })
        .unwrap();
    bo.context
        .db()
        .rollback_to_version("20261025000000")
        .unwrap();
    drop((b, bo));
    let bo = reopen_commit_log_client(&path).await;
    CommitLogWorker::new(bo.context.clone())
        .run_test(CommitLogTestFunction::PublishCommitLogsToRemote, None)
        .await
        .unwrap();
    let mut reader = CommitLogWorker::new(caro.context.clone());
    reader
        .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
        .await
        .unwrap();
    let entries = caro
        .context
        .db()
        .get_remote_commit_log_after_cursor(&group_id, 0, RemoteCommitLogOrder::AscendingByRowid)
        .unwrap();
    assert!(
        !entries
            .iter()
            .any(|entry| entry.commit_sequence_id == old_entry),
        "an unlogged transition must exclude the older role or membership history"
    );
    assert!(
        entries
            .iter()
            .any(|entry| entry.commit_sequence_id == connected_entry),
        "the provably connected suffix must still publish after an unlogged transition"
    );
}
