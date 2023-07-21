use sha2::Sha256;
use sha3::{Digest, Keccak256};

pub fn keccak256(msg: &str) -> Vec<u8> {
    let k = Keccak256::digest(msg);

    k.as_slice().to_vec()
}

/// Sha256 is used in places where cryptographic security is not required, as sha256 has a significant speed improvement over Keccak.
pub fn sha256_bytes(bytes: &[u8]) -> Vec<u8> {
    let k = Sha256::digest(bytes);

    k.as_slice().to_vec()
}
