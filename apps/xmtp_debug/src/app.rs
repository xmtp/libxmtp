//! Application functions

/// Different ways to generate a client
mod clients;
/// Export commands
mod export;
/// Generate functionality
mod generate;
/// E2E health check across all protocol ops
mod health;
/// Information about this app
mod info;
/// Inspect data on the XMTP Network
mod inspect;
/// Modify entities on the network
mod modify;
/// Query for data on the network
mod query;
/// Send functionality
mod send;
/// Local storage
mod store;
/// Message/Conversation Streaming
mod stream;
/// Sync loaded identities against the network and reconcile redb
mod sync;
/// E2E latency test scenarios
mod test;
/// Types shared between App Functions
mod types;

use clap::CommandFactory;
use color_eyre::eyre::{self, Result};
use directories::ProjectDirs;
use redb::{DatabaseError, ReadableDatabase, TableHandle};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    sync::OnceLock,
};
use xmtp_db::EncryptedMessageStore;
use xmtp_id::InboxOwner;
use xmtp_mls::identity::IdentityStrategy;
use xxhash_rust::xxh3;

use crate::args::{self, AppOpts};

pub use clients::*;

#[derive(Debug)]
pub struct App {
    /// Local K/V Store/Cache for values
    opts: AppOpts,
}

impl App {
    pub fn new(opts: AppOpts) -> Self {
        Self { opts }
    }

    pub fn init_db(&self) -> Result<()> {
        fs::create_dir_all(&Self::data_directory()?)?;
        fs::create_dir_all(&Self::db_directory(&self.opts.backend)?)?;
        Self::detect_legacy_db(&Self::redb()?)?;
        debug!(
            directory = %Self::data_directory()?.display(),
            sqlite_stores = %Self::db_directory(&self.opts.backend)?.display(),
            "created project directories",
        );
        Ok(())
    }

    fn readonly_db() -> Result<Arc<redb::ReadOnlyDatabase>> {
        match redb::ReadOnlyDatabase::open(Self::redb()?) {
            // if the db is corrupted attempt a repair
            Err(DatabaseError::RepairAborted) => {
                let integrity = {
                    let mut rw = redb::Database::open(Self::redb()?)?;
                    rw.check_integrity()
                };
                match integrity {
                    // db is ok can be reopened
                    Ok(true) => Self::readonly_db(),
                    // db was broken but is repaired
                    Ok(false) => Self::readonly_db(),
                    Err(DatabaseError::DatabaseAlreadyOpen) => {
                        tracing::warn!(
                            "db repair attempted but cannot continue because opened in a different process."
                        );
                        Err(DatabaseError::DatabaseAlreadyOpen.into())
                    }
                    Err(_) => {
                        panic!("db file corrupted & unrecoverable. run `xdbg --clear` to restart")
                    }
                }
            }
            Ok(db) => Ok(Arc::new(db)),
            Err(e) => Err(e.into()),
        }
    }

    fn db() -> Result<Arc<redb::Database>> {
        Ok(Arc::new(redb::Database::create(Self::redb()?)?))
    }

    /// All data stored here.
    /// Respects `XDBG_DB_ROOT` env var for overriding the default data directory.
    fn data_directory() -> Result<PathBuf> {
        if let Ok(root) = std::env::var("XDBG_DB_ROOT") {
            return Ok(PathBuf::from(root));
        }
        let data = if let Some(dir) = ProjectDirs::from("org", "xmtp", "xdbg") {
            Ok::<_, eyre::Report>(dir.data_dir().to_path_buf())
        } else {
            eyre::bail!("No Home Directory Path could be retrieved");
        }?;
        Ok(data)
    }

    /// Hash of `crate::get_version()`'s raw output, used as the version
    /// dimension of `IdentityKey` and as the SQLite-path version
    /// segment. Computed once per process.
    pub fn current_version_hash() -> u64 {
        static CACHE: OnceLock<u64> = OnceLock::new();
        *CACHE.get_or_init(|| xxh3::xxh3_64(crate::get_version().as_bytes()))
    }

    /// Returns whether `--strict-versioning` was set at startup. Read
    /// from a process-global `OnceLock` populated by `App::run` so
    /// internal helpers (`load_all_identities`, `HealthContext`,
    /// `Sync`, etc.) don't have to thread the flag through every
    /// constructor. Defaults to `false` when not yet initialized (only
    /// meaningful in tests that haven't booted via `App::run`).
    pub fn strict_versioning() -> bool {
        STRICT_VERSIONING.get().copied().unwrap_or(false)
    }

