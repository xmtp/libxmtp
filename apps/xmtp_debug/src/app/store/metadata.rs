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
    () => {
        $crate::app::store::NetworkKey::<0>::new([])
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

/// Mapping of [`MetaKey`] to [`Metadata`]
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
    fn increment<'a>(&self, store: impl Into<MetadataStore<'a>>, n: u32) -> Result<()> {
        Ok(())
    }
    fn decrement<'a>(&self, store: impl Into<MetadataStore<'a>>, n: u32) -> Result<()> {
        Ok(())
    }
}

impl super::DeriveKey<MetaKey> for Metadata {
    fn key(&self) -> MetaKey {
        meta_key!()
    }
}

impl super::DeriveKey<MetaKey> for &Metadata {
    fn key(&self) -> MetaKey {
        meta_key!()
    }
}

impl From<Arc<redb::Database>> for MetadataStore<'static> {
    fn from(value: Arc<redb::Database>) -> Self {
        MetadataStore::new(MetadataStorage, super::DatabaseOrTransaction::Db(value))
    }
}

impl From<Arc<redb::ReadOnlyDatabase>> for MetadataStore<'static> {
    fn from(value: Arc<redb::ReadOnlyDatabase>) -> Self {
        MetadataStore::new(
            MetadataStorage,
            super::DatabaseOrTransaction::ReadOnly(value),
        )
    }
}
