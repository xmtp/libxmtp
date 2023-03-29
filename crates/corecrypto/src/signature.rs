use crate::traits;

use k256::ecdsa::signature::DigestVerifier;
use k256::{ecdsa::VerifyingKey, PublicKey};
use sha2::Sha256;
use sha3::{Digest, Keccak256};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EcdsaSignature {
    // Both carry signature bytes and a recovery id
    EcdsaSecp256k1Sha256Compact(Vec<u8>, u32),
    WalletPersonalSignCompact(Vec<u8>, u32),
}

// This trait acts as a abstraction layer to allow "SignatureVerifiers" to be used with other types of Signature-like enums one day
impl traits::SignatureVerifiable<EcdsaSignature> for EcdsaSignature {
    fn get_signature(&self) -> Option<EcdsaSignature> {
        Some(self.clone())
    }
}

// Implements the verification process for supported signature types in k256
impl traits::SignatureVerifier<EcdsaSignature> for PublicKey {
    fn verify_signature(
        &self,
        predigest_message: &[u8],
        signature: &EcdsaSignature,
    ) -> Result<(), String> {
        match signature {
            EcdsaSignature::EcdsaSecp256k1Sha256Compact(signature_bytes, _) => {
                let signature = ecdsa::Signature::try_from(signature_bytes.as_slice())
                    .map_err(|e| e.to_string())?;
                let verifying_key = VerifyingKey::from(self);
                let digest = Sha256::new_with_prefix(predigest_message);
                verifying_key
                    .verify_digest(digest, &signature)
                    .map_err(|e| e.to_string())
            }
            // Assumes the predigest_messages ie EIP191 processed already
            EcdsaSignature::WalletPersonalSignCompact(signature_bytes, _) => {
                let signature = ecdsa::Signature::try_from(signature_bytes.as_slice())
                    .map_err(|e| e.to_string())?;
                let verifying_key = VerifyingKey::from(self);
                let digest = Keccak256::new_with_prefix(predigest_message);
                verifying_key
                    .verify_digest(digest, &signature)
                    .map_err(|e| e.to_string())
            }
            // The idea for unsupported types is to uncomment this catch-all
            // _ => Err("Unsupported signature type for k256 public key".to_string()),
        }
    }
}
