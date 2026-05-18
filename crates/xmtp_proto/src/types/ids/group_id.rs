use std::{borrow::Borrow, fmt, str::FromStr};

#[cfg(feature = "diesel")]
use diesel::{
    backend::Backend,
    deserialize::{self, FromSql, FromSqlRow},
    expression::AsExpression,
    serialize::{self, IsNull, Output, ToSql},
    sql_types::Binary,
    sqlite::Sqlite,
};
use serde::{Deserialize, Serialize};

use crate::ConversionError;

/// The canonical group identifier. Exactly 16 bytes, by protocol invariant.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "diesel", derive(AsExpression, FromSqlRow))]
#[cfg_attr(feature = "diesel", diesel(sql_type = Binary))]
pub struct GroupId([u8; 16]);

impl GroupId {
    /// `GroupId([0u8; 16])` — sentinel / placeholder. Same as `GroupId::default()`.
    pub const ZERO: GroupId = GroupId([0u8; 16]);
    /// `GroupId([1u8; 16])` — convenience constant for tests.
    pub const ONE: GroupId = GroupId([1u8; 16]);
    /// `GroupId([2u8; 16])` — convenience constant for tests.
    pub const TWO: GroupId = GroupId([2u8; 16]);
    /// `GroupId([3u8; 16])` — convenience constant for tests.
    pub const THREE: GroupId = GroupId([3u8; 16]);
    /// `GroupId([4u8; 16])` — convenience constant for tests.
    pub const FOUR: GroupId = GroupId([4u8; 16]);

    /// Borrowed byte slice view over the underlying 16 bytes.
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }

    /// Borrowed reference to the underlying 16-byte array.
    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Consume the `GroupId` and return its raw bytes.
    pub fn into_bytes(self) -> [u8; 16] {
        self.0
    }

    /// Convert to an owned `Vec<u8>` of the 16 bytes.
    pub fn to_vec(self) -> Vec<u8> {
        self.0.to_vec()
    }

    /// Convert to an `openmls::group::GroupId`.
    pub fn to_openmls(self) -> openmls::group::GroupId {
        openmls::group::GroupId::from_slice(&self.0)
    }

    /// Construct a `GroupId` containing 16 random bytes drawn from `rand`.
    pub fn random<R: openmls_traits::random::OpenMlsRand>(rand: &R) -> Self {
        let mut bytes = [0u8; 16];
        let v = rand
            .random_vec(16)
            .expect("OpenMlsRand failed to produce randomness for GroupId");
        bytes.copy_from_slice(&v);
        GroupId(bytes)
    }
}

// --- Infallible constructors -------------------------------------------------

impl From<[u8; 16]> for GroupId {
    fn from(v: [u8; 16]) -> Self {
        GroupId(v)
    }
}

impl From<&[u8; 16]> for GroupId {
    fn from(v: &[u8; 16]) -> Self {
        GroupId(*v)
    }
}

// --- Fallible constructors ---------------------------------------------------

impl TryFrom<Vec<u8>> for GroupId {
    type Error = ConversionError;
    fn try_from(v: Vec<u8>) -> Result<Self, Self::Error> {
        Ok(GroupId(v.as_slice().try_into()?))
    }
}

impl TryFrom<&[u8]> for GroupId {
    type Error = ConversionError;
    fn try_from(v: &[u8]) -> Result<Self, Self::Error> {
        let bytes: [u8; 16] = v.try_into()?;
        Ok(GroupId(bytes))
    }
}

impl TryFrom<&openmls::group::GroupId> for GroupId {
    type Error = ConversionError;
    fn try_from(id: &openmls::group::GroupId) -> Result<Self, Self::Error> {
        GroupId::try_from(id.as_slice())
    }
}

impl TryFrom<openmls::group::GroupId> for GroupId {
    type Error = ConversionError;
    fn try_from(id: openmls::group::GroupId) -> Result<Self, Self::Error> {
        GroupId::try_from(id.as_slice())
    }
}

// --- Outward conversions -----------------------------------------------------

