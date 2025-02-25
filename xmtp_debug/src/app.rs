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
use parking_lot::{Mutex, MutexGuard};
use std::future::Future;
use std::{
    fs,
    io::Write,
    path::Path,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

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
// we make liberal use of globals here, probably not the best thing
pub static DIRECTORIES: OnceLock<Directories> = OnceLock::new();
// pub static IS_TERMINATED: AtomicBool = AtomicBool::new(false);
// resolves once the app should exit
static IS_TERMINATED_FUTURE: OnceLock<JoinHandle<()>> = OnceLock::new();

pub struct Directories {
    data: PathBuf,
    sqlite: PathBuf,
    redb: PathBuf,
    diagnostics: Mutex<Box<dyn Write + Send + Sync>>,
}

#[derive(Debug)]
pub struct App {
    /// Local K/V Store/Cache for values
    db: Arc<redb::Database>,
    opts: AppOpts,
}

impl App {
    pub fn new(opts: AppOpts) -> Result<Self> {
        let (tx, rx) = oneshot::channel();

        let data = opts.data_directory()?;
        let db = opts.db_directory(&opts.backend)?;
        let redb = opts.redb()?;
        fs::create_dir_all(&data)?;
        fs::create_dir_all(&db)?;
        // a quick & dirty way to get diagnostics about what just happened
        let w = if let Some(ref f) = opts.diagnostics {
            Box::new(fs::File::create(f)?) as Box<dyn Write + Send + Sync>
        } else {
            Box::new(std::io::stdout()) as Box<dyn Write + Send + Sync>
        };

        DIRECTORIES.get_or_init(|| Directories {
            data,
            sqlite: db,
            redb,
            diagnostics: Mutex::new(w),
        });

        std::thread::spawn(move || {
            let mut tx = Some(tx);
            let term = Arc::new(AtomicBool::new(false));
            signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&term))?;
            signal_hook::flag::register(signal_hook::consts::SIGHUP, Arc::clone(&term))?;
            signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&term))?;
            signal_hook::flag::register(signal_hook::consts::SIGQUIT, Arc::clone(&term))?;
            while !term.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(15));
            }
            if let Some(t) = tx.take() {
                let _ = t.send(());
            }
            Ok::<_, eyre::Report>(())
        });
        IS_TERMINATED_FUTURE.get_or_init(|| {
            tokio::task::spawn(async move {
                let _ = rx.await;
            })
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

    pub fn is_terminated_future() -> impl Future<Output = ()> {
        let terminate = IS_TERMINATED_FUTURE
            .get()
            .expect("must exist for app to run")
            .is_finished();
        std::future::poll_fn(move |_cx| {
            if terminate {
                std::task::Poll::Ready(())
            } else {
                std::task::Poll::Pending
            }
        })
    }

    #[allow(unused)]
    pub fn is_terminated() -> bool {
        IS_TERMINATED_FUTURE
            .get()
            .expect("msut exist for app to run")
            .is_finished()
    }

    pub fn diagnostics() -> MutexGuard<'static, Box<dyn Write + Send + Sync>> {
        let d = DIRECTORIES.get().expect("must exist for app to run");
        d.diagnostics.lock()
    }

    pub fn write_diagnostic(diagnostic: types::Diagnostic) -> std::io::Result<()> {
        let mut d = Self::diagnostics();
        serde_json::to_writer(&mut *d, &diagnostic)?;
        Ok(())
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

        let mut d = Self::diagnostics();
        d.flush()?;
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
