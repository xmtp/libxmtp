mod grant_messaging_access;
mod legacy_create_identity;

use crate::{types::Address, InboxOwner};
use chrono::Utc;

use openmls_basic_credential::SignatureKeyPair;

use thiserror::Error;
use xmtp_cryptography::signature::SignatureError;

use xmtp_proto::xmtp::mls::message_contents::{
    mls_credential::Association as AssociationProto, MlsCredential as MlsCredentialProto,
};

use self::grant_messaging_access::GrantMessagingAccessAssociation;
use self::legacy_create_identity::LegacyCreateIdentityAssociation;

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
