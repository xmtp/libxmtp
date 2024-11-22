use ethers::core::utils::keccak256;
pub use ethers::prelude::LocalWallet;
use k256::ecdsa::VerifyingKey;
use rand::{CryptoRng, RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;

pub fn rng() -> impl CryptoRng + RngCore {
    ChaCha20Rng::from_entropy()
}

pub fn seeded_rng(seed: u64) -> impl CryptoRng + RngCore {
    ChaCha20Rng::seed_from_u64(seed)
}

/// Construct an ethereum address from a ecdsa public key
pub fn eth_address(pubkey: &VerifyingKey) -> [u8; 20] {
    // Get the public key bytes
    let binding = pubkey.to_encoded_point(false);
    let public_key_bytes = binding.as_bytes();

    let mut out = [0u8; 20];
    let hash = keccak256(public_key_bytes);
    out.copy_from_slice(&hash[12..]);
    out
}

pub fn generate_local_wallet() -> LocalWallet {
    LocalWallet::new(&mut rng())
}
