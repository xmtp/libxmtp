//! Messages storage table
use color_eyre::eyre::Result;
use redb::TableDefinition;
use std::sync::Arc;

use crate::{app::types::*, constants::STORAGE_PREFIX};

use super::{Database, MetadataStore};

pub const MODULE: &str = "messages";
pub const VERSION: u16 = 1;
pub const NAMESPACE: &str = const_format::concatcp!(STORAGE_PREFIX, ":", VERSION, "//", MODULE);

/// Mapping of [`MessageKey`] to a serialized [`Message`].
const TABLE: TableDefinition<MessageKey, Message> = TableDefinition::new(NAMESPACE);

/// Composite key: `(network, group_id ++ message_id)`. `group_id` first
/// so a range scan `(network, group || 0x00…)..=(network, group || 0xFF…)`
/// returns exactly that group's messages.
pub type MessageKey = super::NetworkKey<48>;

impl super::DeriveKey<MessageKey> for Message {
    fn key(&self, network: u64) -> MessageKey {
        let mut combined = [0u8; 48];
        combined[..16].copy_from_slice(&self.group_id);
        combined[16..].copy_from_slice(&self.id);
        MessageKey::new(network, combined)
    }
}

impl super::DeriveKey<MessageKey> for &Message {
    fn key(&self, network: u64) -> MessageKey {
        let mut combined = [0u8; 48];
        combined[..16].copy_from_slice(&self.group_id);
        combined[16..].copy_from_slice(&self.id);
        MessageKey::new(network, combined)
    }
}

pub type MessageStore<'a> = super::KeyValueStore<'a, MessageStorage>;

impl From<Arc<redb::Database>> for MessageStore<'_> {
    fn from(value: Arc<redb::Database>) -> Self {
        MessageStore {
            db: super::DatabaseOrTransaction::Db(value),
            store: MessageStorage,
        }
    }
}

impl From<Arc<redb::ReadOnlyDatabase>> for MessageStore<'_> {
    fn from(value: Arc<redb::ReadOnlyDatabase>) -> Self {
        MessageStore {
            db: super::DatabaseOrTransaction::ReadOnly(value),
            store: MessageStorage,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MessageStorage;

impl<'a> super::TableProvider<'a, MessageKey, Message> for MessageStorage {
    fn table() -> TableDefinition<'a, MessageKey, Message> {
        TABLE
    }
}

impl super::TrackMetadata for MessageStorage {
    fn increment<'a>(
        &self,
        store: impl Into<MetadataStore<'a>>,
        network: u64,
        n: u32,
    ) -> Result<()> {
        let store = store.into();
        store.modify(crate::meta_key!(network), |meta| meta.messages += n)?;
        Ok(())
    }

    fn decrement<'a>(
        &self,
        store: impl Into<MetadataStore<'a>>,
        network: u64,
        n: u32,
    ) -> Result<()> {
        let store = store.into();
        store.modify(crate::meta_key!(network), |meta| meta.messages -= n)?;
        Ok(())
    }
}
