use xmtp_db::consent_record::ConsentState;

use super::*;
use crate::{groups::DMMetadataOptions, utils::Tester};

#[xmtp_common::test(unwrap_try = "true")]
async fn basic_sync() {
    let alix1 = Tester::new().await;
    let bo = Tester::new().await;

    // Talk with bo
    let (dm, dm_msg) = alix1.test_talk_in_dm_with(&bo).await?;

    // Create a second client for alix
    let alix2 = alix1.clone().await;

    // Have alix1 receive new sync group, and auto-send a sync payload
    alix1.sync_welcomes(&alix1.provider).await?;
    alix1.worker.wait(SyncMetric::PayloadSent, 1).await?;

    // Have alix2 receive payload and process it
    let alix2_sync_group = alix2.get_sync_group(&alix2.provider)?;
    alix2_sync_group.sync().await?;
    alix2.worker.wait(SyncMetric::PayloadProcessed, 1).await?;

    // Ensure the DM is present on the second device.
    let alix2_dm = alix2.group(&dm.group_id)?;
    let alix2_dm_msgs = alix2_dm.find_messages(&MsgQueryArgs::default())?;
    assert_eq!(alix2_dm_msgs.len(), 1);
    assert_eq!(alix2_dm_msgs[0].decrypted_message_bytes, dm_msg.as_bytes());
}

#[xmtp_common::test(unwrap_try = "true")]
#[cfg(not(target_arch = "wasm32"))]
async fn only_one_payload_sent() {
    use std::time::Duration;

    let alix1 = Tester::new().await;
    let alix2 = alix1.clone().await;
    let bo = Tester::new().await;

    let dm = alix1
        .find_or_create_dm_by_inbox_id(bo.inbox_id(), DMMetadataOptions::default())
        .await?;
    dm.send_message(b"Hello there.").await?;

    // Have alix2 fetch the DM
    alix2.sync_welcomes(&alix2.provider).await?;

    // Wait for alix to send a payload to alix2
    alix1.sync_welcomes(&alix1.provider).await?;
    alix1.worker.wait(SyncMetric::PayloadSent, 1).await?;
    alix1.worker.clear_metric(SyncMetric::PayloadSent);

    let alix3 = alix1.clone().await;

    // Have alix1 and 2 fetch the new sync group
    alix1.sync_welcomes(&alix1.provider).await?;
    alix2.sync_welcomes(&alix2.provider).await?;

    let alix1_sg = alix1.get_sync_group(&alix1.provider)?;
    let alix2_sg = alix2.get_sync_group(&alix2.provider)?;
    let alix3_sg = alix3.get_sync_group(&alix3.provider)?;

    // They should all have the same sync group
    assert_eq!(alix1_sg.group_id, alix2_sg.group_id);
    assert_eq!(alix1_sg.group_id, alix3_sg.group_id);

    // Wait for one of the workers to send a payload
    let wait1 = alix1.worker.wait(SyncMetric::PayloadSent, 1);
    let wait2 = alix2.worker.wait(SyncMetric::PayloadSent, 1);
    let timeout1 = xmtp_common::time::timeout(Duration::from_secs(5), wait1).await;
    let timeout2 = xmtp_common::time::timeout(Duration::from_secs(5), wait2).await;

    // We want one of them to timeout (only one payload sent)
    assert_ne!(timeout1.is_ok(), timeout2.is_ok());
}

#[xmtp_common::test(unwrap_try = "true")]
async fn test_double_sync_works_fine() {
    let alix1 = Tester::new().await;

    let bo = Tester::new().await;
    alix1.test_talk_in_dm_with(&bo).await?;

    let alix2 = alix1.clone().await;

    // Pull down the new sync group, triggering a payload to be sent
    alix1.sync_welcomes(&alix1.provider).await?;
    alix1.worker.wait(SyncMetric::PayloadSent, 1).await?;

    alix2.get_sync_group(&alix2.provider)?.sync().await?;
    alix2.worker.wait(SyncMetric::PayloadProcessed, 1).await?;

    alix2
        .send_sync_request(&alix2.provider, &Retry::default())
        .await?;
    alix1.get_sync_group(&alix1.provider)?.sync().await?;
    alix1.worker.wait(SyncMetric::PayloadSent, 2).await?;

    alix2.get_sync_group(&alix2.provider)?.sync().await?;
    alix2.worker.wait(SyncMetric::PayloadProcessed, 2).await?;

    // Alix2 should be able to talk fine with bo
    alix2.test_talk_in_dm_with(&bo).await?;
}

#[xmtp_common::test(unwrap_try = "true")]
async fn test_hmac_and_consent_prefrence_sync() {
    let alix1 = Tester::new().await;

    let bo = Tester::new().await;
    let (dm, _) = alix1.test_talk_in_dm_with(&bo).await?;

    let alix2 = alix1.clone().await;

    alix1.sync_welcomes(&alix1.provider).await?;
    alix1.worker.wait(SyncMetric::PayloadSent, 1).await?;

    alix2.get_sync_group(&alix2.provider)?.sync().await?;
    alix2.worker.wait(SyncMetric::PayloadProcessed, 1).await?;

    let alix1_keys = dm.hmac_keys(-1..=1)?;
    alix1.worker.wait(SyncMetric::HmacSent, 1).await?;

    alix2.get_sync_group(&alix2.provider)?.sync().await?;
    alix2.worker.wait(SyncMetric::HmacReceived, 1).await?;

    let alix2_dm = alix2.group(&dm.group_id)?;
    let alix2_keys = alix2_dm.hmac_keys(-1..=1)?;

    assert_eq!(alix1_keys[0].key, alix2_keys[0].key);
    assert_eq!(dm.consent_state()?, alix2_dm.consent_state()?);

    // Stream consent
    dm.update_consent_state(ConsentState::Denied)?;
    alix1.worker.wait(SyncMetric::ConsentSent, 2).await?;

    alix2.sync_device_sync(&alix2.provider).await?;
    alix2.worker.wait(SyncMetric::ConsentReceived, 1).await?;

    let alix2_dm = alix2.group(&dm.group_id)?;
    assert_eq!(alix2_dm.consent_state()?, ConsentState::Denied);
}
