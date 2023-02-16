use ethers::core::rand::thread_rng;
use ethers::signers::coins_bip39::{English, Mnemonic};

use protobuf;

mod ecdh;
mod encryption;
mod ethereum_utils;
pub mod keys;
pub mod proto;
use keys::{
    key_bundle::{PrivateKeyBundle, PublicKeyBundle, SignedPublicKeyBundle},
    private_key::{PrivateKey, SignedPrivateKey},
    public_key,
};

use ecdh::{ECDHDerivable, ECDHKey};

use base64::{engine::general_purpose, Engine as _};

pub struct Keystore {
    // Private key bundle powers most operations
    private_key_bundle: Option<PrivateKeyBundle>,
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

    // == Keystore methods ==
    pub fn decrypt_v1(
        &self,
        request: proto::keystore::DecryptV1Request,
    ) -> Result<proto::keystore::DecryptResponse, String> {
        // Get the list of requests inside request
        let requests = request.requests;
        // Create a list of responses
        let mut responses = Vec::new();

        // Iterate over the requests
        for request in requests {
            let payload = request.payload;
            let peer_keys = request.peer_keys;
            let header_bytes = request.header_bytes;
            let is_sender = request.is_sender;

            let mut response = proto::keystore::decrypt_response::Response::new();

            // let decrypt_result = encryption::decrypt_v1(payload, peer_keys, header_bytes, is_sender);
            let decrypt_result = encryption::decrypt_v1(&[], &[], &[], &[], None);
            match decrypt_result {
                Ok(decrypted) => {
                    let mut success_response =
                        proto::keystore::decrypt_response::response::Success::new();
                    success_response.decrypted = decrypted;
                    response.response = Some(
                        proto::keystore::decrypt_response::response::Response::Result(
                            success_response,
                        ),
                    );
                }
                Err(e) => {
                    let mut error_response = proto::keystore::KeystoreError::new();
                    error_response.message = e;
                    error_response.code = protobuf::EnumOrUnknown::new(
                        proto::keystore::ErrorCode::ERROR_CODE_UNSPECIFIED,
                    );
                    response.response = Some(
                        proto::keystore::decrypt_response::response::Response::Error(
                            error_response,
                        ),
                    );
                }
            }
        }
        let mut response_proto = proto::keystore::DecryptResponse::new();
        response_proto.responses = responses;
        return Ok(response_proto);
    }
    // == end keystore api ==

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
    fn derive_shared_secret_x3dh(
        &self,
        peer_bundle: &SignedPublicKeyBundle,
        my_prekey: &dyn ecdh::ECDHKey,
        is_recipient: bool,
    ) -> Result<Vec<u8>, String> {
        // Check if self.private_key_bundle is set
        if self.private_key_bundle.is_none() {
            return Err("private key bundle is not set".to_string());
        }
        let private_key_bundle_ref = self.private_key_bundle.as_ref().unwrap();
        let pre_key = private_key_bundle_ref
            .find_pre_key(my_prekey.get_public_key())
            .ok_or("could not find prekey in private key bundle".to_string())?;
        let mut dh1: Vec<u8>;
        let mut dh2: Vec<u8>;
        // (STOPSHIP) TODO: better error handling
        // Get the private key bundle
        if is_recipient {
            dh1 = pre_key.shared_secret(&peer_bundle.identity_key).unwrap();
            dh2 = private_key_bundle_ref
                .identity_key
                .shared_secret(&peer_bundle.pre_key)
                .unwrap();
        } else {
            dh1 = private_key_bundle_ref
                .identity_key
                .shared_secret(&peer_bundle.pre_key)
                .unwrap();
            dh2 = pre_key.shared_secret(&peer_bundle.identity_key).unwrap();
        }
        let dh3 = pre_key.shared_secret(&peer_bundle.pre_key).unwrap();
        println!("dh1: {:?}", dh1);
        println!("dh2: {:?}", dh2);
        println!("dh3: {:?}", dh3);
        let secret = [dh1, dh2, dh3].concat();
        println!("secret: {:?}", secret);
        return Ok(secret);
    }

