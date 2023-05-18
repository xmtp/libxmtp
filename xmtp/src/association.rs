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

/// An Association is link between a blockchain account and an xmtp account for the purposes of
/// authentication. This certifies the user address (0xadd12e555c541A063cDbBD3Feb3C006d6f996745)
///  is associated to the XMTP Account.
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct Association {
    text: AssociationText,
    signature: RecoverableSignature,
}

impl Association {
    pub fn new(
        account_public_key: &[u8],
        text: AssociationText,
        signature: RecoverableSignature,
    ) -> Result<Self, AssociationError> {
        let this = Self { text, signature };
        this.is_valid(account_public_key)?;
        Ok(this)
    }

    fn is_valid(&self, account_public_key: &[u8]) -> Result<(), AssociationError> {
        let assumed_addr = self.text.get_address();

        // Ensure the Text properly links the Address and Keybytes
        self.text.is_valid(&assumed_addr, account_public_key)?;

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

/// AssociationText represents the string which was signed by the authorizing blockchain account. a valid AssociationTest must
/// contain the address of the blockchain account and a representation of the XMTP Account publicKey. Different standards may
/// choose how this information is encoded, as well as adding extra requirements for increased security.
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub enum AssociationText {
    Static {
        addr: Address,
        account_public_key: Vec<u8>,
    },
}

impl AssociationText {
    pub fn get_address(&self) -> Address {
        match self {
            Self::Static { addr, .. } => addr.clone(),
        }
    }

    pub fn text(&self) -> String {
        match self {
            Self::Static {
                addr,
                account_public_key,
            } => gen_static_text_v1(addr, account_public_key),
        }
    }

    pub fn is_valid(&self, addr: &str, account_public_key: &[u8]) -> Result<(), AssociationError> {
        if self.text() == gen_static_text_v1(addr, account_public_key) {
            return Ok(());
        }

        Err(AssociationError::TextMismatch)
    }

    pub fn new_static(addr: String, account_public_key: Vec<u8>) -> Self {
        AssociationText::Static {
            addr,
            account_public_key,
        }
    }
}

fn gen_static_text_v1(addr: &str, key_bytes: &[u8]) -> String {
    format!(
        "AccountAssociation(XMTPv3): {addr} -> keyBytes:{}",
        &hex::encode(key_bytes)
    )
}

#[cfg(test)]
pub mod tests {
    use ethers::signers::{LocalWallet, Signer};
    use xmtp_cryptography::{signature::h160addr_to_string, utils::rng};

    use super::{Association, AssociationText};

    #[tokio::test]
    async fn assoc_gen() {
        let key_bytes = vec![22, 33, 44, 55];

        let wallet = LocalWallet::new(&mut rng());
        let other_wallet = LocalWallet::new(&mut rng());
        let addr = h160addr_to_string(wallet.address());
        let other_addr = h160addr_to_string(other_wallet.address());
        let text = AssociationText::Static {
            addr: addr.clone(),
            account_public_key: key_bytes.clone(),
        };
        let sig = wallet.sign_message(text.text()).await.expect("BadSign");

        let bad_key_bytes = vec![11, 22, 33];
        let bad_text1 = AssociationText::Static {
            addr: addr.clone(),
            account_public_key: bad_key_bytes.clone(),
        };
        let bad_text2 = AssociationText::Static {
            addr: other_addr.clone(),
            account_public_key: key_bytes.clone(),
        };
        let other_text = AssociationText::Static {
            addr: other_addr.clone(),
            account_public_key: key_bytes.clone(),
        };

        let other_sig = wallet
            .sign_message(other_text.text())
            .await
            .expect("BadSign");

        assert!(Association::new(&key_bytes, text.clone(), sig.into()).is_ok());
        assert!(Association::new(&bad_key_bytes, text.clone(), sig.into()).is_err());
        assert!(Association::new(&key_bytes, bad_text1.clone(), sig.into()).is_err());
        assert!(Association::new(&key_bytes, bad_text2.clone(), sig.into()).is_err());
        assert!(Association::new(&key_bytes, text.clone(), other_sig.into()).is_err());
    }
}
