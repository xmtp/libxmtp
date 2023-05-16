use crate::traits;

use ethers_core::types as ethers_types;
use k256::ecdsa::signature::DigestVerifier;
pub use k256::ecdsa::{RecoveryId, SigningKey, VerifyingKey};
use k256::{PublicKey, Secp256k1};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use sha3::{Digest, Keccak256};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EcdsaSignature {
    // Both carry signature bytes and a recovery id
    EcdsaSecp256k1Sha256Compact(Vec<u8>, u32),
    WalletPersonalSignCompact(Vec<u8>, u32),
}

// This means that EcdsaSignature implements the SignatureVerifiable trait, allowing
// us to implement a SignatureVerifier<EcdsaSignature> we could imagine also implementing
// the SignatureVerifiable<EcdsaSignature> trait for a SignedBundle type, etc
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

#[derive(Error, Debug)]
pub enum SignatureError {
    #[error("Bad address format")]
    BadAddressFormat(#[from] hex::FromHexError),
    #[error("supplied signature is not in the proper format")]
    BadSignatureFormat(#[from] ethers_types::SignatureError),
    #[error("Signature is not valid for {addr:?}")]
    BadSignature {
        addr: String,
        e: ethers_types::SignatureError,
    },
    #[error("Error creating signature")]
    SigningError(#[from] ecdsa::Error),
    #[error("unknown data store error")]
    Unknown,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub enum RecoverableSignature {
    // This stores an RSV encoded ECDSA signature
    Eip191Signature(Vec<u8>),
}

impl RecoverableSignature {
    pub fn new_eth_signature(
        key: &SigningKey,
        msg: &str,
    ) -> Result<RecoverableSignature, SignatureError> {
        let digest = Keccak256::new_with_prefix(eip_191_prefix(msg));
        Ok(Self::from(key.sign_digest_recoverable(digest)?))
    }

    pub fn verify_signature(
        &self,
        addr: &str,
        predigest_message: &str,
    ) -> Result<(), SignatureError> {
        match self {
            Self::Eip191Signature(signature_bytes) => {
                let address = ethers_types::Address::from_slice(&hex::decode(addr)?);
                let signature = ethers_types::Signature::try_from(signature_bytes.as_slice())?;
                if let Err(e) = signature.verify(predigest_message, address) {
                    return Err(SignatureError::BadSignature {
                        addr: String::from(addr),
                        e,
                    });
                }

                Ok(())
            }
        }
    }

    pub fn recover_address(&self, predigest_message: &str) -> Result<String, SignatureError> {
        match self {
            Self::Eip191Signature(signature_bytes) => {
                let signature = ethers_types::Signature::try_from(signature_bytes.as_slice())?;
                Ok(signature.recover(predigest_message)?.to_string())
            }
        }
    }
}

impl From<(ecdsa::Signature<Secp256k1>, RecoveryId)> for RecoverableSignature {
    fn from((sig, recid): (ecdsa::Signature<Secp256k1>, RecoveryId)) -> Self {
        let mut bytes = sig.to_vec();
        bytes.push(recid.to_byte());

        RecoverableSignature::Eip191Signature(bytes)
    }
}

fn eip_191_prefix(msg: &str) -> String {
    format!("\x19Ethereum Signed Message:\n{}.", msg.len())
}

#[cfg(test)]
pub mod tests {
    use crate::signature::RecoverableSignature;
    use ethers::core::rand::thread_rng;
    use ethers::signers::{LocalWallet, Signer};

    pub async fn generate_random_signature(msg: &str) -> (String, Vec<u8>) {
        let wallet = LocalWallet::new(&mut thread_rng());
        let signature = wallet.sign_message(msg).await.unwrap();
        (
            hex::encode(wallet.address().to_fixed_bytes()),
            signature.to_vec(),
        )
    }

    fn toggle(index: usize, v: &mut Vec<u8>) {
        v[index] += 1;
    }

    #[tokio::test]
    async fn oracle_signature() {
        let msg = "hello";

        let (addr, bytes) = generate_random_signature(msg).await;
        let sig = RecoverableSignature::Eip191Signature(bytes);
        sig.verify_signature(&addr, msg.into())
            .expect("Baseline Signature failed");

        let (other_addr, mut other_bytes) = generate_random_signature(msg).await;
        toggle(5, &mut other_bytes); // Invalidate Signature by making a small change
        let other = RecoverableSignature::Eip191Signature(other_bytes);

        // Check for Bad Signature Error
        assert_eq!(
            true,
            other.verify_signature(&other_addr, msg.into()).is_err()
        );

        // Check for bad Addr
        assert_eq!(true, sig.verify_signature(&other_addr, msg.into()).is_err());
    }
}