impl From<GroupId> for Vec<u8> {
    fn from(id: GroupId) -> Vec<u8> {
        id.0.to_vec()
    }
}

impl AsRef<[u8]> for GroupId {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Borrow<[u8]> for GroupId {
    fn borrow(&self) -> &[u8] {
        &self.0
    }
}

// --- Display / Debug ---------------------------------------------------------

impl fmt::Display for GroupId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

impl fmt::Debug for GroupId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("GroupId")
            .field(&xmtp_common::fmt::debug_hex(self.0))
            .finish()
    }
}

// --- FromStr / parse error ---------------------------------------------------

/// Error returned by `<GroupId as FromStr>::from_str`.
#[derive(Debug, thiserror::Error)]
pub enum GroupIdParseError {
    /// Input string was not valid hexadecimal.
    #[error(transparent)]
    Hex(#[from] hex::FromHexError),
    /// Decoded byte length was not 16.
    #[error(transparent)]
    Length(#[from] ConversionError),
}

impl FromStr for GroupId {
    type Err = GroupIdParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = hex::decode(s)?;
        Ok(GroupId::try_from(bytes)?)
    }
}

// --- PartialEq family --------------------------------------------------------

impl PartialEq<Vec<u8>> for GroupId {
    fn eq(&self, other: &Vec<u8>) -> bool {
        self.0.eq(&other[..])
    }
}

impl PartialEq<GroupId> for Vec<u8> {
    fn eq(&self, other: &GroupId) -> bool {
        other.0.eq(&self[..])
    }
}

impl PartialEq<&Vec<u8>> for GroupId {
    fn eq(&self, other: &&Vec<u8>) -> bool {
        self.0.eq(&other[..])
    }
}

impl PartialEq<GroupId> for &Vec<u8> {
    fn eq(&self, other: &GroupId) -> bool {
        other.0.eq(&self[..])
    }
}

impl PartialEq<[u8]> for GroupId {
    fn eq(&self, other: &[u8]) -> bool {
        self.0.eq(other)
    }
}

impl PartialEq<GroupId> for [u8] {
    fn eq(&self, other: &GroupId) -> bool {
        other.0.eq(self)
    }
}

impl PartialEq<[u8; 16]> for GroupId {
    fn eq(&self, other: &[u8; 16]) -> bool {
        self.0.eq(other)
    }
}

impl PartialEq<GroupId> for [u8; 16] {
    fn eq(&self, other: &GroupId) -> bool {
        other.0.eq(&self[..])
    }
}

// --- Serde -------------------------------------------------------------------

impl Serialize for GroupId {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.0.as_ref().serialize(s)
    }
}

impl<'de> Deserialize<'de> for GroupId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = Vec::<u8>::deserialize(d)?;
        GroupId::try_from(v).map_err(serde::de::Error::custom)
    }
}

// --- Diesel ------------------------------------------------------------------

#[cfg(feature = "diesel")]
impl ToSql<Binary, Sqlite> for GroupId
where
    [u8]: ToSql<Binary, Sqlite>,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(self.0.to_vec());
        Ok(IsNull::No)
    }
}

#[cfg(feature = "diesel")]
impl FromSql<Binary, Sqlite> for GroupId
where
    Vec<u8>: FromSql<Binary, Sqlite>,
{
    fn from_sql(bytes: <Sqlite as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        let v = Vec::<u8>::from_sql(bytes)?;
        GroupId::try_from(v).map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }
}

// --- Generate (test-only) ----------------------------------------------------

xmtp_common::if_test! {
    impl xmtp_common::Generate for GroupId {
        fn generate() -> Self {
            GroupId(xmtp_common::rand_array::<16>())
        }
    }
}

// --- Tests -------------------------------------------------------------------

