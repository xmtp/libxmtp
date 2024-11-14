#![allow(unused)]
use color_eyre::eyre;
use color_eyre::eyre::Result;
use redb::TableDefinition;
use rkyv::{Archive, Deserialize, Serialize};
use std::sync::Arc;
use xxhash_rust::xxh3;

use crate::{app::types::Identity, args, constants::STORAGE_PREFIX};

pub const MODULE: &str = "generate";
pub const VERSION: u16 = 1;
pub const NAMESPACE: &str = const_format::concatcp!(STORAGE_PREFIX, ":", VERSION, "//", MODULE);

type InboxId = [u8; 32];

/// Mapping of InboxID to a bare-minimum serialized Identity
const TABLE: TableDefinition<&IdentityKey, &Identity> = TableDefinition::new(NAMESPACE);

/// Internal key for an identity
#[derive(Debug, Archive, Serialize, Deserialize)]
struct IdentityKey {
    /// a short network identifier
    network: u64,
    inbox_id: InboxId,
}

impl IdentityKey {
    pub fn new(network: u64, identity: &Identity) -> Self {
        Self {
            network: network.into(),
            inbox_id: identity.inbox_id,
        }
    }

    fn create_low(prefix: u64) -> Self {
        IdentityKey {
            network: prefix,
            inbox_id: [0; 32],
        }
    }

    fn create_high(prefix: u64) -> Self {
        IdentityKey {
            network: prefix,
            inbox_id: [u8::MAX; 32],
        }
    }
}

impl<'key> redb::Value for &'key IdentityKey {
    type SelfType<'a> = IdentityKey
    where
        Self: 'a;

    type AsBytes<'a> = [u8; 40]
    where
        Self: 'a;

    fn fixed_width() -> Option<usize> {
        Some(40)
    }

    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
    where
        Self: 'a,
    {
        // TODO: maybe unwraps, these should never really fail
        let archived: &ArchivedIdentityKey =
            rkyv::access::<ArchivedIdentityKey, rkyv::rancor::Error>(data).unwrap();
        let deserialized: IdentityKey =
            rkyv::deserialize::<_, rkyv::rancor::Error>(archived).unwrap();
        deserialized
    }

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>
    where
        Self: 'a,
        Self: 'b,
    {
        let mut array = [0u8; 40];
        let bytes = rkyv::api::high::to_bytes::<rkyv::rancor::Error>(value).unwrap();
        array.copy_from_slice(&bytes[0..]);
        array
    }

    fn type_name() -> redb::TypeName {
        redb::TypeName::new("identity key")
    }
}

impl<'a> redb::Key for &'a IdentityKey {
    fn compare(data1: &[u8], data2: &[u8]) -> std::cmp::Ordering {
        data1.cmp(data2)
    }
}

impl<'a> From<&'a args::BackendOpts> for u64 {
    fn from(value: &'a args::BackendOpts) -> Self {
        use args::BackendKind::*;

        if let Some(ref url) = value.url {
            xxh3::xxh3_64(url.as_str().as_bytes())
        } else {
            match value.backend {
                Production => 2,
                Dev => 1,
                Local => 0,
            }
        }
    }
}

impl From<args::BackendOpts> for u64 {
    fn from(value: args::BackendOpts) -> Self {
        (&value).into()
    }
}

impl From<Arc<redb::Database>> for IdentityStore {
    fn from(value: Arc<redb::Database>) -> Self {
        IdentityStore { db: value }
    }
}

#[derive(Debug)]
pub struct IdentityStore {
    db: Arc<redb::Database>,
}

impl IdentityStore {
    pub fn new(db: Arc<redb::Database>) -> Self {
        Self { db }
    }

    fn set(&self, identity: Identity, network: args::BackendOpts) -> Result<()> {
        let write = self.db.begin_write()?;
        let key = IdentityKey::new(network.into(), &identity);
        {
            let mut table = write.open_table(TABLE)?;
            table.insert(key, identity)?;
        }
        write.commit()?;
        Ok(())
    }

    pub fn set_all(&self, identities: &[Identity], network: args::BackendOpts) -> Result<()> {
        let write = self.db.begin_write()?;
        let network: u64 = network.into();
        {
            let mut table = write.open_table(TABLE)?;
            for identity in identities.iter() {
                let key = IdentityKey::new(network, &identity);
                table.insert(key, identity)?;
            }
        }
        write.commit()?;
        Ok(())
    }

    fn get(&self, key: &IdentityKey) -> Result<Option<Identity>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(TABLE)?;
        let identity = table.get(key)?.map(|v| v.value());
        Ok(identity)
    }

    /// Clear all identities that belong to a certain network
    pub fn clear_network(&self, network: args::BackendOpts) -> Result<()> {
        let network: u64 = network.into();
        let write = self.db.begin_write()?;
        {
            let mut table = write.open_table(TABLE)?;
            table.retain(|k: IdentityKey, _| !k.network == network)?;
        }
        write.commit()?;
        Ok(())
    }

    pub fn identities(
        &self,
        network: &args::BackendOpts,
    ) -> Result<impl Iterator<Item = Result<Identity>>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(TABLE)?;
        let start = IdentityKey::create_low(network.into());
        let end = IdentityKey::create_high(network.into());
        Ok(table.range(start..end)?.map(|r| match r {
            Ok((_, v)) => Ok(v.value()),
            Err(e) => Err(eyre::Error::from(e)),
        }))
    }
}
