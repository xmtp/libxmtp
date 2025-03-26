use super::*;
use crate::{groups::DMMetadataOptions, utils::Tester};

#[xmtp_common::test]
async fn basic_sync() {
    let alix = Tester::new().await;
    let bo = Tester::new().await;

    let dm = alix
        .find_or_create_dm_by_inbox_id(bo.inbox_id(), DMMetadataOptions::default())
        .await
        .unwrap();
    dm.send_message(b"Hello there.").await.unwrap();
    bo.sync_welcomes(&bo.provider).await.unwrap();

    let alix2 = Tester::new_from_wallet(alix.wallet.clone()).await;
    alix2.worker.block(SyncMetric::Init, 1).await;

    alix.sync_welcomes(&alix.provider).await.unwrap();
    alix.worker.block(SyncMetric::PayloadsSent, 1).await;

    let alix2_sync_group = alix2.get_sync_group(&alix2.provider).unwrap();
    alix2_sync_group.sync().await.unwrap();
    alix2.worker.block(SyncMetric::PayloadsProcessed, 1).await;

    // Ensure the DM is present on the second device.
    let alix2_dm = alix2.group(&dm.group_id).unwrap();
    let alix2_dm_msgs = alix2_dm.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(alix2_dm_msgs.len(), 1);
    assert_eq!(alix2_dm_msgs[0].decrypted_message_bytes, b"Hello there.");
}
