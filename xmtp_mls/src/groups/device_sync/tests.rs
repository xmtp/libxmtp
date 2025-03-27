use super::*;
use crate::{groups::DMMetadataOptions, utils::Tester};
use anyhow::Result;

#[xmtp_common::test]
async fn basic_sync() -> Result<()> {
    let alix1 = Tester::new().await;
    let bo = Tester::new().await;

    // Talk with bo
    let (dm, dm_msg) = alix1.test_talk_in_dm_with(&bo).await?;

    // Create a second client for alix
    let alix2 = Tester::new_from_wallet(alix1.wallet.clone()).await;
    alix2.worker.wait_for_init().await?;

    // Have alix1 receive new sync group, and auto-send a sync payload
    alix1.sync_welcomes(&alix1.provider).await?;
    alix1.worker.wait(SyncMetric::PayloadsSent, 1).await?;

    // Have alix2 receive payload and process it
    let alix2_sync_group = alix2.get_sync_group(&alix2.provider)?;
    alix2_sync_group.sync().await?;
    alix2.worker.wait(SyncMetric::PayloadsProcessed, 1).await?;

    // Ensure the DM is present on the second device.
    let alix2_dm = alix2.group(&dm.group_id)?;
    let alix2_dm_msgs = alix2_dm.find_messages(&MsgQueryArgs::default())?;
    assert_eq!(alix2_dm_msgs.len(), 1);
    assert_eq!(alix2_dm_msgs[0].decrypted_message_bytes, dm_msg.as_bytes());

    Ok(())
}

#[xmtp_common::test]
async fn only_one_payload_sent() -> Result<()> {
    let alix1 = Tester::new().await;
    let alix2 = Tester::new_from_wallet(alix1.wallet.clone()).await;
    let bo = Tester::new().await;

    let dm = alix1
        .find_or_create_dm_by_inbox_id(bo.inbox_id(), DMMetadataOptions::default())
        .await?;
    dm.send_message(b"Hello there.").await?;

    // Have alix2 fetch the DM
    alix2.sync_welcomes(&alix2.provider).await?;

    // Wait for alix to send a payload to alix2
    alix1.sync_welcomes(&alix1.provider).await?;
    alix1.worker.wait(SyncMetric::PayloadsSent, 1).await?;
    alix1.worker.clear_metric(SyncMetric::PayloadsSent);

    let alix3 = Tester::new_from_wallet(alix1.wallet.clone()).await;
    alix3.worker.wait_for_init().await?;

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
    let wait1 = alix1.worker.wait(SyncMetric::PayloadsSent, 1);
    let wait2 = alix2.worker.wait(SyncMetric::PayloadsSent, 1);

    let timeout1 = tokio::time::timeout(Duration::from_secs(1), wait1).await;
    let timeout2 = tokio::time::timeout(Duration::from_secs(1), wait2).await;

    // We want one of them to timeout (only one payload sent)
    assert_ne!(timeout1.is_ok(), timeout2.is_ok());

    Ok(())
}

#[xmtp_common::test]
async fn double_sync_works_fine() -> Result<()> {
    let alix1 = Tester::new().await;
    let bo = Tester::new().await;

    // Create a dm and chat with bo
    alix1.test_talk_in_dm_with(&bo).await?;

    let alix2 = Tester::new_from_wallet(alix1.wallet.clone()).await;
    alix2.worker.wait_for_init().await?;

    // Pull down the new sync group, triggering a payload to be sent
    alix1.sync_welcomes(&alix1.provider).await?;
    alix1.worker.wait(SyncMetric::PayloadsSent, 1).await?;

    alix2.get_sync_group(&alix2.provider)?.sync().await?;
    alix2.worker.wait(SyncMetric::PayloadsProcessed, 1).await?;

    alix2.send_sync_request(&alix2.provider).await?;
    alix1.get_sync_group(&alix1.provider)?.sync().await?;
    alix1.worker.wait(SyncMetric::PayloadsSent, 2).await?;

    alix2.get_sync_group(&alix2.provider)?.sync().await?;
    alix2.worker.wait(SyncMetric::PayloadsProcessed, 2).await?;

    Ok(())
}
