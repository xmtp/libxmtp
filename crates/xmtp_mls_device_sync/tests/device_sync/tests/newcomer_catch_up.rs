//! Device sync tests: newcomer catch up.

use super::*;

/// A peer marks everything owed in the transaction that installs a
/// newcomer's sync Welcome, and schedules the adds there, so a peer that
/// stops before its worker runs still sends it all when it runs again: a
/// time-0 record, an inbox record for a DM the newcomer holds no group for,
/// and the root key.
// verifies: SYNC-014, SYNC-035
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn newcomer_gets_everything_after_peer_restart() {
    use prost::Message;
    use xmtp_db::tasks::QueryTasks;
    use xmtp_proto::xmtp::mls::database::{Task as TaskProto, task::Task as TaskKind};

    tester!(alix1, disable_workers);
    tester!(bo, disable_workers);
    let group = alix1
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let records = [
        (ConsentKey::inbox("time-zero"), ConsentState::Allowed, 0),
        (ConsentKey::inbox(bo.inbox_id()), ConsentState::Denied, 5),
    ];
    for (key, state, at) in &records {
        decide_at(&alix1, key, *state, *at)?;
    }
    xmtp_db::user_preferences::ensure_root_key(&alix1.context.db())?;
    publish(&alix1).await?;
    assert!(!has_owed(&alix1.context.db())?);

    tester!(alix2, from: alix1, disable_workers);
    alix2.device_sync_client().get_sync_group().await?;
    alix1.sync_welcomes().await?;
    // The Welcome's transaction marked everything owed and scheduled the adds.
    assert!(has_owed(&alix1.context.db())?);
    let adds: Vec<_> = alix1
        .context
        .db()
        .get_tasks()?
        .iter()
        .filter_map(|t| match TaskProto::decode(t.data.as_slice()).ok()?.task {
            Some(TaskKind::AddMissingInstallations(add)) => Some(add.group_id),
            _ => None,
        })
        .collect();
    assert!(adds.contains(&group.group_id.to_vec()));

    // A client opened later on the same database holds nothing in memory.
    let reopened = xmtp_mls::builder::ClientBuilder::from_client(alix1.client.clone())
        .with_disable_workers(true)
        .build()
        .await?;
    reopened.device_sync_client().publish_outbox().await?;
    receive(&alix2).await?;
    for (key, ..) in &records {
        assert_eq!(held(&alix2, key), held(&alix1, key), "{}", key.entity);
    }
    let group_key = ConsentKey::group(group.group_id.as_ref());
    assert_eq!(held(&alix2, &group_key), held(&alix1, &group_key));
    // The newcomer may have made its own root key first; the greater one
    // becomes the account key once both have published.
    publish(&alix2).await?;
    receive(&alix1).await?;
    publish(&alix1).await?;
    receive(&alix2).await?;
    assert_eq!(
        StoredUserPreferences::load(alix2.context.db())?.hmac_key,
        StoredUserPreferences::load(alix1.context.db())?.hmac_key
    );
}

/// A peer that holds no root key makes one when it installs a newcomer's
/// sync Welcome, owes it, and reports it.
// verifies: SYNC-035, SYNC-026, EVENT-001
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn sync_welcome_makes_a_root_key_when_none_is_held() {
    use xmtp_events::{ClientEvent, EventFilter, EventKind};
    tester!(alix1, disable_workers);
    assert!(
        StoredUserPreferences::load(alix1.context.db())?
            .hmac_key
            .is_none()
    );
    let events = alix1
        .context
        .events()
        .subscribe_app(EventFilter::new([EventKind::HmacKeysUpdated]))?;
    tester!(alix2, from: alix1, disable_workers);
    alix2.device_sync_client().get_sync_group().await?;
    alix1.sync_welcomes().await?;
    let preferences = StoredUserPreferences::load(alix1.context.db())?;
    assert!(preferences.hmac_key.is_some());
    assert!(preferences.hmac_key_owed_seq > 0);
    assert!(
        events
            .drain()
            .iter()
            .any(|event| matches!(event.client, Some(ClientEvent::HmacKeysUpdated(_))))
    );
}

