use chrono::DateTime;
use serde::{Deserialize, Serialize};

use xmtp_cryptography::signature::{ed25519_public_key_to_address, RecoverableSignature};
use xmtp_proto::xmtp::mls::message_contents::{
    GrantMessagingAccessAssociation as GrantMessagingAccessAssociationProto,
    RecoverableEcdsaSignature as RecoverableEcdsaSignatureProto,
};

use crate::{
    types::Address,
    utils::{address::sanitize_evm_addresses, time::NS_IN_SEC},
    InboxOwner,
};

use super::AssociationError;

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct UnsignedGrantMessagingAccessData {
    pub(crate) account_address: Address,
    pub(crate) installation_public_key: Vec<u8>,
    pub(crate) created_ns: u64,
    iso8601_time: String,
}

impl UnsignedGrantMessagingAccessData {
    pub fn new(
        account_address: Address,
        installation_public_key: Vec<u8>,
        created_ns: u64,
    ) -> Result<Self, AssociationError> {
        let account_address = sanitize_evm_addresses(vec![account_address])?[0].clone();
        let created_time = DateTime::from_timestamp(
            created_ns as i64 / NS_IN_SEC,
            (created_ns as i64 % NS_IN_SEC) as u32,
        )
        .ok_or(AssociationError::MalformedAssociation)?;
        let iso8601_time = format!("{}", created_time.format("%+"));

        Ok(Self {
            account_address,
            installation_public_key,
            created_ns,
            iso8601_time,
        })
    }

    pub fn account_address(&self) -> Address {
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

    pub fn text(&self) -> String {
        format!(
            "{}\n{}\n\n{}",
            Self::header_text(),
            Self::body_text(
                &self.account_address,
                &self.installation_public_key,
                &self.iso8601_time
            ),
            Self::footer_text()
        )
        .to_string()
    }
}

/// An Association is link between a blockchain account and an xmtp installation for the purposes of
/// authentication.
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct GrantMessagingAccessAssociation {
    association_data: UnsignedGrantMessagingAccessData,
    signature: RecoverableSignature,
}

impl GrantMessagingAccessAssociation {
    pub(crate) fn new_validated(
        association_data: UnsignedGrantMessagingAccessData,
        signature: RecoverableSignature,
    ) -> Result<Self, AssociationError> {
        let this = Self {
            association_data,
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
        let unsigned_data = UnsignedGrantMessagingAccessData::new(
            owner.get_address(),
            installation_public_key,
            created_ns,
        )?;
        let text = unsigned_data.text();
        let signature = owner.sign(&text)?;
        Self::new_validated(unsigned_data, signature)
    }

    pub(crate) fn from_proto_validated(
        proto: GrantMessagingAccessAssociationProto,
        expected_installation_public_key: &[u8],
    ) -> Result<Self, AssociationError> {
        let signature = RecoverableSignature::Eip191Signature(
            proto
                .signature
                .ok_or(AssociationError::MalformedAssociation)?
                .bytes,
        );
        Self::new_validated(
            UnsignedGrantMessagingAccessData::new(
                proto.account_address,
                expected_installation_public_key.to_vec(),
                proto.created_ns,
            )?,
            signature,
        )
    }

    fn is_valid(&self) -> Result<(), AssociationError> {
        let assumed_addr = self.association_data.account_address();

        let addr = self
            .signature
            .recover_address(&self.association_data.text())?;

        let sanitized_addresses = sanitize_evm_addresses(vec![assumed_addr, addr])?;
        if sanitized_addresses[0] != sanitized_addresses[1] {
            Err(AssociationError::AddressMismatch {
                provided_addr: sanitized_addresses[0].clone(),
                signing_addr: sanitized_addresses[1].clone(),
            })
        } else {
            Ok(())
        }
    }

    pub fn account_address(&self) -> String {
        self.association_data.account_address()
    }

    pub fn installation_public_key(&self) -> Vec<u8> {
        self.association_data.installation_public_key()
    }

    pub fn created_ns(&self) -> u64 {
        self.association_data.created_ns()
    }
}

impl From<GrantMessagingAccessAssociation> for GrantMessagingAccessAssociationProto {
    fn from(assoc: GrantMessagingAccessAssociation) -> Self {
        let account_address = assoc.account_address();
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

    use crate::credential::{
        grant_messaging_access_association::GrantMessagingAccessAssociation,
        UnsignedGrantMessagingAccessData,
    };

    #[tokio::test]
    async fn assoc_gen() {
        let key_bytes = vec![22, 33, 44, 55];

        let wallet = LocalWallet::new(&mut rng());
        let other_wallet = LocalWallet::new(&mut rng());
        let addr = h160addr_to_string(wallet.address());
        let other_addr = h160addr_to_string(other_wallet.address());
        let grant_time = 1609459200000000;
        let bad_grant_time = 1609459200000001;

        let data =
            UnsignedGrantMessagingAccessData::new(addr.clone(), key_bytes.clone(), grant_time)
                .unwrap();
        let sig = wallet.sign_message(data.text()).await.expect("BadSign");

        let other_data = UnsignedGrantMessagingAccessData::new(
            other_addr.clone(),
            key_bytes.clone(),
            grant_time,
        )
        .unwrap();
        let other_sig = wallet
            .sign_message(other_data.text())
            .await
            .expect("BadSign");

        let bad_key_bytes = vec![11, 22, 33];

        assert!(GrantMessagingAccessAssociation::new_validated(
            UnsignedGrantMessagingAccessData::new(addr.clone(), key_bytes.clone(), grant_time)
                .unwrap(),
            sig.into()
        )
        .is_ok());
        assert!(GrantMessagingAccessAssociation::new_validated(
            UnsignedGrantMessagingAccessData::new(addr.clone(), bad_key_bytes.clone(), grant_time)
                .unwrap(),
            sig.into()
        )
        .is_err());
        assert!(GrantMessagingAccessAssociation::new_validated(
            UnsignedGrantMessagingAccessData::new(
                other_addr.clone(),
                key_bytes.clone(),
                grant_time,
            )
            .unwrap(),
            sig.into()
        )
        .is_err());
        assert!(GrantMessagingAccessAssociation::new_validated(
            UnsignedGrantMessagingAccessData::new(addr.clone(), key_bytes.clone(), bad_grant_time)
                .unwrap(),
            sig.into()
        )
        .is_err());
        assert!(GrantMessagingAccessAssociation::new_validated(
            UnsignedGrantMessagingAccessData::new(addr.clone(), key_bytes.clone(), grant_time)
                .unwrap(),
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
        let data = UnsignedGrantMessagingAccessData::new(addr, key_bytes, created_ns).unwrap();
        let sig = wallet.sign_message(data.text()).await.expect("BadSign");

        let assoc = GrantMessagingAccessAssociation::new_validated(data, sig.into()).unwrap();
        let proto_signature: GrantMessagingAccessAssociationProto = assoc.into();

        assert_eq!(proto_signature.association_text_version, 1);
        assert_eq!(proto_signature.signature.unwrap().bytes, sig.to_vec());
    }
}
