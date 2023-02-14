use ethers::core::rand::thread_rng;
use ethers::signers::coins_bip39::{English, Mnemonic};
use ethers::utils::hash_message;

use hkdf::Hkdf;
use sha2::Sha256;

use protobuf;

mod ecdh;
mod ethereum_utils;
mod private_key;
mod proto;
use crate::private_key::EcPrivateKey;

use base64::{engine::general_purpose, Engine as _};

pub struct Keystore {
    // Private key bundle powers most operations
    private_key_bundle: Option<proto::private_key::PrivateKeyBundleV2>,
    // List of invites
    saved_invites: Vec<proto::invitation::InvitationV1>,
}

impl Keystore {
    // new() is a constructor for the Keystore struct
    pub fn new() -> Self {
        Keystore {
            // Empty option for private key bundle
            private_key_bundle: None,
            // Conversation store
            saved_invites: Vec::new(),
        }
    }

    fn hkdf(secret: &[u8], salt: &[u8]) -> Result<[u8; 32], String> {
        let hk = Hkdf::<Sha256>::new(Some(&salt), &secret);
        let mut okm = [0u8; 42];
        let res = hk.expand(&[], &mut okm);
        if res.is_err() {
            return Err(res.err().unwrap().to_string());
        }
        okm[0..32]
            .try_into()
            .map_err(|_| "hkdf failed to fit in 32 bytes".to_string())
    }

    /** Rust implementation of this javascript code:
     * let dh1: Uint8Array, dh2: Uint8Array, preKey: SignedPrivateKey
     * if (isRecipient) {
     *   preKey = this.findPreKey(myPreKey)
     *   dh1 = preKey.sharedSecret(peer.identityKey)
     *   dh2 = this.identityKey.sharedSecret(peer.preKey)
     * } else {
     *   preKey = this.findPreKey(myPreKey)
     *   dh1 = this.identityKey.sharedSecret(peer.preKey)
     *   dh2 = preKey.sharedSecret(peer.identityKey)
     * }
     * const dh3 = preKey.sharedSecret(peer.preKey)
     * const secret = new Uint8Array(dh1.length + dh2.length + dh3.length)
     * secret.set(dh1, 0)
     * secret.set(dh2, dh1.length)
     * secret.set(dh3, dh1.length + dh2.length)
     * return secret
     */
    fn derive_shared_secret(
        &self,
        peer_bundle: &dyn ecdh::ECDHKey,
        my_prekey: &dyn ecdh::ECDHKey,
        is_recipient: bool,
    ) -> Result<[u8; 32], String> {
        // Check if self.private_key_bundle is set
        if self.private_key_bundle.is_none() {
            return Err("private key bundle is not set".to_string());
        }
        // Get the private key bundle
        let private_key_bundle = self.private_key_bundle.as_ref().unwrap();
        let secret: [u8; 32] = [0; 32];
        return Ok(secret);
    }

