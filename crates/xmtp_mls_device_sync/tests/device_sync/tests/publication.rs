//! Device sync tests: publication.

use super::*;

/// After a revocation the revoker cycles the root key. A removal-only refresh
/// drops the revoked leaf and adds no newcomer. The selected group lacks
/// alix4, so the next publish adds it, or a peer's commit already did; the
/// commit that adds it owes alix4 every record alix1 had already sent.
// verifies: SYNC-034, SYNC-035, SYNC-026
#[rstest::rstest]
#[case::local_adder(false)]
#[case::peer_adder(true)]
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn publish_adds_the_installation_the_selected_group_lacks(
    #[case] peer_adds: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    use diesel::prelude::*;
    use xmtp_db::schema::groups::dsl;
    use xmtp_db::sync_outbox::{clear_owed, owed_page};
    use xmtp_mls_validation::commit::extract_group_membership;

    tester!(alix1, disable_workers);
    tester!(alix2, from: alix1, disable_workers);
    tester!(alix3, from: alix1, disable_workers);
    let group = alix1.device_sync_client().get_sync_group().await?;
    alix2.sync_welcomes().await?;
    alix3.sync_welcomes().await?;
    tester!(alix4, from: alix1, disable_workers);
    alix4.device_sync_client().get_sync_group().await?;
    alix1.sync_welcomes().await?;
    // As when the older group's Welcome is installed after alix4's.
    alix1.context.db().raw_query(|conn| {
        diesel::update(dsl::groups.find(&group.group_id))
            .set(dsl::created_at_ns.eq(now_ns()))
            .execute(conn)
    })?;
    let selected = alix1.device_sync_client().primary_sync_group()?.unwrap();
    assert_eq!(selected.group_id, group.group_id);
    // Already sent, so not owed: records that alix3 lacks, and the root key.
    let records = [
        (ConsentKey::inbox("time-zero"), ConsentState::Allowed, 0),
        (ConsentKey::inbox("sent-before"), ConsentState::Denied, 5),
    ];
    for (key, state, at) in &records {
        decide_at(&alix1, key, *state, *at)?;
    }
    let db = alix1.context.db();
    clear_owed(&db, &owed_page(&db, PAGE_UPDATES, PAGE_BYTES)?)?;
    assert!(!has_owed(&db)?);
    revoke(&alix1, &alix2).await?;
    let cycled = StoredUserPreferences::load(&db)?;
    assert!(
        cycled.hmac_key_owed_seq > 0,
        "the revocation cycled an owed key"
    );

    let snapshot = || {
        group.with_group_snapshot(|mls_group| {
            let membership = extract_group_membership(mls_group.extensions())?;
            let leaves: Vec<_> = mls_group.members().map(|m| m.signature_key).collect();
            Ok::<_, GroupError>((leaves, membership.failed_installations))
        })
    };
    let alix2_key = alix2.context.installation_id().to_vec();
    let alix4_key = alix4.context.installation_id().to_vec();
    if peer_adds {
        // One commit swaps alix2's leaf for alix4's and adds no inbox.
        alix3
            .group(&group.group_id)?
            .add_missing_installations()
            .await?;
        group.sync().await?;
        let (leaves, _) = snapshot()?;
        assert_eq!(leaves.len(), 3);
        assert!(!leaves.contains(&alix2_key) && leaves.contains(&alix4_key));
    } else {
        group.remove_revoked_installations().await?;
        let (leaves, failed) = snapshot()?;
        assert!(!leaves.contains(&alix2_key));
        assert!(
            !leaves.contains(&alix4_key),
            "a removal-only refresh added a newer installation"
        );
        // A later joiner checks the skipped newcomer against this list.
        assert!(failed.contains(&alix4_key));
    }
    // A client opened later on the same database holds nothing in memory.
    let reopened = xmtp_mls::builder::ClientBuilder::from_client(alix1.client.clone())
        .with_disable_workers(true)
        .build()
        .await?;
    reopened.device_sync_client().publish_outbox().await?;
    assert!(snapshot()?.0.contains(&alix4_key));

    // alix3 accepts the commits and cycles on the revocation too; the two
    // converge on the greater key, which the revoked alix2 never reads.
    receive(&alix3).await?;
    publish(&alix3).await?;
    receive(&alix1).await?;
    let key = |alix: &xmtp_mls::utils::Tester| {
        StoredUserPreferences::load(alix.context.db())
            .unwrap()
            .hmac_key
    };
    assert_eq!(key(&alix1), key(&alix3));
    let _ = receive(&alix2).await;
    assert_ne!(key(&alix2), key(&alix1));
    receive(&alix4).await?;
    for (record, ..) in &records {
        assert_eq!(
            held(&alix4, record),
            held(&alix1, record),
            "{}",
            record.entity
        );
    }
    Ok(())
}

