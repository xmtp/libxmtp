use super::*;
use crate::groups::send_message_opts::SendMessageOpts;
use crate::tester;
use xmtp_db::{
    consent_record::ConsentState,
    group::{ConversationType, StoredGroup},
    group_message::MsgQueryArgs,
};

#[rstest::rstest]
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn basic_sync() {
    tester!(alix1, sync_server, sync_worker, stream);
    tester!(bo);
    // Talk with bo
    let (dm, dm_msg) = alix1.test_talk_in_dm_with(&bo).await?;
    // Create a second client for alix
    tester!(alix2, from: alix1);

    // Have alix2 receive payload and process it
    alix2
        .worker()
        .register_interest(SyncMetric::PayloadProcessed, 1)
        .wait()
        .await?;

    // Ensure the DM is present on the second device.
    let alix2_dm = alix2.group(&dm.group_id)?;
    let alix2_dm_msgs = alix2_dm.find_messages(&MsgQueryArgs::default())?;
    assert_eq!(alix2_dm_msgs.len(), 2);
    assert_eq!(alix2_dm_msgs[1].decrypted_message_bytes, dm_msg.as_bytes());
}

#[rstest::rstest]
#[xmtp_common::test(unwrap_try = true)]
#[cfg(not(target_arch = "wasm32"))]
async fn only_one_payload_sent() {
    use std::time::Duration;
    use tokio::time::sleep;

    use crate::utils::LocalTesterBuilder;

    tester!(alix1, sync_server, sync_worker, with_name: "alix1");
    let alix2 = alix1.builder.clone().with_name("alix2").build().await;
    let alix3 = alix1.builder.clone().with_name("alix3").build().await;

    // They should all have the same sync group
    alix1.test_has_same_sync_group_as(&alix3).await?;
    alix2.test_has_same_sync_group_as(&alix3).await?;

    // Register interest for next PayloadSent events
    let wait1 = alix1.worker().register_interest(SyncMetric::PayloadSent, 1);
    let wait2 = alix2.worker().register_interest(SyncMetric::PayloadSent, 1);

    // Wait for exactly one PayloadSent event using a race
    let result = tokio::select! {
        _r1 = wait1.wait() => "alix1",
        _r2 = wait2.wait() => "alix2",
        _ = sleep(Duration::from_secs(15)) => "timeout",
    };

    // Register interest for next PayloadSent events
    let wait1 = alix1.worker().register_interest(SyncMetric::PayloadSent, 1);
    let wait2 = alix2.worker().register_interest(SyncMetric::PayloadSent, 1);

    // ensure no other send activity happens
    let result2 = tokio::select! {
        _r1 = wait1.wait() => "alix1",
        _r2 = wait2.wait() => "alix2",
        _ = sleep(Duration::from_secs(3)) => "timeout",
    };

    assert_ne!(
        result, "timeout",
        "Expected one payload to be sent within timeout"
    );

    assert_eq!(result2, "timeout", "expected second send to timeout");

    // Check final counts - should be exactly 1 more total
    let alix1_count = alix1.worker().get(SyncMetric::PayloadSent);
    let alix2_count = alix2.worker().get(SyncMetric::PayloadSent);
    let total_new_payloads = alix1_count + alix2_count;

    // The core assertion: exactly 1 payload sent in response to our request
    assert_eq!(
        total_new_payloads, 1,
        "Expected exactly 1 payload to be sent in response to sync request, got {} (alix1: {}, alix2: {})",
        total_new_payloads, alix1_count, alix2_count
    );

    // Verify mutual exclusion: exactly one client should have sent
    let alix1_sent = alix1_count > 0;
    let alix2_sent = alix2_count > 0;
    assert_ne!(
        alix1_sent, alix2_sent,
        "Expected exactly one client to send payload, but alix1_sent={}, alix2_sent={} (winner was: {})",
        alix1_sent, alix2_sent, result
    );
}

#[rstest::rstest]
#[xmtp_common::test(unwrap_try = true)]
async fn test_double_sync_works_fine() {
    tester!(alix1, sync_worker, sync_server);
    tester!(bo);

    alix1.test_talk_in_dm_with(&bo).await?;

    tester!(alix2, from: alix1);

    // Pull down the new sync group, triggering a payload to be sent
    alix1.sync_welcomes().await?;
    alix1
        .worker()
        .register_interest(SyncMetric::PayloadSent, 1)
        .wait()
        .await?;

    alix2
        .context
        .device_sync_client()
        .get_sync_group()
        .await?
        .sync()
        .await?;
    alix2
        .worker()
        .register_interest(SyncMetric::PayloadProcessed, 1)
        .wait()
        .await?;

    alix2
        .context
        .device_sync_client()
        .send_sync_request()
        .await?;
    alix1
        .context
        .device_sync_client()
        .get_sync_group()
        .await?
        .sync()
        .await?;
    alix1
        .worker()
        .register_interest(SyncMetric::PayloadSent, 2)
        .wait()
        .await?;

    alix2
        .context
        .device_sync_client()
        .get_sync_group()
        .await?
        .sync()
        .await?;
    alix2
        .worker()
        .register_interest(SyncMetric::PayloadProcessed, 2)
        .wait()
        .await?;

    // Alix2 should be able to talk fine with bo
    alix2.test_talk_in_dm_with(&bo).await?;
}