    // Set private identity key from protobuf bytes
    pub fn set_private_key_bundle(&mut self, private_key_bundle: &[u8]) {
        // Deserialize protobuf bytes into a SignedPrivateKey struct
        let private_key_result: protobuf::Result<proto::private_key::PrivateKeyBundle> =
            protobuf::Message::parse_from_bytes(private_key_bundle);
        if private_key_result.is_err() {
            return;
        }
        // Get the private key from the result
        let private_key = private_key_result.as_ref().unwrap();
        let private_key_bundle = private_key.v2();

        // If the deserialization was successful, set the privateIdentityKey field
        if private_key_result.is_ok() {
            self.private_key_bundle =
                Some(PrivateKeyBundle::from_proto(&private_key_bundle).unwrap());
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
        // Test 1
        let secret1 = hex::decode("aff491a0fe153a4ac86065b4b4f6953a4cb33477aa233facb94d5fb88c82778c39167f453aa0690b5358abe9e027ddca5a6185bce3699d8b2ac7efa30510a7991b").unwrap();
        let salt1 = hex::decode("e3412c112c28353088c99bd5c7350c81b1bc879b4d08ea1192ec3c03202ff337")
            .unwrap();
        let expected1 =
            hex::decode("0159d9ad511263c3754a8e2045fadc657c0016b1801720e67bbeb2661c60f176")
                .unwrap();
        let derived1_result = encryption::hkdf(&secret1, &salt1);
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
        let derived2_result = encryption::hkdf(&secret2, &salt2);
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
        let derived1_result = encryption::hkdf(&secret1, &salt1);
        // Check result
        assert!(derived1_result.is_ok());
        // Assert not equal
        assert_ne!(derived1_result.unwrap().to_vec(), expected1);
    }

    #[test]
    fn test_hkdf_invalid_key() {
        let secret1 = hex::decode("").unwrap();
        let salt1 = hex::decode("").unwrap();
        let derived1_result = encryption::hkdf(&secret1, &salt1);
        // Check result
        assert!(derived1_result.is_ok());
    }

    #[test]
    fn test_private_key_from_v2_bundle() {
        // = test vectors generated with xmtp-js =
        let private_key_bundle_raw = "EpYDCsgBCMDw7ZjWtOygFxIiCiAvph+Hg/Gk9G1g2EoW1ZDlWVH1nCkn6uRL7GBG3iNophqXAQpPCMDw7ZjWtOygFxpDCkEEeH4w/gK5HMaKu51aec/jiosmqDduIaEA67V7Lbox1cPhz9SIEi6sY/6jVQQXeIjKxzsZSVrM0LXCXjc0VkRmxhJEEkIKQNSujk9ApV5gIKltm0CFhLLuN3Xt2fjkKZBoUH/mswjTaUMTc3qZZzde3ZKMfkNVZYqns4Sn0sgopXzpjQGgjyUSyAEIwPXBtNa07KAXEiIKIOekWIyRJCelxqX+mR8i76KuDO2QV3e42nv8CxJQL0DXGpcBCk8IwPXBtNa07KAXGkMKQQTIePKpkAHxREbLbXfn6XCOwx9YqQWmqLuTHAnqRNj1q5xDLpbgkiyAORFZmVOK8iVq3dT/PWm6WMasPrqdzD7iEkQKQgpAqIj/yKx2wn8VjeWV6wm/neNDEQ6282p3CeJsPDKS56B11Nqc5Y5vUPKcrC1nB2dqBkwvop0fU49Yx4k0CB2evQ==";
        let message = "hello world!";
        let digest = "dQnlvaDHYtK6x/kNdYtbImP6Acy8VCq1498WO+CObKk=";
        let signature_proto_raw = "CkQKQAROtHwYeoBT4LhZEVM6dYaPCDDVy4/9dYSZBvKizAk7J+9f29+1OkAZoGw+FLCHWr/G9cKGfiZf3ln7bTssuIkQAQ==";
        let expected_address = "0xf4c3d5f8f04da9d5eaa7e92f7a6e7f990450c88b";
        // =====

        // For debugging, the secret key is hex encoded bigint:
        // BigInt('0x2fa61f8783f1a4f46d60d84a16d590e55951f59c2927eae44bec6046de2368a6')
        // > 21552218103791599555364469821754606161148148489927333195317013913723696539814n

        let proto_encoded = general_purpose::STANDARD
            .decode(private_key_bundle_raw)
            .unwrap();
        // Deserialize the proto bytes into proto::private_key::PrivateKeyBundleV2
        let signed_private_key: proto::private_key::PrivateKeyBundle =
            protobuf::Message::parse_from_bytes(&proto_encoded).unwrap();
        let private_key_bundle = signed_private_key.v2();

        // Decode signature proto
        let signature: proto::signature::Signature = protobuf::Message::parse_from_bytes(
            &general_purpose::STANDARD
                .decode(signature_proto_raw)
                .unwrap(),
        )
        .unwrap();
        let key_bundle_result = PrivateKeyBundle::from_proto(private_key_bundle);
        assert!(key_bundle_result.is_ok());
        let key_bundle = key_bundle_result.unwrap();
        // Do a raw byte signature verification
        let signature_verified = &key_bundle
            .identity_key
            .verify_signature(message.as_bytes(), &signature.ecdsa_compact().bytes);
        assert!(signature_verified.is_ok());
        // Calculate the eth wallet address from public key
        let eth_address = &key_bundle.identity_key.eth_address().unwrap();
        assert_eq!(eth_address, expected_address);
    }

    #[test]
    fn test_verify_wallet_signature() {
        // = test vectors generated with xmtp-js =
        let address = "0x2Fb28c95E110C6Bb188B41f9E7d6850ccbE48e61";
        let signature_proto_result: proto::signature::Signature = protobuf::Message::parse_from_bytes(&general_purpose::STANDARD.decode("EkIKQKOfb+lUwNCnJrMWQapvY1YNtFheYXa5gH5jZ+IpHPxrIAtWyvMPTMW7WpBb4Mscrie9yRap7H8XbzPPbJKEybI=").unwrap()).unwrap();
        let bytes_to_sign = general_purpose::STANDARD.decode("CIC07umj5I+hFxpDCkEEE27Yj8R97eSoWjEwE35U3pB439S9OSfdrPrDjGH9/JQ5CCb8rjFK1vxxhbHGM2bq1v0PXdk6k/tkbhXmn2WEmw==").unwrap();
        // Encode string as bytes
        let xmtp_identity_signature_payload =
            ethereum_utils::EthereumUtils::xmtp_identity_key_payload(&bytes_to_sign);
        println!(
            "xmtp_identity_signature_payload: {:?}",
            std::str::from_utf8(&xmtp_identity_signature_payload).unwrap()
        );
        let personal_signature_message =
            SignedPrivateKey::ethereum_personal_sign_payload(&xmtp_identity_signature_payload);
        let signature_verified = SignedPrivateKey::verify_wallet_signature(
            address,
            &personal_signature_message,
            &signature_proto_result,
        );
        assert!(signature_verified.is_ok());
    }

    #[test]
    fn test_recover_wallet_signature() {
        // = test vectors generated with xmtp-js =
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

        let derived_digest =
            SignedPrivateKey::ethereum_personal_digest(xmtp_test_message.as_bytes());
        assert_eq!(
            xmtp_test_digest,
            general_purpose::STANDARD.encode(&derived_digest)
        );
    }

    #[test]
    fn test_simple_decryption() {
        let secret_hex = "7ce6121ed4756aaf8dd0b116ceb7f44ab2f11d4f4caf5924e4bd070353739e6a3c8b039cde75edc2134c7ff76bca5d7ade3fe59bd791f3e73edc97e188c1e4521c";
        let ciphertext_hex = "0ace030a208b2d6b2957ad0fa3fa0ec298c8b4e2308cc6015d50fd40f429450f8bc54dbd35120c37f568081c6b294c36a6b3b71a9b031da6127e0e33bad3d84b1803894d532ea27d8ab3b77d605d46395fcf55c7b49805ee39b8fab9207e324f9a5c326b7807075a131f7c60589291758c1993ac3b1ed5a4bb35e2300093f6fe7ac2abf6f83e3eb08e00e65f0de2d78fedeb693b8b5749b010f068078e1c7be2e4b307ff463d4605dc1427f96ef2262a0e4ad613e87f9d719597b9129517b4fc3e1f1ff95a264d18bc266f8f1f894649508d91f8619e35279cb3879ede9475a528fed2428a878d9f500da9eccadfb2b988c09eed9d6ba2cf6fe40e3730bf7cbec930c2ad5263df7c671e4f8baeeab9e9b45b35f8c4bce74de59009fab8739228eed987b31ce31ff6cbdd688c2055ba3b919b205c59c3b3240d15dc4b527e3b3ebb3ebccb05130e6b42ec80e7b9b49f0d46baf5ae55d1dc5b734c2dee798da6cd6656ba90113fdd0a27aebdb6fbd7de66b0cffbe912d1d9e27b22e77ca8eb13f82bfd2b3adfd8e59c46f115a49727fd1a104d8010ed248bfac0e23632a9b5120fb385c25ff8e76d715df1bc02e6534f2792209796b60c070c4997bfe6aa49f934c8b042624a0377e3ef495c50510f63b934";
        let plaintext_hex = "0a88030ac00108b08b90bfe53012220a20b1d1ae465df4258351c462ea592723753a366263146c69120b4901e4c7a56c8b1a920108b08b90bfe53012440a420a401051d42da81190bbbe080f0cef3356cb476ecf87b112b22a4623f1d22ac358fa08a6160720051acf6ac651335c9114a052a7885ecfaf7c9725f9700075ac22b11a430a41046520443dc4358499e8f0269567bcc27d7264771de694eb84d5c5334e152ede227f3a1606b6dd47129d7c999a6655855cb02dc2b32ee9bf02c01578277dd4ddeb12c20108d88b90bfe53012220a20744cabc19d4d84d9753eed7091bc3047d2e46578cce75193add548f530c7f1d31a940108d88b90bfe53012460a440a409e12294d043420f762ed24e7d21f26328f0f787a964d07f7ebf288f2ab9f750b76b820339ff8cffd4be83adf7177fd29265c4479bf9ab4dc8ed9e5af399a9fab10011a430a4104e0f94416fc0431050a7f4561f8dfdd89e23d24c1d05c50710ef0524316a3bd5ed938c0f111133348fc2aeff399838ce3bd8505182e8582efc6beda0d5144330f";

        // protobuf deseriaize the ciphertext
        let ciphertext_result: proto::ciphertext::Ciphertext =
            protobuf::Message::parse_from_bytes(&hex::decode(ciphertext_hex).unwrap()).unwrap();
        let aes_ciphertext = ciphertext_result.aes256_gcm_hkdf_sha256();
        assert_eq!(aes_ciphertext.gcm_nonce.len(), 12);
        assert_eq!(aes_ciphertext.hkdf_salt.len(), 32);
        assert_eq!(aes_ciphertext.payload.len(), 411);

        // Invoke decrypt_v1 on the ciphertext
        let decrypt_result = encryption::decrypt_v1(
            aes_ciphertext.payload.as_slice(),
            aes_ciphertext.hkdf_salt.as_slice(),
            aes_ciphertext.gcm_nonce.as_slice(),
            hex::decode(secret_hex).unwrap().as_slice(),
            None,
        );

        assert!(decrypt_result.is_ok());
        assert_eq!(hex::encode(decrypt_result.unwrap()), plaintext_hex);
    }

    #[test]
    fn test_x3dh_derivation_selfdh() {
        /*
        peer bundle:  CpIBCNPvn83lMBJECkIKQFksbby30voQCCvTeqXmVf/OPstIMEX+r1lNBWeYa/KGKhXOIpBc/8LOgtqUOptHqLJjs6HWaBCHCfMDxo7R0t0aQwpBBEYsIllTStVEwPth0ukovlbJriqI9OhnqnZU0oyuOJUGWXO4DLlf0r4cAyO7ftTRXRa5TgmFD3KVC2BraNfAhRQSkgEI9++fzeUwEkQKQgpAfsSooQ6ENgHu6JpF7oOPQon80nAcFj83Ygwoqh6oRAJB0KGdX90909hYUT7XRSpYbsQfRWHFx3lCNMHacX/ZKBpDCkEERb2fdceKy9jOmbbMvkWR4a/CWVgDiu652qTaC6ZT7szfJvFxYy06FkS7w+UEqRWXEwuvZxyDNxQW5nseW+UKrQ==
        myPreKey:  CPfvn83lMBJECkIKQH7EqKEOhDYB7uiaRe6Dj0KJ/NJwHBY/N2IMKKoeqEQCQdChnV/dPdPYWFE+10UqWG7EH0Vhxcd5QjTB2nF/2SgaQwpBBEW9n3XHisvYzpm2zL5FkeGvwllYA4ruudqk2gumU+7M3ybxcWMtOhZEu8PlBKkVlxMLr2ccgzcUFuZ7HlvlCq0=
        my identity key:  CNPvn83lMBIiCiBPQL5ryv37t976or91HfmJPKKWiPtMf9NUp7CXZrEcTxqSAQjT75/N5TASRApCCkBZLG28t9L6EAgr03ql5lX/zj7LSDBF/q9ZTQVnmGvyhioVziKQXP/CzoLalDqbR6iyY7Oh1mgQhwnzA8aO0dLdGkMKQQRGLCJZU0rVRMD7YdLpKL5Wya4qiPToZ6p2VNKMrjiVBllzuAy5X9K+HAMju37U0V0WuU4JhQ9ylQtga2jXwIUU
        is recipient:  false
        preKey:  CPfvn83lMBIiCiA2WyXGP4085Xq+TAYNbO6AT0i3YjKttVQzX3VnRjV8LhqSAQj375/N5TASRApCCkB+xKihDoQ2Ae7omkXug49CifzScBwWPzdiDCiqHqhEAkHQoZ1f3T3T2FhRPtdFKlhuxB9FYcXHeUI0wdpxf9koGkMKQQRFvZ91x4rL2M6Ztsy+RZHhr8JZWAOK7rnapNoLplPuzN8m8XFjLToWRLvD5QSpFZcTC69nHIM3FBbmex5b5Qqt
        dh1:  BKegRppINyzve/I8BWnHK3ERJNKDUaFjiPqjOF+aZWNyTkxpudyU/5Sbl4mcPOnU8xWz6dbYudqThzI6cIhbGNY=
        dh2:  BKegRppINyzve/I8BWnHK3ERJNKDUaFjiPqjOF+aZWNyTkxpudyU/5Sbl4mcPOnU8xWz6dbYudqThzI6cIhbGNY=
        dh3:  BBrRQblghj7QkSn23w0+blhVP14HZT1TEwchKH9aDRX4jN37jAU8JxgBmHCA3AzATPnj9XaRHtrvwRYIIjlRRdY=
        secret:  BKegRppINyzve/I8BWnHK3ERJNKDUaFjiPqjOF+aZWNyTkxpudyU/5Sbl4mcPOnU8xWz6dbYudqThzI6cIhbGNYEp6BGmkg3LO978jwFaccrcREk0oNRoWOI+qM4X5plY3JOTGm53JT/lJuXiZw86dTzFbPp1ti52pOHMjpwiFsY1gQa0UG5YIY+0JEp9t8NPm5YVT9eB2U9UxMHISh/Wg0V+Izd+4wFPCcYAZhwgNwMwEz54/V2kR7a78EWCCI5UUXW
        */
        let peer_bundle = "CkwI8oazzOUwGkMKQQT8vluBL7Arf+QabYmDKAMRa/BWbfWlz3sYEn0GDrWDDkPvL3MkKDzHLa7xJlVq1HdFKrjlKz9yN1WMS8bTCQUREpQBCPKGs8zlMBJGCkQKQLxyHFyDM9afxnvVjqrKI8E06gQSpPmPfURLkdi+MoxYHXcXSO1OlEe6r4uulj7axV3aUeivTdUyolpy0AJy34sQARpDCkEEPE4ep8JPcEP3tnoEMPOr7s+bKDEEdPN4JevV8RPE2SuDPi2wXh6gOmOfR5WmffUG3NcKenmfecfpdR/fllAmfg==";
        let my_pre_key_public = "CPOGs8zlMBJGCkQKQDPOq6PTquoSZAuJBZLOAf0ofiESLMi87Sq8EJDpm16PBg3RIk2TSX+e8A4mQDovOxwznS40QKNBHoIvmpEEgOMQARpDCkEEpuULoHF8aFkqJde7amRXcmV7t7AEfuW6VtGnFyEy7sul26D8bPoi5ofwUwWf/eDtCeOQmJzLEMXue6VgAAgxTA==";
        let is_recipient = true;
        let pre_key_private = "CPOGs8zlMBIiCiC2PODgCoZ7vuYOkLUAVOL1btJ3VSrwRU2A6XsrqeZIcRqUAQjzhrPM5TASRgpECkAzzquj06rqEmQLiQWSzgH9KH4hEizIvO0qvBCQ6ZtejwYN0SJNk0l/nvAOJkA6LzscM50uNECjQR6CL5qRBIDjEAEaQwpBBKblC6BxfGhZKiXXu2pkV3Jle7ewBH7lulbRpxchMu7Lpdug/Gz6IuaH8FMFn/3g7QnjkJicyxDF7nulYAAIMUw=";
        let dh1 = "BDxAgPoi9aqd75DwbRr564ZfHfLuDqmkzXuX4lGuGEX71/P7r+zx6H6oYb/euA9Mi1TfzO0zB8by9IVhx31BZow=";
        let dh2 = "BEVk8yLoeYqFoQUtZzr8loAxZH9/PRoiZsUZ48qDtu7QSKYgBAAB5zrff3VhCG8RvbOv7B662AWLGxbFvGZa0ms=";
        let dh3 = "BE+a50DRnTzMPArqdG+9Jca8JEOWzOO3VqoTtFGr8a83iLEG+Qhoj49JerXXbeLP7jrHPZQgXWwOqhr/gAK1oTw=";
        let secret = "BDxAgPoi9aqd75DwbRr564ZfHfLuDqmkzXuX4lGuGEX71/P7r+zx6H6oYb/euA9Mi1TfzO0zB8by9IVhx31BZowERWTzIuh5ioWhBS1nOvyWgDFkf389GiJmxRnjyoO27tBIpiAEAAHnOt9/dWEIbxG9s6/sHrrYBYsbFsW8ZlrSawRPmudA0Z08zDwK6nRvvSXGvCRDlszjt1aqE7RRq/GvN4ixBvkIaI+PSXq1123iz+46xz2UIF1sDqoa/4ACtaE8";
    }

    #[test]
    fn test_x3dh_simple() {
        let peer_bundle =  "CpQBCkwIs46U3eUwGkMKQQSp/qE9WdVygIo8+sb45OtE43s68RCqPz+RikceMh+FLuvPp1FcpNiLqURwSrL0o1p/T4HmG4qHn2Mk0lPZqKIBEkQSQgpA416oJdOWzEAQzGiKgDt9ejOkZAtCJ0EN3b2LyapXv+wZPfTlQSI95Db3tTWb/xz1vO/Of3tHDQ0L4bRIqgTVrhKUAQpMCNWOlN3lMBpDCkEEzR0hsrKL6oZeOAabEo3LDYycTjnZ6HSns5Tl9vg3RQ1iEWLrd0GQ4IN8CwwDlGWRUDqcUZNKmqOVXiicDEATuBJECkIKQJiZjxTenDCM/0dMFvqz0d9g2iyGFOM10mi/jaDSxpdUMYm2ZMyNEh94Jq1kYUpptcixuTtb528dnDKlax8B1SE=";
        let my_pre_key_public =  "CkwIy4yU3eUwGkMKQQRibzecVrKk6rgCPNSPyybJib3lKBk1GrI8r/v1yHXcoVuhtmOKffZcoZ3yYl7R1q8+kx61GhwgBQtihzlDyGrKEkQKQgpALqg2w0lg9uhGApJMtgtKrW5qxNgYDNL2BwvnYCHsE15fu9KOdKq0kYKy9TSL9T0Ue0rCYwonA/Qr6lhnFmbh1A==";
        let my_identity_bundle =  "EpADCsUBCMCvgomt/JiiFxIiCiA8iMJ0t2Kc+ilGyAIDtnQOgeQ19RNQzuZuj3J29d+iPxqUAQpMCKeMlN3lMBpDCkEEtrRkcEuQsvY3c6Hwbpyuzk8lbsZK7YgsxSAdmrWft1DM38oM/rrDswhqKUbrMKobt/lN7ShP5JQV+Q2ypvks0RJEEkIKQJgwindCu1V5K46WxWiibrdqodLii2rxgIF/qbSNVREacZ2GSonzXMOlHTMTTo4sy6nw9W1iwAfukqElUZy7J9QSxQEIwNGXmq38mKIXEiIKIF6tvfEObqASql4MbqwWwdvcB1AtHbx6km21Tk6VwCX5GpQBCkwIy4yU3eUwGkMKQQRibzecVrKk6rgCPNSPyybJib3lKBk1GrI8r/v1yHXcoVuhtmOKffZcoZ3yYl7R1q8+kx61GhwgBQtihzlDyGrKEkQKQgpALqg2w0lg9uhGApJMtgtKrW5qxNgYDNL2BwvnYCHsE15fu9KOdKq0kYKy9TSL9T0Ue0rCYwonA/Qr6lhnFmbh1A==";
        let is_recipient = false;
        let pre_key_private =  "CMDRl5qt/JiiFxIiCiBerb3xDm6gEqpeDG6sFsHb3AdQLR28epJttU5OlcAl+RqUAQpMCMuMlN3lMBpDCkEEYm83nFaypOq4AjzUj8smyYm95SgZNRqyPK/79ch13KFbobZjin32XKGd8mJe0davPpMetRocIAULYoc5Q8hqyhJECkIKQC6oNsNJYPboRgKSTLYLSq1uasTYGAzS9gcL52Ah7BNeX7vSjnSqtJGCsvU0i/U9FHtKwmMKJwP0K+pYZxZm4dQ=";
        let dh1 =  "BNOBBknXpaz9LWs2izeKYFAh3KRS8a7Mibefi38yhyunt3stLHjgvSYPWScBQ4E9VlzTFzOKzR2mnyYhAYrUDSg=";
        let dh2 =  "BAitvQQvKnk7LrBFmVWbKNwAvYxQ11fk+zspePqfqXRzF4Xq+UuMQISSzRKnxnQS59WON+nFZKaSxf5EI6hPA08=";
        let dh3 =  "BNPEbxWF4PLLqy/08Udk3ULRPrEnTUcfrJUx5ksg6RLXd+JuOFnZJQ0NgD7+a4zIH0Ce4bxQ3cw04YtBI+Rxbc4=";
        let secret =  "BNOBBknXpaz9LWs2izeKYFAh3KRS8a7Mibefi38yhyunt3stLHjgvSYPWScBQ4E9VlzTFzOKzR2mnyYhAYrUDSgECK29BC8qeTsusEWZVZso3AC9jFDXV+T7Oyl4+p+pdHMXher5S4xAhJLNEqfGdBLn1Y436cVkppLF/kQjqE8DTwTTxG8VheDyy6sv9PFHZN1C0T6xJ01HH6yVMeZLIOkS13fibjhZ2SUNDYA+/muMyB9AnuG8UN3MNOGLQSPkcW3O";

        let mut x = Keystore::new();
        x.set_private_key_bundle(
            &general_purpose::STANDARD
                .decode(my_identity_bundle)
                .unwrap(),
        );

        let peer_bundle_proto: proto::public_key::SignedPublicKeyBundle =
            protobuf::Message::parse_from_bytes(
                &general_purpose::STANDARD.decode(peer_bundle).unwrap(),
            )
            .unwrap();
        println!("peer_bundle_proto: {:?}", peer_bundle_proto);
        let peer_bundle_object = SignedPublicKeyBundle::from_proto(&peer_bundle_proto).unwrap();

        let pre_key_proto: proto::public_key::SignedPublicKey =
            protobuf::Message::parse_from_bytes(
                &general_purpose::STANDARD.decode(my_pre_key_public).unwrap(),
            )
            .unwrap();
        let pre_key_object = public_key::signed_public_key_from_proto(&pre_key_proto).unwrap();

        // Do a x3dh shared secret derivation
        let shared_secret_result =
            x.derive_shared_secret_x3dh(&peer_bundle_object, &pre_key_object, is_recipient);
        assert!(shared_secret_result.is_ok());
        let shared_secret = shared_secret_result.unwrap();
        assert_eq!(
            shared_secret,
            general_purpose::STANDARD.decode(secret).unwrap()
        );
    }
}
