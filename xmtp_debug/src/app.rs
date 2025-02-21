//! Application functions

/// Different ways to generate a client
mod clients;
/// Export commands
mod export;
/// Generate functionality
mod generate;
/// Information about this app
mod info;
/// Inspect data on the XMTP Network
mod inspect;
/// Modify entitites on the network
mod modify;
/// Query for data on the network
mod query;
/// Send functionality
mod send;
/// Local storage
mod store;
/// Streaming
mod stream;
/// Types shared between App Functions
mod types;

use clap::CommandFactory;
use color_eyre::eyre::{self, Result};
use std::{fs, path::Path, path::PathBuf, sync::Arc};
use xmtp_cryptography::utils::LocalWallet;
use xmtp_id::associations::unverified::UnverifiedRecoverableEcdsaSignature;
use xmtp_id::associations::{generate_inbox_id, unverified::UnverifiedSignature};
use xmtp_id::InboxOwner;
use xmtp_mls::{
    identity::IdentityStrategy,
    storage::{EncryptedMessageStore, StorageOption},
};

use crate::args::{self, AppOpts};

pub use clients::*;

use std::sync::OnceLock;
pub static DIRECTORIES: OnceLock<Directories> = OnceLock::new();

pub struct Directories {
    data: PathBuf,
    sqlite: PathBuf,
    redb: PathBuf,
}

#[derive(Debug)]
pub struct App {
    /// Local K/V Store/Cache for values
    db: Arc<redb::Database>,
    opts: AppOpts,
}

impl App {
    pub fn new(opts: AppOpts) -> Result<Self> {
        let data = opts.data_directory()?;
        let db = opts.db_directory(&opts.backend)?;
        let redb = opts.redb()?;
        fs::create_dir_all(&data)?;
        fs::create_dir_all(&db)?;

        DIRECTORIES.get_or_init(|| Directories {
            data,
            sqlite: db,
            redb,
        });
        debug!(
            directory = %opts.data_directory()?.display(),
            sqlite_stores = %opts.db_directory(&opts.backend)?.display(),
            "created project directories",
        );
        Ok(Self {
            db: Arc::new(redb::Database::create(Self::redb())?),
            opts,
        })
    }

    pub fn data_directory() -> &'static Path {
        let d = DIRECTORIES.get().expect("must exist for app to run");
        d.data.as_path()
    }

    pub fn db_directory() -> &'static Path {
        let d = DIRECTORIES.get().expect("must exist for app to run");
        d.sqlite.as_path()
    }

    pub fn redb() -> &'static Path {
        let d = DIRECTORIES.get().expect("must exist for app to run");
        d.redb.as_path()
    }

    pub async fn run(self) -> Result<()> {
        let App { db, opts } = self;
        use args::Commands::*;
        let AppOpts {
            cmd,
            backend,
            clear,
            ..
        } = opts;
        debug!(fdlimit = get_fdlimit());

        if cmd.is_none() && !clear {
            AppOpts::command().print_help()?;
            eyre::bail!("No subcommand was specified");
        }

        // can 100% turn this into a trait
        if let Some(cmd) = cmd {
            match cmd {
                Generate(g) => generate::Generate::new(g, backend, db).run().await,
                Send(s) => send::Send::new(s, backend, db).run().await,
                Inspect(i) => inspect::Inspect::new(i, backend, db).run().await,
                Query(_q) => todo!(),
                Info(i) => info::Info::new(i, backend, db).run().await,
                Export(e) => export::Export::new(e, db, backend).run(),
                Modify(m) => modify::Modify::new(m, backend, db).run().await,
                Stream(s) => stream::Stream::new(s, backend, db).run().await,
            }?;
        }

        if clear {
            let data = Self::data_directory();
            info!("Clearing app data");
            let _ = std::fs::remove_dir_all(data);
        }
        Ok(())
    }
}

fn generate_wallet() -> types::EthereumWallet {
    types::EthereumWallet::default()
}

static FDLIMIT: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
/// Tries to raise the open file descriptor limit
/// returns a default low-enough number if unsuccesful
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
