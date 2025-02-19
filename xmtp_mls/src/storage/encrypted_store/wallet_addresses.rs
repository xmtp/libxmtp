use std::collections::HashMap;

use super::schema::wallet_addresses;
use crate::storage::{DbConnection, StorageError};
use crate::{impl_fetch, impl_store, Store};
use diesel::prelude::*;
use diesel::{Insertable, Queryable};
use serde::{Deserialize, Serialize};
#[cfg(target_arch = "wasm32")]
use sqlite_web::dsl::RunQueryDsl;
use xmtp_id::associations::RootIdentifier;
use xmtp_id::{InboxId, WalletAddress};

#[derive(Insertable, Queryable, Debug, Clone, Deserialize, Serialize)]
#[diesel(table_name = wallet_addresses)]
#[diesel()]
pub struct WalletEntry {
    inbox_id: InboxId,
    wallet_address: WalletAddress,
}

impl WalletEntry {
    pub fn new(in_id: InboxId, wallet_address: WalletAddress) -> Self {
        Self {
            inbox_id: in_id,
            wallet_address,
        }
    }
}

impl_store!(WalletEntry, wallet_addresses);
impl_fetch!(WalletEntry, wallet_addresses);

impl DbConnection {
    pub fn fetch_cached_inbox_ids(
        &self,
        identifiers: &[RootIdentifier],
    ) -> Result<HashMap<WalletAddress, InboxId>, StorageError> {
        use crate::storage::encrypted_store::schema::wallet_addresses::dsl::{inbox_id, *};
        let keys: Vec<_> = identifiers.iter().map(|i| format!("{i:?}")).collect();
        let cached: Vec<WalletEntry> =
            self.raw_query_read(|conn| wallet_addresses.filter(inbox_id.eq_any(keys)).load(conn))?;
        Ok(cached
            .into_iter()
            .map(|entry| (entry.wallet_address, entry.inbox_id))
            .collect())
    }

    pub fn cache_inbox_id(
        &self,
        identifier: &RootIdentifier,
        inbox_id: impl ToString,
    ) -> Result<(), StorageError> {
        WalletEntry {
            inbox_id: inbox_id.to_string(),
            wallet_address: format!("{identifier:?}"),
        }
        .store(self)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use xmtp_id::associations::RootIdentifier;

    use crate::storage::wallet_addresses::WalletEntry;
    use crate::{storage::encrypted_store::tests::with_connection, FetchListWithKey, Store};

    // Test storing a single wallet
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_store_wallet() {
        with_connection(|conn| {
            let new_entry = WalletEntry {
                inbox_id: "inbox_id_1".to_string(),
                wallet_address: "wallet_address_1".to_string(),
            };
            assert!(new_entry.store(conn).is_ok(), "Failed to store wallet");
        })
        .await;
    }

    // Test storing duplicated wallets (same inbox_id and wallet_address)
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_store_duplicated_wallets() {
        with_connection(|conn| {
            let entry1 = WalletEntry {
                inbox_id: "test_dup".to_string(),
                wallet_address: "wallet_dup".to_string(),
            };
            let entry2 = WalletEntry {
                inbox_id: "test_dup".to_string(),
                wallet_address: "wallet_dup".to_string(),
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

    // Test fetching wallets by a list of inbox_ids
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_fetch_wallets() {
        with_connection(|conn| {
            // Insert multiple entries with different inbox_ids
            let new_entry1 = WalletEntry {
                inbox_id: "fetch_test1".to_string(),
                wallet_address: "wallet1".to_string(),
            };
            let new_entry2 = WalletEntry {
                inbox_id: "fetch_test2".to_string(),
                wallet_address: "wallet2".to_string(),
            };
            let new_entry3 = WalletEntry {
                inbox_id: "fetch_test3".to_string(),
                wallet_address: "wallet3".to_string(),
            };
            new_entry1.store(conn).unwrap();
            new_entry2.store(conn).unwrap();
            new_entry3.store(conn).unwrap();

            // Fetch wallets with inbox_ids "fetch_test1" and "fetch_test2"
            let inbox_ids = vec!["fetch_test1".to_string(), "fetch_test2".to_string()];
            let fetched_wallets: Vec<WalletEntry> =
                conn.load_cached_inbox_ids(&inbox_ids).unwrap_or_default();

            // Verify that 3 entries are fetched (2 from "fetch_test1" and 1 from "fetch_test2")
            assert_eq!(
                fetched_wallets.len(),
                2,
                "Expected 2 wallets, found {}",
                fetched_wallets.len()
            );

            // Verify contents of fetched entries
            let fetched_addresses: Vec<String> = fetched_wallets
                .iter()
                .map(|w| w.wallet_address.clone())
                .collect();
            assert!(
                fetched_addresses.contains(&"wallet1".to_string()),
                "wallet1 not found in fetched results"
            );
            assert!(
                fetched_addresses.contains(&"wallet2".to_string()),
                "wallet2 not found in fetched results"
            );

            // Fetch wallets with a non-existent list of inbox_ids
            let non_existent_wallets: Vec<WalletEntry> = conn
                .fetch_list_with_key(&["nonexistent".to_string()])
                .unwrap_or_default();
            assert!(
                non_existent_wallets.is_empty(),
                "Expected no wallets, found some"
            );
        })
        .await;
    }

    // Test storing and fetching multiple wallet addresses with multiple keys
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_store_wallet_addresses() {
        with_connection(|conn| {
            let ident1 = RootIdentifier::rand_ethereum();
            let ident1_inbox = ident1.inbox_id(0).unwrap();
            let ident2 = RootIdentifier::rand_ethereum();

            conn.cache_inbox_id(&ident1, &ident1_inbox).unwrap();
            let idents = &[ident1, ident2];
            let stored_wallets = conn.fetch_cached_inbox_ids(idents).unwrap();

            // Verify that 1 entries are fetched
            assert_eq!(stored_wallets.len(), 1);

            // Verify it's the correct inbox_id
            let cached_inbox_id = stored_wallets.get(&format!("{:?}", idents[0])).unwrap();
            assert_eq!(*cached_inbox_id, ident1_inbox);
        })
        .await;
    }
}
