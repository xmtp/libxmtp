//! Metadata about the db itself
#![allow(unused)]

use super::Metadata;
use color_eyre::eyre;
use color_eyre::eyre::Result;
use redb::TableDefinition;
use redb::{ReadTransaction, WriteTransaction};
use std::sync::Arc;
use xxhash_rust::xxh3;

use crate::{app::types::*, args, constants::STORAGE_PREFIX};

pub const MODULE: &str = "metadata";
pub const VERSION: u16 = 1;
pub const NAMESPACE: &str = const_format::concatcp!(STORAGE_PREFIX, ":", VERSION, "//", MODULE);

type MetaKey = super::NetworkKey<0>;
pub type MetadataStore<'a> = super::KeyValueStore<'a, MetadataStorage>;

#[macro_export]
#[macro_use]
macro_rules! meta_key {
    ($network:expr) => {
        $crate::app::store::NetworkKey::<0>::new(u64::from($network), [])
    };
}

impl From<args::BackendOpts> for MetaKey {
    fn from(value: args::BackendOpts) -> MetaKey {
        MetaKey {
            network: value.into(),
            key: Default::default(),
        }
    }
}

impl<'a> From<&'a args::BackendOpts> for MetaKey {
    fn from(value: &'a args::BackendOpts) -> MetaKey {
        MetaKey {
            network: value.into(),
            key: Default::default(),
        }
    }
}

impl From<u64> for MetaKey {
    fn from(value: u64) -> MetaKey {
        MetaKey {
            network: value,
            key: Default::default(),
        }
    }
}

// pub fn open_table(store: MetadataStore)

/// Mapping of GroupId to a bare-minimum serialized Group
const TABLE: TableDefinition<MetaKey, Metadata> = TableDefinition::new(NAMESPACE);

#[derive(Debug, Copy, Clone)]
pub struct MetadataStorage;

impl<'a> super::TableProvider<'a, MetaKey, Metadata> for MetadataStorage {
    fn table() -> TableDefinition<'a, MetaKey, Metadata> {
        TABLE
    }
}

// No-Op for Metadata Table
impl super::TrackMetadata for MetadataStorage {
    fn increment<'a>(
        &self,
        store: impl Into<MetadataStore<'a>>,
        network: u64,
        n: u32,
    ) -> Result<()> {
        Ok(())
    }
    fn decrement<'a>(
        &self,
        store: impl Into<MetadataStore<'a>>,
        network: u64,
        n: u32,
    ) -> Result<()> {
        Ok(())
    }
}

impl super::DeriveKey<MetaKey> for Metadata {
    fn key(&self, network: u64) -> MetaKey {
        MetaKey {
            network,
            key: Default::default(),
        }
    }
}

impl<'a> super::DeriveKey<MetaKey> for &'a Metadata {
    fn key(&self, network: u64) -> MetaKey {
        MetaKey {
            network,
            key: Default::default(),
        }
    }
}

impl From<Arc<redb::Database>> for MetadataStore<'static> {
    fn from(value: Arc<redb::Database>) -> Self {
        MetadataStore {
            db: super::DatabaseOrTransaction::Db(value),
            store: MetadataStorage,
        }
    }
}
