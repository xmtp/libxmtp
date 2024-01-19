use chrono::DateTime;
use serde::{Deserialize, Serialize};
use xmtp_cryptography::signature::{ed25519_public_key_to_address, RecoverableSignature};
use xmtp_proto::xmtp::mls::message_contents::{
    GrantMessagingAccessAssociation as GrantMessagingAccessAssociationProto,
    RecoverableEcdsaSignature as RecoverableEcdsaSignatureProto,
};

use crate::{types::Address, utils::time::NS_IN_SEC, InboxOwner};

use super::AssociationError;

/// An Association is link between a blockchain account and an xmtp installation for the purposes of
/// authentication.
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct GrantMessagingAccessAssociation {
    account_address: Address,
    installation_public_key: Vec<u8>,
    created_ns: u64,
    signature: RecoverableSignature,
}

impl GrantMessagingAccessAssociation {
    pub(crate) fn new_validated(
        account_address: Address,
        installation_public_key: Vec<u8>,
        created_ns: u64,
        signature: RecoverableSignature,
    ) -> Result<Self, AssociationError> {
        let this = Self {
            account_address,
            installation_public_key,
            created_ns,
            signature,
        };
        this.is_valid()?;
        Ok(this)
    }

    pub(crate) fn create(
        owner: &impl InboxOwner,
        installation_public_key: Vec<u8>,
        created_ns: u64,
    ) -> Result<Self, AssociationError> {
        let account_address = owner.get_address();
        let text = Self::text(&account_address, &installation_public_key, created_ns)?;
        let signature = owner.sign(&text)?;
        Self::new_validated(
            account_address,
            installation_public_key,
            created_ns,
            signature,
        )
    }

    pub(crate) fn from_proto_validated(
        proto: GrantMessagingAccessAssociationProto,
        expected_installation_public_key: &[u8],
    ) -> Result<Self, AssociationError> {
        let signature = RecoverableSignature::Eip191Signature(proto.signature.unwrap().bytes);
        Self::new_validated(
            proto.account_address,
            expected_installation_public_key.to_vec(),
            proto.created_ns,
            signature,
        )
    }

    fn is_valid(&self) -> Result<(), AssociationError> {
        let assumed_addr = self.account_address.clone();

        let addr = self.signature.recover_address(&Self::text(
            &self.account_address,
            &self.installation_public_key,
            self.created_ns,
        )?)?;
        if assumed_addr != addr {
            Err(AssociationError::AddressMismatch {
                provided_addr: assumed_addr,
                signing_addr: addr,
            })
        } else {
            Ok(())
        }
    }

    pub fn address(&self) -> String {
        self.account_address.clone()
    }

    pub fn installation_public_key(&self) -> Vec<u8> {
        self.installation_public_key.clone()
    }

    pub fn created_ns(&self) -> u64 {
        self.created_ns
    }

    fn header_text() -> String {
        let label = "Grant Messaging Access".to_string();
        format!("XMTP : {}", label)
    }

    fn body_text(
        account_address: &Address,
        installation_public_key: &[u8],
        created_ns: u64,
    ) -> Result<String, AssociationError> {
        let created_time = DateTime::from_timestamp(
            created_ns as i64 / NS_IN_SEC,
            (created_ns as i64 % NS_IN_SEC) as u32,
        )
        .ok_or(AssociationError::MalformedAssociation)?;
        Ok(format!(
            "\nCurrent Time: {}\nAccount Address: {}\nInstallation ID: {}",
            format!("{}", created_time.format("%+")),
            account_address,
            ed25519_public_key_to_address(installation_public_key)
        ))
    }

    fn footer_text() -> String {
        "For more info: https://xmtp.org/signatures/".to_string()
    }

    fn text(
        account_address: &Address,
        installation_public_key: &[u8],
        created_ns: u64,
    ) -> Result<String, AssociationError> {
        Ok(format!(
            "{}\n{}\n\n{}",
            Self::header_text(),
            Self::body_text(account_address, installation_public_key, created_ns)?,
            Self::footer_text()
        )
        .to_string())
    }
}

impl From<GrantMessagingAccessAssociation> for GrantMessagingAccessAssociationProto {
    fn from(assoc: GrantMessagingAccessAssociation) -> Self {
        let account_address = assoc.address();
        let created_ns = assoc.created_ns();
        Self {
            account_address,
            // Hardcoded version for now
            association_text_version: 1,
            signature: Some(RecoverableEcdsaSignatureProto {
                bytes: assoc.signature.into(),
            }),
            created_ns,
        }
    }
}

#[cfg(test)]
pub mod tests {
    use ethers::signers::{LocalWallet, Signer};
    use xmtp_cryptography::{signature::h160addr_to_string, utils::rng};
    use xmtp_proto::xmtp::mls::message_contents::GrantMessagingAccessAssociation as GrantMessagingAccessAssociationProto;

    use crate::credential::grant_messaging_access::GrantMessagingAccessAssociation;

    #[tokio::test]
    async fn assoc_gen() {
        let key_bytes = vec![22, 33, 44, 55];

        let wallet = LocalWallet::new(&mut rng());
        let other_wallet = LocalWallet::new(&mut rng());
        let addr = h160addr_to_string(wallet.address());
        let other_addr = h160addr_to_string(other_wallet.address());
        let grant_time = 1609459200000000;
        let bad_grant_time = 1609459200000001;

        let text = GrantMessagingAccessAssociation::text(&addr, &key_bytes, grant_time).unwrap();
        let sig = wallet.sign_message(text).await.expect("BadSign");

        let other_text =
            GrantMessagingAccessAssociation::text(&other_addr, &key_bytes, grant_time).unwrap();
        let other_sig = wallet.sign_message(other_text).await.expect("BadSign");

        let bad_key_bytes = vec![11, 22, 33];

        assert!(GrantMessagingAccessAssociation::new_validated(
            addr.clone(),
            key_bytes.clone(),
            grant_time,
            sig.into()
        )
        .is_ok());
        assert!(GrantMessagingAccessAssociation::new_validated(
            addr.clone(),
            bad_key_bytes.clone(),
            grant_time,
            sig.into()
        )
        .is_err());
        assert!(GrantMessagingAccessAssociation::new_validated(
            other_addr.clone(),
            key_bytes.clone(),
            grant_time,
            sig.into()
        )
        .is_err());
        assert!(GrantMessagingAccessAssociation::new_validated(
            addr.clone(),
            key_bytes.clone(),
            bad_grant_time,
            sig.into()
        )
        .is_err());
        assert!(GrantMessagingAccessAssociation::new_validated(
            addr.clone(),
            key_bytes.clone(),
            grant_time,
            other_sig.into()
        )
        .is_err());
    }

    #[tokio::test]
    async fn to_proto() {
        let key_bytes = vec![22, 33, 44, 55];
        let wallet = LocalWallet::new(&mut rng());
        let addr = h160addr_to_string(wallet.address());
        let created_ns = 1609459200000000;
        let text = GrantMessagingAccessAssociation::text(&addr, &key_bytes, created_ns).unwrap();
        let sig = wallet.sign_message(text).await.expect("BadSign");

        let assoc = GrantMessagingAccessAssociation::new_validated(
            addr.clone(),
            key_bytes.clone(),
            created_ns,
            sig.into(),
        )
        .unwrap();
        let proto_signature: GrantMessagingAccessAssociationProto = assoc.into();

        assert_eq!(proto_signature.association_text_version, 1);
        assert_eq!(proto_signature.signature.unwrap().bytes, sig.to_vec());
    }
}
