use ed25519_dalek::VerifyingKey;
use xmtp_cryptography::XmtpInstallationCredential;

#[derive(Clone, Debug, PartialEq)]
pub enum MemberKind {
    Installation,
    Address,
}

impl std::fmt::Display for MemberKind {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            MemberKind::Installation => write!(f, "installation"),
            MemberKind::Address => write!(f, "address"),
        }
    }
}

/// A MemberIdentifier can be either an Address or an Installation Public Key
#[derive(Clone, Eq, PartialEq, Hash)]
pub enum MemberIdentifier {
    Address(String),
    Installation(Vec<u8>),
}

impl std::fmt::Debug for MemberIdentifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Address(addr) => f.debug_tuple("Address").field(addr).finish(),
            Self::Installation(i) => f
                .debug_tuple("Installation")
                .field(&hex::encode(i))
                .finish(),
        }
    }
}

impl MemberIdentifier {
    pub fn kind(&self) -> MemberKind {
        match self {
            MemberIdentifier::Address(_) => MemberKind::Address,
            MemberIdentifier::Installation(_) => MemberKind::Installation,
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
        if let Self::Address(ref address) = self {
            Some(address)
        } else {
            None
        }
    }

    /// Get the value for [`MemberIdentifier::Address`], consuming the [`MemberIdentifier`]
    /// in the process
    pub fn to_address(self) -> Option<String> {
        if let Self::Address(address) = self {
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
        let as_string = match self {
            MemberIdentifier::Address(address) => address.to_string(),
            MemberIdentifier::Installation(installation) => hex::encode(installation),
        };

        write!(f, "{}", as_string)
    }
}

impl From<String> for MemberIdentifier {
    fn from(address: String) -> Self {
        MemberIdentifier::Address(address.to_lowercase())
    }
}

impl From<Vec<u8>> for MemberIdentifier {
    fn from(installation: Vec<u8>) -> Self {
        MemberIdentifier::Installation(installation)
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
            MemberIdentifier::Address(rand_hexstring())
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
        let address_1 = MemberIdentifier::Address("0x123".to_string());
        let address_2 = MemberIdentifier::Address("0x456".to_string());
        let address_1_copy = MemberIdentifier::Address("0x123".to_string());

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
