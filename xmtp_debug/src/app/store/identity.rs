use crate::{app::types::*, constants::STORAGE_PREFIX};
use color_eyre::eyre::{Result, eyre};
use redb::TableDefinition;
use std::sync::Arc;
use xmtp_cryptography::XmtpInstallationCredential;

use super::{Database, MetadataStore};

pub const MODULE: &str = "identity";
pub const VERSION: u16 = 1;
pub const NAMESPACE: &str = const_format::concatcp!(STORAGE_PREFIX, ":", VERSION, "//", MODULE);

/// Mapping of InboxID to a bare-minimum serialized Identity
const TABLE: TableDefinition<IdentityKey, Identity> = TableDefinition::new(NAMESPACE);

pub type IdentityKey = super::NetworkKey<32>;
pub type IdentityStore<'a> = super::KeyValueStore<'a, IdentityStorage>;

impl IdentityStore<'_> {
    pub fn installation_ids(&self, network: u64) -> Result<Vec<XmtpInstallationCredential>> {
        Ok(self
            .load(network)?
            .ok_or(eyre!("no identities in store. try generating some."))?
            .map(|id| XmtpInstallationCredential::from_bytes(&id.value().installation_key).unwrap())
            .collect())
    }
}

impl From<Arc<redb::Database>> for IdentityStore<'_> {
    fn from(value: Arc<redb::Database>) -> Self {
        IdentityStore {
            db: super::DatabaseOrTransaction::Db(value),
            store: IdentityStorage,
        }
    }
}

impl From<Arc<redb::ReadOnlyDatabase>> for IdentityStore<'_> {
    fn from(value: Arc<redb::ReadOnlyDatabase>) -> Self {
        IdentityStore {
            db: super::DatabaseOrTransaction::ReadOnly(value),
            store: IdentityStorage,
        }
    }
}

impl super::DeriveKey<IdentityKey> for Identity {
    fn key(&self, network: u64) -> IdentityKey {
        IdentityKey {
            network,
            key: self.inbox_id,
        }
    }
}

impl super::DeriveKey<IdentityKey> for &Identity {
    fn key(&self, network: u64) -> IdentityKey {
        IdentityKey {
            network,
            key: self.inbox_id,
        }
    }
}

#[derive(Debug, Clone)]
pub struct IdentityStorage;

impl<'a> super::TableProvider<'a, IdentityKey, Identity> for IdentityStorage {
    fn table() -> TableDefinition<'a, IdentityKey, Identity> {
        TABLE
    }
}

impl super::TrackMetadata for IdentityStorage {
    fn increment<'a>(
        &self,
        store: impl Into<MetadataStore<'a>>,
        network: u64,
        n: u32,
    ) -> Result<()> {
        let store: MetadataStore = store.into();
        store.modify(crate::meta_key!(network), |meta| {
            meta.identities += n;
        })?;
        Ok(())
    }

    fn decrement<'a>(
        &self,
        store: impl Into<MetadataStore<'a>>,
        network: u64,
        n: u32,
    ) -> Result<()> {
        let store: MetadataStore = store.into();
        store.modify(crate::meta_key!(network), |meta| {
            meta.identities -= n;
        })?;
        Ok(())
    }
}
