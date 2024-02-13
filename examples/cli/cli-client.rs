/*
XLI is a Commandline client using XMTPv3.
*/

extern crate ethers;
extern crate log;
extern crate xmtp_mls;

use std::{fs, path::PathBuf, time::Duration};

use anyhow::{anyhow, Context as _, Error};
use clap::{Parser, Subcommand};
use log::{error, info};
use thiserror::Error;
use xmtp_api_grpc::grpc_api_helper::Client as ApiClient;
use xmtp_cryptography::{
    signature::{RecoverableSignature, SignatureError},
    utils::{rng, seeded_rng, LocalWallet},
};
use xmtp_mls::{
    builder::{ClientBuilderError, IdentityStrategy, LegacyIdentity},
    client::ClientError,
    groups::MlsGroup,
    storage::{EncryptedMessageStore, EncryptionKey, StorageError, StorageOption},
    utils::time::now_ns,
    InboxOwner, Network,
};
use xmtp_proto::api_client::{XmtpApiClient, XmtpMlsClient};
use xps_client::XmtpXpsClient;
type Client = xmtp_mls::client::Client<XmtpXpsClient<ApiClient>>;
type XpsClient = XmtpXpsClient<ApiClient>;
type ClientBuilder = xmtp_mls::builder::ClientBuilder<XmtpXpsClient<ApiClient>>;

fn wallet_path() -> PathBuf {
    let mut cargo_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    cargo_path.push("wallets");
    println!("Full path: {:?}", cargo_path);
    cargo_path
}

/// A fictional versioning CLI
#[derive(Debug, Parser)] // requires `derive` feature
#[command(name = "xli")]
#[command(about = "A lightweight XMTP console client", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    /// Sets a custom config file
    #[arg(long, value_name = "FILE", global = true)]
    db: Option<PathBuf>,
    #[clap(long, default_value_t = false)]
    local: bool,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Register Account on XMTP Network
    Register {
        #[clap(long = "seed", default_value_t = 0)]
        wallet_seed: u64,
    },
    CreateGroup {},
    // List conversations on the registered wallet
    ListGroups {},
    /// Send Message
    Send {
        #[arg(value_name = "Group ID")]
        group_id: String,
        #[arg(value_name = "Message")]
        msg: String,
    },
    ListGroupMessages {
        #[arg(value_name = "Group ID")]
        group_id: String,
    },
    AddGroupMember {
        #[arg(value_name = "Group ID")]
        group_id: String,
        #[arg(value_name = "Wallet Address")]
        account_address: String,
    },
    RemoveGroupMember {
        #[arg(value_name = "Group ID")]
        group_id: String,
        #[arg(value_name = "Wallet Address")]
        account_address: String,
    },
    /// Information about the account that owns the DB
    Info {},
    Clear {},
}