/// A root key made in a write that rolls back is not reported: its events
/// go out only when the write that stores it commits.
// verifies: EVENT-001, SYNC-026
#[xmtp_common::test(unwrap_try = true)]
async fn root_key_made_in_a_rolled_back_write_is_not_reported() {
    use xmtp_db::TransactionOutcome;
    use xmtp_events::{EventFilter, EventKind};
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    let db = alix.context.db();
    assert!(StoredUserPreferences::load(&db)?.hmac_key.is_none());
    let events = alix
        .context
        .events()
        .subscribe_app(EventFilter::new([EventKind::HmacKeysUpdated]))?;
    xmtp_mls_core::state_tx::state_write_with_events(
        alix.context.mls_storage(),
        alix.context.events(),
        |tx, events| {
            group.hmac_keys_in(tx.storage().db(), events, 0..=0)?;
            Ok::<_, StorageError>(TransactionOutcome::<()>::Rollback)
        },
    )?;
    assert!(StoredUserPreferences::load(&db)?.hmac_key.is_none());
    assert!(events.drain().is_empty());
    // The same write, committed, reports the key once.
    group.hmac_keys(0..=0)?;
    assert!(StoredUserPreferences::load(&db)?.hmac_key.is_some());
    assert_eq!(events.drain().len(), 1);
}

/// A value learned from an older sync group is passed on to the selected
/// one, so it reaches a newcomer whose replay was already sent.
// verifies: SYNC-027
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn older_group_value_reaches_newcomer() {
    tester!(alix1, disable_workers);
    tester!(alix2, from: alix1, disable_workers);
    join_sync_group(&alix2, &[&alix1]).await?;
    // alix2 blocks X in the older group and never runs again.
    let x = ConsentKey::inbox("x");
    decide_at(&alix2, &x, ConsentState::Denied, now_ns())?;
    publish(&alix2).await?;

    tester!(alix3, from: alix1, disable_workers);
    let newer = alix3.device_sync_client().get_sync_group().await?;
    alix1.sync_welcomes().await?;
    let selected = alix1.device_sync_client().primary_sync_group()?.unwrap();
    assert_eq!(selected.group_id, newer.group_id);
    publish(&alix1).await?;
    receive(&alix3).await?;
    assert!(held(&alix3, &x).is_none());

    // alix1 reads the older group and passes the value on.
    receive(&alix1).await?;
    assert!(alix1.context.db().consent_row(&x)?.unwrap().owed_seq > 0);
    publish(&alix1).await?;
    receive(&alix3).await?;
    assert_eq!(held(&alix3, &x), held(&alix2, &x));
}

/// After the clock is set back, a newcomer's sync group is still stored
/// after every held group and selected, so the replay reaches it.
// verifies: SYNC-031, SYNC-002
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn clock_set_back_still_selects_newcomer_group() {
    use diesel::prelude::*;
    use xmtp_db::schema::groups::dsl;
    tester!(alix1, disable_workers);
    let older = alix1.device_sync_client().get_sync_group().await?;
    let ahead = now_ns() + 2 * NS_IN_HOUR;
    alix1.context.db().raw_query(|conn| {
        diesel::update(dsl::groups.find(&older.group_id))
            .set(dsl::created_at_ns.eq(ahead))
            .execute(conn)
    })?;
    tester!(alix2, from: alix1, disable_workers);
    let newer = alix2.device_sync_client().get_sync_group().await?;
    alix1.sync_welcomes().await?;
    let stored = alix1.context.db().find_group(&newer.group_id)?.unwrap();
    assert!(stored.created_at_ns > ahead);
    let selected = alix1.device_sync_client().primary_sync_group()?.unwrap();
    assert_eq!(selected.group_id, newer.group_id);
    let key = ConsentKey::inbox("set-back");
    decide_at(&alix1, &key, ConsentState::Allowed, now_ns())?;
    publish(&alix1).await?;
    receive(&alix2).await?;
    assert_eq!(held(&alix2, &key), held(&alix1, &key));
}

