use std::fmt::Display;
use xmtp_cryptography::signature::IdentifierValidationError;
use xmtp_id::associations::{ident, PublicIdentifier};

use crate::GenericError;

#[derive(uniffi::Record, Hash, PartialEq, Eq, Clone)]
pub struct FfiPublicIdentifier {
    pub identifier: String,
    pub identifier_kind: FfiPublicIdentifierKind,
    pub relying_partner: Option<String>,
}

#[derive(uniffi::Enum, Hash, PartialEq, Eq, Clone)]
pub enum FfiPublicIdentifierKind {
    Ethereum,
    Passkey,
}

impl FfiPublicIdentifier {
    pub fn inbox_id(&self, nonce: u64) -> Result<String, GenericError> {
        let ident: PublicIdentifier = self.clone().try_into().map_err(GenericError::from_error)?;
        Ok(ident.inbox_id(nonce)?)
    }
}

impl Display for FfiPublicIdentifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.identifier_kind {
            FfiPublicIdentifierKind::Ethereum => write!(f, "{}", self.identifier),
            FfiPublicIdentifierKind::Passkey => write!(f, "{}", hex::encode(&self.identifier)),
        }
    }
}

#[allow(unused)]
#[uniffi::export]
pub fn generate_inbox_id(
    account_identifier: FfiPublicIdentifier,
    nonce: u64,
) -> Result<String, GenericError> {
    account_identifier.inbox_id(nonce)
}

impl From<PublicIdentifier> for FfiPublicIdentifier {
    fn from(ident: PublicIdentifier) -> Self {
        match ident {
            PublicIdentifier::Ethereum(ident::Ethereum(addr)) => Self {
                identifier: addr,
                identifier_kind: FfiPublicIdentifierKind::Ethereum,
                relying_partner: None,
            },
            PublicIdentifier::Passkey(ident::Passkey {
                key,
                relying_partner,
            }) => Self {
                identifier: hex::encode(key),
                identifier_kind: FfiPublicIdentifierKind::Passkey,
                relying_partner,
            },
        }
    }
}

impl TryFrom<FfiPublicIdentifier> for PublicIdentifier {
    type Error = IdentifierValidationError;
    fn try_from(ident: FfiPublicIdentifier) -> Result<Self, Self::Error> {
        let ident = match ident.identifier_kind {
            FfiPublicIdentifierKind::Ethereum => Self::eth(ident.identifier)?,
            FfiPublicIdentifierKind::Passkey => {
                Self::passkey_str(&ident.identifier, ident.relying_partner)?
            }
        };
        Ok(ident)
    }
}

pub trait IdentityExt<T, U> {
    fn to_internal(self) -> Result<Vec<U>, IdentifierValidationError>;
}

impl IdentityExt<FfiPublicIdentifier, PublicIdentifier> for Vec<FfiPublicIdentifier> {
    fn to_internal(self) -> Result<Vec<PublicIdentifier>, IdentifierValidationError> {
        let ident: Result<Vec<_>, IdentifierValidationError> =
            self.into_iter().map(|ident| ident.try_into()).collect();
        ident
    }
}

pub trait FfiCollectionExt<T> {
    fn to_ffi(self) -> Vec<T>;
}
impl FfiCollectionExt<FfiPublicIdentifier> for Vec<PublicIdentifier> {
    fn to_ffi(self) -> Vec<FfiPublicIdentifier> {
        self.into_iter().map(Into::into).collect()
    }
}
pub trait FfiCollectionTryExt<T> {
    fn to_internal(self) -> Result<Vec<T>, IdentifierValidationError>;
}
impl FfiCollectionTryExt<PublicIdentifier> for Vec<FfiPublicIdentifier> {
    fn to_internal(self) -> Result<Vec<PublicIdentifier>, IdentifierValidationError> {
        self.into_iter().map(|ident| ident.try_into()).collect()
    }
}