#[cfg(test)]
mod test {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case([0u8; 16])]
    #[case([0xffu8; 16])]
    #[case([1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16])]
    #[xmtp_common::test(unwrap_try = true)]
    async fn test_group_id_from_array(#[case] input: [u8; 16]) {
        let id = GroupId::from(input);
        assert_eq!(id.as_slice(), &input);
        assert_eq!(id.as_bytes(), &input);
        assert_eq!(id.into_bytes(), input);
        assert_eq!(GroupId::from(&input).as_slice(), &input);
        assert_eq!(GroupId::from(input).to_vec(), input.to_vec());
    }

    #[rstest]
    #[case(vec![1u8; 16], true)]
    #[case(vec![1u8; 15], false)]
    #[case(vec![1u8; 17], false)]
    #[case(Vec::new(), false)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn test_group_id_try_from_vec(#[case] input: Vec<u8>, #[case] ok: bool) {
        assert_eq!(GroupId::try_from(input).is_ok(), ok);
    }

    #[rstest]
    #[case(&[1u8; 16][..], true)]
    #[case(&[1u8; 15][..], false)]
    #[case(&[1u8; 17][..], false)]
    #[case(&[][..], false)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn test_group_id_try_from_slice(#[case] input: &[u8], #[case] ok: bool) {
        assert_eq!(GroupId::try_from(input).is_ok(), ok);
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_openmls_try_from_valid() {
        let bytes: [u8; 16] = xmtp_common::rand_array::<16>();
        let ommls = openmls::group::GroupId::from_slice(&bytes);
        let xmtp_id = GroupId::try_from(&ommls).unwrap();
        assert_eq!(xmtp_id.as_slice(), &bytes);

        let ommls_owned = openmls::group::GroupId::from_slice(&bytes);
        let xmtp_id_owned = GroupId::try_from(ommls_owned).unwrap();
        assert_eq!(xmtp_id_owned.as_slice(), &bytes);
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_openmls_try_from_wrong_length() {
        let short = openmls::group::GroupId::from_slice(&[1u8; 8]);
        assert!(GroupId::try_from(&short).is_err());

        let long = openmls::group::GroupId::from_slice(&[1u8; 32]);
        assert!(GroupId::try_from(long).is_err());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_to_openmls_roundtrip() {
        let bytes: [u8; 16] = xmtp_common::rand_array::<16>();
        let xmtp_id = GroupId::from(bytes);
        let ommls_id = xmtp_id.to_openmls();
        assert_eq!(ommls_id.as_slice(), xmtp_id.as_slice());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_fromstr_success() {
        let bytes: [u8; 16] = xmtp_common::rand_array::<16>();
        let hex = hex::encode(bytes);
        let id: GroupId = hex.parse().unwrap();
        assert_eq!(id.as_slice(), &bytes);
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_fromstr_bad_hex() {
        let err = "zz".parse::<GroupId>().unwrap_err();
        assert!(matches!(err, GroupIdParseError::Hex(_)));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_fromstr_wrong_length() {
        // 6 hex chars = 3 bytes (not 16).
        let err = "abcdef".parse::<GroupId>().unwrap_err();
        assert!(matches!(err, GroupIdParseError::Length(_)));
    }

    #[rstest]
    #[case(GroupId::from([1u8; 16]), vec![1u8; 16], true)]
    #[case(GroupId::from([1u8; 16]), vec![2u8; 16], false)]
    #[case(GroupId::from([1u8; 16]), vec![1u8; 15], false)] // length mismatch
    #[xmtp_common::test(unwrap_try = true)]
    async fn test_group_id_eq_vec(#[case] id: GroupId, #[case] v: Vec<u8>, #[case] equal: bool) {
        // Each direction exercises a different PartialEq impl: by-value, by-reference,
        // and the reverse pair. The op_ref allow keeps the &v cases intentional.
        #[allow(clippy::op_ref)]
        {
            assert_eq!(id == v, equal);
            assert_eq!(v == id, equal);
            assert_eq!(id == &v, equal);
            assert_eq!(&v == id, equal);
        }
    }

    #[rstest]
    #[case(GroupId::from([1u8; 16]), [1u8; 16], true)]
    #[case(GroupId::from([1u8; 16]), [2u8; 16], false)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn test_group_id_eq_array(#[case] id: GroupId, #[case] a: [u8; 16], #[case] equal: bool) {
        assert_eq!(id == a, equal);
        assert_eq!(a == id, equal);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_group_id_eq_slice() {
        let id = GroupId::from([1u8; 16]);
        let s: &[u8] = &[1u8; 16];
        assert_eq!(id, *s);
        assert_eq!(*s, id);

        let wrong_len: &[u8] = &[1u8; 8];
        assert_ne!(id, *wrong_len);
        assert_ne!(*wrong_len, id);
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_serde_roundtrip() {
        let id = GroupId::from([7u8; 16]);
        let bytes = bincode::serialize(&id).unwrap();
        let decoded: GroupId = bincode::deserialize(&bytes).unwrap();
        assert_eq!(decoded, id);
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_serde_wrong_length_fails() {
        // Manually craft a bincode-encoded Vec<u8> of length 8.
        let bad: Vec<u8> = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let bytes = bincode::serialize(&bad).unwrap();
        assert!(bincode::deserialize::<GroupId>(&bytes).is_err());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_generate_produces_16_bytes() {
        use xmtp_common::Generate;
        let id: GroupId = GroupId::generate();
        assert_eq!(id.as_slice().len(), 16);
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_default_is_zero() {
        let id = GroupId::default();
        assert_eq!(id.as_slice(), &[0u8; 16][..]);
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_const_helpers() {
        assert_eq!(GroupId::ZERO, GroupId::default());
        assert_eq!(GroupId::ZERO.as_bytes(), &[0u8; 16]);
        assert_eq!(GroupId::ONE.as_bytes(), &[1u8; 16]);
        assert_eq!(GroupId::TWO.as_bytes(), &[2u8; 16]);
        assert_eq!(GroupId::THREE.as_bytes(), &[3u8; 16]);
        assert_eq!(GroupId::FOUR.as_bytes(), &[4u8; 16]);
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_display_debug() {
        let id = GroupId::from([0x12, 0x34, 0xab, 0xcd, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        let displayed = format!("{}", id);
        assert!(displayed.starts_with("1234abcd"));
        assert_eq!(displayed.len(), 32);
        let debugged = format!("{:?}", id);
        assert!(debugged.starts_with("GroupId("));
    }

    #[cfg(feature = "diesel")]
    mod diesel_test {
        use super::*;
        use diesel::prelude::*;

        diesel::table! {
            test_group_ids (id) {
                id -> Binary,
            }
        }

        #[derive(Insertable, Queryable)]
        #[diesel(table_name = test_group_ids)]
        struct Row {
            id: GroupId,
        }

        #[xmtp_common::test(unwrap_try = true)]
        fn test_diesel_roundtrip() {
            let mut conn = SqliteConnection::establish(":memory:").unwrap();
            diesel::sql_query("CREATE TABLE test_group_ids (id BLOB NOT NULL PRIMARY KEY)")
                .execute(&mut conn)
                .unwrap();
            let id = GroupId::from([0xabu8; 16]);
            diesel::insert_into(test_group_ids::table)
                .values(&Row { id })
                .execute(&mut conn)
                .unwrap();
            let got: Row = test_group_ids::table.first(&mut conn).unwrap();
            assert_eq!(got.id, id);
        }

        #[xmtp_common::test(unwrap_try = true)]
        fn test_diesel_wrong_length_errors() {
            let mut conn = SqliteConnection::establish(":memory:").unwrap();
            diesel::sql_query("CREATE TABLE test_group_ids (id BLOB NOT NULL PRIMARY KEY)")
                .execute(&mut conn)
                .unwrap();
            // Insert raw 8-byte blob bypassing the type.
            diesel::sql_query("INSERT INTO test_group_ids (id) VALUES (X'0102030405060708')")
                .execute(&mut conn)
                .unwrap();
            let result: Result<Row, _> = test_group_ids::table.first(&mut conn);
            assert!(result.is_err());
        }
    }
}
