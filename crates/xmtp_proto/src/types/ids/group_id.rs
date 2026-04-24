use std::{fmt, ops::Deref, str::FromStr};

use bytes::Bytes;
#[cfg(feature = "diesel")]
use diesel::{
    backend::Backend,
    deserialize::{self, FromSql, FromSqlRow},
    expression::AsExpression,
    serialize::{self, IsNull, Output, ToSql},
    sql_types::Binary,
    sqlite::Sqlite,
};
use hex::FromHexError;
use serde::{Deserialize, Serialize};

/// The canonical group identifier.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "diesel", derive(AsExpression, FromSqlRow))]
#[cfg_attr(feature = "diesel", diesel(sql_type = Binary))]
pub struct GroupId(bytes::Bytes);

impl Serialize for GroupId {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.0.as_ref().serialize(s)
    }
}

impl<'de> Deserialize<'de> for GroupId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Vec::<u8>::deserialize(d).map(GroupId::from)
    }
}

impl GroupId {
    pub fn as_slice(&self) -> &[u8] {
        self.0.as_ref()
    }

    pub fn to_openmls(&self) -> openmls::group::GroupId {
        openmls::group::GroupId::from_slice(self.as_ref())
    }

    pub fn random<R: openmls_traits::random::OpenMlsRand>(rand: &R) -> Self {
        let bytes = rand
            .random_vec(16)
            .expect("OpenMlsRand failed to produce randomness for GroupId");
        GroupId::from(bytes)
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

impl From<&[u8]> for GroupId {
    fn from(v: &[u8]) -> GroupId {
        GroupId(v.to_vec().into())
    }
}

#[cfg(feature = "diesel")]
impl ToSql<Binary, Sqlite> for GroupId
where
    [u8]: ToSql<Binary, Sqlite>,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(self.as_slice().to_vec());
        Ok(IsNull::No)
    }
}

#[cfg(feature = "diesel")]
impl FromSql<Binary, Sqlite> for GroupId
where
    Vec<u8>: FromSql<Binary, Sqlite>,
{
    fn from_sql(bytes: <Sqlite as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        Vec::<u8>::from_sql(bytes).map(GroupId::from)
    }
}

impl From<&openmls::group::GroupId> for GroupId {
    fn from(id: &openmls::group::GroupId) -> Self {
        GroupId::from(id.as_slice())
    }
}

impl From<openmls::group::GroupId> for GroupId {
    fn from(id: openmls::group::GroupId) -> Self {
        GroupId::from(id.as_slice())
    }
}

xmtp_common::if_test! {
    impl xmtp_common::Generate for GroupId {
        fn generate() -> Self {
            GroupId(xmtp_common::rand_vec::<16>().into())
        }
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
    async fn test_group_id_from_vec(#[case] input: Vec<u8>) {
        assert_eq!(GroupId::from(input.clone()).as_slice(), input.as_slice());
        assert_eq!(GroupId::from(input.clone()).as_ref(), input.as_slice());
    }

    #[rstest]
    #[case(b"test")]
    #[case(b"")]
    #[case(b"longer_test_data")]
    #[xmtp_common::test]
    async fn test_group_id_from_slice(#[case] input: &[u8]) {
        assert_eq!(GroupId::from(input).as_slice(), input);
    }

    #[xmtp_common::test]
    fn test_group_id_display_debug() {
        let data = vec![0x12, 0x34, 0xab, 0xcd];
        assert!(format!("{}", GroupId::from(data.clone())).contains("1234abcd"));
        assert!(format!("{:?}", GroupId::from(data)).contains("GroupId"));
    }

    #[xmtp_common::test]
    fn test_to_openmls_roundtrip() {
        let bytes: [u8; 16] = xmtp_common::rand_vec::<16>().try_into().unwrap();
        let xmtp_id = GroupId::from(bytes.as_slice());
        let ommls_id = xmtp_id.to_openmls();
        assert_eq!(ommls_id.as_slice(), xmtp_id.as_slice());
    }
}
