use crate::types::Address;
use crate::InboxOwner;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use xmtp_cryptography::signature::{RecoverableSignature, SignatureError};
use xmtp_cryptography::utils::generate_local_wallet;
use xmtp_proto::xmtp::v3::message_contents::Eip191Association as Eip191AssociationProto;
use xmtp_proto::xmtp::v3::message_contents::RecoverableEcdsaSignature as RecoverableEcdsaSignatureProto;

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

/// An Association is link between a blockchain account and an xmtp identity for the purposes of
/// authentication.
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct Eip191Association {
    text: AssociationText,
    signature: RecoverableSignature,
}

impl Eip191Association {
    pub fn new(
        account_public_key: &[u8],
        text: AssociationText,
        signature: RecoverableSignature,
    ) -> Result<Self, AssociationError> {
        let this = Self { text, signature };
        this.is_valid(account_public_key)?;
        Ok(this)
    }

    // TODO make sure version can't be swapped
    pub fn from_proto_with_expected_address(
        account_public_key: &[u8],
        proto: Eip191AssociationProto,
        expected_wallet_address: String,
    ) -> Result<Self, AssociationError> {
        let text =
            AssociationText::new_static(expected_wallet_address, account_public_key.to_vec());
        let signature = RecoverableSignature::Eip191Signature(proto.signature.unwrap().bytes);
        Self::new(account_public_key, text, signature)
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

    pub fn test(pub_key: Vec<u8>) -> Result<Self, AssociationError> {
        let wallet = generate_local_wallet();
        let addr = wallet.get_address();
        let assoc_text = AssociationText::new_static(addr, pub_key);

        let signature = wallet.sign(&assoc_text.text())?;
        Ok(Self {
            text: assoc_text,
            signature,
        })
    }
}

impl From<Eip191Association> for Eip191AssociationProto {
    fn from(assoc: Eip191Association) -> Self {
        Self {
            signature: Some(RecoverableEcdsaSignatureProto {
                bytes: assoc.signature.into(),
            }),
            association_data: None, // TODO
        }
    }
}

/// AssociationText represents the string which was signed by the authorizing blockchain account. a valid AssociationText must
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
    use xmtp_proto::xmtp::v3::message_contents::Eip191Association as Eip191AssociationProto;

    use super::{AssociationText, Eip191Association};

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

        assert!(Eip191Association::new(&key_bytes, text.clone(), sig.into()).is_ok());
        assert!(Eip191Association::new(&bad_key_bytes, text.clone(), sig.into()).is_err());
        assert!(Eip191Association::new(&key_bytes, bad_text1.clone(), sig.into()).is_err());
        assert!(Eip191Association::new(&key_bytes, bad_text2.clone(), sig.into()).is_err());
        assert!(Eip191Association::new(&key_bytes, text.clone(), other_sig.into()).is_err());
    }

    #[tokio::test]
    async fn to_proto() {
        let key_bytes = vec![22, 33, 44, 55];
        let wallet = LocalWallet::new(&mut rng());
        let addr = h160addr_to_string(wallet.address());
        let text = AssociationText::Static {
            addr: addr.clone(),
            account_public_key: key_bytes.clone(),
        };
        let sig = wallet.sign_message(text.text()).await.expect("BadSign");

        let assoc = Eip191Association::new(&key_bytes, text.clone(), sig.into()).unwrap();
        let proto_signature: Eip191AssociationProto = assoc.into();

        assert_eq!(proto_signature.signature.unwrap().bytes, sig.to_vec());
    }
}
