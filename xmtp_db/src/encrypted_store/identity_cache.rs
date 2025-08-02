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
use std::collections::HashMap;

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

impl_store!(IdentityCache, identity_cache);
impl_fetch!(IdentityCache, identity_cache);

pub trait QueryIdentityCache {
    /// Returns a HashMap of WalletAddress -> InboxId
    fn fetch_cached_inbox_ids<T>(
        &self,
        identifiers: &[T],
    ) -> Result<HashMap<String, String>, StorageError>
    where
        T: std::fmt::Display,
        for<'a> &'a T: Into<StoredIdentityKind>;

    fn cache_inbox_id<T, S>(&self, identifier: &T, inbox_id: S) -> Result<(), StorageError>
    where
        T: std::fmt::Display,
        S: ToString,
        for<'a> &'a T: Into<StoredIdentityKind>;
}

impl<G> QueryIdentityCache for &G
where
    G: QueryIdentityCache,
{
    fn fetch_cached_inbox_ids<T>(
        &self,
        identifiers: &[T],
    ) -> Result<HashMap<String, String>, StorageError>
    where
        T: std::fmt::Display,
        for<'a> &'a T: Into<StoredIdentityKind>,
    {
        (**self).fetch_cached_inbox_ids(identifiers)
    }

    fn cache_inbox_id<T, S>(&self, identifier: &T, inbox_id: S) -> Result<(), StorageError>
    where
        T: std::fmt::Display,
        S: ToString,
        for<'a> &'a T: Into<StoredIdentityKind>,
    {
        (**self).cache_inbox_id(identifier, inbox_id)
    }
}

impl<C: ConnectionExt> QueryIdentityCache for DbConnection<C> {
    /// Returns a HashMap of WalletAddress -> InboxId
    fn fetch_cached_inbox_ids<T>(
        &self,
        identifiers: &[T],
    ) -> Result<HashMap<String, String>, StorageError>
    where
        T: std::fmt::Display,
        for<'a> &'a T: Into<StoredIdentityKind>,
    {
        use crate::encrypted_store::schema::identity_cache::*;

        let mut conditions = identity_cache::table.into_boxed();

        for ident in identifiers {
            let addr = (&ident).to_string();
            let kind: StoredIdentityKind = ident.into();
            let cond = identity.eq(addr).and(identity_kind.eq(kind));
            conditions = conditions.or_filter(cond);
        }

        let result = self
            .raw_query_read(|conn| conditions.load::<IdentityCache>(conn))?
            .into_iter()
            .map(|entry| (entry.identity, entry.inbox_id))
            .collect();
        Ok(result)
    }

    fn cache_inbox_id<T, S>(&self, identifier: &T, inbox_id: S) -> Result<(), StorageError>
    where
        T: std::fmt::Display,
        S: ToString,
        for<'a> &'a T: Into<StoredIdentityKind>,
    {
        IdentityCache {
            inbox_id: inbox_id.to_string(),
            identity: identifier.to_string(),
            identity_kind: identifier.into(),
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
        kind: u8,
        inbox_id: String,
    }

    impl MockIdentity {
        fn create(kind: u8) -> Self {
            Self {
                identity: xmtp_common::rand_hexstring(),
                inbox_id: xmtp_common::rand_string::<32>(),
                kind,
            }
        }
    }

    impl<'a> From<&'a MockIdentity> for StoredIdentityKind {
        fn from(identity: &'a MockIdentity) -> StoredIdentityKind {
            match identity.kind {
                0 => StoredIdentityKind::Ethereum,
                1 => StoredIdentityKind::Ethereum,
                2 => StoredIdentityKind::Passkey,
                _ => panic!("unknown kind"),
            }
        }
    }

    impl std::fmt::Display for MockIdentity {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(&self.identity)
        }
    }

    // Test storing duplicated wallets (same inbox_id and wallet_address)
    #[xmtp_common::test]
    async fn test_store_duplicated_wallets() {
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
        .await
    }

    // Test storing and fetching multiple wallet addresses with multiple keys
    // TODO:insipx: will need to fix & store identity kind
    #[xmtp_common::test]
    async fn test_fetch_and_store_identity_cache() {
        with_connection(|conn| {
            let ident1 = MockIdentity::create(0);
            let ident2 = MockIdentity::create(0);

            conn.cache_inbox_id(&ident1, &ident1.inbox_id).unwrap();
            let idents = &[ident1.clone(), ident2];
            let stored_wallets = conn.fetch_cached_inbox_ids(idents).unwrap();

            // Verify that 1 entries are fetched
            assert_eq!(stored_wallets.len(), 1);

            // Verify it's the correct inbox_id
            let cached_inbox_id = stored_wallets.get(&format!("{}", idents[0])).unwrap();
            assert_eq!(*cached_inbox_id, ident1.inbox_id);

            // Fetch wallets with a non-existent list of inbox_ids
            let non_existent_wallets = conn
                .fetch_cached_inbox_ids(&[MockIdentity::create(1)])
                .unwrap_or_default();
            assert!(
                non_existent_wallets.is_empty(),
                "Expected no wallets, found some"
            );
        })
        .await
    }
}
