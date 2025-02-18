use super::{ident, AssociationError};
use ed25519_dalek::VerifyingKey;
use sha2::{Digest, Sha256};
use std::{
    fmt::{Debug, Display},
    hash::Hash,
};
use xmtp_cryptography::XmtpInstallationCredential;

#[derive(Clone, Debug, PartialEq)]
pub enum MemberKind {
    Installation,
    Ethereum,
    Passkey,
}

impl std::fmt::Display for MemberKind {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            MemberKind::Installation => write!(f, "installation"),
            MemberKind::Ethereum => write!(f, "address"),
            MemberKind::Passkey => write!(f, "passkey"),
        }
    }
}

/// A MemberIdentifier can be either an Address or an Installation Public Key
#[derive(Clone, Eq, PartialEq, Hash)]
pub enum MemberIdentifier {
    Installation(ident::Installation),
    Ethereum(ident::Ethereum),
    Passkey(ident::Passkey),
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum SignerIdentifier {
    Installation(ident::Installation),
    Ethereum(ident::Ethereum),
    Passkey(ident::Passkey),
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum RootIdentifier {
    Ethereum(ident::Ethereum),
    Passkey(ident::Passkey),
}

impl MemberIdentifier {
    pub fn sanitize(self) -> Self {
        match self {
            Self::Ethereum(addr) => Self::Ethereum(addr.sanitize()),
            ident => ident,
        }
    }

    #[cfg(test)]
    pub fn rand_ethereum() -> Self {
        Self::Ethereum(ident::Ethereum::rand())
    }

    pub fn new_ethereum(addr: impl ToString) -> Self {
        RootIdentifier::new_ethereum(addr).into()
    }

    pub fn new_installation(key: impl Into<Vec<u8>>) -> Self {
        Self::Installation(ident::Installation(key.into()))
    }

    /// Get the value for [`MemberIdentifier::Installation`] variant.
    /// Returns `None` if the type is not the correct variant.
    pub fn installation_key(&self) -> Option<&[u8]> {
        if let Self::Installation(installation) = self {
            Some(&installation.0)
        } else {
            None
        }
    }

    /// Get the value for [`MemberIdentifier::Address`] variant.
    /// Returns `None` if the type is not the correct variant.
    pub fn eth_address(&self) -> Option<&str> {
        if let Self::Ethereum(address) = self {
            Some(&address.0)
        } else {
            None
        }
    }

    /// Get the value for [`MemberIdentifier::Address`], consuming the [`MemberIdentifier`]
    /// in the process
    pub fn to_eth_address(self) -> Option<String> {
        if let Self::Ethereum(address) = self {
            Some(address.0)
        } else {
            None
        }
    }

    /// Get the value for [`MemberIdentifier::Installation`] variant.
    /// Returns `None` if the type is not the correct variant.
    pub fn to_installation(&self) -> Option<&[u8]> {
        if let Self::Installation(installation) = self {
            Some(&installation.0)
        } else {
            None
        }
    }
}

impl RootIdentifier {
    #[cfg(test)]
    pub fn rand_ethereum() -> Self {
        Self::Ethereum(ident::Ethereum::rand())
    }

    pub fn new_ethereum(addr: impl ToString) -> Self {
        Self::Ethereum(ident::Ethereum(addr.to_string()))
    }

    /// Get the generated inbox_id for this public identifier.
    /// The same public identifier will always give the same inbox_id.
    pub fn get_inbox_id(&self, nonce: u64) -> Result<String, AssociationError> {
        if !self.is_valid_address() {
            return Err(AssociationError::InvalidAccountAddress);
        }
        let ident: MemberIdentifier = self.clone().into();
        Ok(sha256_string(format!("{ident}{nonce}")))
    }

    /// Validates that the account address is exactly 42 characters, starts with "0x",
    /// and contains only valid hex digits.
    fn is_valid_address(&self) -> bool {
        match self {
            Self::Ethereum(ident::Ethereum(addr)) => {
                addr.len() == 42
                    && addr.starts_with("0x")
                    && addr[2..].chars().all(|c| c.is_ascii_hexdigit())
            }
            _ => true,
        }
    }
}

pub trait HasMemberKind {
    fn kind(&self) -> MemberKind;
}

impl HasMemberKind for MemberIdentifier {
    fn kind(&self) -> MemberKind {
        match self {
            Self::Installation(_) => MemberKind::Installation,
            Self::Ethereum(_) => MemberKind::Ethereum,
            Self::Passkey(_) => MemberKind::Passkey,
        }
    }
}

impl Display for MemberIdentifier {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Ethereum(eth) => write!(f, "{eth}"),
            Self::Installation(ident) => write!(f, "{ident}"),
            Self::Passkey(passkey) => write!(f, "{passkey}"),
        }
    }
}

