use xmtp_cryptography::utils::generate_local_wallet;

use crate::{
    builder::ClientBuilder,
    utils::{ClientTester, LocalTesterBuilder, Tester},
};

use super::FullXmtpClient;
use rstest::*;

#[fixture]
pub async fn alix() -> ClientTester {
    Tester::builder().with_name("alix").build().await
}

#[fixture]
pub async fn bo() -> ClientTester {
    Tester::builder().with_name("bo").build().await
}

#[fixture]
pub async fn bola() -> ClientTester {
    Tester::builder().with_name("bo").build().await
}

#[fixture]
pub async fn caro() -> ClientTester {
    Tester::builder().with_name("caro").build().await
}

#[fixture]
pub async fn eve() -> ClientTester {
    Tester::builder().with_name("eve").build().await
}

#[fixture]
pub async fn xmtp_client() -> FullXmtpClient {
    ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await
}
