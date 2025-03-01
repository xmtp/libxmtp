use k256::ecdsa::SigningKey;
use sha2::{digest::Update, Sha256};
use sha3::{Digest, Keccak256};

pub fn keccak256(msg: &str) -> Vec<u8> {
    let k = Keccak256::digest(msg);

    k.as_slice().to_vec()
}

/// Sha256 is used in places where cryptographic security is not required, as sha256 has a
/// significant speed improvement over Keccak.
pub fn sha256_bytes(bytes: &[u8]) -> Vec<u8> {
    let k = Sha256::digest(bytes);

    k.as_slice().to_vec()
}

pub fn sign_keccak_256(secret_key: &[u8], message: &[u8]) -> Result<(Vec<u8>, u8), String> {
    let signing_key = SigningKey::from_bytes(secret_key.into()).map_err(|e| e.to_string())?;
    let hash = Keccak256::new().chain(message);
    let (signature, recovery_id) = signing_key
        .sign_digest_recoverable::<Keccak256>(hash)
        .map_err(|e| e.to_string())?;
    Ok((signature.to_vec(), recovery_id.to_byte()))
}
