//! Groups storage table
use color_eyre::eyre::Result;
use redb::TableDefinition;
use std::sync::Arc;
use xmtp_proto::types::GroupId;

use crate::{app::types::*, constants::STORAGE_PREFIX};

use super::{Database, MetadataStore};

pub const MODULE: &str = "groups";
pub const VERSION: u16 = 2;
pub const NAMESPACE: &str = const_format::concatcp!(STORAGE_PREFIX, ":", VERSION, "//", MODULE);

/// Mapping of [`GroupKey`] to a bare-minimum serialized [`Group`]
const TABLE: TableDefinition<GroupKey, Group> = TableDefinition::new(NAMESPACE);

impl super::DeriveKey<GroupKey> for Group {
    fn key(&self) -> GroupKey {
        GroupKey::new(*self.id().as_bytes())
    }
}

impl super::DeriveKey<GroupKey> for &Group {
    fn key(&self) -> GroupKey {
        GroupKey::new(*self.id().as_bytes())
    }
}

impl From<GroupId> for GroupKey {
    fn from(value: GroupId) -> Self {
        GroupKey::new(value.into_bytes())
    }
}

impl From<&GroupId> for GroupKey {
    fn from(value: &GroupId) -> Self {
        GroupKey::new(*value.as_bytes())
    }
}

/// Key of groupid/network
pub type GroupKey = super::NetworkKey<16>;
pub type GroupStore<'a> = super::KeyValueStore<'a, GroupStorage>;

impl From<Arc<redb::Database>> for GroupStore<'_> {
    fn from(value: Arc<redb::Database>) -> Self {
        GroupStore::new(GroupStorage, super::DatabaseOrTransaction::Db(value))
    }
}

impl From<Arc<redb::ReadOnlyDatabase>> for GroupStore<'_> {
    fn from(value: Arc<redb::ReadOnlyDatabase>) -> Self {
        GroupStore::new(GroupStorage, super::DatabaseOrTransaction::ReadOnly(value))
    }
}

#[derive(Debug, Clone)]
pub struct GroupStorage;

impl<'a> super::TableProvider<'a, GroupKey, Group> for GroupStorage {
    fn table() -> TableDefinition<'a, GroupKey, Group> {
        TABLE
    }
}

impl super::TrackMetadata for GroupStorage {
    fn increment<'a>(&self, store: impl Into<MetadataStore<'a>>, n: u32) -> Result<()> {
        let store = store.into();
        store.modify(crate::meta_key!(), |meta| meta.groups += n)?;
        Ok(())
    }

    fn decrement<'a>(&self, store: impl Into<MetadataStore<'a>>, n: u32) -> Result<()> {
        let store = store.into();
        store.modify(crate::meta_key!(), |meta| meta.groups -= n)?;
        Ok(())
    }
}
