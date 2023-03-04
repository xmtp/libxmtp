use sha3::{Digest, Keccak256};

// Static functions
// ----------------
// - get_personal_sign_message
// - get_ethereum_address_from_public_key
pub struct EthereumUtils;

impl EthereumUtils {
    pub fn eip55_checksum(address: &str) -> String {
        let mut address = address.to_lowercase();
        let address_copy = address.clone();
        let mut hash = Keccak256::new();
        hash.update(address.as_bytes());
        let hash = hash.finalize();
        for (i, c) in address_copy.chars().enumerate() {
            if c.is_digit(16) {
                let hash_char = hash[i / 2];
                let hash_digit = if i % 2 == 0 {
                    hash_char >> 4
                } else {
                    hash_char & 0xf
                };
                if hash_digit > 7 {
                    address.replace_range(i..i + 1, &c.to_uppercase().to_string());
                }
            }
        }
        address
    }

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
        // Return as hex string with EIP-55 checksumming
        return format!(
            "0x{}",
            EthereumUtils::eip55_checksum(&hex::encode(&hash[12..]))
        );
    }

    // Place the XMTP payload in this utilities class
    pub fn xmtp_identity_key_payload(public_key_bytes: &[u8]) -> Vec<u8> {
        let raw_string = format!(
            "XMTP : Create Identity\n{}\n\nFor more info: https://xmtp.org/signatures/",
            hex::encode(public_key_bytes)
        );
        // Return the string utf-8 encoded
        return raw_string.as_bytes().to_vec();
    }
}

// Define a trait for keys called EthereumCompatibleKey
// this trait allows a public key to be converted to an ethereum address
// and allows a private key to be converted -> public key -> ethereum address
pub trait EthereumCompatibleKey {
    fn get_ethereum_address(&self) -> String;
}
