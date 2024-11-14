//! Application functions

/// Generate functionality
mod generate;
/// Inspect data on the XMTP Network
mod inspect;
/// Query for data on the network
mod query;
/// Local storage
mod store;
/// Types shared between App Functions
mod types;

use color_eyre::eyre::{self, Result};
use directories::ProjectDirs;
use ecdsa::signature::rand_core::RngCore;
use std::{fs, path::PathBuf, sync::Arc};
use xmtp_cryptography::utils::LocalWallet;
use xmtp_id::associations::generate_inbox_id;
use xmtp_id::InboxOwner;
use xmtp_mls::{
    identity::IdentityStrategy,
    storage::{EncryptedMessageStore, StorageOption},
};

use crate::args::{self, AppOpts};

#[derive(Debug)]
pub struct App {
    /// Local K/V Store/Cache for values
    db: Arc<redb::Database>,
    opts: AppOpts,
}

impl App {
    pub fn new(opts: AppOpts) -> Result<Self> {
        let db = Self::data_directory()?.join("xdbg.redb");
        Ok(Self {
            db: Arc::new(redb::Database::create(db)?),
            opts,
        })
    }

    /// All data stored here
    fn data_directory() -> Result<PathBuf> {
        let data = if let Some(dir) = ProjectDirs::from("org", "xmtp", "xdb") {
            Ok::<_, eyre::Report>(dir.data_dir().to_path_buf())
        } else {
            eyre::bail!("No Home Directory Path could be retrieved");
        }?;

        fs::create_dir_all(&data)?;
        Ok(data)
    }

    /// Directory for all SQLite files
    fn db_directory(network: &args::BackendOpts) -> Result<PathBuf> {
        let data = Self::data_directory()?;
        Ok(data.join("sqlite").join(u64::from(network).to_string()))
    }

    pub async fn run(self) -> Result<()> {
        let App { db, opts } = self;
        use args::Commands::*;
        match opts.cmd {
            Generate(g) => {
                generate::Generate::new(g, opts.backend, db.into())
                    .run()
                    .await
            }
            Inspect(_i) => todo!(),
            Query(_q) => todo!(),
        }?;
        Ok(())
    }
}

async fn client(
    network: args::BackendOpts,
    wallet: Option<&types::EthereumWallet>,
) -> Result<crate::DbgClient> {
    let local_wallet = if let Some(w) = wallet {
        w.clone().to_ethers()
    } else {
        generate_wallet().to_ethers()
    };
    client_inner(network, &local_wallet, None).await
}

/// Create a new client + Identity
async fn temp_client(
    network: &args::BackendOpts,
    wallet: Option<&types::EthereumWallet>,
) -> Result<crate::DbgClient> {
    let local_wallet = if let Some(w) = wallet {
        w.clone().to_ethers()
    } else {
        generate_wallet().to_ethers()
    };

    let tmp_dir = (*crate::constants::TMPDIR).path();
    let public = hex::encode(local_wallet.get_address());
    let name = format!("{public}:{}.db3", u64::from(network));

    client_inner(
        network.clone(),
        &local_wallet,
        Some(tmp_dir.to_path_buf().join(name)),
    )
    .await
}

/// Create a new client + Identity
async fn client_inner(
    network: args::BackendOpts,
    wallet: &LocalWallet,
    db_path: Option<PathBuf>,
) -> Result<crate::DbgClient> {
    let url = url::Url::from(network.clone());
    let is_secure = url.scheme() == "https";
    println!("Attempting to create grpc, URL: [{url}], is_secure=[{is_secure}]");
    let api = crate::GrpcClient::create(url.as_str().to_string(), is_secure).await?;

    let nonce = 1;
    let inbox_id = generate_inbox_id(&wallet.get_address(), &nonce).unwrap();

    let dir = if let Some(p) = db_path {
        p
    } else {
        let dir = crate::app::App::db_directory(&network)?;
        let db_name = format!("{inbox_id}:{}.db3", u64::from(network));
        dir.join(db_name)
    };

    crate::DbgClient::builder(IdentityStrategy::CreateIfNotFound(
        inbox_id,
        wallet.get_address(),
        nonce,
        None,
    ))
    .api_client(api)
    .store(
        EncryptedMessageStore::new(
            StorageOption::Persistent(dir.into_os_string().into_string().unwrap()),
            generate_encryption_key(),
        )
        .await?,
    )
    .build()
    .await
    .map_err(Into::into)
}

fn generate_wallet() -> types::EthereumWallet {
    types::EthereumWallet::new()
}

fn generate_encryption_key() -> [u8; 32] {
    let mut key = [0u8; 32];
    xmtp_cryptography::utils::rng().fill_bytes(&mut key[..]);
    key
}
