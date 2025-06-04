use alloy::signers::local::PrivateKeySigner;

use rand::{CryptoRng, RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;

pub fn rng() -> impl CryptoRng + RngCore {
    ChaCha20Rng::from_entropy()
}

pub fn seeded_rng(seed: u64) -> impl CryptoRng + RngCore {
    ChaCha20Rng::seed_from_u64(seed)
}

pub fn generate_local_wallet() -> PrivateKeySigner {
    PrivateKeySigner::random()
}
