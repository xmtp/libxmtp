extern crate env_logger;
extern crate ethers;
extern crate log;
extern crate xmtp;

use ethers_core::rand;
use log::{error, info};
use xmtp::networking::MockXmtpApiClient;
use xmtp::persistence::in_memory_persistence::InMemoryPersistence;
use xmtp::storage::{StorageOption, UnencryptedMessageStore};
use xmtp_cryptography::utils::LocalWallet;

/// A complete example of a minimal xmtp client which can send and recieve messages.
/// run this example from the cli:  `RUST_LOG=DEBUG cargo run --example cli-client`
#[tokio::main]
async fn main() {
    env_logger::init();
    info!("Starting CLI Client....");

    let msg_store = UnencryptedMessageStore::new(StorageOption::Ephemeral).unwrap();

    let wallet = LocalWallet::new(&mut rand::thread_rng());

    let client_result = xmtp::ClientBuilder::new(wallet.into())
        .network(xmtp::Network::Dev)
        .api_client(MockXmtpApiClient::default())
        .persistence(InMemoryPersistence::default())
        .store(msg_store)
        .build();

    let uninit_client = match client_result {
        Err(e) => {
            error!("ClientBuilder Error: {:?}", e);
            return;
        }
        Ok(c) => c,
    };

    let client = match uninit_client.init().await {
        Err(e) => {
            error!("Initialization Failed: {}", e.to_string());
            panic!("Could not init");
        }
        Ok(c) => c,
    };

    // Application logic
    // ...

    info!("Exiting CLI Client....");
}
