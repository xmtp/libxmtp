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

#[derive(uniffi::Record, Hash, PartialEq, Eq, Clone)]
pub struct FfiRootIdentifier {
    pub identifier: String,
    pub identifier_kind: FfiRootIdentifierKind,
    pub relying_partner: Option<String>,
}

#[derive(uniffi::Enum, Hash, PartialEq, Eq, Clone)]
pub enum FfiRootIdentifierKind {
    Ethereum,
    Passkey,
}

impl FfiPublicIdentifier {
    pub fn to_root(self) -> Option<FfiRootIdentifier> {
        Some(FfiRootIdentifier {
            identifier: self.identifier,
            identifier_kind: self.identifier_kind.to_root()?,
            relying_partner: self.relying_partner,
        })
    }
}

impl FfiRootIdentifier {
    pub fn inbox_id(&self, nonce: u64) -> Result<String, GenericError> {
        let ident: PublicIdentifier = self
            .clone()
            .to_public()
            .try_into()
            .map_err(GenericError::from_error)?;
        Ok(ident.inbox_id(nonce)?)
    }
}

impl FfiPublicIdentifierKind {
    fn to_root(self) -> Option<FfiRootIdentifierKind> {
        Some(match self {
            Self::Ethereum => FfiRootIdentifierKind::Ethereum,
            Self::Passkey => FfiRootIdentifierKind::Passkey,
        })
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

impl Display for FfiRootIdentifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.clone().to_public())
    }
}

#[allow(unused)]
#[uniffi::export]
pub fn generate_inbox_id(
    account_identifier: FfiRootIdentifier,
    nonce: u64,
) -> Result<String, GenericError> {
    account_identifier.inbox_id(nonce)
}
impl FfiRootIdentifier {
    pub fn to_public(self) -> FfiPublicIdentifier {
        self.into()
    }
}

impl From<FfiRootIdentifier> for FfiPublicIdentifier {
    fn from(ident: FfiRootIdentifier) -> Self {
        Self {
            identifier: ident.identifier,
            identifier_kind: ident.identifier_kind.into(),
            relying_partner: ident.relying_partner,
        }
    }
}

impl From<FfiRootIdentifierKind> for FfiPublicIdentifierKind {
    fn from(kind: FfiRootIdentifierKind) -> Self {
        match kind {
            FfiRootIdentifierKind::Ethereum => Self::Ethereum,
            FfiRootIdentifierKind::Passkey => Self::Passkey,
        }
    }
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
impl TryFrom<FfiPublicIdentifier> for FfiRootIdentifier {
    type Error = IdentifierValidationError;
    fn try_from(ident: FfiPublicIdentifier) -> Result<Self, Self::Error> {
        let ident = Self {
            identifier: ident.identifier,
            identifier_kind: ident.identifier_kind.try_into()?,
            relying_partner: ident.relying_partner,
        };
        Ok(ident)
    }
}
impl TryFrom<FfiPublicIdentifierKind> for FfiRootIdentifierKind {
    type Error = IdentifierValidationError;
    fn try_from(kind: FfiPublicIdentifierKind) -> Result<Self, Self::Error> {
        let kind = match kind {
            FfiPublicIdentifierKind::Ethereum => Self::Ethereum,
            FfiPublicIdentifierKind::Passkey => Self::Passkey,
        };
        Ok(kind)
    }
}

pub trait IdentityExt<T, U> {
    fn to_internal(self) -> Result<Vec<U>, IdentifierValidationError>;
}

impl IdentityExt<FfiPublicIdentifier, PublicIdentifier> for Vec<FfiPublicIdentifier> {
    fn to_internal(self) -> Result<Vec<PublicIdentifier>, IdentifierValidationError> {
        let ident: Result<Vec<_>, IdentifierValidationError> =
            self.into_iter().map(|ident| ident.try_into()).collect();
        Ok(ident?)
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
