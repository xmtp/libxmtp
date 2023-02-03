use ethers::core::rand::thread_rng;
use ethers::signers::{coins_bip39::{Mnemonic,English}};

// use aes-gcm from ring crate
use ring::aead::{Aead, Nonce, UnboundKey, AES_256_GCM, aead::generic_array::GenericArray};
// use hkdf from ring
use ring::hkdf::{Hkdf, Salt};

use protobuf;

mod proto;

pub struct Keystore {
    // Optional privateIdentityKey
    privateIdentityKey: Option<proto::private_key::SignedPrivateKey>,
}

impl Keystore {
    // new() is a constructor for the Keystore struct
    pub fn new() -> Self {
        Keystore {
            // Empty option for privateIdentityKey
            privateIdentityKey: None,
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
    fn hkdf(secret: &[u8], salt: &[u8]) -> UnboundKey {
        // Create a salt from the salt slice
        let salt = Salt::new(&AES_256_GCM, salt);
        // Create a new Hkdf instance
        let hkdf = Hkdf::<HmacSha256>::new(salt, secret);
        // Create a new UnboundKey instance
        let mut key = [0u8; 32];
        // Fill the key with the derived key
        hkdf.expand(&[], &mut key).unwrap();
        // Return the UnboundKey
        UnboundKey::new(&AES_256_GCM, &key).unwrap()
    }


//    /**
//     * Mirrors this javascript implementation:
//     * // symmetric authenticated encryption of plaintext using the secret;
//     * // additionalData is used to protect un-encrypted parts of the message (header)
//     * // in the authentication scope of the encryption.
//     * export async function encrypt(
//     *   plain: Uint8Array,
//     *   secret: Uint8Array,
//     *   additionalData?: Uint8Array
//     * ): Promise<Ciphertext> {
//     *   const salt = crypto.getRandomValues(new Uint8Array(KDFSaltSize))
//     *   const nonce = crypto.getRandomValues(new Uint8Array(AESGCMNonceSize))
//     *   const key = await hkdf(secret, salt)
//     *   const encrypted: ArrayBuffer = await crypto.subtle.encrypt(
//     *     aesGcmParams(nonce, additionalData),
//     *     key,
//     *     plain
//     *   )
//     *   return new Ciphertext({
//     *     aes256GcmHkdfSha256: {
//     *       payload: new Uint8Array(encrypted),
//     *       hkdfSalt: salt,
//     *       gcmNonce: nonce,
//     *     },
//     *   })
//     * }
//     */
//    fn encrypt(plain: &[u8], secret: &[u8], additionalData: Option<&[u8]>) -> proto::ciphertext::Ciphertext {
//        // Generate a random salt
//        let mut salt = [0u8; 16];
//        thread_rng().fill_bytes(&mut salt);
//
//        // Generate a random nonce
//        let mut nonce = [0u8; 12];
//        thread_rng().fill_bytes(&mut nonce);
//
//        // Generate a key from the secret and salt
//        let key = hkdf(secret, salt);
//
//        // Encrypt the plain text
//        let encrypted = crypto::aes_gcm_encrypt(plain, key, nonce, additionalData);
//
//        // Return the ciphertext
//        proto::Ciphertext {
//            aes256GcmHkdfSha256: Some(proto::Aes256GcmHkdfSha256 {
//                payload: encrypted,
//                hkdfSalt: salt,
//                gcmNonce: nonce,
//            }),
//        }
//    }

    // Set private identity key from protobuf bytes
    pub fn set_private_identity_key(&mut self, private_identity_key: &[u8]) {
        // Deserialize protobuf bytes into a SignedPrivateKey struct
        let privateKeyResult: protobuf::Result<proto::private_key::SignedPrivateKey> = protobuf::Message::parse_from_bytes(private_identity_key);
        // If the deserialization was successful, set the privateIdentityKey field
        if privateKeyResult.is_ok() {
            self.privateIdentityKey = Some(privateKeyResult.unwrap());
        }
    }

//    // Takes a content_topic, message bytes and header bytes and produces proto::ciphertext::Ciphertext
//    pub fn encrypt_v2(&self, content_topic: &[u8], message: &[u8], header: &[u8]) -> proto::ciphertext::Ciphertext {
//        // Create a new Ciphertext struct
//        let mut ciphertext = proto::ciphertext::Ciphertext::new();
//        // Set the version field to 2
//        ciphertext.set_version(2);
//        // Set the content topic field to the content topic bytes
//        ciphertext.set_contentTopic(content_topic.to_vec());
//        // Set the header field to the header bytes
//        ciphertext.set_header(header.to_vec());
//        // Set the body field to the message bytes
//        ciphertext.set_body(message.to_vec());
//        // Return the ciphertext
//        ciphertext
//    }

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
        let x = XMTP {};
        let mnemonic = x.generate_mnemonic();
        assert_eq!(mnemonic.split(" ").count(), 12);
    }
}
