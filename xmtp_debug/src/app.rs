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
use xmtp_db::{EncryptedMessageStore, StorageOption};
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
    pub fn new(opts: AppOpts) -> Result<Self> {
        fs::create_dir_all(&Self::data_directory()?)?;
        fs::create_dir_all(&Self::db_directory(&opts.backend)?)?;
        Self::detect_legacy_db(&Self::redb()?)?;
        debug!(
            directory = %Self::data_directory()?.display(),
            sqlite_stores = %Self::db_directory(&opts.backend)?.display(),
            "created project directories",
        );
        Ok(Self { opts })
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
        let App { opts } = self;
        use args::Commands::*;
        let AppOpts {
            cmd,
            backend,
            clear,
            strict_versioning,
            ..
        } = opts;
        debug!(fdlimit = get_fdlimit(), "setting fdlimit");

        if cmd.is_none() && !clear {
            AppOpts::command().print_help()?;
            eyre::bail!("No subcommand was specified");
        }

        if let Some(cmd) = cmd {
            match cmd {
                Generate(g) => {
                    generate::Generate::new(g, backend, strict_versioning)
                        .run()
                        .await
                }
                Send(s) => send::Send::new(s, backend)?.run().await,
                Inspect(i) => inspect::Inspect::new(i, backend)?.run().await,
                Query(q) => query::Query::new(q, backend)?.run().await,
                Info(i) => info::Info::new(i, backend)?.run().await,
                Export(e) => export::Export::new(e, backend)?.run(),
                Modify(m) => modify::Modify::new(m, backend)?.run().await,
                Stream(s) => stream::Stream::new(s, backend)?.run().await,
                Healthcheck(h) => {
                    health::Health::new(h, backend, strict_versioning)
                        .run()
                        .await
                }
                Sync(s) => sync::Sync::new(s, backend, strict_versioning).run().await,
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

fn generate_wallet() -> types::EthereumWallet {
    types::EthereumWallet::default()
}

#[cfg(test)]
mod app_version_hash_tests {
    use super::App;

    #[test]
    fn current_version_hash_is_stable() {
        let a = App::current_version_hash();
        let b = App::current_version_hash();
        assert_eq!(a, b, "version hash must be stable within a process");
    }

    #[test]
    fn current_version_hash_is_nonzero() {
        // xxh3_64 of any non-empty string is overwhelmingly non-zero
        assert_ne!(App::current_version_hash(), 0);
    }
}

#[cfg(test)]
mod app_db_directory_tests {
    use super::App;
    use std::sync::Mutex;

    // Tests in this module mutate XDBG_DB_ROOT. Run them serialized so
    // they don't observe each other's env state.
    static ENV_GUARD: Mutex<()> = Mutex::new(());

    fn with_db_root<R>(f: impl FnOnce(&std::path::Path) -> R) -> R {
        let _g = ENV_GUARD.lock().unwrap();
        let tmp = tempfile::tempdir().expect("tempdir");
        let prev = std::env::var("XDBG_DB_ROOT").ok();
        // SAFETY: serialized via ENV_GUARD; restored before unlock.
        unsafe {
            std::env::set_var("XDBG_DB_ROOT", tmp.path());
        }
        let out = f(tmp.path());
        unsafe {
            match prev {
                Some(v) => std::env::set_var("XDBG_DB_ROOT", v),
                None => std::env::remove_var("XDBG_DB_ROOT"),
            }
        }
        out
    }

    #[test]
    fn db_directory_includes_version_hash_segment() {
        with_db_root(|root| {
            let net = 42u64;
            let dir = App::db_directory(net).expect("db_directory");
            let vh_hex = format!("{:016x}", App::current_version_hash());
            let expected = root.join("sqlite").join(net.to_string()).join(&vh_hex);
            assert_eq!(dir, expected);
        });
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
    use std::path::Path;

    /// Seed a single arbitrarily-named table with one row so
    /// `detect_legacy_db` sees it via `list_tables`.
    fn seed_table_named(path: &Path, namespace: &'static str) {
        let db = redb::Database::create(path).unwrap();
        let table: TableDefinition<&[u8], &[u8]> = TableDefinition::new(namespace);
        let w = db.begin_write().unwrap();
        {
            let mut t = w.open_table(table).unwrap();
            t.insert(b"k".as_slice(), b"v".as_slice()).unwrap();
        }
        w.commit().unwrap();
    }

    fn assert_aborts_with_clear_hint(path: &Path) {
        let err = super::App::detect_legacy_db(path).expect_err("must abort");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("--clear"),
            "error must instruct user to run --clear, got: {msg}"
        );
    }

    #[test]
    fn legacy_detection_aborts_on_v1_table() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("xdbg.redb");
        seed_table_named(&path, "xdbg:1//identity");
        assert_aborts_with_clear_hint(&path);
    }

    #[test]
    fn legacy_detection_aborts_on_v2_table() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("xdbg.redb");
        seed_table_named(&path, "xdbg:2//identity");
        assert_aborts_with_clear_hint(&path);
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

static FDLIMIT: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
/// Tries to raise the open file descriptor limit
/// returns a default low-enough number if unsuccessful
/// useful when dealing with lots of different sqlite databases
fn get_fdlimit() -> usize {
    *FDLIMIT.get_or_init(|| {
        if let Ok(fdlimit::Outcome::LimitRaised { to, .. }) = fdlimit::raise_fd_limit() {
            if to > 512 {
                // we can go higher but 1024 seems reasonable
                512
            } else {
                to as usize
            }
        } else {
            64
        }
    })
}
