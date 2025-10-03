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
impl xmtp_common::Generate for GroupId {
    fn generate() -> Self {
        GroupId(xmtp_common::rand_vec::<16>().into())
    }
}

#[cfg(test)]
mod test {
    use rstest::rstest;

    use super::*;

    #[xmtp_common::test]
    fn test_fromstr() {
        let hex = hex::encode(xmtp_common::rand_vec::<16>());
        let id: GroupId = hex.parse().unwrap();
        assert_eq!(hex::encode(&id.0), hex);
    }

    #[rstest]
    #[case(b"test_group".to_vec())]
    #[case(vec![1, 2, 3, 4, 5])]
    #[case(Vec::new())]
    #[xmtp_common::test]
    fn test_group_id_from_vec(#[case] input: Vec<u8>) {
        assert_eq!(GroupId::from(input.clone()).as_slice(), input.as_slice());
        assert_eq!(GroupId::from(input.clone()).as_ref(), input.as_slice());
    }

    #[rstest]
    #[case(b"test")]
    #[case(b"")]
    #[case(b"longer_test_data")]
    #[xmtp_common::test]
    fn test_group_id_from_slice(#[case] input: &[u8]) {
        assert_eq!(GroupId::from(input).as_slice(), input);
    }

    #[xmtp_common::test]
    fn test_group_id_display_debug() {
        let data = vec![0x12, 0x34, 0xab, 0xcd];
        assert!(format!("{}", GroupId::from(data.clone())).contains("1234abcd"));
        assert!(format!("{:?}", GroupId::from(data)).contains("GroupId"));
    }

    #[rstest]
    #[case([1u8; 32])]
    #[case([255u8; 32])]
    #[case([0u8; 32])]
    #[xmtp_common::test]
    fn test_installation_id_from_array(#[case] input: [u8; 32]) {
        assert_eq!(*InstallationId::from(input), input);
        assert_eq!(InstallationId::from(input).as_ref(), &input);
        assert_eq!(InstallationId::from(input).to_vec(), input.to_vec());
    }

    #[rstest]
    #[case(vec![1u8; 32], true)]
    #[case(vec![1u8; 31], false)]
    #[case(vec![1u8; 33], false)]
    #[case(Vec::new(), false)]
    #[xmtp_common::test]
    fn test_installation_id_try_from_vec(#[case] input: Vec<u8>, #[case] should_succeed: bool) {
        assert_eq!(InstallationId::try_from(input).is_ok(), should_succeed);
    }

    #[rstest]
    #[case(&[1u8; 32], true)]
    #[case(&[1u8; 31], false)]
    #[case(&[1u8; 33], false)]
    #[case(&[], false)]
    #[xmtp_common::test]
    fn test_installation_id_try_from_slice(#[case] input: &[u8], #[case] should_succeed: bool) {
        assert_eq!(InstallationId::try_from(input).is_ok(), should_succeed);
    }

    #[rstest]
    #[case(InstallationId::from([1u8; 32]), vec![1u8; 32], true)]
    #[case(InstallationId::from([1u8; 32]), vec![2u8; 32], false)]
    #[case(InstallationId::from([1u8; 32]), vec![1u8; 31], false)] // different length
    #[xmtp_common::test]
    fn test_installation_id_equality_with_vec(
        #[case] id: InstallationId,
        #[case] vec: Vec<u8>,
        #[case] should_equal: bool,
    ) {
        assert_eq!(id == vec, should_equal);
    }

    #[rstest]
    #[case(InstallationId::from([1u8; 32]), [1u8; 32], true)]
    #[case(InstallationId::from([1u8; 32]), [2u8; 32], false)]
    #[xmtp_common::test]
    fn test_installation_id_equality_with_array(
        #[case] id: InstallationId,
        #[case] array: [u8; 32],
        #[case] should_equal: bool,
    ) {
        assert_eq!(id == array, should_equal);
        assert_eq!(array == id, should_equal);
    }

    #[xmtp_common::test]
    fn test_installation_id_equality_with_slice() {
        let id = InstallationId::from([1u8; 32]);
        let slice: &[u8] = &[1u8; 32];
        let different_slice: &[u8] = &[2u8; 32];

        assert_eq!(id, *slice);
        assert_eq!(*slice, id);
        assert_ne!(id, *different_slice);
        assert_ne!(*different_slice, id);
    }
}
