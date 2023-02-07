use ethers::core::rand::thread_rng;
use ethers::signers::{coins_bip39::{Mnemonic,English}};

use sha2::Sha256;
use hkdf::Hkdf;

use protobuf;

mod proto;

pub struct Keystore {
    // Optional privateIdentityKey
    private_identity_key: Option<proto::private_key::SignedPrivateKey>,
}

impl Keystore {
    // new() is a constructor for the Keystore struct
    pub fn new() -> Self {
        Keystore {
            // Empty option for privateIdentityKey
            private_identity_key: None,
        }
    }

    /**
     * Rust version of this javascript code:
     * // Derive AES-256-GCM key from a shared secret and salt.
     * // Returns crypto.CryptoKey suitable for the encrypt/decrypt API
     * async function hkdf(secret: Uint8Array, salt: Uint8Array): Promise<CryptoKey> {
     *   const key = await crypto.subtle.importKey('raw', secret, 'HKDF', false, [
     *     'deriveKey',
     *   ])
     *   return crypto.subtle.deriveKey(
     *     { name: 'HKDF', hash: 'SHA-256', salt, info: hkdfNoInfo },
     *     key,
     *     { name: 'AES-GCM', length: 256 },
     *     false,
     *     ['encrypt', 'decrypt']
     *   )
     * }
     */
    fn hkdf(secret: &[u8], salt: &[u8]) -> Result<[u8; 32], String> {
        let hk = Hkdf::<Sha256>::new(Some(&salt[..]), &secret);
        let mut okm = [0u8; 42];
        let res = hk.expand(&[], &mut okm);
        if res.is_err() {
            return Err(res.err().unwrap().to_string());
        }
        okm[0..32].try_into().map_err(|_| "hkdf failed to fit in 32 bytes".to_string())
    }

    // Set private identity key from protobuf bytes
    pub fn set_private_identity_key(&mut self, private_identity_key: &[u8]) {
        // Deserialize protobuf bytes into a SignedPrivateKey struct
        let private_key_result: protobuf::Result<proto::private_key::SignedPrivateKey> = protobuf::Message::parse_from_bytes(private_identity_key);
        // If the deserialization was successful, set the privateIdentityKey field
        if private_key_result.is_ok() {
            self.private_identity_key = Some(private_key_result.unwrap());
        }
    }

    pub fn generate_mnemonic(&self) -> String {
		let mut rng = thread_rng();
		let mnemonic = Mnemonic::<English>::new_with_count(&mut rng, 12).unwrap();
		let phrase = mnemonic.to_phrase();
		// split the phrase by spaces
		let words: Vec<String> = phrase.unwrap().split(" ").map(|s| s.to_string()).collect();
        return words.join(" ");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_mnemonic_works() {
        let x = Keystore { private_identity_key: None };
        let mnemonic = x.generate_mnemonic();
        assert_eq!(mnemonic.split(" ").count(), 12);
    }

    #[test]
    fn test_hkdf_simple() {
        // Test Vectors generated with xmtp-js
        // secret aff491a0fe153a4ac86065b4b4f6953a4cb33477aa233facb94d5fb88c82778c39167f453aa0690b5358abe9e027ddca5a6185bce3699d8b2ac7efa30510a7991b
        // salt e3412c112c28353088c99bd5c7350c81b1bc879b4d08ea1192ec3c03202ff337
        // derived 0159d9ad511263c3754a8e2045fadc657c0016b1801720e67bbeb2661c60f176
        // secret af43ad68d9fcf40967f194497246a6e30515b6c4f574ee2ff58e31df32f5f18040812188cfb5ce34e74ae27b73be08dca626b3eb55c55e6733f32a59dd1b8e021c
        // salt a8500ae6f90a7ccaa096adc55857b90c03508f7d5f8d103a49d58e69058f0c3c
        // derived 6181d0905f3f31cc3940336696afe1337d9e4d7f6655b9a6eaed2880be38150c
        
        // Test hkdf with hardcoded test vectors
        // Test 1
        let secret1 = hex::decode("aff491a0fe153a4ac86065b4b4f6953a4cb33477aa233facb94d5fb88c82778c39167f453aa0690b5358abe9e027ddca5a6185bce3699d8b2ac7efa30510a7991b").unwrap();
        let salt1 = hex::decode("e3412c112c28353088c99bd5c7350c81b1bc879b4d08ea1192ec3c03202ff337").unwrap();
        let expected1 = hex::decode("0159d9ad511263c3754a8e2045fadc657c0016b1801720e67bbeb2661c60f176").unwrap();
        let derived1_result = Keystore::hkdf(&secret1, &salt1);
        // Check result
        assert!(derived1_result.is_ok());
        assert_eq!(derived1_result.unwrap().to_vec(), expected1);

        // Test 2
        let secret2 = hex::decode("af43ad68d9fcf40967f194497246a6e30515b6c4f574ee2ff58e31df32f5f18040812188cfb5ce34e74ae27b73be08dca626b3eb55c55e6733f32a59dd1b8e021c").unwrap();
        let salt2 = hex::decode("a8500ae6f90a7ccaa096adc55857b90c03508f7d5f8d103a49d58e69058f0c3c").unwrap();
        let expected2 = hex::decode("6181d0905f3f31cc3940336696afe1337d9e4d7f6655b9a6eaed2880be38150c").unwrap();
        let derived2_result = Keystore::hkdf(&secret2, &salt2);
        // Check result
        assert!(derived2_result.is_ok());
        assert_eq!(derived2_result.unwrap().to_vec(), expected2);
    }

    #[test]
    fn test_hkdf_error() {
        let secret1 = hex::decode("bff491a0fe153a4ac86065b4b4f6953a4cb33477aa233facb94d5fb88c82778c39167f453aa0690b5358abe9e027ddca5a6185bce3699d8b2ac7efa30510a7991b").unwrap();
        let salt1 = hex::decode("e3412c112c28353088c99bd5c7350c81b1bc879b4d08ea1192ec3c03202ff337").unwrap();
        let expected1 = hex::decode("0159d9ad511263c3754a8e2045fadc657c0016b1801720e67bbeb2661c60f176").unwrap();
        let derived1_result = Keystore::hkdf(&secret1, &salt1);
        // Check result
        assert!(derived1_result.is_ok());
        // Assert not equal
        assert_ne!(derived1_result.unwrap().to_vec(), expected1);
    }
}
