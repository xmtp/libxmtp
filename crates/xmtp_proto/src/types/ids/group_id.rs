use std::{borrow::Borrow, fmt, ops::Deref, str::FromStr};

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
use xmtp_configuration::{GROUP_ID_BYTES, LEGACY_GROUP_ID_BYTES};

/// The canonical group identifier: 32 bytes for a keyed group, whose id is the
/// public key of its group signing key, or 16 bytes for a legacy group known
/// only from imported archives.
///
/// Equality, ordering, and hashing use the id bytes, exactly as for `[u8]`.
#[derive(Copy, Clone)]
#[cfg_attr(feature = "diesel", derive(AsExpression, FromSqlRow))]
#[cfg_attr(feature = "diesel", diesel(sql_type = Binary))]
pub struct GroupId {
    /// `LEGACY_GROUP_ID_BYTES` or `GROUP_ID_BYTES`.
    len: u8,
    /// The id in the first `len` bytes; the rest are zero.
    bytes: [u8; GROUP_ID_BYTES],
}

const fn legacy(byte: u8) -> GroupId {
    let mut bytes = [0u8; GROUP_ID_BYTES];
    let mut i = 0;
    while i < LEGACY_GROUP_ID_BYTES {
        bytes[i] = byte;
        i += 1;
    }
    GroupId {
        len: LEGACY_GROUP_ID_BYTES as u8,
        bytes,
    }
}

impl GroupId {
    /// The 16-byte legacy id of zeros: a sentinel / placeholder. Same as `GroupId::default()`.
    pub const ZERO: GroupId = legacy(0);
    /// The 16-byte legacy id of ones, for tests.
    pub const ONE: GroupId = legacy(1);
    /// The 16-byte legacy id of twos, for tests.
    pub const TWO: GroupId = legacy(2);
    /// The 16-byte legacy id of threes, for tests.
    pub const THREE: GroupId = legacy(3);
    /// The 16-byte legacy id of fours, for tests.
    pub const FOUR: GroupId = legacy(4);

    fn from_slice(v: &[u8]) -> Result<Self, ConversionError> {
        if v.len() != GROUP_ID_BYTES && v.len() != LEGACY_GROUP_ID_BYTES {
            return Err(ConversionError::InvalidValue {
                item: "GroupId",
                expected: "a 32-byte or 16-byte group id",
                got: format!("{} bytes", v.len()),
            });
        }
        let mut bytes = [0u8; GROUP_ID_BYTES];
        bytes[..v.len()].copy_from_slice(v);
        Ok(GroupId {
            len: v.len() as u8,
            bytes,
        })
    }

    /// Borrowed byte slice view over the id bytes.
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes[..self.len as usize]
    }

    /// The id length in bytes: 32, or 16 for a legacy group.
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.len as usize
    }

    /// Whether this is a 16-byte legacy group id.
    pub fn is_legacy(&self) -> bool {
        self.len() == LEGACY_GROUP_ID_BYTES
    }

    /// Convert to an owned `Vec<u8>` of the id bytes.
    pub fn to_vec(self) -> Vec<u8> {
        self.as_slice().to_vec()
    }

    /// Convert to an `openmls::group::GroupId`.
    pub fn to_openmls(self) -> openmls::group::GroupId {
        openmls::group::GroupId::from_slice(self.as_slice())
    }

    /// Construct a 16-byte legacy `GroupId` from random bytes drawn from `rand`.
    pub fn random<R: openmls_traits::random::OpenMlsRand>(rand: &R) -> Self {
        let v = rand
            .random_vec(LEGACY_GROUP_ID_BYTES)
            .expect("OpenMlsRand failed to produce randomness for GroupId");
        GroupId::from_slice(&v).expect("random_vec returns the requested length")
    }
}

impl Default for GroupId {
    fn default() -> Self {
        GroupId::ZERO
    }
}

impl PartialEq for GroupId {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl Eq for GroupId {}

impl PartialOrd for GroupId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for GroupId {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_slice().cmp(other.as_slice())
    }
}

// Hashes as `[u8]`, so `Borrow<[u8]>` lookups find the same entry.
impl std::hash::Hash for GroupId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_slice().hash(state)
    }
}

