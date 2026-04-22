use super::schema::identity_cache;
use super::{ConnectionExt, Sqlite};
use crate::{DbConnection, StorageError};
use crate::{Store, impl_fetch, impl_store};
use diesel::backend::Backend;
use diesel::deserialize::{self, FromSql, FromSqlRow};
use diesel::expression::AsExpression;
use diesel::serialize::{IsNull, Output, ToSql};
use diesel::sql_types::Integer;
use diesel::{Insertable, Queryable};
use diesel::{prelude::*, serialize};
use serde::{Deserialize, Serialize};
use std::any::type_name;
use std::collections::HashMap;
use xmtp_proto::ConversionError;
use xmtp_proto::xmtp::identity::associations::IdentifierKind;

#[derive(Insertable, Queryable, Debug, Clone, Deserialize, Serialize)]
#[diesel(table_name = identity_cache)]
#[diesel()]
pub struct IdentityCache {
    inbox_id: String,
    identity: String,
    identity_kind: StoredIdentityKind,
}

#[repr(i32)]
#[derive(Debug, Copy, Clone, Serialize, Deserialize, Eq, PartialEq, AsExpression, FromSqlRow)]
#[diesel(sql_type = Integer)]
/// Type of identity stored
pub enum StoredIdentityKind {
    Ethereum = 1,
    Passkey = 2,
}

impl TryFrom<IdentifierKind> for StoredIdentityKind {
    type Error = xmtp_proto::ConversionError;
    fn try_from(kind: IdentifierKind) -> Result<Self, Self::Error> {
        match kind {
            IdentifierKind::Ethereum => Ok(StoredIdentityKind::Ethereum),
            IdentifierKind::Passkey => Ok(StoredIdentityKind::Passkey),
            IdentifierKind::Unspecified => {
                Err(ConversionError::Unspecified("IdentifierKind::Unspecified"))
            }
        }
    }
}

impl TryFrom<i32> for StoredIdentityKind {
    type Error = ConversionError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(StoredIdentityKind::Ethereum),
            2 => Ok(StoredIdentityKind::Passkey),
            v => Err(ConversionError::InvalidValue {
                item: type_name::<StoredIdentityKind>(),
                expected: "a integer value of `1` or `2`",
                got: v.to_string(),
            }),
        }
    }
}

impl From<&StoredIdentityKind> for i32 {
    fn from(value: &StoredIdentityKind) -> Self {
        use StoredIdentityKind::*;
        match value {
            Ethereum => 1,
            Passkey => 2,
        }
    }
}

impl From<StoredIdentityKind> for IdentifierKind {
    fn from(value: StoredIdentityKind) -> Self {
        use StoredIdentityKind::*;
        match value {
            Ethereum => IdentifierKind::Ethereum,
            Passkey => IdentifierKind::Passkey,
        }
    }
}

impl_store!(IdentityCache, identity_cache);
impl_fetch!(IdentityCache, identity_cache);

pub trait QueryIdentityCache {
    /// Returns a HashMap of WalletAddress -> InboxId
    fn fetch_cached_inbox_ids(
        &self,
        identifiers: &[(Address, StoredIdentityKind)],
    ) -> Result<HashMap<String, String>, StorageError>;

    fn cache_inbox_id<S: ToString>(
        &self,
        kind: StoredIdentityKind,
        identity: String,
        inbox_id: S,
    ) -> Result<(), StorageError>;
}

impl<G> QueryIdentityCache for &G
where
    G: QueryIdentityCache,
{
    fn fetch_cached_inbox_ids(
        &self,
        identifiers: &[(Address, StoredIdentityKind)],
    ) -> Result<HashMap<String, String>, StorageError> {
        (**self).fetch_cached_inbox_ids(identifiers)
    }

    fn cache_inbox_id<S: ToString>(
        &self,
        kind: StoredIdentityKind,
        identity: String,
        inbox_id: S,
    ) -> Result<(), StorageError> {
        (**self).cache_inbox_id(kind, identity, inbox_id)
    }
}

type Address = String;

