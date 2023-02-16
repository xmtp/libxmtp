use k256::ecdsa::signature::DigestVerifier;
use k256::elliptic_curve::sec1::ToEncodedPoint;
use k256::{
    ecdh::{diffie_hellman, SharedSecret},
    ecdsa::{signature::Verifier, RecoveryId, Signature, VerifyingKey},
    PublicKey, SecretKey,
};
use sha3::{Digest, Keccak256};

use super::super::proto;
use super::private_key::{PrivateKey, SignedPrivateKey};

pub struct PrivateKeyBundle {
    // Underlying protos
    private_key_bundle_proto: proto::private_key::PrivateKeyBundleV2,

    pub identity_key: SignedPrivateKey,
    pub pre_keys: Vec<SignedPrivateKey>,
}

impl PrivateKeyBundle {
    pub fn from_proto(
        private_key_bundle: &proto::private_key::PrivateKeyBundleV2,
    ) -> Result<PrivateKeyBundle, String> {
        // Check if secp256k1 is available
        if !private_key_bundle.identity_key.has_secp256k1() {
            println!("No secp256k1 key found");
        }

        let identity_key_result =
            SignedPrivateKey::from_proto(&private_key_bundle.identity_key.as_ref().unwrap());
        if identity_key_result.is_err() {
            return Err(identity_key_result.err().unwrap().to_string());
        }

        let pre_keys = private_key_bundle
            .pre_keys
            .iter()
            .map(|pre_key| SignedPrivateKey::from_proto(pre_key))
            .collect::<Result<Vec<SignedPrivateKey>, String>>()?;

        return Ok(PrivateKeyBundle {
            private_key_bundle_proto: private_key_bundle.clone(),
            identity_key: identity_key_result.unwrap(),
            pre_keys: pre_keys,
        });
    }

    pub fn eth_wallet_address_from_public_key(public_key_bytes: &[u8]) -> Result<String, String> {
        // Hash the public key bytes
        let mut hasher = Keccak256::new();
        hasher.update(public_key_bytes);
        let result = hasher.finalize();
        // Return the result as hex string, take the last 20 bytes
        return Ok(format!("0x{}", hex::encode(&result[12..])));
    }

    pub fn eth_address(&self) -> Result<String, String> {
        // Get the public key bytes
        let binding = self.identity_key.public_key.to_encoded_point(false);
        let public_key_bytes = binding.as_bytes();
        return PrivateKeyBundle::eth_wallet_address_from_public_key(public_key_bytes);
    }

    pub fn find_pre_key(&self, my_pre_key: PublicKey) -> Option<SignedPrivateKey> {
        for pre_key in self.pre_keys.iter() {
            if pre_key.public_key == my_pre_key {
                return Some(pre_key.clone());
            }
        }
        return None;
    }
}