impl Deref for GroupId {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl AsRef<[u8]> for GroupId {
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

// --- Infallible constructors -------------------------------------------------

impl From<[u8; LEGACY_GROUP_ID_BYTES]> for GroupId {
    fn from(v: [u8; LEGACY_GROUP_ID_BYTES]) -> Self {
        GroupId::from(&v)
    }
}

impl From<&[u8; LEGACY_GROUP_ID_BYTES]> for GroupId {
    fn from(v: &[u8; LEGACY_GROUP_ID_BYTES]) -> Self {
        let mut bytes = [0u8; GROUP_ID_BYTES];
        bytes[..LEGACY_GROUP_ID_BYTES].copy_from_slice(v);
        GroupId {
            len: LEGACY_GROUP_ID_BYTES as u8,
            bytes,
        }
    }
}

impl From<[u8; GROUP_ID_BYTES]> for GroupId {
    fn from(bytes: [u8; GROUP_ID_BYTES]) -> Self {
        GroupId {
            len: GROUP_ID_BYTES as u8,
            bytes,
        }
    }
}

impl From<&[u8; GROUP_ID_BYTES]> for GroupId {
    fn from(v: &[u8; GROUP_ID_BYTES]) -> Self {
        GroupId::from(*v)
    }
}

// --- Fallible constructors ---------------------------------------------------

impl TryFrom<Vec<u8>> for GroupId {
    type Error = ConversionError;
    fn try_from(v: Vec<u8>) -> Result<Self, Self::Error> {
        GroupId::from_slice(&v)
    }
}

impl TryFrom<&[u8]> for GroupId {
    type Error = ConversionError;
    fn try_from(v: &[u8]) -> Result<Self, Self::Error> {
        GroupId::from_slice(v)
    }
}

impl TryFrom<&openmls::group::GroupId> for GroupId {
    type Error = ConversionError;
    fn try_from(id: &openmls::group::GroupId) -> Result<Self, Self::Error> {
        GroupId::from_slice(id.as_slice())
    }
}

impl TryFrom<openmls::group::GroupId> for GroupId {
    type Error = ConversionError;
    fn try_from(id: openmls::group::GroupId) -> Result<Self, Self::Error> {
        GroupId::from_slice(id.as_slice())
    }
}

// --- Outward conversions -----------------------------------------------------

impl From<GroupId> for Vec<u8> {
    fn from(id: GroupId) -> Vec<u8> {
        id.to_vec()
    }
}

impl Borrow<[u8]> for GroupId {
    fn borrow(&self) -> &[u8] {
        self.as_slice()
    }
}

// --- Display / Debug ---------------------------------------------------------

impl fmt::Display for GroupId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(self.as_slice()))
    }
}

impl fmt::Debug for GroupId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("GroupId")
            .field(&xmtp_common::fmt::debug_hex(self.as_slice()))
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
    /// Decoded byte length was neither 16 nor 32.
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
        self.as_slice() == &other[..]
    }
}

impl PartialEq<GroupId> for Vec<u8> {
    fn eq(&self, other: &GroupId) -> bool {
        other.as_slice() == &self[..]
    }
}

impl PartialEq<&Vec<u8>> for GroupId {
    fn eq(&self, other: &&Vec<u8>) -> bool {
        self.as_slice() == &other[..]
    }
}

impl PartialEq<GroupId> for &Vec<u8> {
    fn eq(&self, other: &GroupId) -> bool {
        other.as_slice() == &self[..]
    }
}

impl PartialEq<[u8]> for GroupId {
    fn eq(&self, other: &[u8]) -> bool {
        self.as_slice() == other
    }
}

impl PartialEq<GroupId> for [u8] {
    fn eq(&self, other: &GroupId) -> bool {
        other.as_slice() == self
    }
}

impl PartialEq<[u8; LEGACY_GROUP_ID_BYTES]> for GroupId {
    fn eq(&self, other: &[u8; LEGACY_GROUP_ID_BYTES]) -> bool {
        self.as_slice() == &other[..]
    }
}

impl PartialEq<GroupId> for [u8; LEGACY_GROUP_ID_BYTES] {
    fn eq(&self, other: &GroupId) -> bool {
        other.as_slice() == &self[..]
    }
}

impl PartialEq<[u8; GROUP_ID_BYTES]> for GroupId {
    fn eq(&self, other: &[u8; GROUP_ID_BYTES]) -> bool {
        self.as_slice() == &other[..]
    }
}

impl PartialEq<GroupId> for [u8; GROUP_ID_BYTES] {
    fn eq(&self, other: &GroupId) -> bool {
        other.as_slice() == &self[..]
    }
}

// --- Serde -------------------------------------------------------------------

