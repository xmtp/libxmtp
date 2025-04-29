use super::*;
use crate::{
    groups::DMMetadataOptions,
    tester,
    utils::{LocalTesterBuilder, Tester},
};
use xmtp_db::{
    consent_record::ConsentState,
    group::{ConversationType, StoredGroup},
    group_message::MsgQueryArgs,
};

#[xmtp_common::test(unwrap_try = "true")]
async fn basic_sync() {
    tester!(alix1, sync_server, sync_worker, stream);
    tester!(bo);

    // Talk with bo
    let (dm, dm_msg) = alix1.test_talk_in_dm_with(&bo).await?;

    // Create a second client for alix
    let alix2 = alix1.builder.build().await;

    // Have alix1 receive new sync group, and auto-send a sync payload
    alix1.sync_welcomes(&alix1.provider).await?;

    // Have alix2 receive payload and process it
    alix2.worker().wait(SyncMetric::PayloadProcessed, 1).await?;

    // Ensure the DM is present on the second device.
    let alix2_dm = alix2.group(&dm.group_id)?;
    let alix2_dm_msgs = alix2_dm.find_messages(&MsgQueryArgs::default())?;
    assert_eq!(alix2_dm_msgs.len(), 1);
    assert_eq!(alix2_dm_msgs[0].decrypted_message_bytes, dm_msg.as_bytes());
}

#[xmtp_common::test(unwrap_try = "true")]
#[cfg(not(target_arch = "wasm32"))]
async fn only_one_payload_sent() {
    use crate::tester;
    use std::time::Duration;

    tester!(alix1, sync_worker, sync_server, stream);
    tester!(alix2, from = alix1);

    alix1.test_has_same_sync_group_as(&alix2).await?;
    tester!(bo);

    let dm = alix1
        .find_or_create_dm_by_inbox_id(bo.inbox_id(), DMMetadataOptions::default())
        .await?;
    dm.send_message(b"Hello there.").await?;

    // Have alix2 fetch the DM
    alix2.sync_welcomes(&alix2.provider).await?;

    // Wait for alix to send a payload to alix2
    alix1.sync_welcomes(&alix1.provider).await?;
    alix1.worker().wait(SyncMetric::PayloadSent, 1).await?;
    alix1.worker().clear_metric(SyncMetric::PayloadSent);

    tester!(alix3, from = alix1);
    alix1.worker().reset_metrics();
    alix2.worker().reset_metrics();

    // They should all have the same sync group
    alix1.test_has_same_sync_group_as(&alix3).await?;
    alix2.test_has_same_sync_group_as(&alix3).await?;

    let wait1 = alix1.worker().wait(SyncMetric::PayloadSent, 1);
    let timeout1 = xmtp_common::time::timeout(Duration::from_secs(3), wait1).await;
    let wait2 = alix2.worker().wait(SyncMetric::PayloadSent, 1);
    let timeout2 = xmtp_common::time::timeout(Duration::from_secs(3), wait2).await;

    // We want one of them to timeout (only one payload sent)
    assert_ne!(timeout1.is_ok(), timeout2.is_ok());
}

#[xmtp_common::test(unwrap_try = "true")]
async fn test_double_sync_works_fine() {
    tester!(alix1, sync_worker, sync_server, stream);
    tester!(alix2, from = alix1);
    tester!(bo);

    alix1.test_talk_in_dm_with(&bo).await?;

    alix1.test_has_same_sync_group_as(&alix2).await?;
    alix2.worker().wait(SyncMetric::PayloadProcessed, 1).await?;
    alix2.send_sync_request(&alix2.provider).await?;
    alix2.worker().wait(SyncMetric::PayloadProcessed, 2).await?;

    // Alix2 should be able to talk fine with bo
    alix2.test_talk_in_dm_with(&bo).await?;
}

#[xmtp_common::test(unwrap_try = "true")]
async fn test_hmac_and_consent_prefrence_sync() {
    tester!(alix1, sync_worker, sync_server, stream);
    tester!(bo);

    let (dm, _) = alix1.test_talk_in_dm_with(&bo).await?;

    tester!(alix2, from = alix1);

    alix1.test_has_same_sync_group_as(&alix2).await?;

    alix2.worker().wait(SyncMetric::PayloadProcessed, 1).await?;

    let alix1_keys = dm.hmac_keys(-1..=1)?;
    alix1.worker().wait(SyncMetric::HmacSent, 1).await?;

    alix2.worker().wait(SyncMetric::HmacReceived, 1).await?;

    let alix2_dm = alix2.group(&dm.group_id)?;
    let alix2_keys = alix2_dm.hmac_keys(-1..=1)?;

    assert_eq!(alix1_keys[0].key, alix2_keys[0].key);
    assert_eq!(dm.consent_state()?, alix2_dm.consent_state()?);

    // Stream consent
    dm.update_consent_state(ConsentState::Denied)?;

    alix2.worker().wait(SyncMetric::ConsentReceived, 1).await?;

    let alix2_dm = alix2.group(&dm.group_id)?;
    assert_eq!(alix2_dm.consent_state()?, ConsentState::Denied);
}

#[xmtp_common::test(unwrap_try = "true")]
async fn test_new_devices_not_added_to_old_sync_groups() {
    use diesel::prelude::*;
    use xmtp_db::schema::groups::dsl;

    tester!(alix1, sync_worker);
    tester!(alix2, from = alix1);

    alix1.test_has_same_sync_group_as(&alix2).await?;

    // alix1 should have it's own created sync group and alix2's sync group
    let alix1_sync_groups: Vec<StoredGroup> = alix1.provider.conn_ref().raw_query_read(|conn| {
        dsl::groups
            .filter(dsl::conversation_type.eq(ConversationType::Sync))
            .load(conn)
    })?;
    assert_eq!(alix1_sync_groups.len(), 2);

    // alix2 should not be added to alix1's old sync group
    let alix2_sync_groups: Vec<StoredGroup> = alix2.provider.conn_ref().raw_query_read(|conn| {
        dsl::groups
            .filter(dsl::conversation_type.eq(ConversationType::Sync))
            .load(conn)
    })?;
    assert_eq!(alix2_sync_groups.len(), 1);
}
