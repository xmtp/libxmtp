use xmtp_cryptography::utils::generate_local_wallet;

use crate::builder::ClientBuilder;

use super::FullXmtpClient;
use rstest::*;

#[fixture]
pub async fn alix() -> FullXmtpClient {
    ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await
}

#[fixture]
pub async fn bo() -> FullXmtpClient {
    ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await
}

#[fixture]
pub async fn caro() -> FullXmtpClient {
    ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await
}

#[fixture]
pub async fn eve() -> FullXmtpClient {
    ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await
}

#[fixture]
pub async fn xmtp_client() -> FullXmtpClient {
    ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await
}
