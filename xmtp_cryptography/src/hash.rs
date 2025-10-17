use sha2::{Digest, Sha256};

/// Sha256 is used in places where cryptographic security is not required, as sha256 has a
/// significant speed improvement over Keccak.
#[allow(deprecated)]
pub fn sha256_bytes(bytes: &[u8]) -> Vec<u8> {
    let k = Sha256::digest(bytes);

    k.as_slice().to_vec()
}
