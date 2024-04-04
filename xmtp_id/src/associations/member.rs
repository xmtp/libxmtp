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

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum MemberIdentifier {
    Address(String),
    Installation(Vec<u8>),
}

impl MemberIdentifier {
    pub fn kind(&self) -> MemberKind {
        match self {
            MemberIdentifier::Address(_) => MemberKind::Address,
            MemberIdentifier::Installation(_) => MemberKind::Installation,
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
        MemberIdentifier::Address(address)
    }
}

impl From<Vec<u8>> for MemberIdentifier {
    fn from(installation: Vec<u8>) -> Self {
        MemberIdentifier::Installation(installation)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Member {
    pub identifier: MemberIdentifier,
    pub added_by_entity: Option<MemberIdentifier>,
}

impl Member {
    pub fn new(identifier: MemberIdentifier, added_by_entity: Option<MemberIdentifier>) -> Self {
        Self {
            identifier,
            added_by_entity,
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
mod tests {
    use crate::associations::test_utils;

    use super::*;

    use test_utils::rand_string;

    impl Default for MemberIdentifier {
        fn default() -> Self {
            MemberIdentifier::Address(rand_string())
        }
    }

    impl Default for Member {
        fn default() -> Self {
            Self {
                identifier: MemberIdentifier::default(),
                added_by_entity: None,
            }
        }
    }

    #[test]
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
