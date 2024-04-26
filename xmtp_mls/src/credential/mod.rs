mod grant_messaging_access_association;
mod legacy_create_identity_association;

use openmls_basic_credential::SignatureKeyPair;
use prost::DecodeError;
use thiserror::Error;

use xmtp_cryptography::signature::AddressValidationError;
use xmtp_cryptography::signature::{RecoverableSignature, SignatureError};
use xmtp_proto::xmtp::mls::message_contents::{
    mls_credential::Association as AssociationProto, MlsCredential as MlsCredentialProto,
};

use crate::{types::Address, utils::time::now_ns, InboxOwner};

pub use self::grant_messaging_access_association::GrantMessagingAccessAssociation;
pub use self::grant_messaging_access_association::UnsignedGrantMessagingAccessData;
pub use self::legacy_create_identity_association::LegacyCreateIdentityAssociation;

#[derive(Debug, Error)]
pub enum AssociationError {
    #[error("bad signature")]
    BadSignature(#[from] SignatureError),
    #[error("decode error: {0}")]
    DecodeError(#[from] DecodeError),
    #[error("legacy key: {0}")]
    MalformedLegacyKey(String),
    #[error("legacy signature: {0}")]
    LegacySignature(String),
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
    #[error(transparent)]
    AddressValidationError(#[from] AddressValidationError),
    #[error("Malformed association")]
    MalformedAssociation,

    #[error(transparent)]
    IDAssociationError(#[from] xmtp_id::associations::AssociationError),
    #[error(transparent)]
    SignatureError(#[from] xmtp_id::associations::SignatureError),
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
        let created_ns = now_ns() as u64;
        let association = GrantMessagingAccessAssociation::create(
            owner,
            installation_keys.to_public_vec(),
            created_ns,
        )?;
        Ok(Self::GrantMessagingAccess(association))
    }

    pub fn create_from_external_signer(
        association_data: UnsignedGrantMessagingAccessData,
        signature: Vec<u8>,
    ) -> Result<Self, AssociationError> {
        let association = GrantMessagingAccessAssociation::new_validated(
            association_data,
            RecoverableSignature::Eip191Signature(signature),
        )?;
        Ok(Self::GrantMessagingAccess(association))
    }

    pub fn create_from_legacy(
        installation_keys: &SignatureKeyPair,
        legacy_signed_private_key: Vec<u8>,
    ) -> Result<Self, AssociationError> {
        let association = LegacyCreateIdentityAssociation::create(
            legacy_signed_private_key,
            installation_keys.to_public_vec(),
        )?;
        Ok(Self::LegacyCreateIdentity(association))
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
            AssociationProto::LegacyCreateIdentity(assoc) => {
                LegacyCreateIdentityAssociation::from_proto_validated(
                    assoc,
                    &proto.installation_public_key,
                )
                .map(Credential::LegacyCreateIdentity)
            }
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
            Credential::GrantMessagingAccess(assoc) => assoc.account_address(),
            Credential::LegacyCreateIdentity(assoc) => assoc.account_address(),
        }
    }

    pub fn installation_public_key(&self) -> Vec<u8> {
        match &self {
            Credential::GrantMessagingAccess(assoc) => assoc.installation_public_key(),
            Credential::LegacyCreateIdentity(assoc) => assoc.installation_public_key(),
        }
    }

    pub fn created_ns(&self) -> u64 {
        match &self {
            Credential::GrantMessagingAccess(assoc) => assoc.created_ns(),
            Credential::LegacyCreateIdentity(assoc) => assoc.created_ns(),
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
                Credential::LegacyCreateIdentity(assoc) => {
                    Some(AssociationProto::LegacyCreateIdentity(assoc.into()))
                }
            },
        }
    }
}
