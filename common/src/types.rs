//! Common Primitive Types that may be shared across all XMTP Crates
//! Types should not have any dependencies other than std and std-adjacent crates (like bytes)

pub type Address = String;
pub type InboxId = String;
pub type WalletAddress = String;

use std::fmt;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct InstallationId([u8; 32]);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GroupId(bytes::Bytes);

impl AsRef<[u8]> for GroupId {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Display for GroupId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:x}", self.0)
    }
}

impl std::ops::Deref for GroupId {
    type Target = bytes::Bytes;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::borrow::Borrow<[u8]> for GroupId {
    fn borrow(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl From<Vec<u8>> for GroupId {
    fn from(v: Vec<u8>) -> GroupId {
        GroupId(v.into())
    }
}

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
