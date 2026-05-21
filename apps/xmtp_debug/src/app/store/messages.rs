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

/// Composite key: `(network, group_id ++ message_id)`. The 48-byte payload
/// places `group_id` first so a range scan over `(network, group || 0x00…)
/// ..= (network, group || 0xFF…)` returns exactly that group's messages.
pub type MessageKey = super::NetworkKey<48>;

impl super::DeriveKey<MessageKey> for Message {
    fn key(&self) -> MessageKey {
        let mut combined = [0u8; 48];
        combined[..16].copy_from_slice(&*self.group_id());
        combined[16..].copy_from_slice(&self.id);
        MessageKey::new(combined)
    }
}

impl super::DeriveKey<MessageKey> for &Message {
    fn key(&self) -> MessageKey {
        let mut combined = [0u8; 48];
        combined[..16].copy_from_slice(&*self.group_id());
        combined[16..].copy_from_slice(&self.id);
        MessageKey::new(combined)
    }
}

pub type MessageStore<'a> = super::KeyValueStore<'a, MessageStorage>;

impl From<Arc<redb::Database>> for MessageStore<'_> {
    fn from(value: Arc<redb::Database>) -> Self {
        MessageStore::new(MessageStorage, super::DatabaseOrTransaction::Db(value))
    }
}

impl From<Arc<redb::ReadOnlyDatabase>> for MessageStore<'_> {
    fn from(value: Arc<redb::ReadOnlyDatabase>) -> Self {
        MessageStore::new(
            MessageStorage,
            super::DatabaseOrTransaction::ReadOnly(value),
        )
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
    fn increment<'a>(&self, store: impl Into<MetadataStore<'a>>, n: u32) -> Result<()> {
        let store = store.into();
        store.modify(crate::meta_key!(), |meta| meta.messages += n)?;
        Ok(())
    }

    fn decrement<'a>(&self, store: impl Into<MetadataStore<'a>>, n: u32) -> Result<()> {
        let store = store.into();
        store.modify(crate::meta_key!(), |meta| meta.messages -= n)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, store::Database};
    use std::sync::Arc;
    use tempfile::NamedTempFile;

    fn open_temp_db() -> (Arc<redb::Database>, NamedTempFile) {
        let tmp = NamedTempFile::new().expect("tempfile");
        let db = redb::Database::create(tmp.path()).expect("open redb");
        (Arc::new(db), tmp)
    }

    fn sample_message(group: [u8; 16], msg_id: [u8; 32]) -> Message {
        Message::new(msg_id, group, [1u8; 32], 42)
    }

    #[test]
    fn message_store_set_then_get_roundtrips() {
        App::set_network(1);
        let (db, _tmp) = open_temp_db();
        let store: MessageStore<'static> = db.into();
        let msg = sample_message([0xAAu8; 16], [0xBBu8; 32]);

        store.set(msg.clone()).expect("set");

        let key = <Message as super::super::DeriveKey<MessageKey>>::key(&msg, 1);
        let got = store.get(key).expect("get");
        assert_eq!(got, Some(msg));
    }

    #[test]
    fn message_store_load_returns_all_messages_for_network() {
        let (db, _tmp) = open_temp_db();
        let store: MessageStore<'static> = db.into();
        let m1 = sample_message([0x10u8; 16], [0x01u8; 32]);
        let m2 = sample_message([0x20u8; 16], [0x02u8; 32]);
        let m3 = sample_message([0x20u8; 16], [0x03u8; 32]);

        store
            .set_all(&[m1.clone(), m2.clone(), m3.clone()], 7u64)
            .expect("set_all");

        let iter = store.load(7u64).expect("load").expect("non-empty");
        let collected: Vec<Message> = iter.map(|g| g.value()).collect();
        assert_eq!(collected.len(), 3);
    }

    #[test]
    fn message_store_load_then_filter_by_group_id() {
        let (db, _tmp) = open_temp_db();
        let store: MessageStore<'static> = db.into();
        let group_a = [0x10u8; 16];
        let group_b = [0x20u8; 16];
        let a1 = sample_message(group_a, [0x01u8; 32]);
        let a2 = sample_message(group_a, [0x02u8; 32]);
        let b1 = sample_message(group_b, [0x03u8; 32]);
        store
            .set_all(&[a1.clone(), a2.clone(), b1.clone()], 7u64)
            .expect("set_all");

        let iter = store.load(7u64).expect("load").expect("non-empty");
        let only_a: Vec<Message> = iter
            .map(|g| g.value())
            .filter(|m| m.group_id() == group_a)
            .collect();
        assert_eq!(only_a.len(), 2);
        assert!(only_a.contains(&a1));
        assert!(only_a.contains(&a2));
        assert!(!only_a.contains(&b1));
    }
}
