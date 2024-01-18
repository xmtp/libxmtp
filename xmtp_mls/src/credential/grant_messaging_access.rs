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
