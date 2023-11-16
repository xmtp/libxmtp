/*
XLI is a Commandline client using XMTPv3.
*/

extern crate ethers;
extern crate log;
extern crate xmtp_mls;

use std::{fs, path::PathBuf, time::Duration};

use clap::{Parser, Subcommand};
use log::{error, info};
use thiserror::Error;
use xmtp_api_grpc::grpc_api_helper::Client as ApiClient;
use xmtp_cryptography::{
    signature::{RecoverableSignature, SignatureError},
    utils::{rng, seeded_rng, LocalWallet},
};
use xmtp_mls::{
    builder::{ClientBuilderError, IdentityStrategy},
    client::ClientError,
    groups::MlsGroup,
    storage::{EncryptedMessageStore, EncryptionKey, StorageError, StorageOption},
    utils::time::now_ns,
    InboxOwner, Network,
};
use xmtp_proto::api_client::{XmtpApiClient, XmtpMlsClient};
type Client = xmtp_mls::client::Client<ApiClient>;
type ClientBuilder = xmtp_mls::builder::ClientBuilder<ApiClient, Wallet>;

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
    #[clap(long, default_value_t = true)]
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
        wallet_address: String,
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
enum Wallet {
    LocalWallet(LocalWallet),
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
async fn main() {
    env_logger::init();
    info!("Starting CLI Client....");

    let cli = Cli::parse();

    if let Commands::Register { wallet_seed } = &cli.command {
        info!("Register");
        if let Err(e) = register(&cli, wallet_seed).await {
            error!("Registration failed: {:?}", e)
        }
        return;
    }

    match &cli.command {
        #[allow(unused_variables)]
        Commands::Register { wallet_seed } => {
            unreachable!()
        }
        Commands::Info {} => {
            info!("Info");
            let client = create_client(&cli, IdentityStrategy::CachedOnly)
                .await
                .unwrap();
            info!("Address is: {}", client.account_address());
            info!(
                "Installation_id: {}",
                hex::encode(client.installation_public_key())
            );
        }
        Commands::ListGroups {} => {
            info!("List Conversations");
            let client = create_client(&cli, IdentityStrategy::CachedOnly)
                .await
                .unwrap();

            client
                .sync_welcomes()
                .await
                .expect("failed to sync welcome");

            // recv(&client).await.unwrap();
            let convo_list = client
                .find_groups(None, None, None, None)
                .expect("failed to list groups");

            for (index, convo) in convo_list.iter().enumerate() {
                info!(
                    "====== [{}] Group {} ======",
                    index,
                    hex::encode(convo.group_id.clone()),
                );
            }
        }
        Commands::Send { group_id, msg } => {
            info!("Send");
            let client = create_client(&cli, IdentityStrategy::CachedOnly)
                .await
                .unwrap();
            info!("Address is: {}", client.account_address());
            send(
                client,
                hex::decode(group_id).expect("group id decode"),
                msg.clone(),
            )
            .await
            .unwrap();
        }
        Commands::ListGroupMessages { group_id } => {
            info!("Recv");
            let client = create_client(&cli, IdentityStrategy::CachedOnly)
                .await
                .unwrap();
            let group = client
                .group(hex::decode(group_id).unwrap())
                .expect("failed to find group");
            group
                .sync(&mut client.store.conn().unwrap())
                .await
                .expect("failed to sync");
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
            wallet_address,
        } => {
            let client = create_client(&cli, IdentityStrategy::CachedOnly)
                .await
                .unwrap();
            let group = client
                .group(hex::decode(group_id).unwrap())
                .expect("failed to find group");

            group
                .add_members_by_wallet_address(vec![wallet_address.clone()])
                .await
                .expect("failed to add member");

            info!(
                "Successfully added {} to group {}",
                wallet_address, group_id
            );
        }
        Commands::CreateGroup {} => {
            let client = create_client(&cli, IdentityStrategy::CachedOnly)
                .await
                .unwrap();

            let group = client.create_group().expect("failed to create group");
            info!("Created group {}", hex::encode(group.group_id))
        }

        Commands::Clear {} => {
            fs::remove_file(cli.db.unwrap()).unwrap();
        }
    }
}

async fn create_client(cli: &Cli, account: IdentityStrategy<Wallet>) -> Result<Client, CliError> {
    let msg_store = get_encrypted_store(&cli.db).unwrap();
    let mut builder = ClientBuilder::new(account).store(msg_store);

    if cli.local {
        builder = builder
            .network(Network::Local("http://localhost:5556"))
            .api_client(
                ApiClient::create("http://localhost:5556".into(), false)
                    .await
                    .unwrap(),
            );
    } else {
        builder = builder.network(Network::Dev).api_client(
            ApiClient::create("https://dev.xmtp.network:5556".into(), true)
                .await
                .unwrap(),
        );
    }

    builder.build().map_err(CliError::ClientBuilder)
}

async fn register(cli: &Cli, wallet_seed: &u64) -> Result<(), CliError> {
    let w = if wallet_seed == &0 {
        Wallet::LocalWallet(LocalWallet::new(&mut rng()))
    } else {
        Wallet::LocalWallet(LocalWallet::new(&mut seeded_rng(*wallet_seed)))
    };

    let client = create_client(cli, IdentityStrategy::CreateIfNotFound(w)).await?;
    info!("Address is: {}", client.account_address());

    if let Err(e) = client.register_identity().await {
        error!("Initialization Failed: {}", e.to_string());
        panic!("Could not init");
    };

    Ok(())
}

async fn send(client: Client, group_id: Vec<u8>, msg: String) -> Result<(), CliError> {
    let group = client.group(group_id).unwrap();
    group
        .send_message(msg.into_bytes().as_slice())
        .await
        .unwrap();
    info!("Message successfully sent");

    Ok(())
}

fn format_messages<'c, A: XmtpApiClient + XmtpMlsClient>(
    convo: &MlsGroup<'c, A>,
    my_wallet_address: String,
) -> Result<String, CliError> {
    let mut output: Vec<String> = vec![];

    for msg in convo.find_messages(None, None, None, None).unwrap() {
        let contents = msg.decrypted_message_bytes;
        let sender = if msg.sender_wallet_address == my_wallet_address {
            "Me".to_string()
        } else {
            msg.sender_wallet_address
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
    f.convert(Duration::from_nanos(now - then))
}