impl Serialize for GroupId {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.as_slice().serialize(s)
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
        out.set_value(self.to_vec());
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
        /// A random 16-byte legacy id, the length clients still create.
        fn generate() -> Self {
            GroupId::from(xmtp_common::rand_array::<16>())
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
    async fn test_group_id_from_legacy_array(#[case] input: [u8; 16]) {
        let id = GroupId::from(input);
        assert_eq!(id.as_slice(), &input);
        assert_eq!(id.len(), 16);
        assert!(id.is_legacy());
        assert_eq!(GroupId::from(&input).as_slice(), &input);
        assert_eq!(GroupId::from(input).to_vec(), input.to_vec());
    }

    #[rstest]
    #[case([0u8; 32])]
    #[case([0xffu8; 32])]
    #[xmtp_common::test(unwrap_try = true)]
    async fn test_group_id_from_keyed_array(#[case] input: [u8; 32]) {
        let id = GroupId::from(input);
        assert_eq!(id.as_slice(), &input);
        assert_eq!(id.len(), 32);
        assert!(!id.is_legacy());
        assert_eq!(GroupId::from(&input).as_slice(), &input);
        assert_eq!(Vec::from(id), input.to_vec());
    }

    // 16 and 32 bytes convert; every other length fails.
    #[rstest]
    #[case(vec![1u8; 16], true)]
    #[case(vec![1u8; 32], true)]
    #[case(vec![1u8; 8], false)]
    #[case(vec![1u8; 15], false)]
    #[case(vec![1u8; 17], false)]
    #[case(vec![1u8; 31], false)]
    #[case(vec![1u8; 33], false)]
    #[case(Vec::new(), false)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn test_group_id_try_from_vec(#[case] input: Vec<u8>, #[case] ok: bool) {
        assert_eq!(GroupId::try_from(input.clone()).is_ok(), ok);
        assert_eq!(GroupId::try_from(input.as_slice()).is_ok(), ok);
        if ok {
            assert_eq!(
                GroupId::try_from(input.clone()).unwrap().as_slice(),
                input.as_slice()
            );
        }
    }

    #[rstest]
    #[case(16)]
    #[case(32)]
    #[xmtp_common::test(unwrap_try = true)]
    fn test_openmls_roundtrip(#[case] len: usize) {
        let bytes = xmtp_common::rand_vec::<64>()[..len].to_vec();
        let ommls = openmls::group::GroupId::from_slice(&bytes);
        let xmtp_id = GroupId::try_from(&ommls).unwrap();
        assert_eq!(xmtp_id.as_slice(), &bytes[..]);
        assert_eq!(GroupId::try_from(ommls).unwrap().as_slice(), &bytes[..]);
        assert_eq!(xmtp_id.to_openmls().as_slice(), &bytes[..]);
    }

    #[rstest]
    #[case(8)]
    #[case(33)]
    #[xmtp_common::test(unwrap_try = true)]
    fn test_openmls_try_from_wrong_length(#[case] len: usize) {
        let ommls = openmls::group::GroupId::from_slice(&vec![1u8; len]);
        assert!(GroupId::try_from(&ommls).is_err());
        assert!(GroupId::try_from(ommls).is_err());
    }

