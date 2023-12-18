use serde::{Deserialize, Serialize};
use thiserror::Error;
use xmtp_cryptography::signature::{
    ed25519_public_key_to_address, RecoverableSignature, SignatureError,
};
use xmtp_proto::xmtp::mls::message_contents::{
    Eip191Association as Eip191AssociationProto,
    RecoverableEcdsaSignature as RecoverableEcdsaSignatureProto,
};

use crate::types::Address;

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
}

/// An Association is link between a blockchain account and an xmtp installation for the purposes of
/// authentication.
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct Eip191Association {
    text: AssociationText,
    signature: RecoverableSignature,
}

impl Eip191Association {
    pub fn new(
        installation_public_key: &[u8],
        text: AssociationText,
        signature: RecoverableSignature,
    ) -> Result<Self, AssociationError> {
        let this = Self { text, signature };
        this.is_valid(installation_public_key)?;
        Ok(this)
    }

    pub fn from_proto_with_expected_address(
        context: AssociationContext,
        installation_public_key: &[u8],
        proto: Eip191AssociationProto,
        expected_wallet_address: String,
    ) -> Result<Self, AssociationError> {
        let text = AssociationText::new_static(
            context,
            expected_wallet_address,
            installation_public_key.to_vec(),
            proto.iso8601_time,
        );
        let signature = RecoverableSignature::Eip191Signature(proto.signature.unwrap().bytes);
        Self::new(installation_public_key, text, signature)
    }

    fn is_valid(&self, installation_public_key: &[u8]) -> Result<(), AssociationError> {
        let assumed_context = self.text.get_context();
        let assumed_addr = self.text.get_address();
        let assumed_time = self.text.get_iso8601_time();

        // Ensure the Text properly links the Address and Keybytes
        self.text.is_valid(
            assumed_context,
            &assumed_addr,
            installation_public_key,
            &assumed_time,
        )?;

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

    // The time returned is unverified, call is_valid to ensure the time is correct
    pub fn iso8601_time(&self) -> String {
        self.text.get_iso8601_time()
    }
}

impl From<Eip191Association> for Eip191AssociationProto {
    fn from(assoc: Eip191Association) -> Self {
        let wallet_address = assoc.address();
        let iso8601_time = assoc.iso8601_time();
        Self {
            wallet_address,
            // Hardcoded version for now
            association_text_version: 1,
            signature: Some(RecoverableEcdsaSignatureProto {
                bytes: assoc.signature.into(),
            }),
            iso8601_time,
        }
    }
}

/// AssociationText represents the string which was signed by the authorizing blockchain account. A
/// valid AssociationText must contain the address of the blockchain account and a representation of
/// the XMTP installation public key. Different standards may choose how this information is
/// encoded, as well as adding extra requirements for increased security.
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct AssociationText {
    context: AssociationContext,
    data: AssociationData,
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub enum AssociationContext {
    GrantMessagingAccess,
    RevokeMessagingAccess,
}

impl AssociationContext {}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
enum AssociationData {
    Static {
        account_address: Address,
        installation_public_key: Vec<u8>,
        iso8601_time: String,
    },
}

impl AssociationText {
    pub fn get_context(&self) -> AssociationContext {
        self.context.clone()
    }

    pub fn get_address(&self) -> Address {
        match &self.data {
            AssociationData::Static {
                account_address, ..
            } => account_address.clone(),
        }
    }

    pub fn get_iso8601_time(&self) -> String {
        match &self.data {
            AssociationData::Static { iso8601_time, .. } => iso8601_time.clone(),
        }
    }

    fn header_text(&self) -> String {
        let label = match &self.context {
            AssociationContext::GrantMessagingAccess => "Grant messaging access".to_string(),
            AssociationContext::RevokeMessagingAccess => "Revoke messaging access".to_string(),
        };
        format!("XMTP: {}\n\n", label)
    }

    fn body_text(&self) -> String {
        match &self.data {
            AssociationData::Static {
                account_address,
                installation_public_key,
                iso8601_time,
            } => gen_static_text_v1(account_address, installation_public_key, &iso8601_time),
        }
    }

    pub fn text(&self) -> String {
        format!("{}{}", self.header_text(), self.body_text()).to_string()
    }

    pub fn is_valid(
        &self,
        context: AssociationContext,
        account_address: &str,
        installation_public_key: &[u8],
        iso8601_time: &str,
    ) -> Result<(), AssociationError> {
        if self.text()
            == AssociationText::new_static(
                context,
                account_address.to_string(),
                installation_public_key.to_vec(),
                iso8601_time.to_string(),
            )
            .text()
        {
            return Ok(());
        }

        Err(AssociationError::TextMismatch)
    }

    pub fn new_static(
        context: AssociationContext,
        account_address: String,
        installation_public_key: Vec<u8>,
        iso8601_time: String,
    ) -> Self {
        Self {
            context,
            data: AssociationData::Static {
                account_address,
                installation_public_key,
                iso8601_time,
            },
        }
    }
}

fn gen_static_text_v1(addr: &str, key_bytes: &[u8], iso8601_time: &str) -> String {
    format!(
        "Current Time: {}\nAccount Address: {}\nInstallation ID: {}",
        iso8601_time,
        addr,
        ed25519_public_key_to_address(key_bytes)
    )
}

#[cfg(test)]
pub mod tests {
    use ethers::signers::{LocalWallet, Signer};
    use xmtp_cryptography::{signature::h160addr_to_string, utils::rng};
    use xmtp_proto::xmtp::mls::message_contents::Eip191Association as Eip191AssociationProto;

    use crate::association::AssociationContext;

    use super::{AssociationText, Eip191Association};

    #[tokio::test]
    async fn assoc_gen() {
        let key_bytes = vec![22, 33, 44, 55];

        let wallet = LocalWallet::new(&mut rng());
        let other_wallet = LocalWallet::new(&mut rng());
        let addr = h160addr_to_string(wallet.address());
        let other_addr = h160addr_to_string(other_wallet.address());
        let text = AssociationText::new_static(
            AssociationContext::GrantMessagingAccess,
            addr.clone(),
            key_bytes.clone(),
            "2021-01-01T00:00:00Z".to_string(),
        );
        let sig = wallet.sign_message(text.text()).await.expect("BadSign");

        let bad_key_bytes = vec![11, 22, 33];
        let bad_text1 = AssociationText::new_static(
            AssociationContext::GrantMessagingAccess,
            addr.clone(),
            bad_key_bytes.clone(),
            "2021-01-01T00:00:00Z".to_string(),
        );
        let bad_text2 = AssociationText::new_static(
            AssociationContext::GrantMessagingAccess,
            other_addr.clone(),
            key_bytes.clone(),
            "2021-01-01T00:00:00Z".to_string(),
        );
        let other_text = AssociationText::new_static(
            AssociationContext::GrantMessagingAccess,
            other_addr.clone(),
            key_bytes.clone(),
            "2021-01-01T00:00:00Z".to_string(),
        );

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
        let text = AssociationText::new_static(
            AssociationContext::GrantMessagingAccess,
            addr.clone(),
            key_bytes.clone(),
            "2021-01-01T00:00:00Z".to_string(),
        );
        let sig = wallet.sign_message(text.text()).await.expect("BadSign");

        let assoc = Eip191Association::new(&key_bytes, text.clone(), sig.into()).unwrap();
        let proto_signature: Eip191AssociationProto = assoc.into();

        assert_eq!(proto_signature.association_text_version, 1);
        assert_eq!(proto_signature.signature.unwrap().bytes, sig.to_vec());
    }
}
