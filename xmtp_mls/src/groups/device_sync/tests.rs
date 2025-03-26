use super::*;
use crate::{groups::DMMetadataOptions, utils::Tester};
use anyhow::Result;

#[xmtp_common::test]
async fn basic_sync() -> Result<()> {
    let alix1 = Tester::new().await;
    let bo = Tester::new().await;

    let dm = alix1
        .find_or_create_dm_by_inbox_id(bo.inbox_id(), DMMetadataOptions::default())
        .await?;
    dm.send_message(b"Hello there.").await?;

    let alix2 = Tester::new_from_wallet(alix1.wallet.clone()).await;
    alix2.worker.wait_for_init().await;

    // Have alix1 receive new sync group, and auto-send a sync payload
    alix1.sync_welcomes(&alix1.provider).await?;
    alix1.worker.wait(SyncMetric::PayloadsSent, 1).await;

    // Have alix2 receive payload and process it
    let alix2_sync_group = alix2.get_sync_group(&alix2.provider)?;
    alix2_sync_group.sync().await?;
    alix2.worker.wait(SyncMetric::PayloadsProcessed, 1).await;

    // Ensure the DM is present on the second device.
    let alix2_dm = alix2.group(&dm.group_id)?;
    let alix2_dm_msgs = alix2_dm.find_messages(&MsgQueryArgs::default())?;
    assert_eq!(alix2_dm_msgs.len(), 1);
    assert_eq!(alix2_dm_msgs[0].decrypted_message_bytes, b"Hello there.");

    Ok(())
}

#[xmtp_common::test]
async fn two_old_installations() -> Result<()> {
    let alix1 = Tester::new().await;
    let alix2 = Tester::new_from_wallet(alix1.wallet.clone()).await;
    let bo = Tester::new().await;

    let dm = alix1
        .find_or_create_dm_by_inbox_id(bo.inbox_id(), DMMetadataOptions::default())
        .await?;
    dm.send_message(b"Hello there.").await?;

    // Have alix2 fetch the DM
    alix2.sync_welcomes(&alix2.provider).await?;

    let alix3 = Tester::new_from_wallet(alix1.wallet.clone()).await;
    alix3.worker.wait_for_init().await;

    // Have alix1 and 2 fetch the new sync group
    alix1.sync_welcomes(&alix1.provider).await?;
    alix2.sync_welcomes(&alix2.provider).await?;

    let alix1_sg = alix1.get_sync_group(&alix1.provider)?;
    let alix2_sg = alix2.get_sync_group(&alix2.provider)?;
    let alix3_sg = alix3.get_sync_group(&alix3.provider)?;

    // They should all have the same sync group
    assert_eq!(alix1_sg.group_id, alix2_sg.group_id);
    assert_eq!(alix1_sg.group_id, alix3_sg.group_id);

    alix1
        .worker
        .wait_or(vec![&alix2.worker], SyncMetric::PayloadsSent, 1)
        .await;

    Ok(())
}
