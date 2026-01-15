use alloy::signers::local::PrivateKeySigner;

pub fn generate_local_wallet() -> PrivateKeySigner {
    PrivateKeySigner::random()
}
