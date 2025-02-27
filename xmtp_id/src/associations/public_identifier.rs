use super::{
    member::{HasMemberKind, Passkey},
    AssociationError, MemberIdentifier, MemberKind,
};
use sha2::{Digest, Sha256};
use std::fmt::{Debug, Display};

/// A PublicIdentifier is a public-facing MemberIdentifier.
#[derive(Clone, PartialEq)]
pub enum PublicIdentifier {
    Installation(Vec<u8>),
    Ethereum(String),
    Passkey([u8; Passkey::KEY_SIZE]),
}

impl HasMemberKind for PublicIdentifier {
    fn kind(&self) -> MemberKind {
        match self {
            Self::Installation(_) => MemberKind::Installation,
            Self::Ethereum(_) => MemberKind::Ethereum,
            Self::Passkey(_) => MemberKind::Passkey,
        }
    }
}

impl PublicIdentifier {
    pub fn to_lowercase(self) -> Self {
        match self {
            Self::Ethereum(addr) => Self::Ethereum(addr.to_lowercase()),
            ident => ident,
        }
    }

    pub fn new_eth(addr: impl ToString) -> Self {
        Self::Ethereum(addr.to_string())
    }

    #[cfg(test)]
    pub fn rand_ethereum() -> Self {
        MemberIdentifier::rand_ethereum().into()
    }
}

impl Display for PublicIdentifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let addr;
        let output = match self {
            Self::Ethereum(addr) => addr,
            Self::Installation(key) => {
                addr = hex::encode(key);
                &addr
            }
            Self::Passkey(key) => {
                addr = hex::encode(key);
                &addr
            }
        };
        write!(f, "{output}")
    }
}

impl Debug for PublicIdentifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = self.kind();
        write!(f, "{kind}: {self}")
    }
}

impl From<MemberIdentifier> for PublicIdentifier {
    fn from(ident: MemberIdentifier) -> Self {
        match ident {
            MemberIdentifier::Installation(key) => Self::Installation(key),
            MemberIdentifier::Ethereum(addr) => Self::Ethereum(addr).into(),
            MemberIdentifier::Passkey(Passkey { public_key, .. }) => {
                Self::Passkey(public_key).into()
            }
        }
    }
}

impl PartialEq<MemberIdentifier> for PublicIdentifier {
    fn eq(&self, other: &MemberIdentifier) -> bool {
        match self {
            Self::Installation(key) => match other {
                MemberIdentifier::Installation(other_key) => key == other_key,
                _ => false,
            },
            Self::Ethereum(addr) => match other {
                MemberIdentifier::Ethereum(other_addr) => addr == other_addr,
                _ => false,
            },
            Self::Passkey(key) => match other {
                MemberIdentifier::Passkey(Passkey {
                    public_key: other_key,
                    ..
                }) => key == other_key,
                _ => false,
            },
        }
    }
}
impl PartialEq<PublicIdentifier> for MemberIdentifier {
    fn eq(&self, other: &PublicIdentifier) -> bool {
        other == self
    }
}
