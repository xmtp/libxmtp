pub use sha2::{Digest as Sha2Digest, Sha256 as Sha256Digest};

/// Sha256 is used in places where cryptographic security is not required, as sha256 has a
/// significant speed improvement over Keccak.
#[allow(deprecated)]
pub fn sha256_bytes(bytes: &[u8]) -> Vec<u8> {
    let k = Sha256Digest::digest(bytes);

    k.as_slice().to_vec()
}
