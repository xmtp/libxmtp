extern crate env_logger;
extern crate ethers;
extern crate log;
extern crate xmtp;

use ethers::signers::{LocalWallet, Signer};
use ethers_core::rand;
use log::{error, info, warn};
use xmtp::{
    networking::MockXmtpApiClient, persistence::in_memory_persistence::InMemoryPersistence,
};
use xmtp_cryptography::signature::h160addr_to_string;

/// A complete example of a minimal xmtp client which can send and recieve messages.
/// run this example from the cli:  `RUST_LOG=DEBUG cargo run --example cli-client`
fn main() {
    env_logger::init();
    info!("Starting CLI Client....");

    let wallet = LocalWallet::new(&mut rand::thread_rng());

    let client_result = xmtp::ClientBuilder::new()
        .network(xmtp::client::Network::Dev)
        .api_client(MockXmtpApiClient::default())
        .persistence(InMemoryPersistence::default())
        .wallet_address(&h160addr_to_string(wallet.address()))
        .build();

    let _client = match client_result {
        Err(e) => {
            error!("ClientBuilder Error: {:?}", e);
            return;
        }
        Ok(c) => c,
    };
    warn!("Client Is not properly initialized at this point -- Signed account not present");

    // Application logic
    // ...

    info!("Exiting CLI Client....");
}
