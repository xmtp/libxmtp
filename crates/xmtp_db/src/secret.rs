//! [`StoredSecret`], the column type for client secrets.

use std::fmt;

use diesel::{
    backend::Backend,
    deserialize::{self, FromSql, FromSqlRow},
    expression::AsExpression,
    serialize::{self, Output, ToSql},
    sql_types::Binary,
    sqlite::Sqlite,
};
use subtle::ConstantTimeEq;
use xmtp_cryptography::secret::{Secret, Zeroizing};

/// Secret bytes in a database column. Wiped on drop; `Debug` shows only the
/// length.
///
/// Stored as the same `BLOB` as a `Vec<u8>`, and serde encodes it as a
/// `Vec<u8>`, so stored bytes do not change. Equality is constant time.
///
/// It has no `Drop` of its own, so a row struct's Diesel `Insertable` impl can
/// move it. Every move, clone and builder copy is still wiped on drop.
#[derive(Clone, AsExpression, FromSqlRow)]
#[diesel(sql_type = Binary)]
pub struct StoredSecret(Zeroizing<Vec<u8>>);

impl StoredSecret {
    /// Borrow the secret bytes.
    pub fn as_slice(&self) -> &[u8] {
        self.0.as_slice()
    }
}

impl From<Vec<u8>> for StoredSecret {
    fn from(bytes: Vec<u8>) -> Self {
        Self(Zeroizing::new(bytes))
    }
}

impl From<Zeroizing<Vec<u8>>> for StoredSecret {
    fn from(bytes: Zeroizing<Vec<u8>>) -> Self {
        Self(bytes)
    }
}

impl From<Secret> for StoredSecret {
    fn from(secret: Secret) -> Self {
        Self(secret.into())
    }
}

impl fmt::Debug for StoredSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "StoredSecret(<{} bytes>)", self.0.len())
    }
}

impl PartialEq for StoredSecret {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice().ct_eq(other.as_slice()).into()
    }
}

impl Eq for StoredSecret {}

impl serde::Serialize for StoredSecret {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Same as `Vec<u8>`: a sequence of `u8`.
        serializer.collect_seq(self.as_slice())
    }
}

impl<'de> serde::Deserialize<'de> for StoredSecret {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // `Secret` decodes like `Vec<u8>` without freeing a partial copy.
        Secret::deserialize(deserializer).map(Self::from)
    }
}

impl ToSql<Binary, Sqlite> for StoredSecret {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        // Bind the borrowed bytes; no Rust copy is made.
        <[u8] as ToSql<Binary, Sqlite>>::to_sql(self.as_slice(), out)
    }
}

impl FromSql<Binary, Sqlite> for StoredSecret {
    fn from_sql(bytes: <Sqlite as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        // The decoded buffer is exactly sized and moves straight into `Zeroizing`.
        Ok(Self(Zeroizing::new(Vec::<u8>::from_sql(bytes)?)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The column codec stores a secret exactly like a `Vec<u8>`.
    #[xmtp_common::test]
    fn serde_matches_vec() {
        use crate::{db_deserialize, db_serialize};
        let bytes: Vec<u8> = (0..=200u8).collect();
        let secret = StoredSecret::from(bytes.clone());
        let encoded = db_serialize(&secret).unwrap();
        assert_eq!(encoded, db_serialize(&bytes).unwrap());
        let decoded: StoredSecret = db_deserialize(&encoded).unwrap();
        assert_eq!(decoded.as_slice(), bytes.as_slice());
    }

    #[xmtp_common::test]
    fn equality_compares_bytes() {
        let key = [0x5au8; 32];
        assert_eq!(
            StoredSecret::from(key.to_vec()),
            StoredSecret::from(Zeroizing::new(key.to_vec()))
        );
        assert_ne!(
            StoredSecret::from(key.to_vec()),
            StoredSecret::from(key[..31].to_vec())
        );
        let mut other = key;
        other[31] ^= 1;
        assert_ne!(
            StoredSecret::from(key.to_vec()),
            StoredSecret::from(other.to_vec())
        );
    }
}
