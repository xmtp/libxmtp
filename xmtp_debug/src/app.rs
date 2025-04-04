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
/// Types shared between App Functions
mod types;

use clap::CommandFactory;
use color_eyre::eyre::{self, Result};
use directories::ProjectDirs;
use std::{fs, path::PathBuf, sync::Arc};
use xmtp_cryptography::utils::LocalWallet;
use xmtp_db::{EncryptedMessageStore, StorageOption};
use xmtp_id::InboxOwner;
use xmtp_id::associations::unverified::UnverifiedRecoverableEcdsaSignature;
use xmtp_id::associations::unverified::UnverifiedSignature;
use xmtp_mls::identity::IdentityStrategy;

use crate::args::{self, AppOpts};

pub use clients::*;

#[derive(Debug)]
pub struct App {
    /// Local K/V Store/Cache for values
    db: Arc<redb::Database>,
    opts: AppOpts,
}

impl App {
    pub fn new(opts: AppOpts) -> Result<Self> {
        fs::create_dir_all(&Self::data_directory()?)?;
        fs::create_dir_all(&Self::db_directory(&opts.backend)?)?;
        debug!(
            directory = %Self::data_directory()?.display(),
            sqlite_stores = %Self::db_directory(&opts.backend)?.display(),
            "created project directories",
        );
        Ok(Self {
            db: Arc::new(redb::Database::create(Self::redb()?)?),
            opts,
        })
    }

    /// All data stored here
    fn data_directory() -> Result<PathBuf> {
        let data = if let Some(dir) = ProjectDirs::from("org", "xmtp", "xdbg") {
            Ok::<_, eyre::Report>(dir.data_dir().to_path_buf())
        } else {
            eyre::bail!("No Home Directory Path could be retrieved");
        }?;
        Ok(data)
    }

    /// Directory for all SQLite files
    fn db_directory(network: impl Into<u64>) -> Result<PathBuf> {
        let data = Self::data_directory()?;
        let dir = data.join("sqlite").join(network.into().to_string());
        Ok(dir)
    }

    /// Directory for all SQLite files
    fn redb() -> Result<PathBuf> {
        let data = Self::data_directory()?;
        let mut dir = data.join("xdbg");
        dir.set_extension("redb");
        Ok(dir)
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

        if let Some(cmd) = cmd {
            match cmd {
                Generate(g) => generate::Generate::new(g, backend, db).run().await,
                Send(s) => send::Send::new(s, backend, db).run().await,
                Inspect(i) => inspect::Inspect::new(i, backend, db).run().await,
                Query(_q) => todo!(),
                Info(i) => info::Info::new(i, backend, db).run().await,
                Export(e) => export::Export::new(e, db, backend).run(),
                Modify(m) => modify::Modify::new(m, backend, db).run().await,
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
