use std::{fmt, ops::Deref};

use crate::ConversionError;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InstallationId([u8; 32]);

impl InstallationId {
    pub fn to_vec(self) -> Vec<u8> {
        self.0.to_vec()
    }
}

impl AsRef<InstallationId> for InstallationId {
    fn as_ref(&self) -> &InstallationId {
        self
    }
}

impl fmt::Display for InstallationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

impl fmt::Debug for InstallationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("InstallationId")
            .field(&xmtp_common::fmt::debug_hex(self.0))
            .finish()
    }
}

impl TryFrom<Vec<u8>> for InstallationId {
    type Error = ConversionError;
    fn try_from(v: Vec<u8>) -> Result<InstallationId, Self::Error> {
        Ok(InstallationId(v.as_slice().try_into()?))
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

impl<T> AsRef<T> for InstallationId
where
    T: ?Sized,
    <InstallationId as Deref>::Target: AsRef<T>,
{
    fn as_ref(&self) -> &T {
        self.deref().as_ref()
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

#[cfg(test)]
mod test {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case([1u8; 32])]
    #[case([255u8; 32])]
    #[case([0u8; 32])]
    #[xmtp_common::test]
    async fn test_installation_id_from_array(#[case] input: [u8; 32]) {
        assert_eq!(*InstallationId::from(input), input);
        assert_eq!(&InstallationId::from(input), &input);
        assert_eq!(InstallationId::from(input).to_vec(), input.to_vec());
    }

    #[rstest]
    #[case(vec![1u8; 32], true)]
    #[case(vec![1u8; 31], false)]
    #[case(vec![1u8; 33], false)]
    #[case(Vec::new(), false)]
    #[xmtp_common::test]
    async fn test_installation_id_try_from_vec(
        #[case] input: Vec<u8>,
        #[case] should_succeed: bool,
    ) {
        assert_eq!(InstallationId::try_from(input).is_ok(), should_succeed);
    }

    #[rstest]
    #[case(&[1u8; 32], true)]
    #[case(&[1u8; 31], false)]
    #[case(&[1u8; 33], false)]
    #[case(&[], false)]
    #[xmtp_common::test]
    async fn test_installation_id_try_from_slice(
        #[case] input: &[u8],
        #[case] should_succeed: bool,
    ) {
        assert_eq!(InstallationId::try_from(input).is_ok(), should_succeed);
    }

    #[rstest]
    #[case(InstallationId::from([1u8; 32]), vec![1u8; 32], true)]
    #[case(InstallationId::from([1u8; 32]), vec![2u8; 32], false)]
    #[case(InstallationId::from([1u8; 32]), vec![1u8; 31], false)] // different length
    #[xmtp_common::test]
    async fn test_installation_id_equality_with_vec(
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
    async fn test_installation_id_equality_with_array(
        #[case] id: InstallationId,
        #[case] array: [u8; 32],
        #[case] should_equal: bool,
    ) {
        assert_eq!(id == array, should_equal);
        assert_eq!(array == id, should_equal);
    }

    #[xmtp_common::test]
    async fn test_installation_id_equality_with_slice() {
        let id = InstallationId::from([1u8; 32]);
        let slice: &[u8] = &[1u8; 32];
        let different_slice: &[u8] = &[2u8; 32];

        assert_eq!(id, *slice);
        assert_eq!(*slice, id);
        assert_ne!(id, *different_slice);
        assert_ne!(*different_slice, id);
    }
}
