use std::mem::Discriminant;

use crate::{types::Address, InboxOwner};
use chrono::Utc;
use ethers::etherscan::account;
use openmls_basic_credential::SignatureKeyPair;
use prost::Message;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use xmtp_cryptography::signature::{
    ed25519_public_key_to_address, RecoverableSignature, SignatureError,
};
use xmtp_proto::xmtp::message_contents::signature::Union;
use xmtp_proto::xmtp::message_contents::{
    unsigned_public_key, SignedPrivateKey as V2SignedPrivateKeyProto,
    UnsignedPublicKey as V2UnsignedPublicKeyProto,
};
use xmtp_proto::xmtp::mls::message_contents::{
    mls_credential::Association as AssociationProto,
    GrantMessagingAccessAssociation as GrantMessagingAccessAssociationProto,
    LegacyCreateIdentityAssociation as LegacyCreateIdentityAssociationProto,
    MlsCredential as MlsCredentialProto,
    RecoverableEcdsaSignature as RecoverableEcdsaSignatureProto,
    RevokeMessagingAccessAssociation as RevokeMessagingAccessAssociationProto,
};
use xmtp_v2::k256_helper;

#[derive(Debug, Error)]
pub enum AssociationError {
    #[error("bad signature")]
    BadSignature(#[from] SignatureError),
    #[error("bad legacy signature: {0}")]
    BadLegacySignature(String),
    #[error("Association text mismatch")]
    TextMismatch,
    #[error("Installation public key mismatch")]
    InstallationPublicKeyMismatch,
    #[error(
        "Address mismatch in Association: Provided:{provided_addr:?} != signed:{signing_addr:?}"
    )]
    AddressMismatch {
        provided_addr: Address,
        signing_addr: Address,
    },
    #[error("Malformed association")]
    MalformedAssociation,
}

pub enum Credential {
    GrantMessagingAccess(GrantMessagingAccessAssociation),
    LegacyCreateIdentity(LegacyCreateIdentityAssociation),
}

impl Credential {
    pub fn create(
        installation_keys: &SignatureKeyPair,
        owner: &impl InboxOwner,
    ) -> Result<Self, AssociationError> {
        let iso8601_time = format!("{}", Utc::now().format("%+"));
        let association = GrantMessagingAccessAssociation::create(
            owner,
            installation_keys.to_public_vec(),
            iso8601_time,
        )?;
        Ok(Self::GrantMessagingAccess(association))
    }

    pub fn create_legacy() -> Result<Self, AssociationError> {
        todo!()
    }

    pub fn from_proto_validated(
        proto: MlsCredentialProto,
        expected_account_address: Option<&str>, // Must validate when fetching identity updates
        expected_installation_public_key: Option<&[u8]>, // Must cross-reference against leaf node when relevant
    ) -> Result<Self, AssociationError> {
        let credential = match proto
            .association
            .ok_or(AssociationError::MalformedAssociation)?
        {
            AssociationProto::MessagingAccess(assoc) => {
                GrantMessagingAccessAssociation::from_proto_validated(
                    assoc,
                    &proto.installation_public_key,
                )
                .map(Credential::GrantMessagingAccess)
            }
            AssociationProto::LegacyCreateIdentity(assoc) => todo!(),
        }?;
        if let Some(address) = expected_account_address {
            if credential.address() != address {
                return Err(AssociationError::AddressMismatch {
                    provided_addr: address.to_string(),
                    signing_addr: credential.address(),
                });
            }
        }
        if let Some(public_key) = expected_installation_public_key {
            if credential.installation_public_key() != public_key {
                return Err(AssociationError::InstallationPublicKeyMismatch);
            }
        }
        Ok(credential)
    }

    pub fn address(&self) -> String {
        match &self {
            Credential::GrantMessagingAccess(assoc) => assoc.address(),
            Credential::LegacyCreateIdentity(assoc) => assoc.address(),
        }
    }

    pub fn installation_public_key(&self) -> Vec<u8> {
        match &self {
            Credential::GrantMessagingAccess(assoc) => assoc.installation_public_key(),
            Credential::LegacyCreateIdentity(assoc) => assoc.installation_public_key(),
        }
    }

    pub fn iso8601_time(&self) -> String {
        match &self {
            Credential::GrantMessagingAccess(assoc) => assoc.iso8601_time(),
            Credential::LegacyCreateIdentity(assoc) => assoc.iso8601_time(),
        }
    }
}

