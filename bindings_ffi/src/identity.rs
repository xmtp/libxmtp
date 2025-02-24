use std::fmt::Display;

use xmtp_cryptography::signature::IdentifierValidationError;
use xmtp_id::associations::{ident, PublicIdentifier};

use crate::GenericError;

#[derive(uniffi::Enum, Hash, PartialEq, Eq, Clone)]
pub enum FfiPublicIdentifier {
    Ethereum(String),
    Passkey(Vec<u8>),
}

impl FfiPublicIdentifier {
    pub fn inbox_id(&self, nonce: u64) -> Result<String, GenericError> {
        let ident: PublicIdentifier = self.clone().try_into()?;
        Ok(ident.inbox_id(nonce)?)
    }
}

impl Display for FfiPublicIdentifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ethereum(addr) => write!(f, "{addr}"),
            Self::Passkey(key) => write!(f, "{}", hex::encode(key)),
        }
    }
}

#[allow(unused)]
#[uniffi::export]
pub fn generate_inbox_id(
    account_identifier: FfiPublicIdentifier,
    nonce: u64,
) -> Result<String, GenericError> {
    let ident: PublicIdentifier = account_identifier.try_into()?;
    Ok(ident.inbox_id(nonce)?)
}

impl TryFrom<FfiPublicIdentifier> for PublicIdentifier {
    type Error = IdentifierValidationError;
    fn try_from(ident: FfiPublicIdentifier) -> Result<Self, Self::Error> {
        let ident = match ident {
            FfiPublicIdentifier::Ethereum(addr) => Self::eth(addr)?,
            FfiPublicIdentifier::Passkey(key) => Self::passkey(key),
        };
        Ok(ident)
    }
}
impl From<PublicIdentifier> for FfiPublicIdentifier {
    fn from(ident: PublicIdentifier) -> Self {
        match ident {
            PublicIdentifier::Ethereum(ident::Ethereum(addr)) => Self::Ethereum(addr),
            PublicIdentifier::Passkey(ident::Passkey(key)) => Self::Passkey(key),
        }
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
