pub type Address = String;

use std::fmt;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct InstallationId([u8; 32]);

impl fmt::Display for InstallationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

impl std::ops::Deref for InstallationId {
    type Target = [u8; 32];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<[u8]> for InstallationId {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<InstallationId> for Vec<u8> {
    fn from(value: InstallationId) -> Self {
        value.0.to_vec()
    }
}

impl From<[u8; 32]> for InstallationId {
    fn from(value: [u8; 32]) -> Self {
        InstallationId(value)
    }
}

impl PartialEq<Vec<u8>> for InstallationId {
    fn eq(&self, other: &Vec<u8>) -> bool {
        self.0.eq(&other[..])
    }
}

impl PartialEq<InstallationId> for Vec<u8> {
    fn eq(&self, other: &InstallationId) -> bool {
        other.0.eq(&self[..])
    }
}

impl PartialEq<&Vec<u8>> for InstallationId {
    fn eq(&self, other: &&Vec<u8>) -> bool {
        self.0.eq(&other[..])
    }
}

impl PartialEq<InstallationId> for &Vec<u8> {
    fn eq(&self, other: &InstallationId) -> bool {
        other.0.eq(&self[..])
    }
}

impl PartialEq<[u8]> for InstallationId {
    fn eq(&self, other: &[u8]) -> bool {
        self.0.eq(other)
    }
}

impl PartialEq<InstallationId> for [u8] {
    fn eq(&self, other: &InstallationId) -> bool {
        other.0.eq(self)
    }
}

impl PartialEq<[u8; 32]> for InstallationId {
    fn eq(&self, other: &[u8; 32]) -> bool {
        self.0.eq(other)
    }
}

impl PartialEq<InstallationId> for [u8; 32] {
    fn eq(&self, other: &InstallationId) -> bool {
        other.0.eq(&self[..])
    }
}
