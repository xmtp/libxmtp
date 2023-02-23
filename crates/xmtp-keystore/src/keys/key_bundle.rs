use k256::elliptic_curve::sec1::ToEncodedPoint;
use k256::PublicKey;
use sha3::{Digest, Keccak256};

use super::super::proto;
use super::private_key::SignedPrivateKey;
use super::public_key;

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
            return Err("No secp256k1 key found".to_string());
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

    pub fn public_key_bundle(&self) -> PublicKeyBundle {
        let identity_key = self.identity_key.public_key;
        let pre_keys = self
            .pre_keys
            .iter()
            .map(|pre_key| pre_key.public_key)
            .collect::<Vec<_>>();

        return PublicKeyBundle {
            public_key_bundle_proto: proto::public_key::PublicKeyBundle::new(),
            identity_key: Some(identity_key),
            pre_key: Some(pre_keys[0]),
        };
    }
}

pub struct PublicKeyBundle {
    // Underlying protos
    public_key_bundle_proto: proto::public_key::PublicKeyBundle,

    pub identity_key: Option<PublicKey>,
    pub pre_key: Option<PublicKey>,
}

impl PublicKeyBundle {
    pub fn from_proto(
        public_key_bundle: &proto::public_key::PublicKeyBundle,
    ) -> Result<PublicKeyBundle, String> {
        let mut identity_key: Option<PublicKey> = None;
        let mut pre_key: Option<PublicKey> = None;
        let identity_key_result =
            public_key::public_key_from_proto(public_key_bundle.identity_key.as_ref().unwrap());
        if identity_key_result.is_ok() {
            identity_key = Some(identity_key_result.unwrap());
        }

        let pre_key_result =
            public_key::public_key_from_proto(public_key_bundle.pre_key.as_ref().unwrap());
        if pre_key_result.is_ok() {
            pre_key = Some(pre_key_result.unwrap());
        }

        return Ok(PublicKeyBundle {
            public_key_bundle_proto: public_key_bundle.clone(),
            identity_key: identity_key,
            pre_key: pre_key,
        });
    }
}

pub struct SignedPublicKeyBundle {
    // Underlying protos
    signed_public_key_bundle_proto: proto::public_key::SignedPublicKeyBundle,

    pub identity_key: PublicKey,
    pub pre_key: PublicKey,
    // TODO: keep signature information
}

impl SignedPublicKeyBundle {
    pub fn from_proto(
        signed_public_key_bundle: &proto::public_key::SignedPublicKeyBundle,
    ) -> Result<SignedPublicKeyBundle, String> {
        // Check identity_key is populated
        if signed_public_key_bundle.identity_key.is_none() {
            return Err("No identity key found".to_string());
        }

        // Derive public key from SignedPublicKey
        let identity_key_result = public_key::signed_public_key_from_proto(
            signed_public_key_bundle.identity_key.as_ref().unwrap(),
        );
        if identity_key_result.is_err() {
            return Err(identity_key_result.err().unwrap().to_string());
        }
        let identity_key = identity_key_result.unwrap();

        // Check pre_key is populated
        if signed_public_key_bundle.pre_key.is_none() {
            return Err("No pre key found".to_string());
        }
        let pre_key_result = public_key::signed_public_key_from_proto(
            signed_public_key_bundle.pre_key.as_ref().unwrap(),
        );
        if pre_key_result.is_err() {
            return Err(pre_key_result.err().unwrap().to_string());
        }
        let pre_key = pre_key_result.unwrap();
        return Ok(SignedPublicKeyBundle {
            signed_public_key_bundle_proto: signed_public_key_bundle.clone(),
            identity_key: identity_key,
            pre_key: pre_key,
        });
    }
}
