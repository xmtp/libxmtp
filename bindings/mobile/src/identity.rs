use std::fmt::Display;
use xmtp_cryptography::signature::IdentifierValidationError;
use xmtp_id::associations::{Identifier, ident};

use crate::{FfiError, GenericError};

#[derive(uniffi::Record, Hash, PartialEq, Eq, Clone, Debug)]
pub struct FfiIdentifier {
    pub identifier: String,
    pub identifier_kind: FfiIdentifierKind,
}

#[derive(uniffi::Enum, Hash, PartialEq, Eq, Clone, Debug)]
pub enum FfiIdentifierKind {
    Ethereum,
    Passkey,
}

impl FfiIdentifier {
    pub fn inbox_id(&self, nonce: u64) -> Result<String, FfiError> {
        let ident: Identifier = self
            .clone()
            .try_into()
            .map_err(|e: IdentifierValidationError| GenericError::AddressValidation(e))?;
        Ok(ident.inbox_id(nonce)?)
    }
}

impl Display for FfiIdentifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.identifier_kind {
            FfiIdentifierKind::Ethereum => write!(f, "{}", self.identifier),
            FfiIdentifierKind::Passkey => write!(f, "{}", hex::encode(&self.identifier)),
        }
    }
}

#[allow(unused)]
#[uniffi::export]
pub fn generate_inbox_id(
    account_identifier: FfiIdentifier,
    nonce: u64,
) -> Result<String, FfiError> {
    account_identifier.inbox_id(nonce)
}

impl From<Identifier> for FfiIdentifier {
    fn from(ident: Identifier) -> Self {
        match ident {
            Identifier::Ethereum(ident::Ethereum(addr)) => Self {
                identifier: addr,
                identifier_kind: FfiIdentifierKind::Ethereum,
            },
            Identifier::Passkey(ident::Passkey { key, .. }) => Self {
                identifier: hex::encode(key),
                identifier_kind: FfiIdentifierKind::Passkey,
            },
        }
    }
}

impl TryFrom<FfiIdentifier> for Identifier {
    type Error = IdentifierValidationError;
    fn try_from(ident: FfiIdentifier) -> Result<Self, Self::Error> {
        let ident = match ident.identifier_kind {
            FfiIdentifierKind::Ethereum => Self::eth(ident.identifier)?,
            FfiIdentifierKind::Passkey => Self::passkey_str(&ident.identifier, None)?,
        };
        Ok(ident)
    }
}

pub trait IdentityExt<T, U> {
    fn to_internal(self) -> Result<Vec<U>, IdentifierValidationError>;
}

impl IdentityExt<FfiIdentifier, Identifier> for Vec<FfiIdentifier> {
    fn to_internal(self) -> Result<Vec<Identifier>, IdentifierValidationError> {
        let ident: Result<Vec<_>, IdentifierValidationError> =
            self.into_iter().map(|ident| ident.try_into()).collect();
        ident
    }
}

pub trait FfiCollectionExt<T> {
    fn to_ffi(self) -> Vec<T>;
}
impl FfiCollectionExt<FfiIdentifier> for Vec<Identifier> {
    fn to_ffi(self) -> Vec<FfiIdentifier> {
        self.into_iter().map(Into::into).collect()
    }
}
pub trait FfiCollectionTryExt<T> {
    fn to_internal(self) -> Result<Vec<T>, IdentifierValidationError>;
}
impl FfiCollectionTryExt<Identifier> for Vec<FfiIdentifier> {
    fn to_internal(self) -> Result<Vec<Identifier>, IdentifierValidationError> {
        self.into_iter().map(|ident| ident.try_into()).collect()
    }
}
