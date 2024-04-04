use ethers::signers::LocalWallet;
use ethers_core::utils::keccak256;
use k256::ecdsa::VerifyingKey;
use rand::{CryptoRng, RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;

pub fn rng() -> impl CryptoRng + RngCore {
    ChaCha20Rng::from_entropy()
}

pub fn seeded_rng(seed: u64) -> impl CryptoRng + RngCore {
    ChaCha20Rng::seed_from_u64(seed)
}

pub fn eth_address(pubkey: &VerifyingKey) -> Result<String, String> {
    // Get the public key bytes
    let binding = pubkey.to_encoded_point(false);
    let public_key_bytes = binding.as_bytes();

    let hash = keccak256(public_key_bytes);

    // Return the result as hex string, take the last 20 bytes
    Ok(format!("0x{}", hex::encode(&hash[12..])))
}

pub fn generate_local_wallet() -> LocalWallet {
    LocalWallet::new(&mut rng())
}