#[rstest::rstest]
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn test_hmac_and_consent_preference_sync() {
    tester!(alix1, sync_worker, sync_server, stream);
    tester!(bo);

    let (dm, _) = alix1.test_talk_in_dm_with(&bo).await?;

    tester!(alix2, from: alix1);

    alix1.test_has_same_sync_group_as(&alix2).await?;

    alix2
        .worker()
        .register_interest(SyncMetric::PayloadProcessed, 1)
        .wait()
        .await?;

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
    dm.update_consent_state(ConsentState::Denied)?;
    alix2
        .worker()
        .register_interest(SyncMetric::ConsentReceived, 1)
        .wait()
        .await?;

    let alix2_dm = alix2.group(&dm.group_id)?;
    assert_eq!(alix2_dm.consent_state()?, ConsentState::Denied);

    // Now alix1 receives a group from bo, alix1 consents. Alix2 should see the group as consented as well.
    let bo_group = bo
        .create_group_with_inbox_ids(&[alix1.inbox_id()], None, None)
        .await?;
    alix1.sync_welcomes().await?;
    let alix1_group = alix1.group(&bo_group.group_id)?;
    assert_eq!(alix1_group.consent_state()?, ConsentState::Unknown);
    alix1_group.update_consent_state(ConsentState::Allowed)?;

    alix2
        .worker()
        .register_interest(SyncMetric::ConsentReceived, 2)
        .wait()
        .await?;
    let alix2_group = alix2.group(&bo_group.group_id)?;
    assert_eq!(alix2_group.consent_state()?, ConsentState::Allowed);
}

#[rstest::rstest]
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn test_only_added_to_correct_groups() {
    use diesel::prelude::*;
    use xmtp_db::schema::groups::dsl;

    tester!(alix1, stream, sync_worker);
    tester!(bo);

    let old_group = alix1
        .create_group_with_inbox_ids(&[bo.inbox_id()], None, None)
        .await?;
    old_group
        .send_message(b"hi there", SendMessageOpts::default())
        .await?;
    alix1.context.db().raw_query_write(|conn| {
        diesel::update(dsl::groups.find(&old_group.group_id))
            .set((dsl::last_message_ns.eq(0), dsl::created_at_ns.eq(0)))
            .execute(conn)
    })?;

    let bo_group_denied = bo
        .create_group_with_inbox_ids(&[alix1.inbox_id()], None, None)
        .await?;
    let bo_group_unknown = bo
        .create_group_with_inbox_ids(&[alix1.inbox_id()], None, None)
        .await?;
    let bo_dm = bo
        .find_or_create_dm_by_inbox_id(alix1.inbox_id(), None)
        .await?;

    alix1.sync_welcomes().await?;
    let alix_bo_group_denied = alix1.group(&bo_group_denied.group_id)?;
    let alix_bo_group_unknown = alix1.group(&bo_group_unknown.group_id)?;
    let alix_bo_dm = alix1.group(&bo_dm.group_id)?;

    alix_bo_dm.update_consent_state(ConsentState::Allowed)?;
    alix_bo_group_denied.update_consent_state(ConsentState::Denied)?;

    let new_group = alix1
        .create_group_with_inbox_ids(&[bo.inbox_id()], None, None)
        .await?;
    new_group
        .send_message(b"hi there", SendMessageOpts::default())
        .await?;

    tester!(alix2, from: alix1);

    alix1
        .worker()
        .register_interest(SyncMetric::SyncGroupWelcomesProcessed, 1)
        .wait()
        .await?;
    alix2.sync_welcomes().await?;

    // Added to new fresh group
    let alix2_new_group = alix2.group(&new_group.group_id);
    assert!(alix2_new_group.is_ok());

    // Not added to old stale group
    let alix2_old_group = alix2.group(&old_group.group_id);
    assert!(alix2_old_group.is_err());

    // Added to group with unknown consent state
    let alix2_bo_group_unknown = alix2.group(&alix_bo_group_unknown.group_id);
    assert!(alix2_bo_group_unknown.is_ok());

    // Added to consented DM
    let alix2_bo_dm = alix2.group(&alix_bo_dm.group_id);
    assert!(alix2_bo_dm.is_ok());

    // Not added to denied group from Bo
    let alix2_bo_group_denied = alix2.group(&alix_bo_group_denied.group_id);
    assert!(alix2_bo_group_denied.is_err());
}

#[rstest::rstest]
#[xmtp_common::test(unwrap_try = true)]
#[timeout(std::time::Duration::from_secs(15))]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn test_new_devices_not_added_to_old_sync_groups() {
    use diesel::prelude::*;
    use xmtp_db::schema::groups::dsl;

    tester!(alix1, sync_worker);
    tester!(alix2, from: alix1);

    alix1.test_has_same_sync_group_as(&alix2).await?;
    let groups = alix1.find_groups(GroupQueryArgs {
        include_sync_groups: true,
        ..Default::default()
    })?;
    for group in groups {
        group.maybe_update_installations(None).await?;
    }

    // alix1 should have it's own created sync group and alix2's sync group
    let alix1_sync_groups: Vec<StoredGroup> = alix1.context.db().raw_query_read(|conn| {
        dsl::groups
            .filter(dsl::conversation_type.eq(ConversationType::Sync))
            .load(conn)
    })?;
    assert_eq!(alix1_sync_groups.len(), 2);

    // alix2 should not be added to alix1's old sync group

    alix2.sync_welcomes().await?;
    let alix2_sync_groups: Vec<StoredGroup> = alix2.context.db().raw_query_read(|conn| {
        dsl::groups
            .filter(dsl::conversation_type.eq(ConversationType::Sync))
            .load(conn)
    })?;
    assert_eq!(alix2_sync_groups.len(), 1);
}
