use crate::types::Address;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use xmtp_cryptography::signature::{RecoverableSignature, SignatureError, SigningKey};
use xmtp_cryptography::utils::{self, eth_address};

#[derive(Debug, Error)]
pub enum AssociationError {
    #[error("bad signature")]
    BadSignature(#[from] SignatureError),
    #[error("Association text mismatch")]
    TextMismatch,
    #[error(
        "Address mismatch in Association: Provided:{provided_addr:?} != signed:{signing_addr:?}"
    )]
    AddressMismatch {
        provided_addr: Address,
        signing_addr: Address,
    },
    #[error("unknown association error")]
    Unknown,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum AssociationText {
    Static { addr: Address, key_bytes: Vec<u8> },
}

impl AssociationText {
    pub fn get_address(&self) -> Address {
        match self {
            Self::Static { addr, .. } => addr.clone(),
        }
    }

    pub fn text(&self) -> String {
        match self {
            Self::Static { addr, key_bytes } => gen_static_text_v1(addr, key_bytes),
        }
    }

    pub fn is_valid(&self, addr: &str, key_bytes: &[u8]) -> Result<(), AssociationError> {
        if self.text() == gen_static_text_v1(addr, key_bytes) {
            return Ok(());
        }

        Err(AssociationError::TextMismatch)
    }

    pub fn new_static(addr: String, key_bytes: Vec<u8>) -> Self {
        AssociationText::Static { addr, key_bytes }
    }
}

fn gen_static_text_v1(addr: &str, key_bytes: &[u8]) -> String {
    format!("XMTPv3 Link:{addr} -> keyBytes:{}", &hex::encode(key_bytes))
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct Association {
    pub text: AssociationText,
    pub signature: RecoverableSignature,
}

impl Association {
    pub fn new(text: AssociationText, signature: RecoverableSignature) -> Self {
        Self { text, signature }
    }

    pub fn is_valid(&self, key_bytes: &[u8]) -> Result<(), AssociationError> {
        let assumed_addr = self.text.get_address();

        // Ensure the Text properly links the Address and Keybytes
        self.text.is_valid(&assumed_addr, key_bytes)?;

        let addr = self.signature.recover_address(&self.text.text())?;

        if assumed_addr != addr {
            Err(AssociationError::AddressMismatch {
                provided_addr: assumed_addr,
                signing_addr: addr,
            })
        } else {
            Ok(())
        }
    }

    // The address returned is unverified, call is_valid to ensure the address is correct
    pub fn address(&self) -> String {
        self.text.get_address()
    }

    pub fn test() -> Result<Self, AssociationError> {
        let key = SigningKey::random(&mut utils::rng());
        let pubkey = key.verifying_key();
        let addr = eth_address(pubkey).unwrap();

        let assoc_text =
            AssociationText::new_static(addr, pubkey.to_encoded_point(false).to_bytes().to_vec());
        let text = assoc_text.text();
        Ok(Self {
            text: assoc_text,
            signature: RecoverableSignature::new_eth_signature(&key, &text)?,
        })
    }
}
