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

    tokio::time::sleep(Duration::from_secs(1)).await;

    let sync_group = alix2.get_sync_group(&alix2_provider).unwrap();
    sync_group.update_installations().await;

    alix.sync_welcomes(&alix_provider).await.unwrap();

    alix_worker
        .block_for_metric(SyncMetric::SyncGroupWelcomesProcessed, 1)
        .await;

    // alix2_worker
    // .block_for_metric(SyncMetric::SyncRepliesProcessed, 1)
    // .await;
}