/// A page whose read-back is not published, because its confirmation failed
/// or the deployment refused it, clears no mark: the pass fails, and a later
/// pass sends the values again and only then clears them.
// verifies: SYNC-033
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn unpublished_page_stays_owed_until_sent_again() {
    tester!(alix1, disable_workers);
    tester!(alix2, from: alix1, disable_workers);
    join_sync_group(&alix2, &[&alix1]).await?;
    let x = ConsentKey::inbox("unconfirmed");
    decide_at(&alix1, &x, ConsentState::Denied, now_ns())?;
    let owed = || {
        alix1
            .context
            .db()
            .consent_row(&x)
            .unwrap()
            .unwrap()
            .owed_seq
    };
    *UNCONFIRMED_NEXT_PAGE.lock() = Some(alix1.context.installation_id().to_vec());
    assert!(matches!(
        publish(&alix1).await,
        Err(DeviceSyncError::OutboxPageUnpublished)
    ));
    assert!(owed() > 0, "an unconfirmed page clears nothing");
    publish(&alix1).await?;
    assert_eq!(owed(), 0);
    receive(&alix2).await?;
    assert_eq!(held(&alix2, &x), held(&alix1, &x));
}

/// The revocation's root key is timed a day after the revocation's server
/// time, so a key the revoked installation sent before it, timed far ahead,
/// never replaces it, in either processing order.
// verifies: SYNC-026, SYNC-030, SYNC-032
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn revocation_key_beats_delayed_pre_revocation_key() {
    use xmtp_db::user_preferences::{RootKeyWrite, write_root_key};
    use xmtp_mls_identity::identity_updates::load_identity_updates_for_client;
    // alix1 cycles, then applies; alix2 is revoked; alix3 revokes; alix4
    // applies, then cycles.
    tester!(alix1, disable_workers);
    tester!(alix2, from: alix1, disable_workers);
    tester!(alix3, from: alix1, disable_workers);
    tester!(alix4, from: alix1, disable_workers);
    alix1.device_sync_client().get_sync_group().await?;
    for client in [&alix2, &alix3, &alix4] {
        client.sync_welcomes().await?;
    }
    let k = [0x4c; 42];
    let ahead = RootKeyWrite::Received {
        key: &k,
        cycled_at_ns: now_ns() + 2 * NS_IN_DAY,
        bound_ns: i64::MAX,
    };
    write_root_key(&alix2.context.db(), ahead, Owed::Mark)?;
    publish(&alix2).await?;
    receive(&alix4).await?;
    let key_of = |client: &xmtp_mls::utils::Tester| {
        StoredUserPreferences::load(client.context.db()).unwrap()
    };
    assert_eq!(key_of(&alix4).hmac_key, Some(k.to_vec().into()));

    revoke(&alix3, &alix2).await?;
    let revoked_at = alix3
        .context
        .db()
        .get_identity_updates(alix3.inbox_id(), None, None)?
        .last()
        .unwrap()
        .server_timestamp_ns;
    let db = alix1.context.db();
    load_identity_updates_for_client(&alix1.context, &db, &[alix1.inbox_id()]).await?;
    let cycled = key_of(&alix1);
    assert!(cycled.hmac_key_cycled_at_ns.unwrap() >= revoked_at + MAX_AHEAD_NS);
    receive(&alix1).await?;
    assert_eq!(key_of(&alix1).hmac_key, cycled.hmac_key);
    assert_ne!(cycled.hmac_key, Some(k.to_vec().into()));

    // alix4 applied C's key first, then loads the revocation and cycles past it.
    let db = alix4.context.db();
    load_identity_updates_for_client(&alix4.context, &db, &[alix4.inbox_id()]).await?;
    let after = key_of(&alix4);
    assert_ne!(after.hmac_key, Some(k.to_vec().into()));
    assert!(after.hmac_key_cycled_at_ns.unwrap() >= revoked_at + MAX_AHEAD_NS);
    // The revoked installation does not cycle on its own revocation.
    let db = alix2.context.db();
    load_identity_updates_for_client(&alix2.context, &db, &[alix2.inbox_id()]).await?;
    assert_eq!(key_of(&alix2).hmac_key, Some(k.to_vec().into()));
}