    // Set private identity key from protobuf bytes
    pub fn set_private_key_bundle(&mut self, private_key_bundle: &[u8]) {
        // Deserialize protobuf bytes into a SignedPrivateKey struct
        let private_key_result: protobuf::Result<proto::private_key::PrivateKeyBundleV2> =
            protobuf::Message::parse_from_bytes(private_key_bundle);
        // If the deserialization was successful, set the privateIdentityKey field
        if private_key_result.is_ok() {
            self.private_key_bundle = Some(private_key_result.unwrap());
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

    /*
     * Rust version of this javascript code:
     *  async getInvitation(viewer: PrivateKeyBundleV2): Promise<InvitationV1> {
     *    // Use cached value if already exists
     *    if (this._invitation) {
     *      return this._invitation
     *    }
     *    // The constructors for child classes will validate that this is complete
     *    const header = this.header
     *    let secret: Uint8Array
     *    if (viewer.identityKey.matches(this.header.sender.identityKey)) {
     *      secret = await viewer.sharedSecret(
     *        header.recipient,
     *        header.sender.preKey,
     *        false
     *      )
     *    } else {
     *      secret = await viewer.sharedSecret(
     *        header.sender,
     *        header.recipient.preKey,
     *        true
     *      )
     *    }

     *    const decryptedBytes = await decrypt(
     *      this.ciphertext,
     *      secret,
     *      this.headerBytes
     *    )
     *    this._invitation = InvitationV1.fromBytes(decryptedBytes)
     *    return this._invitation
     *  }
    */
    fn decrypt_sealed_invite(
        &self,
        invite: proto::invitation::SealedInvitationV1,
    ) -> Result<proto::invitation::InvitationV1, String> {
        // Check that the private identity key is set
        if self.private_key_bundle.is_none() {
            return Err("private identity key is not yet set".to_string());
        }
        // Get the private identity key
        let private_key_bundle = self.private_key_bundle.as_ref().unwrap();
        // A sealed invite consists of:
        // - A SealedInvitationHeaderV1 serialized as protobuf bytes
        // - A Ciphertext serialized as protobuf bytes
        // Get the header bytes
        let header_bytes = invite.header_bytes;
        // Deserialize the header bytes into a SealedInvitationHeaderV1 struct
        let header_result: protobuf::Result<proto::invitation::SealedInvitationHeaderV1> =
            protobuf::Message::parse_from_bytes(&header_bytes);
        // If the deserialization was successful, get the header
        // otherwise return an error
        let header = if header_result.is_ok() {
            header_result.unwrap()
        } else {
            return Err(header_result.err().unwrap().to_string());
        };
        return Err("not implemented".to_string());
    }

    // Store a proto::invitation into memory after decrypting
    pub fn save_invite(&mut self, invite_bytes: &[u8]) -> Result<(), String> {
        // Attempt to deserialize the invite
        let sealed_invite_result: protobuf::Result<proto::invitation::SealedInvitation> =
            protobuf::Message::parse_from_bytes(invite_bytes);
        // If the deserialization was successful, store the invite
        if sealed_invite_result.is_ok() {
            // Check that sealed_invite_result is a SealedInivationV1
            let sealed_invite: proto::invitation::SealedInvitation = sealed_invite_result.unwrap();
            let sealed_invite_v1: proto::invitation::SealedInvitationV1 =
                sealed_invite.v1().clone();
            let unsealed_invite_result = self.decrypt_sealed_invite(sealed_invite_v1);
            if unsealed_invite_result.is_ok() {
                // Add unsealed_invite_result to the invites list
                self.saved_invites.push(unsealed_invite_result.unwrap());
                return Ok(());
            } else {
                return Err(unsealed_invite_result.unwrap_err().to_string());
            }
        } else {
            // Wrap the deserialization error
            // TODO: Return a custom error
            Err("SealedInvitation deserialization failed".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_mnemonic_works() {
        let x = Keystore::new();
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
        let salt1 = hex::decode("e3412c112c28353088c99bd5c7350c81b1bc879b4d08ea1192ec3c03202ff337")
            .unwrap();
        let expected1 =
            hex::decode("0159d9ad511263c3754a8e2045fadc657c0016b1801720e67bbeb2661c60f176")
                .unwrap();
        let derived1_result = Keystore::hkdf(&secret1, &salt1);
        // Check result
        assert!(derived1_result.is_ok());
        assert_eq!(derived1_result.unwrap().to_vec(), expected1);

        // Test 2
        let secret2 = hex::decode("af43ad68d9fcf40967f194497246a6e30515b6c4f574ee2ff58e31df32f5f18040812188cfb5ce34e74ae27b73be08dca626b3eb55c55e6733f32a59dd1b8e021c").unwrap();
        let salt2 = hex::decode("a8500ae6f90a7ccaa096adc55857b90c03508f7d5f8d103a49d58e69058f0c3c")
            .unwrap();
        let expected2 =
            hex::decode("6181d0905f3f31cc3940336696afe1337d9e4d7f6655b9a6eaed2880be38150c")
                .unwrap();
        let derived2_result = Keystore::hkdf(&secret2, &salt2);
        // Check result
        assert!(derived2_result.is_ok());
        assert_eq!(derived2_result.unwrap().to_vec(), expected2);
    }

    #[test]
    fn test_hkdf_error() {
        let secret1 = hex::decode("bff491a0fe153a4ac86065b4b4f6953a4cb33477aa233facb94d5fb88c82778c39167f453aa0690b5358abe9e027ddca5a6185bce3699d8b2ac7efa30510a7991b").unwrap();
        let salt1 = hex::decode("e3412c112c28353088c99bd5c7350c81b1bc879b4d08ea1192ec3c03202ff337")
            .unwrap();
        let expected1 =
            hex::decode("0159d9ad511263c3754a8e2045fadc657c0016b1801720e67bbeb2661c60f176")
                .unwrap();
        let derived1_result = Keystore::hkdf(&secret1, &salt1);
        // Check result
        assert!(derived1_result.is_ok());
        // Assert not equal
        assert_ne!(derived1_result.unwrap().to_vec(), expected1);
    }

    #[test]
    fn test_hkdf_invalid_key() {
        let secret1 = hex::decode("").unwrap();
        let salt1 = hex::decode("").unwrap();
        let derived1_result = Keystore::hkdf(&secret1, &salt1);
        // Check result
        assert!(derived1_result.is_ok());
    }

    #[test]
    fn test_private_key_from_v2_bundle() {
        // = test vectors generated with xmtp-js =
        // encoded v2:  EpYDCsgBCMDw7ZjWtOygFxIiCiAvph+Hg/Gk9G1g2EoW1ZDlWVH1nCkn6uRL7GBG3iNophqXAQpPCMDw7ZjWtOygFxpDCkEEeH4w/gK5HMaKu51aec/jiosmqDduIaEA67V7Lbox1cPhz9SIEi6sY/6jVQQXeIjKxzsZSVrM0LXCXjc0VkRmxhJEEkIKQNSujk9ApV5gIKltm0CFhLLuN3Xt2fjkKZBoUH/mswjTaUMTc3qZZzde3ZKMfkNVZYqns4Sn0sgopXzpjQGgjyUSyAEIwPXBtNa07KAXEiIKIOekWIyRJCelxqX+mR8i76KuDO2QV3e42nv8CxJQL0DXGpcBCk8IwPXBtNa07KAXGkMKQQTIePKpkAHxREbLbXfn6XCOwx9YqQWmqLuTHAnqRNj1q5xDLpbgkiyAORFZmVOK8iVq3dT/PWm6WMasPrqdzD7iEkQKQgpAqIj/yKx2wn8VjeWV6wm/neNDEQ6282p3CeJsPDKS56B11Nqc5Y5vUPKcrC1nB2dqBkwvop0fU49Yx4k0CB2evQ==
        // digest:  dQnlvaDHYtK6x/kNdYtbImP6Acy8VCq1498WO+CObKk=
        // testMessageSignature:  CkQKQAROtHwYeoBT4LhZEVM6dYaPCDDVy4/9dYSZBvKizAk7J+9f29+1OkAZoGw+FLCHWr/G9cKGfiZf3ln7bTssuIkQAQ==
        let private_key_bundle_raw = "EpYDCsgBCMDw7ZjWtOygFxIiCiAvph+Hg/Gk9G1g2EoW1ZDlWVH1nCkn6uRL7GBG3iNophqXAQpPCMDw7ZjWtOygFxpDCkEEeH4w/gK5HMaKu51aec/jiosmqDduIaEA67V7Lbox1cPhz9SIEi6sY/6jVQQXeIjKxzsZSVrM0LXCXjc0VkRmxhJEEkIKQNSujk9ApV5gIKltm0CFhLLuN3Xt2fjkKZBoUH/mswjTaUMTc3qZZzde3ZKMfkNVZYqns4Sn0sgopXzpjQGgjyUSyAEIwPXBtNa07KAXEiIKIOekWIyRJCelxqX+mR8i76KuDO2QV3e42nv8CxJQL0DXGpcBCk8IwPXBtNa07KAXGkMKQQTIePKpkAHxREbLbXfn6XCOwx9YqQWmqLuTHAnqRNj1q5xDLpbgkiyAORFZmVOK8iVq3dT/PWm6WMasPrqdzD7iEkQKQgpAqIj/yKx2wn8VjeWV6wm/neNDEQ6282p3CeJsPDKS56B11Nqc5Y5vUPKcrC1nB2dqBkwvop0fU49Yx4k0CB2evQ==";
        let message = "hello world!";
        let digest = "dQnlvaDHYtK6x/kNdYtbImP6Acy8VCq1498WO+CObKk=";
        let signature_proto_raw = "CkQKQAROtHwYeoBT4LhZEVM6dYaPCDDVy4/9dYSZBvKizAk7J+9f29+1OkAZoGw+FLCHWr/G9cKGfiZf3ln7bTssuIkQAQ==";
        let expected_address = "0xf4c3d5f8f04da9d5eaa7e92f7a6e7f990450c88b";
        // =====

        // For debugging, the secret key is hex encoded bigint:
        // BigInt('0x2fa61f8783f1a4f46d60d84a16d590e55951f59c2927eae44bec6046de2368a6')
        // > 21552218103791599555364469821754606161148148489927333195317013913723696539814n

        let proto_encoded = base64::decode(private_key_bundle_raw).unwrap();
        // Deserialize the proto bytes into proto::private_key::PrivateKeyBundleV2
        let signed_private_key: proto::private_key::PrivateKeyBundle =
            protobuf::Message::parse_from_bytes(&proto_encoded).unwrap();
        let private_key_bundle = signed_private_key.v2();

        // Decode signature proto
        let signature: proto::signature::Signature =
            protobuf::Message::parse_from_bytes(&base64::decode(signature_proto_raw).unwrap())
                .unwrap();
        let ec_private_key_result = EcPrivateKey::from_proto(private_key_bundle);
        assert!(ec_private_key_result.is_ok());
        let ec_private_key = ec_private_key_result.unwrap();
        // Do a raw byte signature verification
        let signature_verified =
            &ec_private_key.verify_signature(message.as_bytes(), &signature.ecdsa_compact().bytes);
        assert!(signature_verified.is_ok());
        // Calculate the eth wallet address from public key
        let eth_address = &ec_private_key.eth_address().unwrap();
        assert_eq!(eth_address, expected_address);
    }

    #[test]
    fn test_verify_wallet_signature() {
        // = test vectors generated with xmtp-js =
        // =====
        let address = "0x2Fb28c95E110C6Bb188B41f9E7d6850ccbE48e61";
        let signature_proto_result: proto::signature::Signature = protobuf::Message::parse_from_bytes(&base64::decode("EkIKQKOfb+lUwNCnJrMWQapvY1YNtFheYXa5gH5jZ+IpHPxrIAtWyvMPTMW7WpBb4Mscrie9yRap7H8XbzPPbJKEybI=").unwrap()).unwrap();
        let bytes_to_sign = base64::decode("CIC07umj5I+hFxpDCkEEE27Yj8R97eSoWjEwE35U3pB439S9OSfdrPrDjGH9/JQ5CCb8rjFK1vxxhbHGM2bq1v0PXdk6k/tkbhXmn2WEmw==").unwrap();
        // Encode string as bytes
        let xmtp_identity_signature_payload =
            ethereum_utils::EthereumUtils::xmtp_identity_key_payload(&bytes_to_sign);
        println!(
            "xmtp_identity_signature_payload: {:?}",
            std::str::from_utf8(&xmtp_identity_signature_payload).unwrap()
        );
        let personal_signature_message =
            EcPrivateKey::ethereum_personal_sign_payload(&xmtp_identity_signature_payload);
        let signature_verified = EcPrivateKey::verify_wallet_signature(
            address,
            &personal_signature_message,
            &signature_proto_result,
        );
        assert!(signature_verified.is_ok());
    }

    #[test]
    fn test_recover_wallet_signature() {
        // XMTP : Create Identity
        // 08b8cff59ae3301a430a4104ac471e1ff54947e91e30a4640fe093e6dcb9ac097330b2e2506135d42980454e83bdc639ef7ae4de3debf82aa6800bdd4d1a635d0cdeeab8ed2401d64de22dde

        // For more info: https://xmtp.org/signatures/
        // digest LDK+7DM/jgDncHBEegvPq0fM9sirQXNHcuNcEPLe5E4= address 0x9DaBcF16c361493e41192BF5901DB1E4E7E7Ca30

        let hex_public_key = "08b8cff59ae3301a430a4104ac471e1ff54947e91e30a4640fe093e6dcb9ac097330b2e2506135d42980454e83bdc639ef7ae4de3debf82aa6800bdd4d1a635d0cdeeab8ed2401d64de22dde";
        let xmtp_test_message = "XMTP : Create Identity\n08b8cff59ae3301a430a4104ac471e1ff54947e91e30a4640fe093e6dcb9ac097330b2e2506135d42980454e83bdc639ef7ae4de3debf82aa6800bdd4d1a635d0cdeeab8ed2401d64de22dde\n\nFor more info: https://xmtp.org/signatures/";
        let xmtp_test_digest = "LDK+7DM/jgDncHBEegvPq0fM9sirQXNHcuNcEPLe5E4=";
        let xmtp_test_address = "0x9DaBcF16c361493e41192BF5901DB1E4E7E7Ca30";

        let xmtp_identity_signature_payload =
            ethereum_utils::EthereumUtils::xmtp_identity_key_payload(
                &hex::decode(hex_public_key).unwrap(),
            );

        assert_eq!(
            xmtp_identity_signature_payload,
            xmtp_test_message.as_bytes()
        );

        let derived_digest = EcPrivateKey::ethereum_personal_digest(xmtp_test_message.as_bytes());
        assert_eq!(xmtp_test_digest, base64::encode(&derived_digest));
        assert_eq!(
            xmtp_test_digest,
            base64::encode(hash_message(xmtp_test_message.as_bytes()))
        );
    }
}