/// package the backend never serves gets no leaf and is listed as failed, and
/// nothing stays owed.
// verifies: SYNC-034
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn revocation_publish_lists_an_installation_without_a_package_as_failed() {
    use xmtp_mls::utils::test_mocks_helpers::{
        RestoreKeyPackages, set_test_mode_upload_malformed_keypackage,
    };

    tester!(alix1, disable_workers);
    tester!(alix2, from: alix1, disable_workers);
    let group = alix1.device_sync_client().get_sync_group().await?;
    alix2.sync_welcomes().await?;
    tester!(alix3, from: alix1, disable_workers);
    let alix3_key = alix3.context.installation_id().to_vec();
    let _restore = RestoreKeyPackages;
    set_test_mode_upload_malformed_keypackage(true, Some(vec![alix3_key.clone()]));
    revoke(&alix1, &alix2).await?;
    publish(&alix1).await?;

    let (leaves, failed) = group.with_group_snapshot(|mls_group| {
        Ok::<_, GroupError>((
            mls_group
                .members()
                .map(|member| member.signature_key)
                .collect::<Vec<_>>(),
            xmtp_mls_validation::commit::extract_group_membership(mls_group.extensions())?
                .failed_installations,
        ))
    })?;
    assert_eq!(leaves, vec![alix1.context.installation_id().to_vec()]);
    assert!(failed.contains(&alix3_key));
    assert!(!has_owed(&alix1.context.db())?);
}

/// When a publish arms the retry, a row that an older build stored for the
/// group with the three-day lifetime is kept, and becomes one that never
/// expires.
// verifies: SYNC-034
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn arming_a_retry_makes_an_older_expiring_row_durable() {
    use xmtp_db::tasks::QueryTasks;

    tester!(alix1, disable_workers);
    let sync = alix1.device_sync_client();
    let group = sync.get_sync_group().await?;
    let db = alix1.context.db();
    let [creation] = db.get_tasks()?.try_into().unwrap();
    db.delete_task(creation.id)?;
    schedule_add_missing_installations(&db, alix1.context.events(), group.group_id, None, false)?;
    let [old] = db.get_tasks()?.try_into().unwrap();
    assert_ne!(old.expires_at_ns, i64::MAX);
    sync.schedule_add_missing_installations_task(group.group_id, None)?;
    let [row] = db.get_tasks()?.try_into().unwrap();
    assert_eq!(row.id, old.id);
    assert_eq!((row.expires_at_ns, row.max_attempts), (i64::MAX, i32::MAX));
}

/// A dropped pool reached through an outbox publish stops the worker for a
/// reconnect. The publish fails in the sync-group read, so it arrives as
/// `Group(Db)`; `an_own_state_disconnect_keeps_the_reconnect_signal` reaches
/// `IdentityUpdate(Storage)` through the own-state read. The other wrappers in
/// the table have no route to a dropped pool that a test can reach today: an
/// own-state disconnect is returned before `OwnStateUnavailable` wraps it, and
/// the retry task that returns `TaskWorkerError::DeviceSync` records its
/// failure on the same dropped pool first and fails as `Storage`. They are
/// pinned by building the error (disclosed helper exception); each forwards
/// the disconnect.
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn device_sync_publish_failures_keep_reconnect_signal() {
    use xmtp_db::PlatformStorageError;
    use xmtp_mls::task_handlers::TaskWorkerError;
    use xmtp_mls_core::worker::NeedsDbReconnect;
    use xmtp_mls_engine::groups::GroupError;
    use xmtp_mls_identity::identity_updates::IdentityUpdateError;
    tester!(alix, persistent_db, disable_workers);
    alix.device_sync_client().get_sync_group().await?;
    decide_at(
        &alix,
        &ConsentKey::inbox("owed"),
        ConsentState::Denied,
        now_ns(),
    )?;
    alix.context.db().disconnect()?;
    let error = publish(&alix).await.unwrap_err();
    assert!(
        matches!(&error, DeviceSyncError::Group(GroupError::Db(_))),
        "{error:?}"
    );
    assert!(error.needs_db_reconnect());
    alix.context.db().reconnect()?;

    let storage = || StorageError::Platform(PlatformStorageError::PoolNeedsConnection);
    let own_state =
        || DeviceSyncError::OwnStateUnavailable(IdentityUpdateError::Storage(storage()));
    let observed = [
        (
            "IdentityUpdate(Storage)",
            DeviceSyncError::IdentityUpdate(IdentityUpdateError::Storage(storage()))
                .needs_db_reconnect(),
        ),
        (
            "OwnStateUnavailable(Storage)",
            own_state().needs_db_reconnect(),
        ),
        (
            "TaskWorkerError::DeviceSync(IdentityUpdate(Storage))",
            TaskWorkerError::DeviceSync(DeviceSyncError::IdentityUpdate(
                IdentityUpdateError::Storage(storage()),
            ))
            .needs_db_reconnect(),
        ),
        (
            "TaskWorkerError::DeviceSync(OwnStateUnavailable)",
            TaskWorkerError::DeviceSync(own_state()).needs_db_reconnect(),
        ),
    ];
    assert_eq!(
        observed,
        [
            ("IdentityUpdate(Storage)", true),
            ("OwnStateUnavailable(Storage)", true),
            ("TaskWorkerError::DeviceSync(IdentityUpdate(Storage))", true),
            ("TaskWorkerError::DeviceSync(OwnStateUnavailable)", true),
        ]
    );
}
