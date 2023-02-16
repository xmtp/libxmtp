use k256::ecdsa::signature::DigestVerifier;
use k256::elliptic_curve::sec1::ToEncodedPoint;
use k256::{
    ecdh::{diffie_hellman, SharedSecret},
    ecdsa::{signature::Verifier, RecoveryId, Signature, VerifyingKey},
    PublicKey, SecretKey,
};
use sha3::{Digest, Keccak256};

use super::ecdh::{ECDHDerivable, ECDHKey};
use super::ethereum_utils::{EthereumCompatibleKey, EthereumUtils};
use super::proto;

pub struct EcPrivateKey {
    private_key: SecretKey,
    public_key: PublicKey,
}

impl EcPrivateKey {
    // Static function to parse an EcPrivateKey from proto::private_key::PrivateKeyBundleV2
    pub fn from_proto(
        private_key_bundle: &proto::private_key::PrivateKeyBundleV2,
    ) -> Result<EcPrivateKey, String> {
        // Check if secp256k1 is available
        if !private_key_bundle.identity_key.has_secp256k1() {
            println!("No secp256k1 key found");
        }

        // Parse the private key from the proto
        let secret_key_bytes = private_key_bundle.identity_key.secp256k1().bytes.as_slice();
        // Check that bytes are not empty
        if secret_key_bytes.is_empty() {
            return Err("No bytes found".to_string());
        }

        // Try to derive secret key from big-endian hex-encoded BigInt, check the result
        let secret_key_result = SecretKey::from_be_bytes(secret_key_bytes);
        if secret_key_result.is_err() {
            return Err(secret_key_result.err().unwrap().to_string());
        }
        let secret_key = secret_key_result.unwrap();
        let public_key = secret_key.public_key().clone();
        return Ok(EcPrivateKey {
            private_key: secret_key,
            public_key: public_key,
        });
    }
}

impl EthereumCompatibleKey for proto::private_key::PrivateKey {
    fn get_ethereum_address(&self) -> String {
        let private_key = EcPrivateKey::from_proto(self);
        return private_key.eth_address().unwrap();
    }
}

// Implement the EthereumCompatibleKey trait for EcPrivateKey
// this provides a get_ethereum_address method
impl EthereumCompatibleKey for SecretKey {
    fn get_ethereum_address(&self) -> String {
        // Get public key from self
        let public_key = self.public_key();
        // Get encoded public key
        let encoded_public_key = public_key.to_encoded_point(false);
        // Get public key bytes
        let public_key_bytes = encoded_public_key.as_bytes();
        // Get ethereum address from public key bytes
        let eth_address =
            EthereumUtils::get_ethereum_address_from_public_key_bytes(&public_key_bytes[1..]);
        return eth_address;
    }
}

impl EthereumCompatibleKey for PublicKey {
    fn get_ethereum_address(&self) -> String {
        // Get encoded public key
        let encoded_public_key = self.to_encoded_point(false);
        // Get public key bytes
        let public_key_bytes = encoded_public_key.as_bytes();
        // Get ethereum address from public key bytes
        let eth_address =
            EthereumUtils::get_ethereum_address_from_public_key_bytes(&public_key_bytes[1..]);
        return eth_address;
    }
}

impl ECDHDerivable for SecretKey {
    fn get_shared_secret(&self, other: &dyn ECDHKey) -> Result<SharedSecret, String> {
        // Get other public key
        let other_public_key = other.get_public_key();
        let shared_secret = diffie_hellman(
            self.to_nonzero_scalar(),
            other_public_key.as_affine(),
        );
        return Ok(shared_secret);
    }
}