impl<C: ConnectionExt> QueryIdentityCache for DbConnection<C> {
    /// Returns a HashMap of WalletAddress -> InboxId
    fn fetch_cached_inbox_ids(
        &self,
        identifiers: &[(Address, StoredIdentityKind)],
    ) -> Result<HashMap<String, String>, StorageError> {
        use crate::encrypted_store::schema::identity_cache::*;

        let mut conditions = identity_cache::table.into_boxed();

        for (addr, ident) in identifiers {
            let kind: i32 = ident.into();
            let cond = identity.eq(addr).and(identity_kind.eq(kind));
            conditions = conditions.or_filter(cond);
        }

        let result = self
            .raw_query(|conn| conditions.load::<IdentityCache>(conn))?
            .into_iter()
            .map(|entry| (entry.identity, entry.inbox_id))
            .collect();
        Ok(result)
    }

    fn cache_inbox_id<S: ToString>(
        &self,
        kind: StoredIdentityKind,
        identity: String,
        inbox_id: S,
    ) -> Result<(), StorageError> {
        IdentityCache {
            inbox_id: inbox_id.to_string(),
            identity,
            identity_kind: kind,
        }
        .store(self)
    }
}

impl ToSql<Integer, Sqlite> for StoredIdentityKind
where
    i32: ToSql<Integer, Sqlite>,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(*self as i32);
        Ok(IsNull::No)
    }
}

impl FromSql<Integer, Sqlite> for StoredIdentityKind
where
    i32: FromSql<Integer, Sqlite>,
{
    fn from_sql(bytes: <Sqlite as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        match i32::from_sql(bytes)? {
            1 => Ok(Self::Ethereum),
            2 => Ok(Self::Passkey),
            x => Err(format!("Unrecognized variant {}", x).into()),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::IdentityCache;
    use crate::{
        Store, identity_cache::StoredIdentityKind, prelude::*, test_utils::with_connection,
    };

    #[derive(Clone)]
    struct MockIdentity {
        identity: String,
        inbox_id: String,
    }

    impl MockIdentity {
        fn create() -> Self {
            Self {
                identity: xmtp_common::rand_hexstring(),
                inbox_id: xmtp_common::rand_string::<32>(),
            }
        }
    }

    // Test storing duplicated wallets (same inbox_id and wallet_address)
    #[xmtp_common::test]
    fn test_store_duplicated_wallets() {
        with_connection(|conn| {
            let entry1 = IdentityCache {
                inbox_id: "test_dup".to_string(),
                identity: "wallet_dup".to_string(),
                identity_kind: StoredIdentityKind::Ethereum,
            };
            let entry2 = IdentityCache {
                inbox_id: "test_dup".to_string(),
                identity: "wallet_dup".to_string(),
                identity_kind: StoredIdentityKind::Ethereum,
            };
            entry1.store(conn).expect("Failed to store wallet");
            let result = entry2.store(conn);
            assert!(
                result.is_err(),
                "Duplicated wallet stored without error, expected failure"
            );
        })
    }

    // Test storing and fetching multiple wallet addresses with multiple keys
    #[xmtp_common::test]
    fn test_fetch_and_store_identity_cache() {
        with_connection(|conn| {
            let ident1 = MockIdentity::create();
            let ident2 = MockIdentity::create();

            conn.cache_inbox_id(
                StoredIdentityKind::Ethereum,
                ident1.identity.clone(),
                &ident1.inbox_id,
            )
            .unwrap();

            let idents = &[
                (ident1.identity.clone(), StoredIdentityKind::Ethereum),
                (ident2.identity.clone(), StoredIdentityKind::Ethereum),
            ];
            let stored_wallets = conn.fetch_cached_inbox_ids(idents).unwrap();

            // Verify that 1 entries are fetched
            assert_eq!(stored_wallets.len(), 1);

            // Verify it's the correct inbox_id
            let cached_inbox_id = stored_wallets.get(&idents[0].0).unwrap();
            assert_eq!(*cached_inbox_id, ident1.inbox_id);

            // Fetch wallets with a non-existent list of inbox_ids
            let ident = MockIdentity::create();
            let non_existent_wallets = conn
                .fetch_cached_inbox_ids(&[(ident.identity, StoredIdentityKind::Ethereum)])
                .unwrap_or_default();
            assert!(
                non_existent_wallets.is_empty(),
                "Expected no wallets, found some"
            );
        })
    }
}
