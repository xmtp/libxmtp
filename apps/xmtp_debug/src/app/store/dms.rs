//! Dms storage table
use color_eyre::eyre::Result;
use redb::TableDefinition;
use std::sync::Arc;

use crate::{app::types::*, constants::STORAGE_PREFIX};

use super::{Database, MetadataStore};

pub const MODULE: &str = "dms";
pub const VERSION: u16 = 1;
pub const NAMESPACE: &str = const_format::concatcp!(STORAGE_PREFIX, ":", VERSION, "//", MODULE);

/// Mapping of [`DmKey`] to a bare-minimum serialized [`Dm`]
const TABLE: TableDefinition<DmKey, Dm> = TableDefinition::new(NAMESPACE);

impl super::DeriveKey<DmKey> for Dm {
    fn key(&self) -> DmKey {
        DmKey::new(*self.redb_key())
    }
}

impl super::DeriveKey<DmKey> for &Dm {
    fn key(&self) -> DmKey {
        DmKey::new(*self.redb_key())
    }
}

impl From<DmId> for DmKey {
    fn from(value: DmId) -> Self {
        DmKey::new(value.into_bytes())
    }
}

impl From<&DmId> for DmKey {
    fn from(value: &DmId) -> Self {
        DmKey::new(*value.as_bytes())
    }
}

/// Key of groupid/network
pub type DmKey = super::NetworkKey<64>;
pub type DmStore<'a> = super::KeyValueStore<'a, DmStorage>;

impl From<Arc<redb::Database>> for DmStore<'_> {
    fn from(value: Arc<redb::Database>) -> Self {
        DmStore::new(DmStorage, super::DatabaseOrTransaction::Db(value))
    }
}

impl From<Arc<redb::ReadOnlyDatabase>> for DmStore<'_> {
    fn from(value: Arc<redb::ReadOnlyDatabase>) -> Self {
        DmStore::new(DmStorage, super::DatabaseOrTransaction::ReadOnly(value))
    }
}

#[derive(Debug, Clone)]
pub struct DmStorage;

impl<'a> super::TableProvider<'a, DmKey, Dm> for DmStorage {
    fn table() -> TableDefinition<'a, DmKey, Dm> {
        TABLE
    }
}
impl super::TrackMetadata for DmStorage {
    fn increment<'a>(&self, store: impl Into<MetadataStore<'a>>, n: u32) -> Result<()> {
        let store = store.into();
        store.modify(crate::meta_key!(), |meta| meta.dms += n)?;
        Ok(())
    }

    fn decrement<'a>(&self, store: impl Into<MetadataStore<'a>>, n: u32) -> Result<()> {
        let store = store.into();
        store.modify(crate::meta_key!(), |meta| meta.dms -= n)?;
        Ok(())
    }
}
