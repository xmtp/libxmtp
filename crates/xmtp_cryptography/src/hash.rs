pub use sha2::{Digest as Sha2Digest, Sha256 as Sha256Digest};

/// Sha256 is used in places where cryptographic security is not required, as sha256 has a
/// significant speed improvement over Keccak.
pub fn sha256_bytes(bytes: &[u8]) -> Vec<u8> {
    sha256_array(bytes).to_vec()
}

/// Compute the SHA-256 hash of `bytes` and return it as a fixed `[u8; 32]`,
/// avoiding the heap allocation done by [`sha256_bytes`].
#[inline]
pub fn sha256_array(bytes: &[u8]) -> [u8; 32] {
    Sha256Digest::digest(bytes).into()
}
