use crate::{
    builder::ClientBuilder,
    groups::device_sync::handle::{SyncMetric, WorkerHandle},
};
use ethers::signers::LocalWallet;
use std::{ops::Deref, sync::Arc};
use xmtp_cryptography::utils::generate_local_wallet;
use xmtp_db::XmtpOpenMlsProvider;

use super::FullXmtpClient;

/// A test client wrapper that auto-exposes all of the usual component access boilerplate.
/// Makes testing easier and less repetetive.
#[allow(dead_code)]
pub(crate) struct Tester {
    pub wallet: LocalWallet,
    pub client: FullXmtpClient,
    pub provider: Arc<XmtpOpenMlsProvider>,
    pub worker: Arc<WorkerHandle<SyncMetric>>,
}

#[allow(dead_code)]
impl Tester {
    pub(crate) async fn new() -> Self {
        let wallet = generate_local_wallet();
        Self::new_from_wallet(wallet).await
    }
    pub(crate) async fn new_from_wallet(wallet: LocalWallet) -> Self {
        let client = ClientBuilder::new_test_client(&wallet).await;
        let provider = client.mls_provider().unwrap();
        let worker = client.device_sync.worker_handle().unwrap();

        Self {
            wallet,
            client,
            provider: Arc::new(provider),
            worker,
        }
    }
}

impl Deref for Tester {
    type Target = FullXmtpClient;

    fn deref(&self) -> &Self::Target {
        &self.client
    }
}
