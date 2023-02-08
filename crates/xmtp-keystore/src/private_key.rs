// Import k256 crate
use k256::{
    ecdsa::{SigningKey, Signature, RecoveryId, VerifyingKey, signature::{Verifier}},
    EncodedPoint,
    PublicKey,
    SecretKey,
};
use sha2::{Sha256, Digest};

use protobuf;
use super::proto;

pub struct EcPrivateKey {
    private_key: SecretKey,
    public_key: PublicKey,
}

impl EcPrivateKey {

    // Static function to parse an EcPrivateKey from proto::private_key::PrivateKeyBundleV2
    pub fn from_proto(private_key_bundle: &proto::private_key::PrivateKeyBundleV2) -> Result<EcPrivateKey, String> {
        // Check if secp256k1 is available
        if !private_key_bundle.identity_key.has_secp256k1() {
            println!("No secp256k1 key found");
        }
        println!("{:?}", private_key_bundle.identity_key.secp256k1());
        println!("{:?}", private_key_bundle.identity_key.secp256k1().bytes);
        // Parse the private key from the proto
        let secret_key_bytes = private_key_bundle.identity_key.secp256k1().bytes.as_slice();
        // Print hex encoded secret_key_bytes
        println!("Secret key bytes: {}", hex::encode(secret_key_bytes));
        // From encoded point
        let secret_key_result = SecretKey::from_be_bytes(secret_key_bytes);
        // Check secret_key_result
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

    // Verify signature
    pub fn verify_signature(&self, message: &[u8], signature: &[u8]) -> Result<(), String> {
        // Parse signature from raw compressed bytes
        let signature_result = Signature::try_from(signature);
        // Check signature_result
        if signature_result.is_err() {
            return Err(signature_result.err().unwrap().to_string());
        }
        let signature = signature_result.unwrap();

        // Verifying key from self.public_key
        let verifying_key = VerifyingKey::from(&self.public_key);
        // Verify signature
        let verify_result = verifying_key.verify(message, &signature);
        // Check verify_result
        if verify_result.is_err() {
            return Err(verify_result.err().unwrap().to_string());
        }
        return Ok(verify_result.unwrap());
    }
}
