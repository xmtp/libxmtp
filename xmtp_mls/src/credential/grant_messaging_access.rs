use serde::{Deserialize, Serialize};
use xmtp_cryptography::signature::{ed25519_public_key_to_address, RecoverableSignature};
use xmtp_proto::xmtp::mls::message_contents::{
    GrantMessagingAccessAssociation as GrantMessagingAccessAssociationProto,
    RecoverableEcdsaSignature as RecoverableEcdsaSignatureProto,
};

use crate::{types::Address, InboxOwner};

use super::AssociationError;

/// An Association is link between a blockchain account and an xmtp installation for the purposes of
/// authentication.
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub(super) struct GrantMessagingAccessAssociation {
    account_address: Address,
    installation_public_key: Vec<u8>,
    iso8601_time: String,
    signature: RecoverableSignature,
}

impl GrantMessagingAccessAssociation {
    pub(crate) fn new_validated(
        account_address: Address,
        installation_public_key: Vec<u8>,
        iso8601_time: String,
        signature: RecoverableSignature,
    ) -> Result<Self, AssociationError> {
        let this = Self {
            account_address,
            installation_public_key,
            iso8601_time,
            signature,
        };
        this.is_valid()?;
        Ok(this)
    }

    pub(crate) fn create(
        owner: &impl InboxOwner,
        installation_public_key: Vec<u8>,
        iso8601_time: String,
    ) -> Result<Self, AssociationError> {
        let account_address = owner.get_address();
        let text = Self::text(&account_address, &installation_public_key, &iso8601_time);
        let signature = owner.sign(&text)?;
        Self::new_validated(
            account_address,
            installation_public_key,
            iso8601_time,
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
            proto.iso8601_time,
            signature,
        )
    }

    fn is_valid(&self) -> Result<(), AssociationError> {
        let assumed_addr = self.account_address.clone();

        let addr = self.signature.recover_address(&Self::text(
            &self.account_address,
            &self.installation_public_key,
            &self.iso8601_time,
        ))?;
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

    pub fn iso8601_time(&self) -> String {
        self.iso8601_time.clone()
    }

    fn header_text() -> String {
        let label = "Grant Messaging Access".to_string();
        format!("XMTP : {}", label)
    }

    fn body_text(
        account_address: &Address,
        installation_public_key: &[u8],
        iso8601_time: &str,
    ) -> String {
        format!(
            "\nCurrent Time: {}\nAccount Address: {}\nInstallation ID: {}",
            iso8601_time,
            account_address,
            ed25519_public_key_to_address(installation_public_key)
        )
    }

    fn footer_text() -> String {
        "For more info: https://xmtp.org/signatures/".to_string()
    }

    fn text(
        account_address: &Address,
        installation_public_key: &[u8],
        iso8601_time: &str,
    ) -> String {
        format!(
            "{}\n{}\n\n{}",
            Self::header_text(),
            Self::body_text(account_address, installation_public_key, iso8601_time),
            Self::footer_text()
        )
        .to_string()
    }
}

impl From<GrantMessagingAccessAssociation> for GrantMessagingAccessAssociationProto {
    fn from(assoc: GrantMessagingAccessAssociation) -> Self {
        let account_address = assoc.address();
        let iso8601_time = assoc.iso8601_time();
        Self {
            account_address,
            // Hardcoded version for now
            association_text_version: 1,
            signature: Some(RecoverableEcdsaSignatureProto {
                bytes: assoc.signature.into(),
            }),
            iso8601_time,
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
        let grant_time = "2021-01-01T00:00:00Z";
        let bad_grant_time = "2021-01-01T00:00:01Z";

        let text = GrantMessagingAccessAssociation::text(&addr, &key_bytes, &grant_time);
        let sig = wallet.sign_message(text).await.expect("BadSign");

        let other_text =
            GrantMessagingAccessAssociation::text(&other_addr, &key_bytes, &grant_time);
        let other_sig = wallet.sign_message(other_text).await.expect("BadSign");

        let bad_key_bytes = vec![11, 22, 33];

        assert!(GrantMessagingAccessAssociation::new_validated(
            addr.clone(),
            key_bytes.clone(),
            grant_time.to_string(),
            sig.into()
        )
        .is_ok());
        assert!(GrantMessagingAccessAssociation::new_validated(
            addr.clone(),
            bad_key_bytes.clone(),
            grant_time.to_string(),
            sig.into()
        )
        .is_err());
        assert!(GrantMessagingAccessAssociation::new_validated(
            other_addr.clone(),
            key_bytes.clone(),
            grant_time.to_string(),
            sig.into()
        )
        .is_err());
        assert!(GrantMessagingAccessAssociation::new_validated(
            addr.clone(),
            key_bytes.clone(),
            bad_grant_time.to_string(),
            sig.into()
        )
        .is_err());
        assert!(GrantMessagingAccessAssociation::new_validated(
            addr.clone(),
            key_bytes.clone(),
            grant_time.to_string(),
            sig.into()
        )
        .is_err());
        assert!(GrantMessagingAccessAssociation::new_validated(
            addr.clone(),
            key_bytes.clone(),
            grant_time.to_string(),
            other_sig.into()
        )
        .is_err());
    }

    #[tokio::test]
    async fn to_proto() {
        let key_bytes = vec![22, 33, 44, 55];
        let wallet = LocalWallet::new(&mut rng());
        let addr = h160addr_to_string(wallet.address());
        let iso8601_time = "2021-01-01T00:00:00Z";
        let text = GrantMessagingAccessAssociation::text(&addr, &key_bytes, &iso8601_time);
        let sig = wallet.sign_message(text).await.expect("BadSign");

        let assoc = GrantMessagingAccessAssociation::new_validated(
            addr.clone(),
            key_bytes.clone(),
            iso8601_time.to_string(),
            sig.into(),
        )
        .unwrap();
        let proto_signature: GrantMessagingAccessAssociationProto = assoc.into();

        assert_eq!(proto_signature.association_text_version, 1);
        assert_eq!(proto_signature.signature.unwrap().bytes, sig.to_vec());
    }
}
