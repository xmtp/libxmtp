//! The author migration must keep an existing handover able to publish.
#![cfg(not(target_arch = "wasm32"))]

use std::sync::Arc;

use diesel::RunQueryDsl;
use diesel::sql_types::{Binary, Bool, Integer};
use xmtp_db::consent_record::ConsentState;
use xmtp_db::prelude::*;
use xmtp_db::refresh_state::EntityKind;
use xmtp_db::remote_commit_log::RemoteCommitLogOrder;
use xmtp_db::{ConnectionExt, StorageOption, TestDb, XmtpTestDb};

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
    let store = TestDb::create_persistent_store(Some(database.into())).await;
    let api = Arc::new(DefaultTestClientCreator::create().build().unwrap());
    Client::builder(IdentityStrategy::CachedOnly)
        .store(store)
        .api_client(api)
        .default_mls_store()
        .unwrap()
        .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
        .with_disable_workers(true)
        .with_commit_log_worker(false)
        .with_device_sync_worker_mode(Some(DeviceSyncMode::Disabled))
        .build()
        .await
        .unwrap()
}

// verifies: FORK-081
#[xmtp_common::test(unwrap_try = true)]
async fn an_ordinary_commit_repairs_preupgrade_publisher_flags() {
    tester!(alix, persistent_db, disable_workers);
    tester!(bo, persistent_db, disable_workers);
    tester!(caro, persistent_db, disable_workers);
    let a = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await?;
    bo.sync_welcomes().await?;
    caro.sync_welcomes().await?;
    let b = bo.group(&a.group_id)?;
    let c = caro.group(&a.group_id)?;
    b.update_consent_state(ConsentState::Allowed)?;
    c.update_consent_state(ConsentState::Allowed)?;
    a.update_admin_list(AddSuper, bo.inbox_id().to_string())
        .await?;
    b.sync().await?;
    b.update_admin_list(RemoveSuper, alix.inbox_id().to_string())
        .await?;
    a.sync().await?;
    c.sync().await?;
    let group_id = a.group_id;

    // The prior build kept the creator's flag and never enabled the promoted
    // member. Write that input directly, without the new setter's cutoff.
    for (client, old_flag) in [(&alix, true), (&bo, false)] {
        client.context.db().raw_query(|conn| {
            diesel::sql_query("UPDATE groups SET should_publish_commit_log = ? WHERE id = ?")
                .bind::<Bool, _>(old_flag)
                .bind::<Binary, _>(group_id.as_slice())
                .execute(conn)?;
            diesel::sql_query("DELETE FROM refresh_state WHERE entity_id = ? AND entity_kind = ?")
                .bind::<Binary, _>(group_id.as_slice())
                .bind::<Integer, _>(EntityKind::CommitLogUpload as i32)
                .execute(conn)
        })?;
    }
    // Run the real down migration and remove its tracking row. Reopening each
    // file runs pending migrations before opening its cached identity.
    for client in [&alix, &bo, &caro] {
        // The prior build had no publisher-boundary records.
        client.context.db().raw_query(|conn| {
            diesel::sql_query("DELETE FROM refresh_state WHERE entity_kind = ?")
                .bind::<Integer, _>(EntityKind::CommitLogPublisherBoundary as i32)
                .execute(conn)
        })?;
        client.context.db().rollback_to_version("20261025000000")?;
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
            .find_group(&group_id)??
            .should_publish_commit_log
    );
    assert!(
        !bo.context
            .db()
            .find_group(&group_id)??
            .should_publish_commit_log
    );
    let a = alix.group(&group_id)?;
    let b = bo.group(&group_id)?;
    let c = caro.group(&group_id)?;
    b.update_group_name("ordinary commit after upgrade".into())
        .await?;
    a.sync().await?;
    c.sync().await?;
    let latest_commit = b.local_commit_log().await?.last()?.commit_sequence_id;

    for client in [&alix, &bo] {
        CommitLogWorker::new(client.context.clone())
            .run_test(CommitLogTestFunction::PublishCommitLogsToRemote, None)
            .await?;
    }
    let mut reader = CommitLogWorker::new(caro.context.clone());
    reader
        .run_test(CommitLogTestFunction::SaveRemoteCommitLog, None)
        .await?;
    reader
        .run_test(CommitLogTestFunction::CheckForkedState, None)
        .await?;
    let accepted = caro.context.db().get_remote_commit_log_after_cursor(
        &group_id,
        0,
        RemoteCommitLogOrder::AscendingByRowid,
    )?;
    assert!(
        accepted.iter().any(|entry| {
            entry.author_installation_key == bo.context.installation_id().to_vec()
                && entry.commit_sequence_id == latest_commit
        }),
        "the current publisher must publish the ordinary commit after upgrade"
    );
    assert_eq!(
        caro.context
            .db()
            .get_group_commit_log_forked_status(&group_id)?,
        Some(false)
    );
}
