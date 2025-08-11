mod groups;
mod identity;
mod metadata;

use std::{borrow::Borrow, sync::Arc};

use color_eyre::eyre::{self, Result};
use rand::{Rng, seq::IteratorRandom};
use redb::{AccessGuard, ReadTransaction, WriteTransaction};
use speedy::{Readable, Writable};

pub use groups::*;
pub use identity::*;
pub use metadata::*;

#[derive(Debug, Copy, Clone)]
pub struct NetworkKey<const N: usize> {
    network: u64,
    key: [u8; N],
}

impl<const N: usize> NetworkKey<N> {
    pub fn new(network: u64, key: [u8; N]) -> Self {
        Self { network, key }
    }
}

impl<const N: usize> From<(u64, [u8; N])> for NetworkKey<N> {
    fn from(value: (u64, [u8; N])) -> Self {
        NetworkKey {
            network: value.0,
            key: value.1,
        }
    }
}

impl<const N: usize> NetworkKey<N> {
    fn create_low(prefix: impl Into<u64>) -> Self {
        Self {
            network: prefix.into(),
            key: [0u8; N],
        }
    }

    fn create_high(prefix: impl Into<u64>) -> Self {
        Self {
            network: prefix.into(),
            key: [u8::MAX; N],
        }
    }
}

impl<'a, C: speedy::Context, const N: usize> Readable<'a, C> for NetworkKey<N> {
    #[inline]
    fn read_from<R: speedy::Reader<'a, C>>(reader: &mut R) -> std::result::Result<Self, C::Error> {
        let network = reader.read_u64()?;
        let key = reader.read_value()?;
        Ok(NetworkKey { network, key })
    }
}

impl<C: speedy::Context, const N: usize> Writable<C> for NetworkKey<N> {
    #[inline]
    fn write_to<T: ?Sized + speedy::Writer<C>>(
        &self,
        writer: &mut T,
    ) -> std::result::Result<(), <C as speedy::Context>::Error> {
        let NetworkKey { network, key } = self;
        writer.write_value(network)?;
        if N > 0 {
            writer.write_value(key)?;
        }
        Ok(())
    }
}

impl<const N: usize> redb::Value for NetworkKey<N> {
    type SelfType<'a>
        = NetworkKey<N>
    where
        Self: 'a;

    type AsBytes<'a>
        = Vec<u8>
    // TODO: It _has_ be possible to make this a const [u8; N] somehow.
    // We're not allowed to use `size_of::<NetworkKey<N>>()` yet, even though size_of and N are
    // both constant
    where
        Self: 'a;

    fn fixed_width() -> Option<usize> {
        Some(std::mem::size_of::<NetworkKey<N>>())
    }

    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
    where
        Self: 'a,
    {
        NetworkKey::<N>::read_from_buffer(data).unwrap()
    }

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>
    where
        Self: 'a,
        Self: 'b,
    {
        value.write_to_vec().unwrap()
    }

    fn type_name() -> redb::TypeName {
        let crate_name = env!("CARGO_CRATE_NAME");
        redb::TypeName::new(&format!("{crate_name}-generic-network-key"))
    }
}

impl<const N: usize> redb::Key for NetworkKey<N> {
    fn compare(data1: &[u8], data2: &[u8]) -> std::cmp::Ordering {
        data1.cmp(data2)
    }
}

pub trait DeriveKey<Key> {
    fn key(&self, network: u64) -> Key;
}

pub trait Database<Key, Value> {
    #[allow(unused)]
    /// store only `value` to disk
    fn set(&self, value: Value, network: impl Into<u64>) -> Result<()> {
        Database::set_all(self, &[value], network)
    }

    /// store all entities in `values` to disk
    fn set_all(&self, values: &[Value], network: impl Into<u64>) -> Result<()>;

    /// Get an entity by key from this database store
    fn get(&self, value: Key) -> Result<Option<Value>>;

    fn load(
        &'_ self,
        network: impl Into<u64>,
    ) -> Result<Option<impl Iterator<Item = AccessGuard<'_, Value>>>>
    where
        Value: redb::Value + 'static;

    fn clear_network(&self, network: impl Into<u64>) -> Result<()>;

    /// Modify a single value by removing and re-inserting
    fn modify(&self, key: Key, f: impl FnMut(&mut Value)) -> Result<()>
    where
        Value: Default;
}

