use super::schema::identity_cache;
use super::Sqlite;
use crate::storage::{DbConnection, StorageError};
use crate::{impl_fetch, impl_store, Store};
use diesel::backend::Backend;
use diesel::deserialize::{self, FromSql, FromSqlRow};
use diesel::expression::AsExpression;
use diesel::serialize::{IsNull, Output, ToSql};
use diesel::sql_types::Integer;
use diesel::{prelude::*, serialize};
use diesel::{Insertable, Queryable};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use xmtp_id::associations::PublicIdentifier;
use xmtp_id::{InboxId, WalletAddress};

#[derive(Insertable, Queryable, Debug, Clone, Deserialize, Serialize)]
#[diesel(table_name = identity_cache)]
#[diesel()]
pub struct IdentityCache {
    inbox_id: InboxId,
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

impl DbConnection {
    pub fn fetch_cached_inbox_ids(
        &self,
        identifiers: &[PublicIdentifier],
    ) -> Result<HashMap<WalletAddress, InboxId>, StorageError> {
        use crate::storage::encrypted_store::schema::identity_cache::*;

        let mut conditions = identity_cache::table.into_boxed();

        for ident in identifiers {
            let addr = format!("{ident}");
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

    pub fn cache_inbox_id(
        &self,
        identifier: &PublicIdentifier,
        inbox_id: impl ToString,
    ) -> Result<(), StorageError> {
        IdentityCache {
            inbox_id: inbox_id.to_string(),
            identity: format!("{identifier}"),
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

impl From<&PublicIdentifier> for StoredIdentityKind {
    fn from(ident: &PublicIdentifier) -> Self {
        match ident {
            PublicIdentifier::Ethereum(_) => Self::Ethereum,
            PublicIdentifier::Passkey(_) => Self::Passkey,
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::IdentityCache;
    use crate::{
        storage::{encrypted_store::tests::with_connection, identity_cache::StoredIdentityKind},
        Store,
    };
    use xmtp_id::associations::PublicIdentifier;

    // Test storing duplicated wallets (same inbox_id and wallet_address)
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
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
        .await;
    }

    // Test storing and fetching multiple wallet addresses with multiple keys
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_fetch_and_store_identity_cache() {
        with_connection(|conn| {
            let ident1 = PublicIdentifier::rand_ethereum();
            let ident1_inbox = ident1.inbox_id(0).unwrap();
            let ident2 = PublicIdentifier::rand_ethereum();

            conn.cache_inbox_id(&ident1, &ident1_inbox).unwrap();
            let idents = &[ident1, ident2];
            let stored_wallets = conn.fetch_cached_inbox_ids(idents).unwrap();

            // Verify that 1 entries are fetched
            assert_eq!(stored_wallets.len(), 1);

            // Verify it's the correct inbox_id
            let cached_inbox_id = stored_wallets.get(&format!("{}", idents[0])).unwrap();
            assert_eq!(*cached_inbox_id, ident1_inbox);

            // Fetch wallets with a non-existent list of inbox_ids
            let non_existent_wallets = conn
                .fetch_cached_inbox_ids(&[PublicIdentifier::rand_ethereum()])
                .unwrap_or_default();
            assert!(
                non_existent_wallets.is_empty(),
                "Expected no wallets, found some"
            );
        })
        .await;
    }
}