impl From<Credential> for MlsCredentialProto {
    fn from(credential: Credential) -> Self {
        Self {
            installation_public_key: credential.installation_public_key(),
            association: match credential {
                Credential::GrantMessagingAccess(assoc) => {
                    Some(AssociationProto::MessagingAccess(assoc.into()))
                }
                Credential::LegacyCreateIdentity(assoc) => todo!(),
            },
        }
    }
}

/// An Association is link between a blockchain account and an xmtp installation for the purposes of
/// authentication.
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
struct GrantMessagingAccessAssociation {
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

struct LegacyCreateIdentityAssociation {
    account_address: Address,
    installation_public_key: Vec<u8>,
    delegating_signature: Vec<u8>,
    serialized_legacy_key: Vec<u8>,
    wallet_signature: RecoverableSignature,
}

impl LegacyCreateIdentityAssociation {
    pub(crate) fn new_validated(
        installation_public_key: Vec<u8>,
        delegating_signature: Vec<u8>,
        serialized_legacy_key: Vec<u8>,
        wallet_signature: RecoverableSignature,
    ) -> Result<Self, AssociationError> {
        let account_address =
            wallet_signature.recover_address(&Self::text(&serialized_legacy_key))?;
        let this = Self {
            account_address,
            installation_public_key,
            delegating_signature,
            serialized_legacy_key,
            wallet_signature,
        };
        this.is_valid()?;
        Ok(this)
    }

    pub(crate) fn create(
        legacy_key: Vec<u8>,
        installation_public_key: Vec<u8>,
        iso8601_time: String,
    ) -> Result<Self, AssociationError> {
        // let account_address = owner.get_address();
        // let text = Self::text(&account_address, &installation_public_key, &iso8601_time);
        // let signature = owner.sign(&text)?;
        // Self::new_validated(
        //     account_address,
        //     installation_public_key,
        //     iso8601_time,
        //     signature,
        // )
    }

    pub(crate) fn from_proto_validated(
        proto: LegacyCreateIdentityAssociationProto,
        expected_installation_public_key: &[u8],
    ) -> Result<Self, AssociationError> {
        let delegating_signature = proto
            .signature
            .ok_or(AssociationError::MalformedAssociation)?
            .bytes;
        let legacy_signed_public_key_proto = proto
            .signed_legacy_create_identity_key
            .ok_or(AssociationError::MalformedAssociation)?;
        let serialized_legacy_key = legacy_signed_public_key_proto.key_bytes;
        let Union::WalletEcdsaCompact(wallet_ecdsa_compact) = legacy_signed_public_key_proto
            .signature
            .ok_or(AssociationError::MalformedAssociation)?
            .union
            .ok_or(AssociationError::MalformedAssociation)?
        else {
            return Err(AssociationError::MalformedAssociation);
        };
        let mut wallet_signature = wallet_ecdsa_compact.bytes.clone();
        wallet_signature.push(wallet_ecdsa_compact.recovery as u8); // TODO: normalize recovery ID if necessary
        Self::new_validated(
            expected_installation_public_key.to_vec(),
            delegating_signature,
            serialized_legacy_key,
            RecoverableSignature::Eip191Signature(wallet_signature),
        )
    }

