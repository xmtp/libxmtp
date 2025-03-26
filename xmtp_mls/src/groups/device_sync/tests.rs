use std::ops::Deref;

use super::*;
use crate::{builder::ClientBuilder, groups::DMMetadataOptions, utils::FullXmtpClient};
use ethers::signers::LocalWallet;
use xmtp_cryptography::utils::generate_local_wallet;

#[xmtp_common::test]
async fn basic_sync() {
    let alix = TestClient::new().await;
    let bo = TestClient::new().await;

    let dm = alix
        .find_or_create_dm_by_inbox_id(bo.inbox_id(), DMMetadataOptions::default())
        .await
        .unwrap();
    dm.send_message(b"Hello there.").await.unwrap();
    bo.sync_welcomes(&bo.provider).await.unwrap();

    let alix2 = TestClient::new_from_wallet(alix.wallet.clone()).await;
    alix2.worker.block_for_metric(SyncMetric::Init, 1).await;

    alix.sync_welcomes(&alix.provider).await.unwrap();
    alix.worker
        .block_for_metric(SyncMetric::SyncPayloadsSent, 1)
        .await;

    let alix2_sync_group = alix2.get_sync_group(&alix2.provider).unwrap();
    alix2_sync_group.sync().await.unwrap();
    alix2
        .worker
        .block_for_metric(SyncMetric::SyncPayloadsProcessed, 1)
        .await;

    // Ensure the DM is present on the second device.
    let alix2_dm = alix2.group(&dm.group_id).unwrap();
    let alix2_dm_msgs = alix2_dm.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(alix2_dm_msgs.len(), 1);
    assert_eq!(alix2_dm_msgs[0].decrypted_message_bytes, b"Hello there.");
}

struct TestClient {
    wallet: LocalWallet,
    client: FullXmtpClient,
    provider: XmtpOpenMlsProvider,
    worker: Arc<WorkerHandle<SyncMetric>>,
}

impl TestClient {
    async fn new() -> Self {
        let wallet = generate_local_wallet();
        Self::new_from_wallet(wallet).await
    }
    async fn new_from_wallet(wallet: LocalWallet) -> Self {
        let client = ClientBuilder::new_test_client(&wallet).await;
        let provider = client.mls_provider().unwrap();
        let worker = client.device_sync.worker_handle().unwrap();

        Self {
            wallet,
            client,
            provider,
            worker,
        }
    }
}

impl Deref for TestClient {
    type Target = FullXmtpClient;

    fn deref(&self) -> &Self::Target {
        &self.client
    }
}
