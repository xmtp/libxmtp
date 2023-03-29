use k256::elliptic_curve::sec1::ToEncodedPoint;
use k256::{
    ecdsa::{RecoveryId, Signature, VerifyingKey},
    PublicKey, SecretKey,
};
use sha3::{Digest, Keccak256};

use crate::ecdh::{ECDHDerivable, ECDHKey};
use crate::ethereum_utils::{EthereumCompatibleKey, EthereumUtils};
use crate::proto;

use corecrypto::{signature::EcdsaSignature, traits::SignatureVerifier};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrivateKey {
    pub private_key: SecretKey,
    pub public_key: PublicKey,
}

impl PrivateKey {
    pub fn from_proto(proto: &proto::private_key::PrivateKey) -> Result<PrivateKey, String> {
        // Check if has_secp256k1
        if !proto.has_secp256k1() {
            return Err("PrivateKey does not have secp256k1".to_string());
        }
        let secret_key_bytes = proto.secp256k1().bytes.as_slice();
        if secret_key_bytes.is_empty() {
            return Err("PrivateKey does not have secp256k1 bytes".to_string());
        }
        // Try to derive secret key from big-endian hex-encoded BigInt, check the result
        let secret_key_result = SecretKey::from_be_bytes(secret_key_bytes);
        if secret_key_result.is_err() {
            return Err(secret_key_result.err().unwrap().to_string());
        }
        let secret_key = secret_key_result.unwrap();
        let public_key = secret_key.public_key();

        Ok(PrivateKey {
            private_key: secret_key,
            public_key,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SignedPrivateKey {
    proto: proto::private_key::SignedPrivateKey,

    pub private_key: SecretKey,
    // (STOPSHIP) TODO: needs to be a signed PublicKey
    pub public_key: PublicKey,
}

impl SignedPrivateKey {
    pub fn from_proto(
        proto: &proto::private_key::SignedPrivateKey,
    ) -> Result<SignedPrivateKey, String> {
        // Check if has_secp256k1
        if !proto.has_secp256k1() {
            return Err("SignedPrivateKey does not have secp256k1".to_string());
        }
        let secret_key_bytes = proto.secp256k1().bytes.as_slice();
        if secret_key_bytes.is_empty() {
            return Err("SignedPrivateKey does not have secp256k1 bytes".to_string());
        }
        // Try to derive secret key from big-endian hex-encoded BigInt, check the result
        let secret_key_result = SecretKey::from_be_bytes(secret_key_bytes);
        if secret_key_result.is_err() {
            return Err(secret_key_result.err().unwrap().to_string());
        }
        let secret_key = secret_key_result.unwrap();
        let public_key = secret_key.public_key();

        Ok(SignedPrivateKey {
            proto: proto.clone(),
            private_key: secret_key,
            public_key,
        })
    }

    pub fn eth_wallet_address_from_public_key(public_key_bytes: &[u8]) -> Result<String, String> {
        // Hash the public key bytes
        let mut hasher = Keccak256::new();
        hasher.update(public_key_bytes);
        let result = hasher.finalize();
        // Return the result as hex string, take the last 20 bytes
        Ok(format!("0x{}", hex::encode(&result[12..])))
    }

    pub fn eth_address(&self) -> Result<String, String> {
        // Get the public key bytes
        let binding = self.public_key.to_encoded_point(false);
        let public_key_bytes = binding.as_bytes();
        // Return the result as hex string, take the last 20 bytes
        // Need to remove the 04 prefix for uncompressed point representation
        Self::eth_wallet_address_from_public_key(&public_key_bytes[1..])
    }

    pub fn ethereum_personal_digest(xmtp_payload: &[u8]) -> Vec<u8> {
        // Hash the entire thing one more time with keccak256
        let personal_sign_payload = EthereumUtils::get_personal_sign_message(xmtp_payload);
        let mut hasher = Keccak256::new();
        hasher.update(personal_sign_payload);
        let result = hasher.finalize();
        result.to_vec()
    }

    // Verify wallet signature from proto
    pub fn verify_wallet_signature(
        address: &str,
        message: &[u8],
        signature: &proto::signature::Signature,
    ) -> Result<(), String> {
        // Expect ecdsa_compact field with subfields: bytes, recovery_id
        if !signature.has_wallet_ecdsa_compact() {
            return Err("No wallet_ecdsa_compact field found".to_string());
        }
        let wallet_ecdsa_compact = signature.wallet_ecdsa_compact();
        let signature_bytes = wallet_ecdsa_compact.bytes.as_slice();
        let recovery_id_result = RecoveryId::try_from(wallet_ecdsa_compact.recovery as u8);
        if recovery_id_result.is_err() {
            return Err(recovery_id_result.err().unwrap().to_string());
        }
        let recovery_id = recovery_id_result.unwrap();
        let ecdsa_signature_result = Signature::try_from(signature_bytes);
        if ecdsa_signature_result.is_err() {
            return Err(ecdsa_signature_result.err().unwrap().to_string());
        }
        let ec_signature = ecdsa_signature_result.unwrap();

        let recovered_key_result = VerifyingKey::recover_from_digest(
            Keccak256::new_with_prefix(message),
            &ec_signature,
            recovery_id,
        );

        if recovered_key_result.is_err() {
            return Err(recovered_key_result.err().unwrap().to_string());
        }
        let recovered_key = recovered_key_result.unwrap();

        // Check if ethereum address from recovered key matches the address from the proto
        // First extract the public key from the recovered key
        let public_key = PublicKey::from(&recovered_key);
        let eth_address = public_key.get_ethereum_address();

        // Compare both in lower case
        if address.to_lowercase() != eth_address.to_lowercase() {
            return Err("Recovered address does not match the address from the proto".to_string());
        }
        // Reverify (not strictly necessary, but exercises the trait)
        // move signature bytes into this enum: EcdsaSecp256k1Sha256Compact(Vec<u8>, u32),
        let ecdsa_signature =
            EcdsaSignature::WalletPersonalSignCompact(signature_bytes.to_vec(), 0);
        // PublicKey already implements the SignatureVerifier trait for ecdsa signatures
        public_key.verify_signature(message, &ecdsa_signature)?;
        Ok(())
    }

    // Verify signature with default sha256 digest mechanism
    pub fn verify_signature(&self, message: &[u8], signature: &[u8]) -> Result<(), String> {
        // Move signature bytes into this enum: EcdsaSecp256k1Sha256Compact(Vec<u8>, u32),
        let ecdsa_signature = EcdsaSignature::EcdsaSecp256k1Sha256Compact(signature.to_vec(), 0);
        // PublicKey already implements the SignatureVerifier trait for ecdsa signatures
        self.public_key.verify_signature(message, &ecdsa_signature)
    }
}

impl ECDHKey for PublicKey {
    fn get_public_key(&self) -> PublicKey {
        *self
    }
}

// NOTE: XMTP uses the entire point in uncompressed format as secret material
// this diverges from the convention of using only the `x` coordinate.
// For this reason, we need to duplicate the diffie_hellman operation otherwise
// RustCrypto hides the `y` coordinate from visibility when constructing a SharedSecret
// https://github.com/RustCrypto/traits/blob/d57b54b9fcf5b28745547cb9fef313ab09780918/elliptic-curve/src/ecdh.rs#L60
// let public_point = ProjectivePoint::<C>::from(*public_key.borrow());
// let secret_point = (public_point * secret_key.borrow().as_ref()).to_affine();
fn diffie_hellman(secret_key: &SecretKey, public_key: &PublicKey) -> Result<Vec<u8>, String> {
    // Get the public projective point from the public key
    let public_point = public_key.to_projective();
    // Multiply with nonzero scalar of secret key
    let shared_secret_point = (public_point * secret_key.to_nonzero_scalar().as_ref()).to_affine();
    // Encode the entire point in uncompressed format
    let shared_secret_encoded = shared_secret_point.to_encoded_point(false);
    return Ok(shared_secret_encoded.as_bytes().to_vec());
}

impl ECDHDerivable for PrivateKey {
    fn shared_secret(&self, other_key: &dyn ECDHKey) -> Result<Vec<u8>, String> {
        diffie_hellman(&self.private_key, &other_key.get_public_key())
    }
}

impl ECDHDerivable for SignedPrivateKey {
    fn shared_secret(&self, other_key: &dyn ECDHKey) -> Result<Vec<u8>, String> {
        diffie_hellman(&self.private_key, &other_key.get_public_key())
    }
}

impl EthereumCompatibleKey for proto::private_key::PrivateKey {
    fn get_ethereum_address(&self) -> String {
        let private_key_result = PrivateKey::from_proto(self);
        if private_key_result.is_err() {
            return "".to_string();
        }
        let private_key = private_key_result.unwrap();
        private_key.private_key.get_ethereum_address()
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

        EthereumUtils::get_ethereum_address_from_public_key_bytes(&public_key_bytes[1..])
    }
}

impl EthereumCompatibleKey for PublicKey {
    fn get_ethereum_address(&self) -> String {
        // Get encoded public key
        let encoded_public_key = self.to_encoded_point(false);
        // Get public key bytes
        let public_key_bytes = encoded_public_key.as_bytes();
        // Get ethereum address from public key bytes

        EthereumUtils::get_ethereum_address_from_public_key_bytes(&public_key_bytes[1..])
    }
}
