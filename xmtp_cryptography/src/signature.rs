use std::array::TryFromSliceError;

use alloy::primitives::{self as alloy_types, Address};
use hex::FromHexError;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{configuration::ED25519_KEY_LENGTH, Secret};

pub fn to_public_key(private_key: &Secret) -> Result<[u8; ED25519_KEY_LENGTH], TryFromSliceError> {
    let private_key = private_key.as_slice().try_into()?;
    let mut computed_public_key = [0u8; ED25519_KEY_LENGTH];
    libcrux_ed25519::secret_to_public(&mut computed_public_key, &private_key);
    Ok(computed_public_key)
}

#[derive(Error, Debug)]
pub enum SignatureError {
    #[error("Bad address format")]
    BadAddressFormat(#[from] hex::FromHexError),
    #[error("supplied signature is not in the proper format")]
    BadSignatureFormat(#[from] alloy_types::SignatureError),
    #[error("Signature is not valid for {addr:?}")]
    BadSignature { addr: String },
    #[error(transparent)]
    Signer(#[from] alloy::signers::Error),
    #[error("unknown data store error")]
    Unknown,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub enum RecoverableSignature {
    // This Signature is primary used by EVM compatible accounts. It assumes that the recoveryid
    // is included in the signature and that all messages passed in have not been prefixed
    // with '\0x19Ethereum....'
    Eip191Signature(Vec<u8>),
}

impl RecoverableSignature {
    pub fn recover_address(&self, predigest_message: &str) -> Result<String, SignatureError> {
        match self {
            Self::Eip191Signature(signature_bytes) => {
                let signature = alloy_types::Signature::try_from(signature_bytes.as_slice())?;
                let addr = signature.recover_address_from_msg(predigest_message)?;
                Ok(addr.to_string())
            }
        }
    }
}

impl From<Vec<u8>> for RecoverableSignature {
    fn from(value: Vec<u8>) -> Self {
        RecoverableSignature::Eip191Signature(value)
    }
}

impl From<RecoverableSignature> for Vec<u8> {
    fn from(value: RecoverableSignature) -> Self {
        match value {
            RecoverableSignature::Eip191Signature(bytes) => bytes,
        }
    }
}
/*
impl From<(ecdsa::Signature<Secp256k1>, RecoveryId)> for RecoverableSignature {
    fn from((sig, recid): (ecdsa::Signature<Secp256k1>, RecoveryId)) -> Self {
        let mut bytes = sig.to_vec();
        bytes.push(recid.to_byte());

        RecoverableSignature::Eip191Signature(bytes)
    }
}
*/
impl From<alloy::primitives::Signature> for RecoverableSignature {
    fn from(value: alloy::primitives::Signature) -> Self {
        RecoverableSignature::Eip191Signature(value.as_bytes().to_vec())
    }
}

pub fn h160addr_to_string(bytes: Address) -> String {
    let mut s = String::from("0x");
    s.push_str(&hex::encode(bytes));
    s.to_lowercase()
}

/// Check if an string is a valid ethereum address (valid hex and length 20).
pub fn is_valid_ethereum_address<S: AsRef<str>>(address: S) -> bool {
    let address = address.as_ref();
    let address = address.strip_prefix("0x").unwrap_or(address);

    if address.len() != 40 {
        return false;
    }

    address.chars().all(|c| c.is_ascii_hexdigit())
}

#[derive(Debug, Error)]
pub enum IdentifierValidationError {
    #[error("invalid addresses: {0:?}")]
    InvalidAddresses(Vec<String>),
    #[error("address is invalid hex address")]
    HexDecode(#[from] FromHexError),
    #[error("generic error: {0}")]
    Generic(String),
}

pub fn sanitize_evm_addresses(
    account_addresses: &[impl AsRef<str>],
) -> Result<Vec<String>, IdentifierValidationError> {
    let mut invalid = account_addresses
        .iter()
        .filter(|a| !is_valid_ethereum_address(a))
        .peekable();

    if invalid.peek().is_some() {
        return Err(IdentifierValidationError::InvalidAddresses(
            invalid
                .map(|addr| addr.as_ref().to_string())
                .collect::<Vec<_>>(),
        ));
    }

    Ok(account_addresses
        .iter()
        .map(|addr| addr.as_ref().to_lowercase())
        .collect())
}

#[cfg(test)]
pub mod tests {
    use super::is_valid_ethereum_address;

    use alloy::signers::local::PrivateKeySigner;
    use alloy::signers::SignerSync;

    pub fn generate_random_signature(msg: &str) -> (String, Vec<u8>) {
        let signer = PrivateKeySigner::random();
        let signature = signer.sign_message_sync(msg.as_bytes()).unwrap();
        (hex::encode(signer.address()), signature.as_bytes().to_vec())
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_eth_address() {
        assert!(is_valid_ethereum_address(
            "0x7e57Aed10441c8879ce08E45805EC01Ee9689c9f"
        ));
        assert!(!is_valid_ethereum_address("123"));
    }
}
