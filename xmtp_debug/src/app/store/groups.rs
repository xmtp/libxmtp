//! Groups storage table
use color_eyre::eyre::Result;
use redb::TableDefinition;
use std::sync::Arc;

use crate::{app::types::*, constants::STORAGE_PREFIX};

use super::{Database, MetadataStore};

pub const MODULE: &str = "groups";
pub const VERSION: u16 = 1;
pub const NAMESPACE: &str = const_format::concatcp!(STORAGE_PREFIX, ":", VERSION, "//", MODULE);

/// Mapping of GroupId to a bare-minimum serialized Group
const TABLE: TableDefinition<GroupKey, Group> = TableDefinition::new(NAMESPACE);

impl super::DeriveKey<GroupKey> for Group {
    fn key(&self, network: u64) -> GroupKey {
        GroupKey {
            network,
            key: self.id,
        }
    }
}

impl super::DeriveKey<GroupKey> for &Group {
    fn key(&self, network: u64) -> GroupKey {
        GroupKey {
            network,
            key: self.id,
        }
    }
}

/// Key of groupid/network
pub type GroupKey = super::NetworkKey<16>;
pub type GroupStore<'a> = super::KeyValueStore<'a, GroupStorage>;

impl From<Arc<redb::Database>> for GroupStore<'_> {
    fn from(value: Arc<redb::Database>) -> Self {
        GroupStore {
            db: super::DatabaseOrTransaction::Db(value),
            store: GroupStorage,
        }
    }
}

#[derive(Debug)]
pub struct GroupStorage;

impl<'a> super::TableProvider<'a, GroupKey, Group> for GroupStorage {
    fn table() -> TableDefinition<'a, GroupKey, Group> {
        TABLE
    }
}

impl super::TrackMetadata for GroupStorage {
    fn increment<'a>(
        &self,
        store: impl Into<MetadataStore<'a>>,
        network: u64,
        n: u32,
    ) -> Result<()> {
        let store = store.into();
        store.modify(crate::meta_key!(network), |meta| meta.groups += n)?;
        Ok(())
    }

    fn decrement<'a>(
        &self,
        store: impl Into<MetadataStore<'a>>,
        network: u64,
        n: u32,
    ) -> Result<()> {
        let store = store.into();
        store.modify(crate::meta_key!(network), |meta| meta.groups -= n)?;
        Ok(())
    }
}