    /// Record the strict-versioning flag at startup. Called once from
    /// `App::run`. Subsequent calls are silently ignored — the first
    /// write wins so accidental re-initialization in tests can't
    /// corrupt state.
    fn set_strict_versioning(value: bool) {
        let _ = STRICT_VERSIONING.set(value);
    }

    /// Directory for SQLite files belonging to the *current* binary
    /// version. Always version-bucketed via `current_version_hash`.
    /// Same shape in both strict and non-strict mode — the
    /// `--strict-versioning` flag does not influence path construction.
    fn db_directory(network: impl Into<u64>) -> Result<PathBuf> {
        Self::db_directory_for(network, Self::current_version_hash())
    }

    /// Directory for SQLite files belonging to a specific
    /// `version_hash`. Used by non-strict reads that load identities
    /// written by another xdbg binary version. Most callers should use
    /// `db_directory(network)` instead.
    fn db_directory_for(network: impl Into<u64>, version_hash: u64) -> Result<PathBuf> {
        let data = Self::data_directory()?;
        let dir = data
            .join("sqlite")
            .join(network.into().to_string())
            .join(format!("{version_hash:016x}"));
        Ok(dir)
    }

    /// Directory for all SQLite files
    fn redb() -> Result<PathBuf> {
        let data = Self::data_directory()?;
        let mut dir = data.join("xdbg");
        dir.set_extension("redb");
        Ok(dir)
    }

    /// Refuse to run against a redb file written by an xdbg version
    /// whose `Identity` value layout is incompatible with the current
    /// binary. Each schema bump renames the table namespace
    /// (`xdbg:N//identity`); presence of any old namespace (v1, v2)
    /// triggers abort. v3 adds a fixed-width `version_string` field
    /// to `Identity`.
    ///
    /// Called by `App::new` before any other DB activity.
    pub fn detect_legacy_db(redb_path: &Path) -> Result<()> {
        if !redb_path.exists() {
            return Ok(());
        }
        let db = redb::Database::open(redb_path)?;
        let r = db.begin_read()?;
        const LEGACY_NAMESPACES: &[&str] = &[
            const_format::concatcp!(crate::constants::STORAGE_PREFIX, ":1//identity"),
            const_format::concatcp!(crate::constants::STORAGE_PREFIX, ":2//identity"),
        ];
        for handle in r.list_tables()? {
            if LEGACY_NAMESPACES.contains(&handle.name()) {
                eyre::bail!(
                    "this XDBG_DB_ROOT was written by an older xdbg with an \
                     incompatible IdentityStore schema ({}). Run `xdbg --clear` \
                     to remove all xdbg state, or use a different XDBG_DB_ROOT.",
                    handle.name()
                );
            }
        }
        Ok(())
    }

    pub async fn run(self) -> Result<()> {
        if !self.opts.clear {
            self.init_db()?
        }
        let App { opts } = self;
        use args::Commands::*;
        let AppOpts {
            cmd,
            backend,
            clear,
            strict_versioning,
            ..
        } = opts;
        Self::set_strict_versioning(strict_versioning);
        debug!(fdlimit = get_fdlimit(), "setting fdlimit");

        if cmd.is_none() && !clear {
            AppOpts::command().print_help()?;
            eyre::bail!("No subcommand was specified");
        }

        if let Some(cmd) = cmd {
            match cmd {
                Generate(g) => generate::Generate::new(g, backend).run().await,
                Send(s) => send::Send::new(s, backend)?.run().await,
                Inspect(i) => inspect::Inspect::new(i, backend)?.run().await,
                Query(q) => query::Query::new(q, backend)?.run().await,
                Info(i) => info::Info::new(i, backend)?.run().await,
                Export(e) => export::Export::new(e, backend)?.run(),
                Modify(m) => modify::Modify::new(m, backend)?.run().await,
                Stream(s) => stream::Stream::new(s, backend)?.run().await,
                Test(t) => test::Test::new(t, backend).run().await,
                Healthcheck(h) => health::Health::new(h, backend).run().await,
                Sync(s) => sync::Sync::new(s, backend).run().await,
            }?;
        }

        if clear {
            info!("Clearing app data");
            let data = Self::data_directory()?;
            let _ = std::fs::remove_dir_all(data);
        }
        Ok(())
    }
}

pub(super) fn generate_wallet() -> types::EthereumWallet {
    types::EthereumWallet::default()
}

