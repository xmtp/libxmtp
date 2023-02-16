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

pub struct PublicKeyBundle {
    // Underlying protos
    public_key_bundle_proto: proto::public_key::PublicKeyBundle,

    pub identity_key: PublicKey,
    pub pre_key: PublicKey,
}

impl PublicKeyBundle {
    pub fn from_proto(
        public_key_bundle: &proto::public_key::PublicKeyBundle,
    ) -> Result<PublicKeyBundle, String> {
        // Check if secp256k1 is available
        if !public_key_bundle.identity_key.has_secp256k1_uncompressed() {
            println!("No secp256k1 key found");
        }

        // Parse the public key from the proto
        let public_key_bytes = public_key_bundle
            .identity_key
            .secp256k1_uncompressed()
            .bytes
            .as_slice();
        // Check that bytes are not empty
        if public_key_bytes.is_empty() {
            return Err("No bytes found".to_string());
        }

        // Try to derive public key from big-endian hex-encoded BigInt, check the result
        let public_key_result = PublicKey::from_sec1_bytes(public_key_bytes);
        if public_key_result.is_err() {
            return Err(public_key_result.err().unwrap().to_string());
        }
        let public_key = public_key_result.unwrap();

        // Check if secp256k1 is available
        if !public_key_bundle.pre_key.has_secp256k1_uncompressed() {
            println!("No secp256k1 key found");
        }

        // Parse the public key from the proto
        let pre_key_bytes = public_key_bundle
            .pre_key
            .secp256k1_uncompressed()
            .bytes
            .as_slice();
        // Check that bytes are not empty
        if pre_key_bytes.is_empty() {
            return Err("No bytes found".to_string());
        }

        // Try to derive public key from big-endian hex-encoded BigInt, check the result
        let pre_key_result = PublicKey::from_sec1_bytes(pre_key_bytes);
        if pre_key_result.is_err() {
            return Err(pre_key_result.err().unwrap().to_string());
        }
        let pre_key = pre_key_result.unwrap();

        return Ok(PublicKeyBundle {
            public_key_bundle_proto: public_key_bundle.clone(),
            identity_key: public_key,
            pre_key: pre_key,
        });
    }
}
