use std::{fmt, ops::Deref, str::FromStr};

use bytes::Bytes;
use hex::FromHexError;

use crate::ConversionError;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct InstallationId([u8; 32]);

impl InstallationId {
    pub fn to_vec(self) -> Vec<u8> {
        self.0.to_vec()
    }
}

impl fmt::Display for InstallationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct GroupId(bytes::Bytes);

impl GroupId {
    pub fn as_slice(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl fmt::Debug for GroupId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("GroupId")
            .field(&xmtp_common::fmt::debug_hex(&self.0))
            .finish()
    }
}

impl FromStr for GroupId {
    type Err = FromHexError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(GroupId(Bytes::from(hex::decode(s)?)))
    }
}

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

impl Deref for GroupId {
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

impl TryFrom<Vec<u8>> for InstallationId {
    type Error = ConversionError;
    fn try_from(v: Vec<u8>) -> Result<InstallationId, Self::Error> {
        Ok(InstallationId(v.as_slice().try_into()?))
    }
}

impl From<&[u8]> for GroupId {
    fn from(v: &[u8]) -> GroupId {
        GroupId(v.to_vec().into())
    }
}

impl TryFrom<&[u8]> for InstallationId {
    type Error = ConversionError;
    fn try_from(v: &[u8]) -> Result<InstallationId, Self::Error> {
        let bytes: [u8; 32] = v.try_into()?;
        Ok(InstallationId(bytes))
    }
}

impl Deref for InstallationId {
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

#[cfg(any(test, feature = "test-utils"))]
mod test {
    use xmtp_common::{rand_vec, Generate};

    use super::*;

    impl Generate for GroupId {
        fn generate() -> Self {
            GroupId(rand_vec::<16>().into())
        }
    }
}