#[cfg(test)]
mod app_db_directory_tests {
    use super::App;
    use std::sync::Mutex;

    // Tests in this module mutate XDBG_DB_ROOT. Run them serialized so
    // they don't observe each other's env state.
    static ENV_GUARD: Mutex<()> = Mutex::new(());

    fn with_db_root<R>(f: impl FnOnce(&std::path::Path) -> R) -> R {
        // Drop-based guard so XDBG_DB_ROOT is restored even if `f` panics —
        // otherwise the env var would leak across tests pointing at a
        // deleted tempdir, violating the ENV_GUARD invariant.
        struct Restore(Option<String>);
        impl Drop for Restore {
            fn drop(&mut self) {
                // SAFETY: serialized via ENV_GUARD; this runs before the
                // mutex guard is released because guards drop in reverse
                // declaration order.
                unsafe {
                    match self.0.take() {
                        Some(v) => std::env::set_var("XDBG_DB_ROOT", v),
                        None => std::env::remove_var("XDBG_DB_ROOT"),
                    }
                }
            }
        }

        let _g = ENV_GUARD.lock().unwrap();
        let tmp = tempfile::tempdir().expect("tempdir");
        let _restore = Restore(std::env::var("XDBG_DB_ROOT").ok());
        // SAFETY: serialized via ENV_GUARD; `_restore` reverts on unwind.
        unsafe {
            std::env::set_var("XDBG_DB_ROOT", tmp.path());
        }
        f(tmp.path())
    }

    #[test]
    fn db_directory_for_uses_provided_version_hash() {
        with_db_root(|root| {
            let net = 1u64;
            let vh: u64 = 0xDEAD_BEEF_FEED_FACE;
            let dir = App::db_directory_for(net, vh).expect("db_directory_for");
            let vh_hex = format!("{vh:016x}");
            let expected = root.join("sqlite").join(net.to_string()).join(&vh_hex);
            assert_eq!(dir, expected);
        });
    }

    #[test]
    fn db_directory_matches_db_directory_for_current_version() {
        with_db_root(|_root| {
            let net = 7u64;
            let a = App::db_directory(net).expect("db_directory");
            let b =
                App::db_directory_for(net, App::current_version_hash()).expect("db_directory_for");
            assert_eq!(a, b);
        });
    }
}

#[cfg(test)]
mod legacy_detection_tests {
    use redb::TableDefinition;

    fn seed_table_named(path: &std::path::Path, name: &'static str) {
        let db = redb::Database::create(path).unwrap();
        let table: TableDefinition<&[u8], &[u8]> = TableDefinition::new(name);
        let w = db.begin_write().unwrap();
        {
            let mut t = w.open_table(table).unwrap();
            t.insert(b"k".as_slice(), b"v".as_slice()).unwrap();
        }
        w.commit().unwrap();
    }

    #[test]
    fn legacy_detection_aborts_on_v1_table() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("xdbg.redb");
        seed_table_named(&path, "xdbg:1//identity");
        let err = super::App::detect_legacy_db(&path).expect_err("must abort");
        assert!(format!("{err:#}").contains("--clear"));
    }

    #[test]
    fn legacy_detection_aborts_on_v2_table() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("xdbg.redb");
        seed_table_named(&path, "xdbg:2//identity");
        let err = super::App::detect_legacy_db(&path).expect_err("must abort");
        assert!(format!("{err:#}").contains("--clear"));
    }

    #[test]
    fn legacy_detection_passes_on_fresh_db() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("xdbg.redb");
        // Path doesn't exist; detect_legacy_db must be a no-op.
        super::App::detect_legacy_db(&path).expect("fresh DB ok");
    }

    #[test]
    fn legacy_detection_passes_on_current_table_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("xdbg.redb");
        seed_table_named(&path, "xdbg:3//identity");
        super::App::detect_legacy_db(&path).expect("only-current-table must pass");
    }
}

static STRICT_VERSIONING: OnceLock<bool> = OnceLock::new();
static FDLIMIT: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
/// Tries to raise the open file descriptor limit
/// returns a default low-enough number if unsuccessful
/// useful when dealing with lots of different sqlite databases
fn get_fdlimit() -> usize {
    *FDLIMIT.get_or_init(|| {
        if let Ok(fdlimit::Outcome::LimitRaised { to, .. }) = fdlimit::raise_fd_limit() {
            if to > 2048 { 2048 } else { to as usize }
        } else {
            64
        }
    })
}
