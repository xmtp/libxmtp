use crate::{app::types::*, constants::STORAGE_PREFIX};
use color_eyre::eyre::Result;
use redb::TableDefinition;
use std::sync::Arc;

use super::{Database, MetadataStore};

pub const MODULE: &str = "identity";
pub const VERSION: u16 = 3;
pub const NAMESPACE: &str = const_format::concatcp!(STORAGE_PREFIX, ":", VERSION, "//", MODULE);

// Re-export the canonical key type at this path so the rest of the codebase
// can keep using `crate::app::store::IdentityKey` without needing to know
// about the `store` module's internal layout.
pub use super::IdentityKey;

/// Mapping of (network, version_hash, inbox) to a bare-minimum
/// serialized Identity
const TABLE: TableDefinition<IdentityKey, Identity> = TableDefinition::new(NAMESPACE);

pub type IdentityStore<'a> = super::KeyValueStore<'a, IdentityStorage>;

impl From<Arc<redb::Database>> for IdentityStore<'_> {
    fn from(value: Arc<redb::Database>) -> Self {
        IdentityStore {
            db: super::DatabaseOrTransaction::Db(value),
            store: IdentityStorage,
        }
    }
}

impl From<Arc<redb::ReadOnlyDatabase>> for IdentityStore<'_> {
    fn from(value: Arc<redb::ReadOnlyDatabase>) -> Self {
        IdentityStore {
            db: super::DatabaseOrTransaction::ReadOnly(value),
            store: IdentityStorage,
        }
    }
}

impl IdentityStore<'_> {
    /// Look up an identity by `(network, inbox)` across every version
    /// partition. Used by subcommands that perform point lookups without
    /// knowing which xdbg version wrote the identity (e.g. `xdbg send`,
    /// `xdbg inspect`). Strict-mode point lookups should construct the
    /// `IdentityKey` directly with `App::current_version_hash()` and call
    /// `Database::get` instead.
    pub fn find_by_inbox(&self, network: u64, inbox: [u8; 32]) -> Result<Option<Identity>> {
        use super::Database;
        let Some(iter) = self.load(network)? else {
            return Ok(None);
        };
        for guard in iter {
            let id = guard.value();
            if id.inbox_id == inbox {
                return Ok(Some(id));
            }
        }
        Ok(None)
    }

    /// Strict-mode load: returns only identities under
    /// `(network, version_hash)`. The default `Database::load` returns
    /// every version on the network; this filtered variant backs
    /// `--strict-versioning`.
    pub fn load_for_version(
        &'_ self,
        network: impl Into<u64>,
        version_hash: u64,
    ) -> Result<Option<impl Iterator<Item = redb::AccessGuard<'_, Identity>>>> {
        use super::TableProvider;
        let network: u64 = network.into();
        self.apply_read(|r| {
            let Some(table) =
                super::open_table_optional(r, <Self as TableProvider<IdentityKey, Identity>>::table())?
            else {
                return Ok(None);
            };
            let start = IdentityKey::low_one_version(network, version_hash);
            let end = IdentityKey::high_one_version(network, version_hash);
            let rows: Vec<_> = table.range(start..=end)?.collect::<Result<_, _>>()?;
            Ok(Some(rows.into_iter().map(|(_, v)| v)))
        })
    }
}

impl super::DeriveKey<IdentityKey> for Identity {
    fn key(&self, network: u64) -> IdentityKey {
        IdentityKey {
            network,
            version_hash: self.version_hash,
            inbox: self.inbox_id,
        }
    }
}

impl super::DeriveKey<IdentityKey> for &Identity {
    fn key(&self, network: u64) -> IdentityKey {
        IdentityKey {
            network,
            version_hash: self.version_hash,
            inbox: self.inbox_id,
        }
    }
}

#[derive(Debug, Clone)]
pub struct IdentityStorage;

impl<'a> super::TableProvider<'a, IdentityKey, Identity> for IdentityStorage {
    fn table() -> TableDefinition<'a, IdentityKey, Identity> {
        TABLE
    }
}

impl super::TrackMetadata for IdentityStorage {
    fn increment<'a>(
        &self,
        store: impl Into<MetadataStore<'a>>,
        network: u64,
        n: u32,
    ) -> Result<()> {
        let store: MetadataStore = store.into();
        store.modify(crate::meta_key!(network), |meta| {
            meta.identities += n;
        })?;
        Ok(())
    }

    fn decrement<'a>(
        &self,
        store: impl Into<MetadataStore<'a>>,
        network: u64,
        n: u32,
    ) -> Result<()> {
        let store: MetadataStore = store.into();
        store.modify(crate::meta_key!(network), |meta| {
            meta.identities -= n;
        })?;
        Ok(())
    }
}
