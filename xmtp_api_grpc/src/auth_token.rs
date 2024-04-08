use base64::Engine;
use prost::Message;
use xmtp_proto::xmtp::message_api::v1::{AuthData, Token};
use xmtp_proto::xmtp::message_contents::private_key_bundle::Version;
use xmtp_proto::xmtp::message_contents::signature::{EcdsaCompact, Union as SignatureUnion};
use xmtp_proto::xmtp::message_contents::{
    private_key::Union as PrivateKeyUnion, PrivateKeyBundle, PrivateKeyBundleV1, PublicKey,
    Signature,
};
use xmtp_v2::k256_helper::sign_keccak_256;

fn create_auth_data(wallet_address: String) -> AuthData {
    AuthData {
        wallet_addr: wallet_address,
        created_ns: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Time went backwards")
            .as_nanos() as u64,
    }
}

pub struct Authenticator {
    identity_key: PublicKey,
    wallet_address: String,
    private_key_bytes: Vec<u8>,
}

impl Authenticator {
    pub fn new(
        identity_key: PublicKey,
        wallet_address: String,
        private_key_bytes: Vec<u8>,
    ) -> Self {
        Self {
            identity_key,
            wallet_address,
            private_key_bytes,
        }
    }

    pub fn create_token(&self) -> String {
        let auth_data = create_auth_data(self.wallet_address.clone());
        let mut serialized = Vec::new();
        auth_data
            .encode(&mut serialized)
            .expect("serialization failed");

        let signature = self.sign(serialized.as_slice());

        let token = Token {
            identity_key: Some(self.identity_key.clone()),
            auth_data_bytes: serialized,
            auth_data_signature: Some(signature),
        };
        let mut token_bytes = Vec::new();
        let _ = token.encode(&mut token_bytes);

        base64::engine::general_purpose::STANDARD.encode(&token_bytes)
    }

    fn sign(&self, bytes_to_sign: &[u8]) -> Signature {
        let (sig, recovery) = sign_keccak_256(self.private_key_bytes.as_slice(), bytes_to_sign)
            .expect("signature failed");

        Signature {
            union: Some(SignatureUnion::EcdsaCompact(EcdsaCompact {
                bytes: sig,
                recovery: recovery as u32,
            })),
        }
    }

    pub fn from_bytes(private_key_bundle_bytes: Vec<u8>, wallet_address: String) -> Self {
        let bundle = PrivateKeyBundle::decode(&mut private_key_bundle_bytes.as_slice())
            .expect("deserialization");
        let identity_key = match bundle.version {
            Some(Version::V1(PrivateKeyBundleV1 {
                identity_key,
                pre_keys: _,
            })) => identity_key.unwrap(),
            _ => panic!("missing identity key"),
        };

        let private_key_bytes = match identity_key.union {
            Some(PrivateKeyUnion::Secp256k1(inner)) => inner.bytes.clone(),
            _ => panic!("missing private key bytes"),
        };

        Self {
            wallet_address,
            identity_key: identity_key.public_key.unwrap(),
            private_key_bytes,
        }
    }

    pub fn from_hex(private_key_bundle_string: String, wallet_address: String) -> Self {
        let decoded_bytes = hex::decode(private_key_bundle_string).unwrap();
        Self::from_bytes(decoded_bytes, wallet_address)
    }
}
