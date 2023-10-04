use sha3::{Digest, Sha3_256};

pub fn sha3_256(msg: &[u8]) -> Vec<u8> {
    let mut hasher = Sha3_256::new();
    hasher.update(msg);

    hasher.finalize().to_vec()
}
