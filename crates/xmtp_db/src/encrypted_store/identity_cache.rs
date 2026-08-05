#[cfg(feature = "sync")]
use super::ConnectionExt;
#[cfg(feature = "sync")]
use super::schema::identity_cache;
#[cfg(feature = "sync")]
use crate::DbConnection;
use crate::StorageError;
#[cfg(feature = "sync")]
use crate::{Store, impl_fetch, impl_store};
#[cfg(feature = "sync")]
use diesel::deserialize::FromSqlRow;
#[cfg(feature = "sync")]
use diesel::expression::AsExpression;
#[cfg(feature = "sync")]
use diesel::prelude::*;
#[cfg(feature = "sync")]
use diesel::sql_types::Integer;
#[cfg(feature = "sync")]
use diesel::{Insertable, Queryable};
use serde::{Deserialize, Serialize};
use std::any::type_name;
use std::collections::HashMap;
use xmtp_proto::ConversionError;
use xmtp_proto::xmtp::identity::associations::IdentifierKind;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "sync", derive(Insertable, Queryable))]
#[cfg_attr(feature = "sync", diesel(table_name = identity_cache))]
#[cfg_attr(feature = "sync", diesel())]
pub struct IdentityCache {
    inbox_id: String,
    identity: String,
    identity_kind: StoredIdentityKind,
}

#[repr(i32)]
#[derive(Debug, Copy, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "sync", derive(AsExpression, FromSqlRow))]
#[cfg_attr(feature = "sync", diesel(sql_type = Integer))]
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

#[cfg(feature = "sync")]
impl_store!(IdentityCache, identity_cache);
#[cfg(feature = "sync")]
impl_fetch!(IdentityCache, identity_cache);

pub trait QueryIdentityCache {
    /// Returns a HashMap of WalletAddress -> InboxId
    fn fetch_cached_inbox_ids(
        &self,
        identifiers: &[(Address, StoredIdentityKind)],
    ) -> impl std::future::Future<Output = Result<HashMap<String, String>, StorageError>>
    + xmtp_common::MaybeSend;

    fn cache_inbox_id(
        &self,
        kind: StoredIdentityKind,
        identity: String,
        inbox_id: &str,
    ) -> impl std::future::Future<Output = Result<(), StorageError>> + xmtp_common::MaybeSend;
}

impl<G> QueryIdentityCache for &G
where
    G: QueryIdentityCache + xmtp_common::MaybeSync,
{
    async fn fetch_cached_inbox_ids(
        &self,
        identifiers: &[(Address, StoredIdentityKind)],
    ) -> Result<HashMap<String, String>, StorageError> {
        (**self).fetch_cached_inbox_ids(identifiers).await
    }

    async fn cache_inbox_id(
        &self,
        kind: StoredIdentityKind,
        identity: String,
        inbox_id: &str,
    ) -> Result<(), StorageError> {
        (**self).cache_inbox_id(kind, identity, inbox_id).await
    }
}

type Address = String;

#[cfg(feature = "sync")]
impl<C: ConnectionExt> QueryIdentityCache for DbConnection<C> {
    /// Returns a HashMap of WalletAddress -> InboxId
    async fn fetch_cached_inbox_ids(
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

    async fn cache_inbox_id(
        &self,
        kind: StoredIdentityKind,
        identity: String,
        inbox_id: &str,
    ) -> Result<(), StorageError> {
        IdentityCache {
            inbox_id: inbox_id.to_string(),
            identity,
            identity_kind: kind,
        }
        .store(self)
    }
}

/// sqlx backend -- Postgres only. See the note on `QueryGroupVersion`'s impl for
/// why this is gated `not(feature = "sync")`.
#[cfg(all(feature = "async", not(feature = "sync"), not(target_arch = "wasm32")))]
impl QueryIdentityCache for crate::pg::PgDb {
    async fn fetch_cached_inbox_ids(
        &self,
        identifiers: &[(Address, StoredIdentityKind)],
    ) -> Result<HashMap<String, String>, StorageError> {
        use sqlx::Row;
        // The diesel impl builds an OR-chain; with no identifiers that chain is
        // empty and the query degenerates to loading the whole table. Guard it.
        if identifiers.is_empty() {
            return Ok(HashMap::new());
        }

        // Two parallel arrays zipped back by `UNNEST`: matches on the
        // (identity, kind) *pair*, in one prepared statement.
        let (identities, kinds): (Vec<&str>, Vec<i32>) = identifiers
            .iter()
            .map(|(addr, kind)| (addr.as_str(), i32::from(kind)))
            .unzip();

        let mut c = self.conn().await?;
        let rows = sqlx::query(
            "SELECT identity, inbox_id FROM identity_cache \
             WHERE (identity, identity_kind) IN (SELECT * FROM UNNEST($1::text[], $2::int4[]))",
        )
        .bind(&identities)
        .bind(&kinds)
        .fetch_all(&mut *c)
        .await
        .map_err(crate::ConnectionError::from)?;

        rows.into_iter()
            .map(|row| {
                let identity: String = row.try_get(0).map_err(crate::ConnectionError::from)?;
                let inbox_id: String = row.try_get(1).map_err(crate::ConnectionError::from)?;
                Ok((identity, inbox_id))
            })
            .collect()
    }

    /// Plain `INSERT`, matching the sync track's `store`: caching the same
    /// identity twice is a primary-key violation, not a silent overwrite.
    async fn cache_inbox_id(
        &self,
        kind: StoredIdentityKind,
        identity: String,
        inbox_id: &str,
    ) -> Result<(), StorageError> {
        let mut c = self.conn().await?;
        sqlx::query(
            "INSERT INTO identity_cache (inbox_id, identity, identity_kind) VALUES ($1, $2, $3)",
        )
        .bind(inbox_id.to_string())
        .bind(identity)
        .bind(kind)
        .execute(&mut *c)
        .await
        .map_err(crate::ConnectionError::from)?;
        Ok(())
    }
}

crate::impl_sql_int_enum!(StoredIdentityKind {
    Ethereum = 1,
    Passkey = 2,
});

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