    fn is_valid(&self) -> Result<(), AssociationError> {
        // Validate legacy key signs installation key
        let legacy_unsigned_public_key_proto =
            V2UnsignedPublicKeyProto::decode(self.serialized_legacy_key.as_slice())
                .or(Err(AssociationError::MalformedAssociation))?;
        let legacy_public_key_bytes = match legacy_unsigned_public_key_proto
            .union
            .ok_or(AssociationError::MalformedAssociation)?
        {
            unsigned_public_key::Union::Secp256k1Uncompressed(secp256k1_uncompressed) => {
                secp256k1_uncompressed.bytes
            }
        };
        if self.delegating_signature.len() != 65 {
            return Err(AssociationError::MalformedAssociation);
        }
        assert!(k256_helper::verify_sha256(
            &legacy_public_key_bytes,          // signed_by
            &self.installation_public_key,     // message
            &self.delegating_signature[0..64], // signature
            self.delegating_signature[64],     // recovery_id
        )
        .map_err(|err| AssociationError::BadLegacySignature(err))?); // always returns true if no error

        // Validate wallet signs legacy key
        let account_address = self
            .wallet_signature
            .recover_address(&Self::text(&self.serialized_legacy_key))?;
        if self.account_address != account_address {
            Err(AssociationError::AddressMismatch {
                provided_addr: self.account_address.clone(),
                signing_addr: account_address,
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
        todo!()
    }

    fn header_text() -> String {
        let label = "Create Identity".to_string();
        format!("XMTP : {}", label)
    }

    fn body_text(serialized_legacy_key: &[u8]) -> String {
        hex::encode(serialized_legacy_key)
    }

    fn footer_text() -> String {
        "For more info: https://xmtp.org/signatures/".to_string()
    }

    fn text(serialized_legacy_key: &[u8]) -> String {
        format!(
            "{}\n{}\n\n{}",
            Self::header_text(),
            Self::body_text(serialized_legacy_key),
            Self::footer_text()
        )
        .to_string()
    }
}

#[cfg(test)]
pub mod tests {
    use ethers::signers::{LocalWallet, Signer};
    use xmtp_cryptography::{signature::h160addr_to_string, utils::rng};
    use xmtp_proto::xmtp::mls::message_contents::MessagingAccessAssociation as MessagingAccessAssociationProto;

    use crate::association::AssociationContext;

    use super::{AssociationData, Eip191Association};

    #[tokio::test]
    async fn assoc_gen() {
        let key_bytes = vec![22, 33, 44, 55];

        let wallet = LocalWallet::new(&mut rng());
        let other_wallet = LocalWallet::new(&mut rng());
        let addr = h160addr_to_string(wallet.address());
        let other_addr = h160addr_to_string(other_wallet.address());
        let grant_time = "2021-01-01T00:00:00Z";
        let bad_grant_time = "2021-01-01T00:00:01Z";
        let text = AssociationData::new_grant_messaging_access(
            addr.clone(),
            key_bytes.clone(),
            grant_time.to_string(),
        );
        let sig = wallet.sign_message(text.text()).await.expect("BadSign");

        let bad_key_bytes = vec![11, 22, 33];
        let bad_text1 = AssociationData::new_grant_messaging_access(
            addr.clone(),
            bad_key_bytes.clone(),
            grant_time.to_string(),
        );
        let bad_text2 = AssociationData::new_grant_messaging_access(
            other_addr.clone(),
            key_bytes.clone(),
            grant_time.to_string(),
        );
        let bad_text3 = AssociationData::new_grant_messaging_access(
            addr.clone(),
            key_bytes.clone(),
            bad_grant_time.to_string(),
        );
        let bad_text4 = AssociationData::new_grant_messaging_access(
            addr.clone(),
            key_bytes.clone(),
            grant_time.to_string(),
        );
        let other_text = AssociationData::new_grant_messaging_access(
            other_addr.clone(),
            key_bytes.clone(),
            grant_time.to_string(),
        );

        let other_sig = wallet
            .sign_message(other_text.text())
            .await
            .expect("BadSign");

        assert!(Eip191Association::new_validated(text.clone(), sig.into()).is_ok());
        assert!(Eip191Association::new_validated(bad_text1.clone(), sig.into()).is_err());
        assert!(Eip191Association::new_validated(bad_text2.clone(), sig.into()).is_err());
        assert!(Eip191Association::new_validated(bad_text3.clone(), sig.into()).is_err());
        assert!(Eip191Association::new_validated(bad_text4.clone(), sig.into()).is_err());
        assert!(Eip191Association::new_validated(text.clone(), other_sig.into()).is_err());
    }

    #[tokio::test]
    async fn to_proto() {
        let key_bytes = vec![22, 33, 44, 55];
        let wallet = LocalWallet::new(&mut rng());
        let addr = h160addr_to_string(wallet.address());
        let text = AssociationData::new_grant_messaging_access(
            addr.clone(),
            key_bytes.clone(),
            "2021-01-01T00:00:00Z".to_string(),
        );
        let sig = wallet.sign_message(text.text()).await.expect("BadSign");

        let assoc = Eip191Association::new_validated(text.clone(), sig.into()).unwrap();
        let proto_signature: MessagingAccessAssociationProto = assoc.into();

        assert_eq!(proto_signature.association_text_version, 1);
        assert_eq!(proto_signature.signature.unwrap().bytes, sig.to_vec());
    }
}