pub trait RandomDatabase<Key, Value> {
    /// Get a random entity from this database store
    fn random(&self, network: impl Into<u64> + Copy, rng: &mut impl Rng) -> Result<Option<Value>>;

    fn random_n(
        &self,
        network: impl Into<u64> + Copy,
        rng: &mut impl Rng,
        n: usize,
    ) -> Result<Vec<Value>>
    where
        Value: std::hash::Hash + Eq;
}

pub trait TableProvider<'a, K: redb::Key + 'static, V: redb::Value + 'static> {
    fn table() -> redb::TableDefinition<'a, K, V>;
}

pub trait TrackMetadata {
    fn increment<'a>(
        &self,
        store: impl Into<MetadataStore<'a>>,
        network: u64,
        n: u32,
    ) -> Result<()>;
    fn decrement<'a>(
        &self,
        store: impl Into<MetadataStore<'a>>,
        network: u64,
        n: u32,
    ) -> Result<()>;
}

#[derive(Clone)]
enum DatabaseOrTransaction<'a> {
    Db(Arc<redb::Database>),
    WriteTx(&'a WriteTransaction),
    ReadTx(&'a redb::ReadTransaction),
}

#[derive(Clone)]
pub struct KeyValueStore<'db, Storage> {
    db: DatabaseOrTransaction<'db>,
    store: Storage,
}

impl<Storage> KeyValueStore<'_, Storage> {
    fn apply_write(&self, op: impl FnOnce(&WriteTransaction) -> Result<()>) -> Result<()> {
        use DatabaseOrTransaction::*;
        match self.db {
            Db(ref d) => {
                let w = d.begin_write()?;
                op(&w)?;
                Ok(w.commit()?)
            }
            WriteTx(w) => Ok(op(w)?),
            ReadTx(_) => eyre::bail!("requires write"),
        }
    }

    fn apply_read<T>(
        &self,
        op: impl FnOnce(&ReadTransaction) -> Result<Option<T>>,
    ) -> Result<Option<T>> {
        use DatabaseOrTransaction::*;
        match self.db {
            Db(ref d) => {
                let r = d.begin_read()?;
                Ok(op(&r)?)
            }
            ReadTx(r) => Ok(op(r)?),
            WriteTx(_) => eyre::bail!("requires read only"),
        }
    }
}

impl<'db, Storage, Key, Value> TableProvider<'db, Key, Value> for KeyValueStore<'db, Storage>
where
    Storage: TableProvider<'db, Key, Value>,
    Key: redb::Key + 'static,
    Value: redb::Value + 'static,
{
    fn table() -> redb::TableDefinition<'db, Key, Value> {
        Storage::table()
    }
}

impl<'key, const N: usize, Value, Storage> Database<NetworkKey<N>, Value>
    for KeyValueStore<'key, Storage>
where
    Storage: TrackMetadata + TableProvider<'key, NetworkKey<N>, Value>,
    for<'a> Value: redb::Value<SelfType<'a> = Value> + DeriveKey<NetworkKey<N>> + 'static,
    for<'a> Value: Borrow<<Value as redb::Value>::SelfType<'a>>,
{
    fn set_all(&self, values: &[Value], network: impl Into<u64>) -> Result<()> {
        let network: u64 = network.into();
        let op = |w: &WriteTransaction| -> Result<()> {
            let mut table = w.open_table(Self::table())?;
            let mut total = 0;
            for value in values.iter() {
                let key: NetworkKey<N> = value.key(network);
                if table.insert(key, value)?.is_none() {
                    total += 1;
                }
            }
            self.store.increment(w, network, total)?;
            Ok(())
        };
        self.apply_write(op)?;
        Ok(())
    }

    fn get(&self, key: NetworkKey<N>) -> Result<Option<Value>> {
        let op = |r: &ReadTransaction| -> Result<Option<Value>> {
            let table = r.open_table(Self::table())?;
            Ok(table.get(key)?.map(|v| v.value()))
        };
        self.apply_read(op)
    }

    fn load(
        &'_ self,
        network: impl Into<u64>,
    ) -> Result<Option<impl Iterator<Item = AccessGuard<'_, Value>>>>
    where
        Value: redb::Value + 'static,
    {
        self.apply_read(|r| {
            if let Ok(table) = r.open_table(Self::table()) {
                let network: u64 = network.into();
                let start = NetworkKey::<N>::create_low(network);
                let end = NetworkKey::<N>::create_high(network);
                Ok(Some(table.range(start..end)?.map(|r| r.unwrap().1)))
            } else {
                Ok(None)
            }
        })
    }

    fn clear_network(&self, network: impl Into<u64>) -> Result<()> {
        let network: u64 = network.into();
        self.apply_write(|w| {
            let mut table = w.open_table(Self::table())?;
            let mut total = 0;
            table.retain(|k: NetworkKey<N>, _| {
                if !k.network == network {
                    total += 1;
                    return false;
                }
                true
            })?;
            self.store.decrement(w, network, total)?;
            Ok(())
        })
    }

    fn modify(&self, key: NetworkKey<N>, mut f: impl FnMut(&mut Value)) -> Result<()>
    where
        Value: Default,
    {
        self.apply_write(|w| {
            let mut table = w.open_table(Self::table())?;
            let mut item = table
                .remove(key)?
                .map(|v| v.value())
                .unwrap_or(Default::default());
            f(&mut item);
            table.insert(key, item)?;
            Ok(())
        })
    }
}

