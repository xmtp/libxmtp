//! Metadata about the db itself
#![allow(unused)]

use super::Metadata;
use color_eyre::eyre;
use color_eyre::eyre::Result;
use redb::{ReadTransaction, ReadableTable, TableDefinition, WriteTransaction};

use super::{
    Database, DmKey, DmStorage, GroupKey, GroupStorage, IdentityKey, IdentityStorage, MessageKey,
    MessageStorage, TableProvider,
};
use std::sync::Arc;
use xxhash_rust::xxh3;

use crate::{app::types::*, args, constants::STORAGE_PREFIX};

pub const MODULE: &str = "metadata";
pub const VERSION: u16 = 1;
/// The counters follow the tables they count: a new version of any counted
/// table gives a new counter table, so the counts never carry rows the stores
/// no longer read. A counter record is seeded from the rows the tables hold
/// when it is first read or written.
pub const NAMESPACE: &str = const_format::concatcp!(
    STORAGE_PREFIX,
    ":",
    VERSION,
    "//",
    MODULE,
    "/",
    super::identity::VERSION,
    ".",
    super::groups::VERSION,
    ".",
    super::dms::VERSION,
    ".",
    super::messages::VERSION
);

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
    fn increment(&self, _tx: &WriteTransaction, _network: u64, _n: u32) -> Result<()> {
        Ok(())
    }
    fn decrement(&self, _tx: &WriteTransaction, _network: u64, _n: u32) -> Result<()> {
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

/// The rows in `range` of `table`.
fn count_rows<K, V, KR>(
    table: &impl ReadableTable<K, V>,
    range: impl std::ops::RangeBounds<KR>,
) -> Result<u32>
where
    K: redb::Key + 'static,
    V: redb::Value + 'static,
    KR: for<'a> std::borrow::Borrow<K::SelfType<'a>>,
{
    let mut rows = 0u32;
    for row in table.range(range)? {
        row?;
        rows += 1;
    }
    Ok(rows)
}

/// The rows each counted table holds for `network`, read in a write
/// transaction.
fn seed_in_write(tx: &WriteTransaction, network: u64) -> Result<Metadata> {
    Ok(Metadata {
        identities: count_rows(
            &tx.open_table(IdentityStorage::table())?,
            IdentityKey::low_all_versions(network)..=IdentityKey::high_all_versions(network),
        )?,
        groups: count_rows(
            &tx.open_table(GroupStorage::table())?,
            GroupKey::create_low(network)..=GroupKey::create_high(network),
        )?,
        messages: count_rows(
            &tx.open_table(MessageStorage::table())?,
            MessageKey::create_low(network)..=MessageKey::create_high(network),
        )?,
        dms: count_rows(
            &tx.open_table(DmStorage::table())?,
            DmKey::create_low(network)..=DmKey::create_high(network),
        )?,
    })
}

/// The rows each counted table holds for `network`, read in a read
/// transaction. A table that does not exist holds none.
fn seed_in_read(r: &ReadTransaction, network: u64) -> Result<Metadata> {
    fn count<K, V, KR>(
        r: &ReadTransaction,
        table: TableDefinition<'_, K, V>,
        range: impl std::ops::RangeBounds<KR>,
    ) -> Result<u32>
    where
        K: redb::Key + 'static,
        V: redb::Value + 'static,
        KR: for<'a> std::borrow::Borrow<K::SelfType<'a>>,
    {
        match super::open_table_optional(r, table)? {
            Some(table) => count_rows(&table, range),
            None => Ok(0),
        }
    }
    Ok(Metadata {
        identities: count(
            r,
            IdentityStorage::table(),
            IdentityKey::low_all_versions(network)..=IdentityKey::high_all_versions(network),
        )?,
        groups: count(
            r,
            GroupStorage::table(),
            GroupKey::create_low(network)..=GroupKey::create_high(network),
        )?,
        messages: count(
            r,
            MessageStorage::table(),
            MessageKey::create_low(network)..=MessageKey::create_high(network),
        )?,
        dms: count(
            r,
            DmStorage::table(),
            DmKey::create_low(network)..=DmKey::create_high(network),
        )?,
    })
}

/// Apply `f` to `network`'s counters in `tx`. A record that does not exist
/// yet is seeded from the rows the counted tables hold now, which already
/// include this transaction's writes, so `f` is not applied to it. Callers
/// must not hold a counted table open.
pub(super) fn update_counters(
    tx: &WriteTransaction,
    network: u64,
    mut f: impl FnMut(&mut Metadata),
) -> Result<()> {
    let recorded = tx
        .open_table(TABLE)?
        .get(meta_key!().with_network(network))?
        .is_some();
    let seeded = if recorded {
        None
    } else {
        Some(seed_in_write(tx, network)?)
    };
    MetadataStore::with_network(
        MetadataStorage,
        super::DatabaseOrTransaction::WriteTx(tx),
        network,
    )
    .modify(meta_key!(), |counters| match &seeded {
        Some(seeded) => *counters = seeded.clone(),
        None => f(counters),
    })
}

impl MetadataStore<'_> {
    /// This network's counters. Before any write records them, as after a
    /// table version change, they are counted from the tables.
    pub fn counters(&self) -> Result<Metadata> {
        let network = self.network_hash();
        let counters = self.apply_read(|r| {
            let key = meta_key!().with_network(network);
            if let Some(table) = super::open_table_optional(r, TABLE)?
                && let Some(counters) = table.get(key)?
            {
                return Ok(Some(counters.value()));
            }
            Ok(Some(seed_in_read(r, network)?))
        })?;
        Ok(counters.unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::store::{
        DatabaseOrTransaction, DeriveKey, DmStore, GroupStore, KeyValueStore, MessageStore,
    };
    use xmtp_proto::types::GroupId;

    /// The metadata namespace before the counted tables changed version.
    const OLD_NAMESPACE: &str = const_format::concatcp!(STORAGE_PREFIX, ":1//metadata");

    fn counts(identities: u32, groups: u32, messages: u32, dms: u32) -> Metadata {
        Metadata {
            identities,
            groups,
            messages,
            dms,
        }
    }

    /// A database written before the group, DM, and message tables moved to
    /// new namespaces holds counters for rows the stores no longer read. The
    /// counters reflect only the current tables: identities, whose table did
    /// not move, are counted from their rows, the moved tables start empty,
    /// and later writes add to those counts.
    #[xmtp_common::test(unwrap_try = true)]
    fn counters_follow_the_tables_they_count() {
        let dir = tempfile::tempdir()?;
        let db = Arc::new(redb::Database::create(dir.path().join("xdbg.redb"))?);
        let network = 7;
        let w = db.begin_write()?;
        w.open_table(TableDefinition::<MetaKey, Metadata>::new(OLD_NAMESPACE))?
            .insert(meta_key!().with_network(network), counts(2, 5, 9, 3))?;
        {
            let mut identities = w.open_table(IdentityStorage::table())?;
            for inbox in [1u8, 2] {
                let mut identity = Identity::dummy_for_test();
                identity.inbox_id = [inbox; 32];
                identities.insert(identity.key().with_network(network), identity)?;
            }
        }
        w.commit()?;

        let metadata: MetadataStore<'static> = KeyValueStore::with_network(
            MetadataStorage,
            DatabaseOrTransaction::Db(db.clone()),
            network,
        );
        assert_eq!(metadata.counters()?, counts(2, 0, 0, 0));

        let groups: GroupStore<'static> = KeyValueStore::with_network(
            GroupStorage,
            DatabaseOrTransaction::Db(db.clone()),
            network,
        );
        let messages: MessageStore<'static> = KeyValueStore::with_network(
            MessageStorage,
            DatabaseOrTransaction::Db(db.clone()),
            network,
        );
        let dms: DmStore<'static> =
            KeyValueStore::with_network(DmStorage, DatabaseOrTransaction::Db(db.clone()), network);
        let group_ids = [0x11u8, 0x22, 0x33]
            .map(|byte| StoredGroupId::try_from(&GroupId::from([byte; 32])).unwrap());
        groups.set_all(&[
            Group::new(group_ids[0], [1; 32], vec![[1; 32]]),
            Group::new(group_ids[1], [1; 32], vec![[1; 32]]),
        ])?;
        messages.set_all(&[
            Message::new([1; 32], group_ids[0], [1; 32], 1),
            Message::new([2; 32], group_ids[0], [1; 32], 2),
            Message::new([3; 32], group_ids[1], [1; 32], 3),
        ])?;
        dms.set(Dm::new([1; 32], [2; 32], group_ids[1]))?;
        assert_eq!(metadata.counters()?, counts(2, 2, 3, 1));

        groups.set(Group::new(group_ids[2], [1; 32], vec![[1; 32]]))?;
        assert_eq!(metadata.counters()?, counts(2, 3, 3, 1));
    }
}
