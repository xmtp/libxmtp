use sha2::{Digest, Sha256};

fn sha256_string(input: String) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();
    format!("{:x}", result)
}

pub fn generate_inbox_id(account_address: &String, nonce: &u64) -> String {
    sha256_string(format!("{}{}", account_address.to_lowercase(), nonce))
}