impl Debug for MemberIdentifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Installation(ident::Installation(key)) => f
                .debug_tuple("Installation")
                .field(&hex::encode(key))
                .finish(),
            Self::Ethereum(ident::Ethereum(addr)) => f.debug_tuple("Address").field(addr).finish(),
            Self::Passkey(ident::Passkey(key)) => {
                f.debug_tuple("Passkey").field(&hex::encode(key)).finish()
            }
        }
    }
}

impl Display for RootIdentifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ethereum(eth) => write!(f, "{eth}"),
            Self::Passkey(passkey) => write!(f, "{passkey}"),
        }
    }
}

impl From<VerifyingKey> for MemberIdentifier {
    fn from(installation: VerifyingKey) -> Self {
        Self::Installation(ident::Installation(installation.as_bytes().to_vec()))
    }
}

impl<'a> From<&'a XmtpInstallationCredential> for MemberIdentifier {
    fn from(cred: &'a XmtpInstallationCredential) -> MemberIdentifier {
        MemberIdentifier::Installation(ident::Installation(cred.public_slice().to_vec()))
    }
}

impl From<XmtpInstallationCredential> for MemberIdentifier {
    fn from(cred: XmtpInstallationCredential) -> MemberIdentifier {
        MemberIdentifier::Installation(ident::Installation(cred.public_slice().to_vec()))
    }
}

impl From<RootIdentifier> for MemberIdentifier {
    fn from(ident: RootIdentifier) -> Self {
        match ident {
            RootIdentifier::Ethereum(addr) => Self::Ethereum(addr),
            RootIdentifier::Passkey(passkey) => Self::Passkey(passkey),
        }
    }
}

/// A Member of Inbox
#[derive(Clone, Debug, PartialEq)]
pub struct Member {
    pub identifier: MemberIdentifier,
    pub added_by_entity: Option<MemberIdentifier>,
    pub client_timestamp_ns: Option<u64>,
    pub added_on_chain_id: Option<u64>,
}

impl Member {
    pub fn new(
        identifier: MemberIdentifier,
        added_by_entity: Option<MemberIdentifier>,
        client_timestamp_ns: Option<u64>,
        added_on_chain_id: Option<u64>,
    ) -> Self {
        Self {
            identifier,
            added_by_entity,
            client_timestamp_ns,
            added_on_chain_id,
        }
    }

    pub fn kind(&self) -> MemberKind {
        self.identifier.kind()
    }
}

impl PartialEq<MemberIdentifier> for Member {
    fn eq(&self, other: &MemberIdentifier) -> bool {
        self.identifier.eq(other)
    }
}

/// Helper function to generate a SHA256 hash as a hex string.
fn sha256_string(input: String) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use super::*;

    #[allow(clippy::derivable_impls)]
    impl Default for Member {
        fn default() -> Self {
            Self {
                identifier: MemberIdentifier::rand_ethereum(),
                added_by_entity: None,
                client_timestamp_ns: None,
                added_on_chain_id: None,
            }
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_identifier_comparisons() {
        let address_1 = MemberIdentifier::new_ethereum("0x123");
        let address_2 = MemberIdentifier::new_ethereum("0x456");
        let address_1_copy = MemberIdentifier::new_ethereum("0x123");

        assert!(address_1 != address_2);
        assert!(address_1.ne(&address_2));
        assert!(address_1 == address_1_copy);

        let installation_1 = MemberIdentifier::new_installation([1, 2, 3]);
        let installation_2 = MemberIdentifier::new_installation([4, 5, 6]);
        let installation_1_copy = MemberIdentifier::new_installation([1, 2, 3]);

        assert!(installation_1 != installation_2);
        assert!(installation_1.ne(&installation_2));
        assert!(installation_1 == installation_1_copy);
    }
}
