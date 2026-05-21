use crate::{app::types::*, constants::STORAGE_PREFIX};
use color_eyre::eyre::Result;
use itertools::Itertools;
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
        IdentityStore::new(IdentityStorage, super::DatabaseOrTransaction::Db(value))
    }
}

impl From<Arc<redb::ReadOnlyDatabase>> for IdentityStore<'_> {
    fn from(value: Arc<redb::ReadOnlyDatabase>) -> Self {
        IdentityStore::new(
            IdentityStorage,
            super::DatabaseOrTransaction::ReadOnly(value),
        )
    }
}

impl IdentityStore<'_> {
    /// Look up an identity by `(network, inbox)` across every version
    /// partition. Used by subcommands that perform point lookups without
    /// knowing which xdbg version wrote the identity (e.g. `xdbg send`,
    /// `xdbg inspect`). Strict-mode point lookups should construct the
    /// `IdentityKey` directly with `App::current_version_hash()` and call
    /// `Database::get` instead.
    pub fn find_by_inbox(&self, inbox: [u8; 32]) -> Result<Option<Identity>> {
        use super::Database;
        let Some(iter) = self.load()? else {
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
        &self,
        version_hash: u64,
    ) -> Result<Option<impl Iterator<Item = redb::AccessGuard<'_, Identity>>>> {
        use super::TableProvider;
        self.apply_read(|r| {
            let Some(table) = super::open_table_optional(
                r,
                <Self as TableProvider<IdentityKey, Identity>>::table(),
            )?
            else {
                return Ok(None);
            };
            let start = IdentityKey::low_one_version(version_hash);
            let end = IdentityKey::high_one_version(version_hash);
            let rows: Vec<_> = table.range(start..=end)?.collect::<Result<_, _>>()?;
            Ok(Some(rows.into_iter().map(|(_, v)| v)))
        })
    }

    /// the inverse of the above. load every identity except for identities of version `version_hash`.
    pub fn load_for_other_versions(
        &self,
        version_hash: u64,
    ) -> Result<Option<impl Iterator<Item = redb::AccessGuard<'_, Identity>>>> {
        use super::TableProvider;
        self.apply_read(|r| {
            let Some(table) = super::open_table_optional(
                r,
                <Self as TableProvider<IdentityKey, Identity>>::table(),
            )?
            else {
                return Ok(None);
            };

            let start = IdentityKey::low_one_version(version_hash);
            let end = IdentityKey::high_one_version(version_hash);
            let before = table.range(..start)?;
            let after = table.range(end..)?;
            let rows: Vec<_> = before.chain(after).try_collect()?;
            Ok(Some(rows.into_iter().map(|(_, v)| v)))
        })
    }

    /// Sample up to `n` random identities for `network`, honoring
    /// `--strict-versioning`. When `strict` is true, samples only from
    /// the current binary's version partition (`App::current_version_hash`).
    /// When false, samples from every version partition under `network`.
    ///
    /// Caps at the number of identities available. Callers that need
    /// to exclude additional inboxes (e.g. the group owner) should
    /// over-sample and filter the result.
    pub fn sample_n(
        &self,
        rng: &mut impl rand::Rng,
        n: usize,
        strict: bool,
    ) -> Result<Vec<Identity>> {
        use rand::seq::IteratorRandom;
        if !strict {
            use super::RandomDatabase;
            return Ok(self
                .random_n_capped(rng, n)?
                .iter()
                .map(|g| g.value())
                .collect());
        }
        let Some(iter) = self.load_for_version(crate::app::App::current_version_hash())? else {
            return Ok(Vec::new());
        };
        Ok(iter.map(|g| g.value()).sample(rng, n))
    }
}

impl super::DeriveKey<IdentityKey> for Identity {
    fn key(&self) -> IdentityKey {
        IdentityKey::new(self.version_hash, self.inbox_id)
    }
}

impl super::DeriveKey<IdentityKey> for &Identity {
    fn key(&self) -> IdentityKey {
        IdentityKey::new(self.version_hash, self.inbox_id)
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
    fn increment<'a>(&self, store: impl Into<MetadataStore<'a>>, n: u32) -> Result<()> {
        let store: MetadataStore = store.into();
        store.modify(crate::meta_key!(), |meta| {
            meta.identities += n;
        })?;
        Ok(())
    }

    fn decrement<'a>(&self, store: impl Into<MetadataStore<'a>>, n: u32) -> Result<()> {
        let store: MetadataStore = store.into();
        store.modify(crate::meta_key!(), |meta| {
            meta.identities -= n;
        })?;
        Ok(())
    }
}
