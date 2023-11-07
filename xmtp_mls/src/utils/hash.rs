use xmtp_cryptography::hash::sha256_bytes;

pub fn sha256(bytes: &[u8]) -> Vec<u8> {
    sha256_bytes(bytes)
}
