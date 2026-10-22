//! Groups storage table
use color_eyre::eyre::Result;
use redb::TableDefinition;
use std::sync::Arc;
use xmtp_proto::types::GroupId;

use crate::{app::types::*, constants::STORAGE_PREFIX};

use super::{Database, MetadataStore};

pub const MODULE: &str = "groups";
pub const VERSION: u16 = 3;
pub const NAMESPACE: &str = const_format::concatcp!(STORAGE_PREFIX, ":", VERSION, "//", MODULE);

/// Mapping of [`GroupKey`] to a bare-minimum serialized [`Group`]
const TABLE: TableDefinition<GroupKey, Group> = TableDefinition::new(NAMESPACE);

impl super::DeriveKey<GroupKey> for Group {
    fn key(&self) -> GroupKey {
        self.stored_id().into()
    }
}

impl super::DeriveKey<GroupKey> for &Group {
    fn key(&self) -> GroupKey {
        self.stored_id().into()
    }
}

impl From<StoredGroupId> for GroupKey {
    fn from(value: StoredGroupId) -> Self {
        GroupKey::new(value.0)
    }
}

impl TryFrom<&GroupId> for GroupKey {
    type Error = color_eyre::eyre::Report;

    fn try_from(value: &GroupId) -> Result<Self> {
        StoredGroupId::try_from(value).map(Self::from)
    }
}

/// Key of groupid/network
pub type GroupKey = super::NetworkKey<32>;
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
    fn increment(&self, tx: &redb::WriteTransaction, network: u64, n: u32) -> Result<()> {
        let store = MetadataStore::from_write_tx(tx, network);
        store.modify(crate::meta_key!(), |meta| meta.groups += n)?;
        Ok(())
    }

    fn decrement(&self, tx: &redb::WriteTransaction, network: u64, n: u32) -> Result<()> {
        let store = MetadataStore::from_write_tx(tx, network);
        store.modify(crate::meta_key!(), |meta| meta.groups -= n)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::store::{MessageStorage, MessageStore};
    use tempfile::NamedTempFile;

    fn open_temp_db() -> (Arc<redb::Database>, NamedTempFile) {
        let tmp = NamedTempFile::new().expect("tempfile");
        let db = redb::Database::create(tmp.path()).expect("open redb");
        (Arc::new(db), tmp)
    }

    /// Every group a client creates has a 32-byte keyed id; xdbg must store
    /// it and its messages without truncating or rejecting the id.
    #[xmtp_common::test(unwrap_try = true)]
    fn keyed_group_and_message_round_trip() {
        let (db, _tmp) = open_temp_db();
        let groups: GroupStore<'static> = super::super::KeyValueStore::with_network(
            GroupStorage,
            super::super::DatabaseOrTransaction::Db(db.clone()),
            0,
        );
        let messages: MessageStore<'static> = super::super::KeyValueStore::with_network(
            MessageStorage,
            super::super::DatabaseOrTransaction::Db(db),
            0,
        );
        let group_id = GroupId::from([0x5au8; 32]);
        // Differs from `group_id` only in its last byte, so a key that kept
        // fewer than 32 bytes would make the two collide.
        let mut other = [0x5au8; 32];
        other[31] = 0x5b;
        let other_id = GroupId::from(other);
        let stored = StoredGroupId::try_from(&group_id)?;
        let other_stored = StoredGroupId::try_from(&other_id)?;

        let group = Group::new(stored, [1u8; 32], vec![[1u8; 32], [2u8; 32]]);
        groups.set(group.clone())?;
        assert_eq!(groups.get(stored.into())?, Some(group.clone()));
        assert_eq!(group.id(), group_id);
        assert_eq!(groups.get(other_stored.into())?, None);

        let message = Message::new([9u8; 32], stored, [1u8; 32], 7);
        messages.set(message.clone())?;
        assert_eq!(
            messages.get(Message::redb_key(stored, [9u8; 32]))?,
            Some(message.clone())
        );
        assert_eq!(message.group_id(), group_id);
        assert_eq!(
            messages.get(Message::redb_key(other_stored, [9u8; 32]))?,
            None
        );
    }
}
