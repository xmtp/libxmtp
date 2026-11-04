//! Device sync tests: consent and preferences.

use super::*;

/// A thousand decisions are each published once, and the receiver, which got
/// them from the selected sync group, does not send them back.
// verifies: EVENT-024, SYNC-020, SYNC-027
#[rstest::rstest]
#[timeout(std::time::Duration::from_secs(240))]
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn thousand_local_consent_changes_publish_once_without_echo() {
    use std::time::Duration;
    async fn until(done: impl AsyncFn() -> bool) {
        xmtp_common::time::timeout(Duration::from_secs(200), async {
            while !done().await {
                xmtp_common::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .expect("condition holds in time");
    }
    tester!(alix1, sync_worker);
    tester!(alix2, from: alix1);
    alix1.test_has_same_sync_group_as(&alix2).await?;
    let drained =
        async || !has_owed(&alix1.context.db()).unwrap() && !has_owed(&alix2.context.db()).unwrap();
    until(drained).await;
    alix1.worker().clear_metric(SyncMetric::ConsentSent);
    alix2.worker().clear_metric(SyncMetric::ConsentSent);

    let keys: Vec<_> = (0..1_000)
        .map(|i| ConsentKey::inbox(&format!("consent-burst-{i}")))
        .collect();
    for key in &keys {
        let act = ConsentAct::new(
            ConsentType::InboxId,
            ConsentState::Allowed,
            key.entity.clone(),
        );
        alix1.set_consent_states(&[act]).await?;
    }
    until(async || keys.iter().all(|key| held(&alix2, key).is_some())).await;
    until(drained).await;
    assert_eq!(alix1.worker().get(SyncMetric::ConsentSent), 1_000);
    assert_eq!(
        alix2.worker().get(SyncMetric::ConsentSent),
        0,
        "the receiver echoed"
    );
    for key in &keys {
        assert_eq!(held(&alix2, key), held(&alix1, key));
    }
}

/// Changes are owed in the database: fifty acts on one key are one record,
/// failed passes never drop it, and a client opened later on the same
/// database publishes it.
// verifies: SYNC-027, SYNC-029, SYNC-033
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn outbox_is_durable_and_coalesced() {
    tester!(alix1, disable_workers);
    tester!(alix2, from: alix1, disable_workers);
    let group_id = join_sync_group(&alix2, &[&alix1]).await?;
    let key = ConsentKey::inbox("durable");
    // The own identity state is unreadable, so every publish waits.
    let restore = hide_own_identity_updates(alix1.context.db(), alix1.inbox_id())?;
    for state in [ConsentState::Denied, ConsentState::Allowed]
        .into_iter()
        .cycle()
        .take(50)
    {
        let act = ConsentAct::new(ConsentType::InboxId, state, key.entity.clone());
        alix1.set_consent_states(&[act]).await?;
    }
    for _ in 0..25 {
        assert!(publish(&alix1).await.is_err());
    }
    assert!(alix1.context.db().consent_row(&key)?.unwrap().owed_seq > 0);
    restore()?;
    // A client opened later on the same database holds no state in memory.
    let reopened = xmtp_mls::builder::ClientBuilder::from_client(alix1.client.clone())
        .with_disable_workers(true)
        .build()
        .await?;
    reopened.device_sync_client().publish_outbox().await?;
    receive(&alix2).await?;
    assert_eq!(held(&alix2, &key), held(&alix1, &key));
    assert_eq!(held(&alix2, &key).unwrap().state, ConsentState::Allowed);
    let carried = alix2
        .context
        .db()
        .sync_group_messages_paged(0, 100)?
        .iter()
        .filter(|message| message.group_id == group_id)
        .filter_map(|message| decode_sync_message(&message.decrypted_message_bytes))
        .filter(|content| {
            matches!(&content.0, ContentProto::PreferenceUpdates(p) if p.updates.iter().any(|u|
                matches!(&u.update, Some(PreferenceUpdateProto::Consent(c)) if c.entity == "durable")))
        })
        .count();
    assert_eq!(carried, 1, "fifty acts on one key are one published record");
}

// verifies: SYNC-023
#[xmtp_common::test(unwrap_try = true)]
fn unknown_device_sync_content_is_ignored() {
    // Field 1 was DeviceSyncContent.request. It is reserved after history
    // transfer removal, so a message from an older installation is ignored.
    assert!(decode_supported_content(&[0x0a, 0x00]).is_none());
}

// verifies: SYNC-027, SYNC-029, SYNC-033
#[rstest::rstest]
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn test_hmac_and_consent_preference_sync() {
    tester!(alix1, sync_worker);
    tester!(bo);

    let (dm, _) = alix1.test_talk_in_dm_with(&bo).await?;

    tester!(alix2, from: alix1);
    alix1.test_has_same_sync_group_as(&alix2).await?;

    // The TaskRunner adds alix2 to alix1's existing DM. Poll while alix2
    // syncs welcomes because both the membership commit and welcome delivery
    // are asynchronous.
    xmtp_common::wait_for_some(|| async {
        let _ = alix2.sync_welcomes().await;
        alix2.group(&dm.group_id).ok().map(|_| ())
    })
    .await
    .expect("alix2 must receive the DM via the TaskRunner membership add");

    alix1
        .worker()
        .register_interest(SyncMetric::HmacSent, 1)
        .wait()
        .await?;
    alix1
        .worker()
        .register_interest(SyncMetric::HmacReceived, 1)
        .wait()
        .await?;
    let alix1_keys = dm.hmac_keys(-1..=1)?;

    alix2
        .worker()
        .register_interest(SyncMetric::HmacReceived, 1)
        .wait()
        .await?;

    let alix2_dm = alix2.group(&dm.group_id)?;
    let alix2_keys = alix2_dm.hmac_keys(-1..=1)?;

    assert_eq!(alix1_keys[0].key, alix2_keys[0].key);
    assert_eq!(dm.consent_state()?, alix2_dm.consent_state()?);

    // Stream consent
    alix1.worker().clear_metric(SyncMetric::ConsentSent);
    dm.update_consent_state(ConsentState::Denied)?;
    alix1
        .worker()
        .register_interest(SyncMetric::ConsentSent, 1)
        .wait()
        .await?;

    // Wait for the state, not for a ConsentReceived count: the count is
    // cumulative, and alix2 may already have received alix1's earlier
    // Allowed update for the DM, which would end the wait before Denied
    // arrives.
    xmtp_common::wait_for_eq(
        || async { alix2.group(&dm.group_id).unwrap().consent_state().unwrap() },
        ConsentState::Denied,
    )
    .await?;

    // Now alix1 receives a group from bo, alix1 consents. Alix2 should see the group as consented as well.
    let bo_group = bo
        .create_group_with_members(&[alix1.inbox_id()], None, None)
        .await?;
    alix1.sync_welcomes().await?;
    let alix1_group = alix1.group(&bo_group.group_id)?;
    assert_eq!(alix1_group.consent_state()?, ConsentState::Unknown);

    // Wait for publication before syncing the new group. The device-sync
    // worker keeps receiving consent without an app message stream.
    alix1.worker().clear_metric(SyncMetric::ConsentSent);
    alix1_group.update_consent_state(ConsentState::Allowed)?;
    alix1
        .worker()
        .register_interest(SyncMetric::ConsentSent, 1)
        .wait()
        .await?;

    alix2.sync_all_welcomes_and_groups(None).await?;

    let alix2_group = alix2.group(&bo_group.group_id)?;
    xmtp_common::wait_for_eq(
        || async { alix2_group.consent_state().unwrap() },
        ConsentState::Allowed,
    )
    .await?;
}

#[xmtp_common::timeout(std::time::Duration::from_secs(60))]
#[rstest::rstest]
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn test_incremental_consent() {
    tester!(alix1, sync_worker);
    tester!(alix2, from: alix1);

    tester!(bo);
    let (dm, _) = bo.test_talk_in_dm_with(&alix1).await?;
    alix1
        .worker()
        .register_interest(SyncMetric::ConsentSent, 1)
        .wait()
        .await?;

    alix2.sync_all_welcomes_and_groups(None).await?;

    alix2
        .worker()
        .register_interest(SyncMetric::ConsentReceived, 1)
        .wait()
        .await?;

    let dm2 = alix2.group(&dm.group_id)?;
    assert_eq!(dm2.consent_state()?, ConsentState::Allowed);
}

/// Fails the own-state read like an unreachable verifier until the returned fn runs.
pub(super) fn hide_own_identity_updates<'a>(
    db: impl ConnectionExt + 'a,
    inbox_id: &'a str,
) -> Result<impl FnOnce() -> Result<usize, xmtp_db::ConnectionError> + 'a, xmtp_db::ConnectionError>
{
    use diesel::RunQueryDsl;
    let set = move |from: &str, to: &str| {
        let sql =
            format!("UPDATE identity_updates SET inbox_id = '{to}' WHERE inbox_id = '{from}'");
        db.raw_query(|conn| diesel::sql_query(sql).execute(conn))
    };
    set(inbox_id, &format!("{inbox_id}-hidden"))?;
    Ok(move || set(&format!("{inbox_id}-hidden"), inbox_id))
}

// A root HMAC key that another installation syncs is a `hmac_keys.updated`
// trigger (EVENT section 1). One message with two keys is one change.
// verifies: EVENT-001
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn synced_hmac_key_emits_hmac_keys_updated_once() {
    use diesel::prelude::*;
    use xmtp_db::schema::groups::dsl;
    use xmtp_events::{ClientEvent, EventEnvelope, EventFilter, EventKind};

    tester!(alix1, disable_workers);
    tester!(alix2, from: alix1);
    tester!(bo);
    let group = bo
        .create_group_with_members(&[alix1.inbox_id()], None, None)
        .await?;
    alix1.sync_welcomes().await?;
    alix2.sync_welcomes().await?;
    // Stands in for the shared sync group, which needs the sync worker.
    alix1.context.db().raw_query(|conn| {
        diesel::update(dsl::groups.find(&group.group_id))
            .set(dsl::conversation_type.eq(ConversationType::Sync))
            .execute(conn)
    })?;

    let now = now_ns();
    let updates = sync_bytes(vec![
        root_key_update(vec![1; 42], now),
        root_key_update(vec![2; 42], now + 1),
    ]);
    alix2
        .group(&group.group_id)?
        .send_message(&updates, SendMessageOpts::default())
        .await?;
    alix1.group(&group.group_id)?.sync().await?;

    let events = alix1
        .context
        .events()
        .subscribe_app(EventFilter::new([EventKind::HmacKeysUpdated]))?;
    let db = alix1.context.db();
    alix1
        .device_sync_client()
        .process_sync_group_messages()
        .await?;

    assert_eq!(
        StoredUserPreferences::load(&db)?.hmac_key,
        Some(vec![2; 42].into())
    );
    assert!(matches!(
        events.drain().as_slice(),
        [EventEnvelope {
            client: Some(ClientEvent::HmacKeysUpdated(_)),
            ..
        }]
    ));
}