impl<'key, const N: usize, Value, Storage> RandomDatabase<NetworkKey<N>, Value>
    for KeyValueStore<'key, Storage>
where
    Storage: TrackMetadata + TableProvider<'key, NetworkKey<N>, Value>,
    for<'a> Value: redb::Value<SelfType<'a> = Value> + DeriveKey<NetworkKey<N>> + 'static,
{
    fn random(&self, network: impl Into<u64> + Copy, rng: &mut impl Rng) -> Result<Option<Value>> {
        self.apply_read(|r| {
            let table = r.open_table(Self::table())?;
            let start = NetworkKey::create_low(network);
            let end = NetworkKey::create_high(network);
            Ok(table
                .range(start..end)?
                .choose(rng)
                .transpose()?
                .map(|(_, v)| v.value()))
        })
    }

    fn random_n(
        &self,
        network: impl Into<u64> + Copy,
        rng: &mut impl Rng,
        n: usize,
    ) -> Result<Vec<Value>>
    where
        Value: std::hash::Hash + Eq,
    {
        if n == 0 {
            return Ok(Vec::new());
        }

        if let Some(items) = self.load(network)? {
            Ok(items
                .choose_multiple(rng, n)
                .into_iter()
                .map(|v| v.value())
                .collect())
        } else {
            Ok(Vec::new())
        }
    }
}

#[derive(Debug, Clone, Default, Readable, Writable)]
pub struct Metadata {
    pub identities: u32,
    pub groups: u32,
    pub messages: u32,
}

impl redb::Value for Metadata {
    type SelfType<'a>
        = Metadata
    where
        Self: 'a;

    type AsBytes<'a>
        = [u8; size_of::<Metadata>()]
    where
        Self: 'a;

    fn fixed_width() -> Option<usize> {
        Some(size_of::<Metadata>())
    }

    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
    where
        Self: 'a,
    {
        Metadata::read_from_buffer(data).unwrap()
    }

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>
    where
        Self: 'a,
        Self: 'b,
    {
        let mut buffer = [0u8; size_of::<Metadata>()];
        value.write_to_buffer(&mut buffer).unwrap();
        buffer
    }

    fn type_name() -> redb::TypeName {
        redb::TypeName::new("store metadata")
    }
}

impl<'a: 'b, 'b> From<IdentityStore<'a>> for MetadataStore<'b> {
    fn from(store: IdentityStore<'a>) -> MetadataStore<'b> {
        MetadataStore {
            db: store.db,
            store: MetadataStorage,
        }
    }
}

impl<'a: 'b, 'b> From<GroupStore<'a>> for MetadataStore<'b> {
    fn from(store: GroupStore<'a>) -> MetadataStore<'b> {
        MetadataStore {
            db: store.db,
            store: MetadataStorage,
        }
    }
}

impl<'a> From<&'a WriteTransaction> for MetadataStore<'a> {
    fn from(tx: &'a WriteTransaction) -> MetadataStore<'a> {
        MetadataStore {
            db: DatabaseOrTransaction::WriteTx(tx),
            store: MetadataStorage,
        }
    }
}
impl<'a> From<&'a ReadTransaction> for MetadataStore<'a> {
    fn from(tx: &'a ReadTransaction) -> MetadataStore<'a> {
        MetadataStore {
            db: DatabaseOrTransaction::ReadTx(tx),
            store: MetadataStorage,
        }
    }
}
