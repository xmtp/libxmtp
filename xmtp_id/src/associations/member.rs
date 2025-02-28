use super::{ident, AssociationError, DeserializationError};
use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fmt::{Debug, Display},
    hash::Hash,
};
use xmtp_api::identity::ApiIdentifier;
use xmtp_cryptography::{signature::IdentifierValidationError, XmtpInstallationCredential};
use xmtp_proto::{
    xmtp::identity::{
        api::v1::get_inbox_ids_request::Request as GetInboxIdsRequestProto,
        associations::IdentifierKind,
    },
    ConversionError,
};

#[derive(Clone, Eq, PartialEq, Hash)]
/// All identity logic happens here
pub enum MemberIdentifier {
    Installation(ident::Installation),
    Ethereum(ident::Ethereum),
    Passkey(ident::Passkey),
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
/// MemberIdentifier without the installation variant
/// is uesd to enforce parameters.
/// Not everything in this enum will be able to sign,
/// which will be enforced on the unverified signature counterparts.
pub enum PublicIdentifier {
    Ethereum(ident::Ethereum),
    Passkey(ident::Passkey),
}

impl MemberIdentifier {
    pub fn sanitize(self) -> Result<Self, IdentifierValidationError> {
        let ident = match self {
            Self::Ethereum(addr) => Self::Ethereum(addr.sanitize()?),
            ident => ident,
        };
        Ok(ident)
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn rand_ethereum() -> Self {
        Self::Ethereum(ident::Ethereum::rand())
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn rand_installation() -> Self {
        Self::Installation(ident::Installation::rand())
    }

    pub fn eth(addr: impl ToString) -> Result<Self, IdentifierValidationError> {
        Ok(PublicIdentifier::eth(addr)?.into())
    }

    pub fn installation(key: Vec<u8>) -> Self {
        Self::Installation(ident::Installation(key))
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

impl PublicIdentifier {
    #[cfg(any(test, feature = "test-utils"))]
    pub fn rand_ethereum() -> Self {
        Self::Ethereum(ident::Ethereum::rand())
    }

    pub fn sanitize(self) -> Result<Self, IdentifierValidationError> {
        let ident = match self {
            Self::Ethereum(addr) => Self::Ethereum(addr.sanitize()?),
            ident => ident,
        };
        Ok(ident)
    }

    pub fn eth(addr: impl ToString) -> Result<Self, IdentifierValidationError> {
        Self::Ethereum(ident::Ethereum(addr.to_string())).sanitize()
    }

    pub fn passkey(key: Vec<u8>, relying_partner: Option<String>) -> Self {
        Self::Passkey(ident::Passkey {
            key,
            relying_partner,
        })
    }

    pub fn passkey_str(
        key: &str,
        relying_partner: Option<String>,
    ) -> Result<Self, IdentifierValidationError> {
        Ok(Self::Passkey(ident::Passkey {
            key: hex::decode(key)?,
            relying_partner,
        }))
    }

    pub fn from_proto(
        ident: impl AsRef<str>,
        kind: IdentifierKind,
    ) -> Result<Self, ConversionError> {
        let ident = ident.as_ref();
        let public_ident = match kind {
            IdentifierKind::Unspecified | IdentifierKind::Ethereum => {
                Self::Ethereum(ident::Ethereum(ident.to_string()))
            }
            IdentifierKind::Passkey => Self::Passkey(ident::Passkey {
                key: hex::decode(ident).map_err(|_| ConversionError::InvalidPublicKey {
                    description: "passkey",
                    value: None,
                })?,
                // TODO
                relying_partner: None,
            }),
        };
        Ok(public_ident)
    }

    /// Get the generated inbox_id for this public identifier.
    /// The same public identifier will always give the same inbox_id.
    pub fn inbox_id(&self, nonce: u64) -> Result<String, AssociationError> {
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

#[derive(Clone, Debug, PartialEq)]
pub enum MemberKind {
    Installation,
    Ethereum,
    Passkey,
}

impl Display for MemberKind {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            MemberKind::Installation => write!(f, "installation"),
            MemberKind::Ethereum => write!(f, "ethereum"),
            MemberKind::Passkey => write!(f, "passkey"),
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
impl HasMemberKind for PublicIdentifier {
    fn kind(&self) -> MemberKind {
        match self {
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
            Self::Passkey(ident::Passkey { key, .. }) => {
                f.debug_tuple("Passkey").field(&hex::encode(key)).finish()
            }
        }
    }
}

impl Display for PublicIdentifier {
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

impl From<PublicIdentifier> for MemberIdentifier {
    fn from(ident: PublicIdentifier) -> Self {
        match ident {
            PublicIdentifier::Ethereum(addr) => Self::Ethereum(addr),
            PublicIdentifier::Passkey(passkey) => Self::Passkey(passkey),
        }
    }
}
impl From<MemberIdentifier> for Option<PublicIdentifier> {
    fn from(ident: MemberIdentifier) -> Self {
        let ident = match ident {
            MemberIdentifier::Passkey(passkey) => PublicIdentifier::Passkey(passkey),
            MemberIdentifier::Ethereum(eth) => PublicIdentifier::Ethereum(eth),
            _ => {
                return None;
            }
        };
        Some(ident)
    }
}
impl From<&PublicIdentifier> for GetInboxIdsRequestProto {
    fn from(ident: &PublicIdentifier) -> Self {
        Self {
            identifier: format!("{ident}"),
            identifier_kind: {
                let kind: IdentifierKind = ident.into();
                kind as i32
            },
        }
    }
}

impl From<&PublicIdentifier> for ApiIdentifier {
    fn from(ident: &PublicIdentifier) -> Self {
        Self {
            identifier: format!("{ident}"),
            identifier_kind: ident.into(),
        }
    }
}
impl From<PublicIdentifier> for ApiIdentifier {
    fn from(ident: PublicIdentifier) -> Self {
        (&ident).into()
    }
}
impl TryFrom<ApiIdentifier> for PublicIdentifier {
    type Error = DeserializationError;
    fn try_from(ident: ApiIdentifier) -> Result<Self, Self::Error> {
        let ident = match ident.identifier_kind {
            IdentifierKind::Unspecified | IdentifierKind::Ethereum => {
                PublicIdentifier::eth(ident.identifier)?
            }
            IdentifierKind::Passkey => PublicIdentifier::Passkey(ident::Passkey {
                key: hex::decode(ident.identifier)
                    .map_err(|_| DeserializationError::InvalidPasskey)?,
                relying_partner: None,
            }),
        };
        Ok(ident)
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
impl PartialEq<MemberIdentifier> for PublicIdentifier {
    fn eq(&self, other: &MemberIdentifier) -> bool {
        match (self, other) {
            (Self::Ethereum(ident), MemberIdentifier::Ethereum(other_ident)) => {
                ident == other_ident
            }
            (Self::Passkey(ident), MemberIdentifier::Passkey(other_ident)) => ident == other_ident,
            _ => false,
        }
    }
}
impl PartialEq<PublicIdentifier> for MemberIdentifier {
    fn eq(&self, other: &PublicIdentifier) -> bool {
        other == self
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
        let address_1 = MemberIdentifier::rand_ethereum();
        let address_2 = MemberIdentifier::rand_ethereum();
        let address_1_copy = address_1.clone();

        assert!(address_1 != address_2);
        assert!(address_1.ne(&address_2));
        assert!(address_1 == address_1_copy);

        let installation_1 = MemberIdentifier::installation([1, 2, 3].to_vec());
        let installation_2 = MemberIdentifier::installation([4, 5, 6].to_vec());
        let installation_1_copy = MemberIdentifier::installation([1, 2, 3].to_vec());

        assert!(installation_1 != installation_2);
        assert!(installation_1.ne(&installation_2));
        assert!(installation_1 == installation_1_copy);
    }
}
