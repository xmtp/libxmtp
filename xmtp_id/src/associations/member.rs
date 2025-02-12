use std::hash::Hash;

use ed25519_dalek::VerifyingKey;
use xmtp_cryptography::XmtpInstallationCredential;
use xmtp_proto::xmtp::identity::associations::Passkey as PasskeyProto;

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
    Ethereum(String),
    Installation(Vec<u8>),
    Passkey(Passkey),
}

impl MemberIdentifier {
    pub fn to_lowercase(self) -> Self {
        match self {
            Self::Ethereum(addr) => Self::Ethereum(addr.to_lowercase()),
            ident => ident,
        }
    }

    #[cfg(test)]
    pub fn rand_ethereum() -> Self {
        Self::Ethereum(xmtp_common::rand_hexstring())
    }
}

#[derive(Clone, Eq, PartialEq, Hash)]
pub struct Passkey {
    pub public_key: [u8; Self::KEY_SIZE],
    pub relying_party: String,
}
impl Passkey {
    pub const KEY_SIZE: usize = 33;
}

impl std::fmt::Debug for MemberIdentifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ethereum(addr) => f.debug_tuple("Address").field(addr).finish(),
            Self::Installation(i) => f
                .debug_tuple("Installation")
                .field(&hex::encode(i))
                .finish(),
            Self::Passkey(pk) => f
                .debug_tuple("Passkey")
                .field(&hex::encode(&pk.public_key))
                .field(&pk.relying_party)
                .finish(),
        }
    }
}

impl MemberIdentifier {
    pub fn kind(&self) -> MemberKind {
        match self {
            MemberIdentifier::Ethereum(_) => MemberKind::Ethereum,
            MemberIdentifier::Installation(_) => MemberKind::Installation,
            MemberIdentifier::Passkey(_) => MemberKind::Passkey,
        }
    }

    /// Get the value for [`MemberIdentifier::Installation`] variant.
    /// Returns `None` if the type is not the correct variant.
    pub fn installation(&self) -> Option<&[u8]> {
        if let Self::Installation(ref installation) = self {
            Some(installation)
        } else {
            None
        }
    }

    /// Get the value for [`MemberIdentifier::Address`] variant.
    /// Returns `None` if the type is not the correct variant.
    pub fn address(&self) -> Option<&str> {
        if let Self::Ethereum(ref address) = self {
            Some(address)
        } else {
            None
        }
    }

    /// Get the value for [`MemberIdentifier::Address`], consuming the [`MemberIdentifier`]
    /// in the process
    pub fn to_address(self) -> Option<String> {
        if let Self::Ethereum(address) = self {
            Some(address)
        } else {
            None
        }
    }

    /// Get the value for [`MemberIdentifier::Installation`] variant.
    /// Returns `None` if the type is not the correct variant.
    pub fn to_installation(&self) -> Option<&[u8]> {
        if let Self::Installation(ref installation) = self {
            Some(installation)
        } else {
            None
        }
    }
}

impl std::fmt::Display for MemberIdentifier {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let addr;
        let output = match self {
            MemberIdentifier::Ethereum(addr) => addr,
            MemberIdentifier::Installation(installation) => {
                addr = hex::encode(installation);
                &addr
            }
            MemberIdentifier::Passkey(passkey) => {
                addr = format!(
                    "Passkey: {}, {}",
                    hex::encode(&passkey.public_key),
                    &passkey.relying_party
                );
                &addr
            }
        };

        write!(f, "{}", output)
    }
}

impl From<String> for MemberIdentifier {
    fn from(address: String) -> Self {
        MemberIdentifier::Ethereum(address.to_lowercase())
    }
}

impl From<Vec<u8>> for MemberIdentifier {
    fn from(installation: Vec<u8>) -> Self {
        MemberIdentifier::Installation(installation)
    }
}

impl From<PasskeyProto> for MemberIdentifier {
    fn from(passkey: PasskeyProto) -> Self {
        MemberIdentifier::Passkey(passkey.into())
    }
}

impl From<VerifyingKey> for MemberIdentifier {
    fn from(installation: VerifyingKey) -> Self {
        installation.as_bytes().to_vec().into()
    }
}

impl<'a> From<&'a XmtpInstallationCredential> for MemberIdentifier {
    fn from(cred: &'a XmtpInstallationCredential) -> MemberIdentifier {
        MemberIdentifier::Installation(cred.public_slice().to_vec())
    }
}

impl From<XmtpInstallationCredential> for MemberIdentifier {
    fn from(cred: XmtpInstallationCredential) -> MemberIdentifier {
        MemberIdentifier::Installation(cred.public_slice().to_vec())
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

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use super::*;
    use xmtp_common::rand_hexstring;

    impl Default for MemberIdentifier {
        fn default() -> Self {
            MemberIdentifier::Ethereum(rand_hexstring())
        }
    }

    #[allow(clippy::derivable_impls)]
    impl Default for Member {
        fn default() -> Self {
            Self {
                identifier: MemberIdentifier::default(),
                added_by_entity: None,
                client_timestamp_ns: None,
                added_on_chain_id: None,
            }
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_identifier_comparisons() {
        let address_1 = MemberIdentifier::Ethereum("0x123".to_string());
        let address_2 = MemberIdentifier::Ethereum("0x456".to_string());
        let address_1_copy = MemberIdentifier::Ethereum("0x123".to_string());

        assert!(address_1 != address_2);
        assert!(address_1.ne(&address_2));
        assert!(address_1 == address_1_copy);

        let installation_1 = MemberIdentifier::Installation(vec![1, 2, 3]);
        let installation_2 = MemberIdentifier::Installation(vec![4, 5, 6]);
        let installation_1_copy = MemberIdentifier::Installation(vec![1, 2, 3]);

        assert!(installation_1 != installation_2);
        assert!(installation_1.ne(&installation_2));
        assert!(installation_1 == installation_1_copy);
    }
}
