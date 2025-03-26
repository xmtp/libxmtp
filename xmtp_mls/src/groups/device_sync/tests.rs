use super::*;
use crate::{builder::ClientBuilder, groups::DMMetadataOptions};
use xmtp_cryptography::utils::generate_local_wallet;

#[xmtp_common::test]
async fn basic_sync() {
    let alix_wallet = generate_local_wallet();
    let alix = ClientBuilder::new_test_client(&alix_wallet).await;
    let alix_provider = alix.mls_provider().unwrap();
    let alix_worker = alix.device_sync.worker_handle().unwrap();

    let bo_wallet = generate_local_wallet();
    let bo = ClientBuilder::new_test_client(&bo_wallet).await;
    let bo_provider = bo.mls_provider().unwrap();

    let dm = alix
        .find_or_create_dm_by_inbox_id(bo.inbox_id(), DMMetadataOptions::default())
        .await
        .unwrap();
    dm.send_message(b"Hello there.").await.unwrap();
    bo.sync_welcomes(&bo_provider).await.unwrap();

    let alix2 = ClientBuilder::new_test_client(&alix_wallet).await;
    let alix2_provider = alix2.mls_provider().unwrap();
    let alix2_worker = alix2.device_sync.worker_handle().unwrap();
    alix2_worker.block_for_metric(SyncMetric::Init, 1).await;

    alix.sync_welcomes(&alix_provider).await.unwrap();
    alix_worker
        .block_for_metric(SyncMetric::SyncPayloadsSent, 1)
        .await;

    let alix2_sync_groupo = alix2.get_sync_group(&alix2_provider).unwrap();
    alix2_sync_groupo.sync().await.unwrap();
    alix2_worker
        .block_for_metric(SyncMetric::SyncPayloadsProcessed, 1)
        .await;

    // Ensure the DM is present on the second device.
    let alix2_dm = alix2.group(&dm.group_id).unwrap();
    let alix2_dm_msgs = alix2_dm.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(alix2_dm_msgs.len(), 1);
    assert_eq!(alix2_dm_msgs[0].decrypted_message_bytes, b"Hello there.");
}
