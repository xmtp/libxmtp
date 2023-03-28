use std::collections::HashMap;

use corecrypto::encryption;

use protobuf;

mod conversation;
mod ecdh;
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

    // Process proto::keystore::DecryptV1Request
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

            let decrypt_result = encryption::decrypt(&[], &[], &[], &[], None);
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

    // Save invites
    pub fn save_invitation(&mut self, sealed_invitation_bytes: &[u8]) -> Result<bool, String> {
        // Check that self.private_key_bundle is set, otherwise return an error
        if self.private_key_bundle.is_none() {
            return Err("private key bundle not set yet".to_string());
        }

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
        let decrypt_result = self
            .private_key_bundle
            .as_ref()
            .unwrap()
            .unseal_invitation(&invitation, &invitation_header);
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
            decrypted_invitation.topic.clone(),
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

    pub fn getTopicKey(&self, topic_id: &str) -> Option<Vec<u8>> {
        let topic_data = self.topic_keys.get(topic_id);
        if topic_data.is_none() {
            return None;
        }
        return Some(topic_data.unwrap().key.clone());
    }
    // == end keystore api ==
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let shared_secret_result = x
            .private_key_bundle
            .expect("Must be present for test")
            .derive_shared_secret_xmtp(&peer_bundle_object, &pre_key_object, is_recipient);
        assert!(shared_secret_result.is_ok());
        let shared_secret = shared_secret_result.unwrap();
        assert_eq!(
            shared_secret,
            general_purpose::STANDARD.decode(secret).unwrap()
        );
    }

    #[test]
    fn test_decrypt_invite() {
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
        assert!(save_invite_result.is_ok());
    }
}
