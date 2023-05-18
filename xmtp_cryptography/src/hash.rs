use sha3::{Digest, Keccak256};

pub fn keccak256(msg: &str) -> Vec<u8> {
    let k = Keccak256::digest(msg);

    k.as_slice().to_vec()
}
