use sha3::{Digest, Keccak256};

// Static functions
// ----------------
// - get_personal_sign_message
// - get_ethereum_address_from_public_key
pub struct EthereumUtils;

impl EthereumUtils {
    // Generate per EIP-191
    pub fn get_personal_sign_message(message: &[u8]) -> Vec<u8> {
        // Prefix byte array is: "\x19Ethereum Signed Message:\n"
        let mut prefix = format!("\x19Ethereum Signed Message:\n{}", message.len())
            .as_bytes()
            .to_vec();
        prefix.append(&mut message.to_vec());
        return prefix;
    }

    // Generate an ethereum address, no EIP-55  mixed-case checksum address encoding for now
    // The public key must be encoded as a 64-byte array, not compressed, watch out for
    // leading zeros in either point component getting trimmed when encoding prior to using this method
    pub fn get_ethereum_address_from_public_key_bytes(public_key: &[u8]) -> String {
        let mut hasher = Keccak256::new();
        hasher.update(public_key);
        let hash = hasher.finalize();
        // Return as hex string
        // TODO: EIP-55 checksum address encoding
        return format!("0x{}", hex::encode(&hash[12..]));
    }
}

// Define a trait for keys called EthereumCompatibleKey
// this trait allows a public key to be converted to an ethereum address
// and allows a private key to be converted -> public key -> ethereum address
pub trait EthereumCompatibleKey {
    fn get_ethereum_address(&self) -> String;
}
