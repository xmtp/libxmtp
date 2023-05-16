use ethers_core::types::{self as ethers_types, H160};
pub use k256::ecdsa::{RecoveryId, SigningKey, VerifyingKey};
use k256::Secp256k1;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};
use thiserror::Error;

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
    // This Signature is primary used by EVM compatible accounts. It assumes that the recoveryid is included in the signature and
    // that all messages passed in have not been prefixed with '\0x19Ethereum....'
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
                let address = ethers_types::Address::from_slice(&addr_string_to_bytes(addr)?);
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
                let addr = h160addr_to_string(signature.recover(predigest_message)?);
                Ok(addr)
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

impl From<ethers_core::types::Signature> for RecoverableSignature {
    fn from(value: ethers_core::types::Signature) -> Self {
        RecoverableSignature::Eip191Signature(value.to_vec())
    }
}

fn eip_191_prefix(msg: &str) -> String {
    format!("\x19Ethereum Signed Message:\n{}.", msg.len())
}

fn addr_string_to_bytes(str: &str) -> Result<Vec<u8>, SignatureError> {
    let unprefixed_address = str::strip_prefix(str, "0x").unwrap_or(str);
    hex::decode(unprefixed_address).map_err(SignatureError::BadAddressFormat)
}

pub fn h160addr_to_string(bytes: H160) -> String {
    let mut s = String::from("0x");
    s.push_str(&hex::encode(bytes));
    s
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

    fn toggle(index: usize, v: &mut [u8]) {
        v[index] += 1;
    }

    #[tokio::test]
    async fn oracle_signature() {
        let msg = "hello";

        let (addr, bytes) = generate_random_signature(msg).await;
        let sig = RecoverableSignature::Eip191Signature(bytes);
        sig.verify_signature(&addr, msg)
            .expect("Baseline Signature failed");

        let (other_addr, mut other_bytes) = generate_random_signature(msg).await;
        toggle(5, &mut other_bytes); // Invalidate Signature by making a small change
        let other = RecoverableSignature::Eip191Signature(other_bytes);

        // Check for Bad Signature Error
        assert!(other.verify_signature(&other_addr, msg).is_err());

        // Check for bad Addr
        assert!(sig.verify_signature(&other_addr, msg).is_err());
    }

    #[test]
    fn known_test_vector() {
        // This test was generated using Etherscans Signature tool: https://etherscan.io/verifySig/18959
        let addr = "0x1B2a516d691aBb8f08a75B2C73c95c62A1632431";
        let msg = "TestVector1";
        let sig_hash = "19d6bec562518e365d07ba3cce26d08a5fffa2cbb1e7fe03c1f2d6a722fd3a5e544097b91f8f8cd11d43b032659f30529139ab1a9ecb6c81ed4a762179e87db81c";

        let addr_alt = addr.strip_prefix("0x").unwrap();
        let addr_bad = &addr.replacen('b', "c", 1);
        let sig_bytes = hex::decode(sig_hash).unwrap();
        let sig = RecoverableSignature::Eip191Signature(sig_bytes);
        let msg_bad = "Testvector1";

        let recovered_addr = sig.recover_address(msg).unwrap();
        assert_eq!(recovered_addr, addr.to_lowercase());

        assert!(sig.verify_signature(addr, msg).is_ok());
        assert!(sig.verify_signature(addr_alt, msg).is_ok());
        assert!(sig.verify_signature(addr_bad, msg).is_err());
        assert!(sig.verify_signature(addr, msg_bad).is_err());
    }
}