#[derive(Debug, Error)]
enum CliError {
    #[error("signature failed to generate")]
    Signature(#[from] SignatureError),
    #[error("client error")]
    ClientError(#[from] ClientError),
    #[error("clientbuilder error")]
    ClientBuilder(#[from] ClientBuilderError),
    #[error("storage error")]
    StorageError(#[from] StorageError),
    #[error("generic:{0}")]
    Generic(String),
}

impl From<String> for CliError {
    fn from(value: String) -> Self {
        Self::Generic(value)
    }
}

impl From<&str> for CliError {
    fn from(value: &str) -> Self {
        Self::Generic(value.to_string())
    }
}
/// This is an abstraction which allows the CLI to choose between different wallet types.
#[derive(Debug, Clone)]
enum Wallet {
    LocalWallet(LocalWallet),
}

impl Wallet {
    fn inner(&self) -> &LocalWallet {
        match self {
            Wallet::LocalWallet(w) => w,
        }
    }
}

impl InboxOwner for Wallet {
    fn get_address(&self) -> String {
        match self {
            Wallet::LocalWallet(w) => w.get_address(),
        }
    }

    fn sign(&self, text: &str) -> Result<RecoverableSignature, SignatureError> {
        match self {
            Wallet::LocalWallet(w) => w.sign(text),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    env_logger::init();
    info!("Starting CLI Client....");

    let cli = Cli::parse();
    let wallet_name = format!(
        "{}.wallet",
        cli.db.clone().unwrap().as_path().to_str().unwrap()
    );

    let wallet_path = wallet_path();
    let mut wallet: Option<Wallet> = LocalWallet::decrypt_keystore(wallet_path, "")
        .ok()
        .map(Wallet::LocalWallet);

    if let Commands::Register { wallet_seed } = &cli.command {
        info!("Register");
        let inner_wallet =
            init_wallet(wallet_seed, wallet_name).context("Wallet could not be generated")?;

        register(&cli, &inner_wallet)
            .await
            .context("Registration failed")?;
        wallet.replace(inner_wallet);
        return Ok(());
    }

    let wallet = wallet.ok_or(anyhow!("Wallet does not exist. Please register first"))?;

    match &cli.command {
        #[allow(unused_variables)]
        Commands::Register { wallet_seed } => {
            unreachable!()
        }
        Commands::Info {} => {
            info!("Info");
            let client = create_client(&cli, IdentityStrategy::CachedOnly, wallet.clone())
                .await
                .unwrap();
            info!("Address is: {}", client.account_address());
            info!(
                "Installation_id: {}",
                hex::encode(client.installation_public_key())
            );
        }
        Commands::ListGroups {} => {
            info!("List Groups");
            let client = create_client(&cli, IdentityStrategy::CachedOnly, wallet.clone())
                .await
                .unwrap();

            client
                .sync_welcomes()
                .await
                .expect("failed to sync welcomes");

            // recv(&client).await.unwrap();
            let group_list = client
                .find_groups(None, None, None, None)
                .expect("failed to list groups");

            for group in group_list.iter() {
                group.sync().await.expect("error syncing group");
                info!(
                    "\n====== Group {} ======\n====================== Members ======================\n{}",
                    hex::encode(group.group_id.clone()),
                    group
                        .members()
                        .unwrap()
                        .into_iter()
                        .map(|m| m.account_address)
                        .collect::<Vec<String>>()
                        .join("\n"),
                );
            }
        }
        Commands::Send { group_id, msg } => {
            info!("Send");
            let client = create_client(&cli, IdentityStrategy::CachedOnly, wallet.clone())
                .await
                .unwrap();
            info!("Address is: {}", client.account_address());
            let group = get_group(&client, hex::decode(group_id).expect("group id decode"))
                .await
                .expect("failed to get group");
            send(group, msg.clone()).await.unwrap();
        }
        Commands::ListGroupMessages { group_id } => {
            info!("Recv");
            let client = create_client(&cli, IdentityStrategy::CachedOnly, wallet.clone())
                .await
                .unwrap();

            let group = get_group(&client, hex::decode(group_id).expect("group id decode"))
                .await
                .expect("failed to get group");

            let messages =
                format_messages(&group, client.account_address()).expect("failed to get messages");
            info!(
                "====== Group {} ======\n{}",
                hex::encode(group.group_id),
                messages
            )
        }
        Commands::AddGroupMember {
            group_id,
            account_address,
        } => {
            let client = create_client(&cli, IdentityStrategy::CachedOnly, wallet.clone())
                .await
                .unwrap();

            let group = get_group(&client, hex::decode(group_id).expect("group id decode"))
                .await
                .expect("failed to get group");

            group
                .add_members(vec![account_address.clone()])
                .await
                .expect("failed to add member");

            info!(
                "Successfully added {} to group {}",
                account_address, group_id
            );
        }
        Commands::RemoveGroupMember {
            group_id,
            account_address,
        } => {
            let client = create_client(&cli, IdentityStrategy::CachedOnly, wallet.clone())
                .await
                .unwrap();

            let group = get_group(&client, hex::decode(group_id).expect("group id decode"))
                .await
                .expect("failed to get group");

            group
                .remove_members(vec![account_address.clone()])
                .await
                .expect("failed to add member");

            info!(
                "Successfully removed {} from group {}",
                account_address, group_id
            );
        }
        Commands::CreateGroup {} => {
            let client = create_client(&cli, IdentityStrategy::CachedOnly, wallet.clone())
                .await
                .unwrap();

            let group = client.create_group().expect("failed to create group");
            info!("Created group {}", hex::encode(group.group_id))
        }

        Commands::Clear {} => {
            fs::remove_file(cli.db.unwrap()).unwrap();
        }
    }
    Ok(())
}

async fn create_client(
    cli: &Cli,
    account: IdentityStrategy,
    owner: Wallet,
) -> Result<Client, CliError> {
    let msg_store = get_encrypted_store(&cli.db).unwrap();
    let mut builder = ClientBuilder::new(account).store(msg_store);

    if cli.local {
        info!("Using local network");

        let api_client = ApiClient::create("http://localhost:5556".into(), false)
            .await
            .unwrap();

        builder = builder
            .network(Network::Local("http://localhost:5556"))
            .api_client(
                XmtpXpsClient::new(
                    api_client,
                    owner.inner().clone(),
                    "ws://127.0.0.1:9944",
                    "ws://127.0.0.1:8545",
                )
                .await
                .unwrap(),
            );
    } else {
        info!("Using dev network");

        let api_client = ApiClient::create("https://grpc.dev.xmtp.network:443".into(), true)
            .await
            .unwrap();
        builder = builder.network(Network::Dev).api_client(
            XmtpXpsClient::new(
                api_client,
                owner.inner().clone(),
                "ws://127.0.0.1:9944",
                "wss://ethereum-sepolia.publicnode.com",
            )
            .await
            .unwrap(),
        );
    }

    builder.build().await.map_err(CliError::ClientBuilder)
}

async fn register(cli: &Cli, w: &Wallet) -> Result<(), CliError> {
    let client = create_client(
        cli,
        IdentityStrategy::CreateIfNotFound(w.get_address(), LegacyIdentity::None),
        w.clone(),
    )
    .await?;

    info!("Address is: {}", client.account_address());
    let signature: Option<Vec<u8>> = client.text_to_sign().map(|t| w.sign(&t).unwrap().into());

    if let Err(e) = client.register_identity(signature).await {
        error!("Initialization Failed: {}", e.to_string());
        panic!("Could not init");
    };

    Ok(())
}

// initializes a wallet and stores it in {db_name}.wallet.json
fn init_wallet<S: AsRef<str>>(wallet_seed: &u64, name: S) -> Result<Wallet, Error> {
    info!("Creating wallet {}", name.as_ref());
    let wallet_path = wallet_path();

    let w = if wallet_seed == &0 {
        LocalWallet::new_keystore(wallet_path, &mut rng(), "", Some(name.as_ref()))?
    } else {
        LocalWallet::new_keystore(
            wallet_path,
            &mut seeded_rng(*wallet_seed),
            "",
            Some(name.as_ref()),
        )?
    };
    Ok(Wallet::LocalWallet(w.0))
}

async fn get_group(client: &Client, group_id: Vec<u8>) -> Result<MlsGroup<XpsClient>, CliError> {
    client.sync_welcomes().await?;
    let group = client.group(group_id)?;
    group
        .sync()
        .await
        .map_err(|_| CliError::Generic("failed to sync group".to_string()))?;

    Ok(group)
}

async fn send(group: MlsGroup<'_, XpsClient>, msg: String) -> Result<(), CliError> {
    group
        .send_message(msg.into_bytes().as_slice())
        .await
        .unwrap();
    info!(
        "Message successfully sent to group {}",
        hex::encode(group.group_id)
    );

    Ok(())
}

fn format_messages<A: XmtpApiClient + XmtpMlsClient>(
    convo: &MlsGroup<'_, A>,
    my_account_address: String,
) -> Result<String, CliError> {
    let mut output: Vec<String> = vec![];

    for msg in convo.find_messages(None, None, None, None).unwrap() {
        let contents = msg.decrypted_message_bytes;
        let sender = if msg.sender_account_address == my_account_address {
            "Me".to_string()
        } else {
            msg.sender_account_address
        };

        let msg_line = format!(
            "[{:>15} ] {}:   {}",
            pretty_delta(now_ns() as u64, msg.sent_at_ns as u64),
            sender,
            String::from_utf8(contents).unwrap()
        );
        output.push(msg_line);
    }
    output.reverse();

    Ok(output.join("\n"))
}

fn static_enc_key() -> EncryptionKey {
    [2u8; 32]
}

fn get_encrypted_store(db: &Option<PathBuf>) -> Result<EncryptedMessageStore, CliError> {
    let store = match db {
        Some(path) => {
            let s = path.as_path().to_string_lossy().to_string();
            info!("Using persistent storage: {} ", s);
            EncryptedMessageStore::new_unencrypted(StorageOption::Persistent(s))
        }

        None => {
            info!("Using ephemeral store");
            EncryptedMessageStore::new(StorageOption::Ephemeral, static_enc_key())
        }
    };

    store.map_err(|e| e.into())
}

fn pretty_delta(now: u64, then: u64) -> String {
    let f = timeago::Formatter::new();
    let diff = if now > then { now - then } else { then - now };
    f.convert(Duration::from_nanos(diff))
}