    #[rstest]
    #[case(16)]
    #[case(32)]
    #[xmtp_common::test(unwrap_try = true)]
    fn test_hex_roundtrip(#[case] len: usize) {
        let bytes = xmtp_common::rand_vec::<64>()[..len].to_vec();
        let hex = hex::encode(&bytes);
        let id: GroupId = hex.parse().unwrap();
        assert_eq!(id.as_slice(), &bytes[..]);
        assert_eq!(id.to_string(), hex);
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_fromstr_bad_hex() {
        let err = "zz".parse::<GroupId>().unwrap_err();
        assert!(matches!(err, GroupIdParseError::Hex(_)));
    }

    #[rstest]
    #[case(8)]
    #[case(33)]
    #[xmtp_common::test(unwrap_try = true)]
    fn test_fromstr_wrong_length(#[case] len: usize) {
        let err = hex::encode(vec![0xab; len]).parse::<GroupId>().unwrap_err();
        assert!(matches!(err, GroupIdParseError::Length(_)));
    }

    #[rstest]
    #[case(GroupId::from([1u8; 16]), vec![1u8; 16], true)]
    #[case(GroupId::from([1u8; 16]), vec![2u8; 16], false)]
    #[case(GroupId::from([1u8; 16]), vec![1u8; 15], false)] // length mismatch
    #[case(GroupId::from([1u8; 16]), vec![1u8; 32], false)] // length mismatch
    #[case(GroupId::from([1u8; 32]), vec![1u8; 32], true)]
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

    /// Equality, order, and hashing follow the id bytes, so a 16-byte id is
    /// never equal to a 32-byte id that starts with it, and map lookups by
    /// `&[u8]` find the entry.
    #[xmtp_common::test(unwrap_try = true)]
    fn test_eq_ord_hash_follow_bytes() {
        use std::collections::{BTreeSet, HashMap};
        let legacy = GroupId::from([0u8; 16]);
        let keyed = GroupId::from([0u8; 32]);
        assert_ne!(legacy, keyed);
        assert!(legacy < keyed);
        assert!(GroupId::from([1u8; 16]) > keyed);
        let set: BTreeSet<_> = [keyed, legacy, GroupId::from([1u8; 16])].into();
        assert_eq!(set.len(), 3);
        let map = HashMap::from([(legacy, 1), (keyed, 2)]);
        assert_eq!(map.get(&[0u8; 16][..]), Some(&1));
        assert_eq!(map.get(&[0u8; 32][..]), Some(&2));
    }

    /// The client database stores values as CBOR.
    #[rstest]
    #[case(GroupId::from([7u8; 16]))]
    #[case(GroupId::from([7u8; 32]))]
    #[xmtp_common::test(unwrap_try = true)]
    fn test_serde_roundtrip(#[case] id: GroupId) {
        let bytes = cbor4ii::serde::to_vec(Vec::new(), &id).unwrap();
        let decoded: GroupId = cbor4ii::serde::from_slice(&bytes).unwrap();
        assert_eq!(decoded, id);
        assert_eq!(decoded.len(), id.len());
    }

    #[rstest]
    #[case(8)]
    #[case(33)]
    #[xmtp_common::test(unwrap_try = true)]
    fn test_serde_wrong_length_fails(#[case] len: usize) {
        let bad: Vec<u8> = vec![1; len];
        let bytes = cbor4ii::serde::to_vec(Vec::new(), &bad).unwrap();
        assert!(cbor4ii::serde::from_slice::<GroupId>(&bytes).is_err());
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
        assert_eq!(GroupId::ZERO.as_slice(), &[0u8; 16]);
        assert_eq!(GroupId::ONE.as_slice(), &[1u8; 16]);
        assert_eq!(GroupId::TWO.as_slice(), &[2u8; 16]);
        assert_eq!(GroupId::THREE.as_slice(), &[3u8; 16]);
        assert_eq!(GroupId::FOUR.as_slice(), &[4u8; 16]);
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_display_debug() {
        let id = GroupId::from([0x12, 0x34, 0xab, 0xcd, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        let displayed = format!("{}", id);
        assert!(displayed.starts_with("1234abcd"));
        assert_eq!(displayed.len(), 32);
        assert_eq!(GroupId::from([0xab; 32]).to_string(), "ab".repeat(32));
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

        fn connection() -> SqliteConnection {
            let mut conn = SqliteConnection::establish(":memory:").unwrap();
            diesel::sql_query("CREATE TABLE test_group_ids (id BLOB NOT NULL PRIMARY KEY)")
                .execute(&mut conn)
                .unwrap();
            conn
        }

        #[rstest]
        #[case(GroupId::from([0xabu8; 16]))]
        #[case(GroupId::from([0xabu8; 32]))]
        #[xmtp_common::test(unwrap_try = true)]
        fn test_diesel_roundtrip(#[case] id: GroupId) {
            let mut conn = connection();
            diesel::insert_into(test_group_ids::table)
                .values(&Row { id })
                .execute(&mut conn)
                .unwrap();
            let got: Row = test_group_ids::table.first(&mut conn).unwrap();
            assert_eq!(got.id, id);
            // The stored blob is the id bytes, with no padding.
            let stored: Vec<u8> = test_group_ids::table
                .select(diesel::dsl::sql::<diesel::sql_types::Binary>("id"))
                .first(&mut conn)
                .unwrap();
            assert_eq!(stored, id.to_vec());
        }

        #[rstest]
        #[case("X'0102030405060708'")]
        #[case("zeroblob(33)")]
        #[xmtp_common::test(unwrap_try = true)]
        fn test_diesel_wrong_length_errors(#[case] blob: &str) {
            let mut conn = connection();
            // Insert a raw blob of the wrong length, bypassing the type.
            diesel::sql_query(format!("INSERT INTO test_group_ids (id) VALUES ({blob})"))
                .execute(&mut conn)
                .unwrap();
            let result: Result<Row, _> = test_group_ids::table.first(&mut conn);
            assert!(result.is_err());
        }
    }
}
