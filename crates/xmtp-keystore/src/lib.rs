use std::collections::HashMap;

use protobuf;

mod conversation;
mod ecdh;
mod encryption;
mod ethereum_utils;
pub mod keys;
pub mod proto;
use keys::{
    key_bundle::{PrivateKeyBundle, SignedPublicKeyBundle},
    private_key::SignedPrivateKey,
    public_key,
};

use conversation::{InvitationContext, TopicData};

use base64::{engine::general_purpose, Engine as _};
use ecdh::{ECDHDerivable, ECDHKey};

pub struct Keystore {
    // Private key bundle powers most operations
    private_key_bundle: Option<PrivateKeyBundle>,
    // Topic Keys
    topic_keys: HashMap<String, TopicData>,
}

impl Keystore {
    // new() is a constructor for the Keystore struct
    pub fn new() -> Self {
        Keystore {
            // Empty option for private key bundle
            private_key_bundle: None,
            // Topic keys
            topic_keys: HashMap::new(),
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
        let responses = Vec::new();

        // Iterate over the requests
        for request in requests {
            let _payload = request.payload;
            let _peer_keys = request.peer_keys;
            let _header_bytes = request.header_bytes;
            let _is_sender = request.is_sender;

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

    // XMTP X3DH-like scheme for invitation decryption
    fn derive_shared_secret_xmtp(
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
        let dh1: Vec<u8>;
        let dh2: Vec<u8>;
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
        let secret = [dh1, dh2, dh3].concat();
        return Ok(secret);
    }

    // Set private identity key from protobuf bytes
    pub fn set_private_key_bundle(&mut self, private_key_bundle: &[u8]) -> Result<(), String> {
        // Deserialize protobuf bytes into a SignedPrivateKey struct
        let private_key_result: protobuf::Result<proto::private_key::PrivateKeyBundle> =
            protobuf::Message::parse_from_bytes(private_key_bundle);
        if private_key_result.is_err() {
            return Err("could not parse private key bundle".to_string());
        }
        // Get the private key from the result
        let private_key = private_key_result.as_ref().unwrap();
        let private_key_bundle = private_key.v2();

        // If the deserialization was successful, set the privateIdentityKey field
        if private_key_result.is_ok() {
            self.private_key_bundle =
                Some(PrivateKeyBundle::from_proto(&private_key_bundle).unwrap());
            return Ok(());
        } else {
            return Err("could not parse private key bundle".to_string());
        }
    }

    // Save invites
    pub fn save_invitation(&mut self, sealed_invitation_bytes: &[u8]) -> Result<bool, String> {
        // Deserialize invitation bytes into a protobuf::invitation::InvitationV1 struct
        let invitation_result: protobuf::Result<proto::invitation::SealedInvitation> =
            protobuf::Message::parse_from_bytes(sealed_invitation_bytes);
        if invitation_result.is_err() {
            return Err("could not parse invitation".to_string());
        }
        // Get the invitation from the result
        let sealed_invitation = invitation_result.as_ref().unwrap();
        let invitation = sealed_invitation.v1();

        // Need to parse the header_bytes as protobuf::invitation::SealedInvitationHeaderV1
        let header_result: protobuf::Result<proto::invitation::SealedInvitationHeaderV1> =
            protobuf::Message::parse_from_bytes(&invitation.header_bytes);
        if header_result.is_err() {
            return Err("could not parse invitation header".to_string());
        }
        // Get the invitation header from the result
        let invitation_header = header_result.as_ref().unwrap();

        // Check the header time from the sealed invite
        // TODO: check header time from the sealed invite
        let header_time = invitation_header.created_ns;

        // Attempt to decrypt the invitation
        let decrypt_result = self.unseal_invitation(&invitation, &invitation_header);
        if decrypt_result.is_err() {
            return Err("could not decrypt invitation".to_string());
        }
        // Get the decrypted invitation from the result
        let decrypted_invitation = decrypt_result.unwrap();

        // Encryption field should contain the key bytes
        let key_bytes = decrypted_invitation
            .aes256_gcm_hkdf_sha256()
            .key_material
            .as_slice();

        // Context field should contain conversationId
        let conversation_id = &decrypted_invitation.context.conversation_id;
        let mut context_fields = HashMap::new();
        // Iterate through metadata map and add to context_fields
        for key in decrypted_invitation.context.metadata.keys() {
            context_fields.insert(
                key.to_string(),
                decrypted_invitation.context.metadata[key].to_string(),
            );
        }

        // TODO: process additional metadata here
        let topic = &decrypted_invitation.topic;

        self.topic_keys.insert(
            conversation_id.clone(),
            TopicData {
                key: key_bytes.to_vec(),
                context: Some(InvitationContext {
                    conversation_id: conversation_id.to_string(),
                    metadata: context_fields,
                }),
                created: header_time,
            },
        );

        return Ok(true);
    }

    fn unseal_invitation(
        &self,
        sealed_invitation: &proto::invitation::SealedInvitationV1,
        sealed_invitation_header: &proto::invitation::SealedInvitationHeaderV1,
    ) -> Result<proto::invitation::InvitationV1, String> {
        // Parse public key bundles from sealed_invitation header
        let sender_public_key_bundle =
            SignedPublicKeyBundle::from_proto(&sealed_invitation_header.sender).unwrap();
        let recipient_public_key_bundle =
            SignedPublicKeyBundle::from_proto(&sealed_invitation_header.recipient).unwrap();

        let secret: Vec<u8>;
        // reference our own identity key
        let viewer_identity_key = &self.private_key_bundle.as_ref().unwrap().identity_key;
        if viewer_identity_key.public_key == sender_public_key_bundle.identity_key {
            let secret_result = self.derive_shared_secret_xmtp(
                &recipient_public_key_bundle,
                &sender_public_key_bundle.pre_key,
                false,
            );
            if secret_result.is_err() {
                return Err("could not derive shared secret".to_string());
            }
            secret = secret_result.unwrap();
        } else {
            let secret_result = self.derive_shared_secret_xmtp(
                &sender_public_key_bundle,
                &recipient_public_key_bundle.pre_key,
                true,
            );
            if secret_result.is_err() {
                return Err("could not derive shared secret".to_string());
            }
            secret = secret_result.unwrap();
        }

        // Unwrap ciphertext
        let ciphertext = sealed_invitation.ciphertext.aes256_gcm_hkdf_sha256();

        let hkdf_salt = &ciphertext.hkdf_salt;
        let gcm_nonce = &ciphertext.gcm_nonce;
        let payload = &ciphertext.payload;

        // Try decrypting the invitation
        let decrypt_result = encryption::decrypt_v1_with_associated_data(
            payload,
            hkdf_salt,
            gcm_nonce,
            &secret,
            &sealed_invitation.header_bytes,
        );
        if decrypt_result.is_err() {
            return Err("could not decrypt invitation".to_string());
        }
        let decrypted_bytes = decrypt_result.unwrap();

        // Deserialize invitation bytes into a protobuf::invitation::InvitationV1 struct
        let invitation_result: protobuf::Result<proto::invitation::InvitationV1> =
            protobuf::Message::parse_from_bytes(&decrypted_bytes);
        if invitation_result.is_err() {
            return Err("could not parse invitation from decrypted bytes".to_string());
        }
        // Get the invitation from the result
        let invitation = invitation_result.as_ref().unwrap();
        return Ok(invitation.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_xmtp_x3dh_simple() {
        let peer_bundle =  "CpQBCkwIs46U3eUwGkMKQQSp/qE9WdVygIo8+sb45OtE43s68RCqPz+RikceMh+FLuvPp1FcpNiLqURwSrL0o1p/T4HmG4qHn2Mk0lPZqKIBEkQSQgpA416oJdOWzEAQzGiKgDt9ejOkZAtCJ0EN3b2LyapXv+wZPfTlQSI95Db3tTWb/xz1vO/Of3tHDQ0L4bRIqgTVrhKUAQpMCNWOlN3lMBpDCkEEzR0hsrKL6oZeOAabEo3LDYycTjnZ6HSns5Tl9vg3RQ1iEWLrd0GQ4IN8CwwDlGWRUDqcUZNKmqOVXiicDEATuBJECkIKQJiZjxTenDCM/0dMFvqz0d9g2iyGFOM10mi/jaDSxpdUMYm2ZMyNEh94Jq1kYUpptcixuTtb528dnDKlax8B1SE=";
        let my_pre_key_public =  "CkwIy4yU3eUwGkMKQQRibzecVrKk6rgCPNSPyybJib3lKBk1GrI8r/v1yHXcoVuhtmOKffZcoZ3yYl7R1q8+kx61GhwgBQtihzlDyGrKEkQKQgpALqg2w0lg9uhGApJMtgtKrW5qxNgYDNL2BwvnYCHsE15fu9KOdKq0kYKy9TSL9T0Ue0rCYwonA/Qr6lhnFmbh1A==";
        let my_identity_bundle =  "EpADCsUBCMCvgomt/JiiFxIiCiA8iMJ0t2Kc+ilGyAIDtnQOgeQ19RNQzuZuj3J29d+iPxqUAQpMCKeMlN3lMBpDCkEEtrRkcEuQsvY3c6Hwbpyuzk8lbsZK7YgsxSAdmrWft1DM38oM/rrDswhqKUbrMKobt/lN7ShP5JQV+Q2ypvks0RJEEkIKQJgwindCu1V5K46WxWiibrdqodLii2rxgIF/qbSNVREacZ2GSonzXMOlHTMTTo4sy6nw9W1iwAfukqElUZy7J9QSxQEIwNGXmq38mKIXEiIKIF6tvfEObqASql4MbqwWwdvcB1AtHbx6km21Tk6VwCX5GpQBCkwIy4yU3eUwGkMKQQRibzecVrKk6rgCPNSPyybJib3lKBk1GrI8r/v1yHXcoVuhtmOKffZcoZ3yYl7R1q8+kx61GhwgBQtihzlDyGrKEkQKQgpALqg2w0lg9uhGApJMtgtKrW5qxNgYDNL2BwvnYCHsE15fu9KOdKq0kYKy9TSL9T0Ue0rCYwonA/Qr6lhnFmbh1A==";
        let is_recipient = false;
        let pre_key_private =  "CMDRl5qt/JiiFxIiCiBerb3xDm6gEqpeDG6sFsHb3AdQLR28epJttU5OlcAl+RqUAQpMCMuMlN3lMBpDCkEEYm83nFaypOq4AjzUj8smyYm95SgZNRqyPK/79ch13KFbobZjin32XKGd8mJe0davPpMetRocIAULYoc5Q8hqyhJECkIKQC6oNsNJYPboRgKSTLYLSq1uasTYGAzS9gcL52Ah7BNeX7vSjnSqtJGCsvU0i/U9FHtKwmMKJwP0K+pYZxZm4dQ=";
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
        let peer_bundle_object = SignedPublicKeyBundle::from_proto(&peer_bundle_proto).unwrap();

        let pre_key_proto: proto::public_key::SignedPublicKey =
            protobuf::Message::parse_from_bytes(
                &general_purpose::STANDARD.decode(my_pre_key_public).unwrap(),
            )
            .unwrap();
        let pre_key_object = public_key::signed_public_key_from_proto(&pre_key_proto).unwrap();

        // Do a x3dh shared secret derivation
        let shared_secret_result =
            x.derive_shared_secret_xmtp(&peer_bundle_object, &pre_key_object, is_recipient);
        assert!(shared_secret_result.is_ok());
        let shared_secret = shared_secret_result.unwrap();
        assert_eq!(
            shared_secret,
            general_purpose::STANDARD.decode(secret).unwrap()
        );
    }

    #[test]
    fn test_decrypt_invite() {
        // New invitation serialized: CtgGCvgECrQCCpcBCk8IgIez7pm8kqMXGkMKQQRhgSMw1R/8S7WEkfxpknxsmt9WK1AsYKrQ/ZAVwSx60+kgce8Hu+pkRy0a+rXeltBuTH4tO4pQzBP5xhHo5UFAEkQSQgpA9uKlM/f9TiNbBEsjxJusMky21MV77annlu4v34hQ6QIz8hhoHa15qmuNZceGzd00SQ6IjVzl0t4Y2sAUF+BgcxKXAQpPCMCHyomavJKjFxpDCkEE1OvlXZeGc7Ca4UUL89jZe/nvAjXTGNODpwtbuSWGehAjfins9KScHNmIcqahYrMlrqurOfhYSGvNwOkQLSNuuBJECkIKQBx9fFP8pMoyxYOU+fXN+bbf0y2eZZx8FiRgy2kWQJUfNVFX1ut+PkrxIoy20tCFieWWW5cG7zGq7345tBtotGsStAIKlwEKTwiA/+ajmrySoxcaQwpBBEzal8mv0MYw04DqoFJK8RRJmrKDXPRmPN50RFOiF1gIW1M1i26WqT+n1Te2nU5hve7c7RQ2/2aPSuMsbWeeK1MSRBJCCkDWVrt1G16A83mNhT1y1gXDLX9HVwk5Rqab/I0VjS0cr2gUqX2l3O0HXirWyFlCqua4HfPDlpB4WY4ANDTxgajAEpcBCk8IwKW5tZq8kqMXGkMKQQRX0e1CP6Jc5kbjtXF1oxgbFciNSt002UlP4ZS6vDmkCYvQyclEtY3TQcrBXSNNK2JbDwu30+1z+h6DqasrMJc6EkQKQgpAw2rkuwL7e0s3XrrtY6+YhEMmh2nijAMFQKXPFa8edKE1LfMqp0IAGhYXBiGlV7A7yPZDXLLasf11Uy4ww2WiyxiAqva1mrySoxcS2gEK1wEKIPl1rD6K3Oj8Ps+zIzfp+n2/hUKqE/ORkHOsZ8kJpIFtEgwb7/dw52hTPD37IsYapAGAJTWRotzIHUtMu1bLd7izktJOh3cJ+ZXODtho02lsNp6DuwNIoEXesdoFRtVZCYqvaiOwnctX+nnPsSfemDmQ1mJ/o4sZyvFAF25ufSBaBqRJeyQjUBbfyuJSWYoDiqAAAMzsWPzrPeVJZFXrcOdDSTA11b+MevlfzcFjitqv/0J2j+pcQo4RFOgtpFK9cUkbcIB2xjRBRXOUQL89BuyMQmb+gg==
        // ALICE PRIVATE KEY BUNDLE ENCODED BYTES: EpYDCsgBCICHs+6ZvJKjFxIiCiCNtoFf4wgcj3UH5Nhy6vHD94+HbVWUAdYlQ9IYGMv5tBqXAQpPCICHs+6ZvJKjFxpDCkEEYYEjMNUf/Eu1hJH8aZJ8bJrfVitQLGCq0P2QFcEsetPpIHHvB7vqZEctGvq13pbQbkx+LTuKUMwT+cYR6OVBQBJEEkIKQPbipTP3/U4jWwRLI8SbrDJMttTFe+2p55buL9+IUOkCM/IYaB2teaprjWXHhs3dNEkOiI1c5dLeGNrAFBfgYHMSyAEIwIfKiZq8kqMXEiIKIAOcJgVnEPy1OPad9KytYnvN+X67I33mqVKlHMqU9qsZGpcBCk8IwIfKiZq8kqMXGkMKQQTU6+Vdl4ZzsJrhRQvz2Nl7+e8CNdMY04OnC1u5JYZ6ECN+Kez0pJwc2YhypqFisyWuq6s5+FhIa83A6RAtI264EkQKQgpAHH18U/ykyjLFg5T59c35tt/TLZ5lnHwWJGDLaRZAlR81UVfW634+SvEijLbS0IWJ5ZZblwbvMarvfjm0G2i0aw==
        // BOB PRIVATE KEY BUNDLE ENCODED BYTES: EpYDCsgBCID/5qOavJKjFxIiCiAEu89bIFnCDu1NvDUnPrcW/QwVoBD3MBkDmSW8JCb6gxqXAQpPCID/5qOavJKjFxpDCkEETNqXya/QxjDTgOqgUkrxFEmasoNc9GY83nREU6IXWAhbUzWLbpapP6fVN7adTmG97tztFDb/Zo9K4yxtZ54rUxJEEkIKQNZWu3UbXoDzeY2FPXLWBcMtf0dXCTlGppv8jRWNLRyvaBSpfaXc7QdeKtbIWUKq5rgd88OWkHhZjgA0NPGBqMASyAEIwKW5tZq8kqMXEiIKIMoytCr53r3f/k9Wae/QPdGdPWsAPSLQWFwVez5K8ZGxGpcBCk8IwKW5tZq8kqMXGkMKQQRX0e1CP6Jc5kbjtXF1oxgbFciNSt002UlP4ZS6vDmkCYvQyclEtY3TQcrBXSNNK2JbDwu30+1z+h6DqasrMJc6EkQKQgpAw2rkuwL7e0s3XrrtY6+YhEMmh2nijAMFQKXPFa8edKE1LfMqp0IAGhYXBiGlV7A7yPZDXLLasf11Uy4ww2Wiyw==
        // ALICE INVITE BYTES base64: Cm4yMTAsODYsMTk5LDIsMjM5LDI0Nyw1MSwyMDgsMjA1LDE5NywzMiwxNjIsMjE1LDExMCwxODUsNywxMTUsNzMsNywyMjMsNSwxMCw3NSwxOSwyNTIsMTYwLDEzOSwyNDEsNCwyMDUsMTI4LDE1MhoiCiCkJkTKfSCTBOs0M6V7BUZMdi6pd1bO/2z5YpRYK10UKQ==
        // BOB public key bundle: CpcBCk8IgP/mo5q8kqMXGkMKQQRM2pfJr9DGMNOA6qBSSvEUSZqyg1z0ZjzedERTohdYCFtTNYtulqk/p9U3tp1OYb3u3O0UNv9mj0rjLG1nnitTEkQSQgpA1la7dRtegPN5jYU9ctYFwy1/R1cJOUamm/yNFY0tHK9oFKl9pdztB14q1shZQqrmuB3zw5aQeFmOADQ08YGowBKXAQpPCMClubWavJKjFxpDCkEEV9HtQj+iXOZG47VxdaMYGxXIjUrdNNlJT+GUurw5pAmL0MnJRLWN00HKwV0jTStiWw8Lt9Ptc/oeg6mrKzCXOhJECkIKQMNq5LsC+3tLN1667WOvmIRDJodp4owDBUClzxWvHnShNS3zKqdCABoWFwYhpVewO8j2Q1yy2rH9dVMuMMNloss=
        // ALICE public key bundle: CpcBCk8IgIez7pm8kqMXGkMKQQRhgSMw1R/8S7WEkfxpknxsmt9WK1AsYKrQ/ZAVwSx60+kgce8Hu+pkRy0a+rXeltBuTH4tO4pQzBP5xhHo5UFAEkQSQgpA9uKlM/f9TiNbBEsjxJusMky21MV77annlu4v34hQ6QIz8hhoHa15qmuNZceGzd00SQ6IjVzl0t4Y2sAUF+BgcxKXAQpPCMCHyomavJKjFxpDCkEE1OvlXZeGc7Ca4UUL89jZe/nvAjXTGNODpwtbuSWGehAjfins9KScHNmIcqahYrMlrqurOfhYSGvNwOkQLSNuuBJECkIKQBx9fFP8pMoyxYOU+fXN+bbf0y2eZZx8FiRgy2kWQJUfNVFX1ut+PkrxIoy20tCFieWWW5cG7zGq7345tBtotGs=
        // Expected key material bytes: pCZEyn0gkwTrNDOlewVGTHYuqXdWzv9s+WKUWCtdFCk=
        // Expected topic 210,86,199,2,239,247,51,208,205,197,32,162,215,110,185,7,115,73,7,223,5,10,75,19,252,160,139,241,4,205,128,152
        let alice_private_b64 = "EpYDCsgBCICHs+6ZvJKjFxIiCiCNtoFf4wgcj3UH5Nhy6vHD94+HbVWUAdYlQ9IYGMv5tBqXAQpPCICHs+6ZvJKjFxpDCkEEYYEjMNUf/Eu1hJH8aZJ8bJrfVitQLGCq0P2QFcEsetPpIHHvB7vqZEctGvq13pbQbkx+LTuKUMwT+cYR6OVBQBJEEkIKQPbipTP3/U4jWwRLI8SbrDJMttTFe+2p55buL9+IUOkCM/IYaB2teaprjWXHhs3dNEkOiI1c5dLeGNrAFBfgYHMSyAEIwIfKiZq8kqMXEiIKIAOcJgVnEPy1OPad9KytYnvN+X67I33mqVKlHMqU9qsZGpcBCk8IwIfKiZq8kqMXGkMKQQTU6+Vdl4ZzsJrhRQvz2Nl7+e8CNdMY04OnC1u5JYZ6ECN+Kez0pJwc2YhypqFisyWuq6s5+FhIa83A6RAtI264EkQKQgpAHH18U/ykyjLFg5T59c35tt/TLZ5lnHwWJGDLaRZAlR81UVfW634+SvEijLbS0IWJ5ZZblwbvMarvfjm0G2i0aw==";
        let bob_private_b64 = "EpYDCsgBCID/5qOavJKjFxIiCiAEu89bIFnCDu1NvDUnPrcW/QwVoBD3MBkDmSW8JCb6gxqXAQpPCID/5qOavJKjFxpDCkEETNqXya/QxjDTgOqgUkrxFEmasoNc9GY83nREU6IXWAhbUzWLbpapP6fVN7adTmG97tztFDb/Zo9K4yxtZ54rUxJEEkIKQNZWu3UbXoDzeY2FPXLWBcMtf0dXCTlGppv8jRWNLRyvaBSpfaXc7QdeKtbIWUKq5rgd88OWkHhZjgA0NPGBqMASyAEIwKW5tZq8kqMXEiIKIMoytCr53r3f/k9Wae/QPdGdPWsAPSLQWFwVez5K8ZGxGpcBCk8IwKW5tZq8kqMXGkMKQQRX0e1CP6Jc5kbjtXF1oxgbFciNSt002UlP4ZS6vDmkCYvQyclEtY3TQcrBXSNNK2JbDwu30+1z+h6DqasrMJc6EkQKQgpAw2rkuwL7e0s3XrrtY6+YhEMmh2nijAMFQKXPFa8edKE1LfMqp0IAGhYXBiGlV7A7yPZDXLLasf11Uy4ww2Wiyw==";
        let alice_invite_b64 = "CtgGCvgECrQCCpcBCk8IgIez7pm8kqMXGkMKQQRhgSMw1R/8S7WEkfxpknxsmt9WK1AsYKrQ/ZAVwSx60+kgce8Hu+pkRy0a+rXeltBuTH4tO4pQzBP5xhHo5UFAEkQSQgpA9uKlM/f9TiNbBEsjxJusMky21MV77annlu4v34hQ6QIz8hhoHa15qmuNZceGzd00SQ6IjVzl0t4Y2sAUF+BgcxKXAQpPCMCHyomavJKjFxpDCkEE1OvlXZeGc7Ca4UUL89jZe/nvAjXTGNODpwtbuSWGehAjfins9KScHNmIcqahYrMlrqurOfhYSGvNwOkQLSNuuBJECkIKQBx9fFP8pMoyxYOU+fXN+bbf0y2eZZx8FiRgy2kWQJUfNVFX1ut+PkrxIoy20tCFieWWW5cG7zGq7345tBtotGsStAIKlwEKTwiA/+ajmrySoxcaQwpBBEzal8mv0MYw04DqoFJK8RRJmrKDXPRmPN50RFOiF1gIW1M1i26WqT+n1Te2nU5hve7c7RQ2/2aPSuMsbWeeK1MSRBJCCkDWVrt1G16A83mNhT1y1gXDLX9HVwk5Rqab/I0VjS0cr2gUqX2l3O0HXirWyFlCqua4HfPDlpB4WY4ANDTxgajAEpcBCk8IwKW5tZq8kqMXGkMKQQRX0e1CP6Jc5kbjtXF1oxgbFciNSt002UlP4ZS6vDmkCYvQyclEtY3TQcrBXSNNK2JbDwu30+1z+h6DqasrMJc6EkQKQgpAw2rkuwL7e0s3XrrtY6+YhEMmh2nijAMFQKXPFa8edKE1LfMqp0IAGhYXBiGlV7A7yPZDXLLasf11Uy4ww2WiyxiAqva1mrySoxcS2gEK1wEKIPl1rD6K3Oj8Ps+zIzfp+n2/hUKqE/ORkHOsZ8kJpIFtEgwb7/dw52hTPD37IsYapAGAJTWRotzIHUtMu1bLd7izktJOh3cJ+ZXODtho02lsNp6DuwNIoEXesdoFRtVZCYqvaiOwnctX+nnPsSfemDmQ1mJ/o4sZyvFAF25ufSBaBqRJeyQjUBbfyuJSWYoDiqAAAMzsWPzrPeVJZFXrcOdDSTA11b+MevlfzcFjitqv/0J2j+pcQo4RFOgtpFK9cUkbcIB2xjRBRXOUQL89BuyMQmb+gg==";
        let bob_public_key_b64 = "CpcBCk8IgP/mo5q8kqMXGkMKQQRM2pfJr9DGMNOA6qBSSvEUSZqyg1z0ZjzedERTohdYCFtTNYtulqk/p9U3tp1OYb3u3O0UNv9mj0rjLG1nnitTEkQSQgpA1la7dRtegPN5jYU9ctYFwy1/R1cJOUamm/yNFY0tHK9oFKl9pdztB14q1shZQqrmuB3zw5aQeFmOADQ08YGowBKXAQpPCMClubWavJKjFxpDCkEEV9HtQj+iXOZG47VxdaMYGxXIjUrdNNlJT+GUurw5pAmL0MnJRLWN00HKwV0jTStiWw8Lt9Ptc/oeg6mrKzCXOhJECkIKQMNq5LsC+3tLN1667WOvmIRDJodp4owDBUClzxWvHnShNS3zKqdCABoWFwYhpVewO8j2Q1yy2rH9dVMuMMNloss=";
        let alice_public_key_b64 = "CpcBCk8IgIez7pm8kqMXGkMKQQRhgSMw1R/8S7WEkfxpknxsmt9WK1AsYKrQ/ZAVwSx60+kgce8Hu+pkRy0a+rXeltBuTH4tO4pQzBP5xhHo5UFAEkQSQgpA9uKlM/f9TiNbBEsjxJusMky21MV77annlu4v34hQ6QIz8hhoHa15qmuNZceGzd00SQ6IjVzl0t4Y2sAUF+BgcxKXAQpPCMCHyomavJKjFxpDCkEE1OvlXZeGc7Ca4UUL89jZe/nvAjXTGNODpwtbuSWGehAjfins9KScHNmIcqahYrMlrqurOfhYSGvNwOkQLSNuuBJECkIKQBx9fFP8pMoyxYOU+fXN+bbf0y2eZZx8FiRgy2kWQJUfNVFX1ut+PkrxIoy20tCFieWWW5cG7zGq7345tBtotGs=";
        let expected_key_material_b64 = "pCZEyn0gkwTrNDOlewVGTHYuqXdWzv9s+WKUWCtdFCk=";
        let topic_bytes = [
            210, 86, 199, 2, 239, 247, 51, 208, 205, 197, 32, 162, 215, 110, 185, 7, 115, 73, 7,
            223, 5, 10, 75, 19, 252, 160, 139, 241, 4, 205, 128, 152,
        ];

        // Create a keystore, then save Alice's private key bundle
        let mut x = Keystore::new();
        let set_private_result = x.set_private_key_bundle(
            &general_purpose::STANDARD
                .decode(alice_private_b64.as_bytes())
                .unwrap(),
        );
        assert!(set_private_result.is_ok());

        // Save an invite for alice
        let save_invite_result = x.save_invitation(
            &general_purpose::STANDARD
                .decode(alice_invite_b64.as_bytes())
                .unwrap(),
        );
        println!("save_invite_result: {:?}", save_invite_result);
        assert!(save_invite_result.is_ok());
    }
}
