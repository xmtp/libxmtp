/*
XLI is a Commandline client using XMTPv3.
*/

mod json_logger;
mod serializable;

extern crate ethers;
extern crate log;
extern crate xmtp_mls;

use std::{fs, path::PathBuf, time::Duration};

use clap::{Parser, Subcommand, ValueEnum};
use ethers::signers::{coins_bip39::English, LocalWallet, MnemonicBuilder};
use kv_log_macro::{error, info};
use prost::Message;

use crate::{
    json_logger::make_value,
    serializable::{SerializableGroup, SerializableMessage},
};
use serializable::maybe_get_text;
use thiserror::Error;
use xmtp_api_grpc::grpc_api_helper::Client as ApiClient;
use xmtp_cryptography::{
    signature::{RecoverableSignature, SignatureError},
    utils::rng,
};
use xmtp_mls::{
    builder::ClientBuilderError,
    client::ClientError,
    codecs::{text::TextCodec, ContentCodec},
    groups::MlsGroup,
    identity::IdentityStrategy,
    storage::{
        group_message::StoredGroupMessage, EncryptedMessageStore, EncryptionKey, StorageError,
        StorageOption,
    },
    utils::time::now_ns,
    InboxOwner, Network,
};
type Client = xmtp_mls::client::Client<ApiClient>;
type ClientBuilder = xmtp_mls::builder::ClientBuilder<ApiClient>;

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
    #[clap(long, default_value_t = false)]
    json: bool,
}

#[derive(ValueEnum, Debug, Copy, Clone)]
enum Permissions {
    EveryoneIsAdmin,
    GroupCreatorIsAdmin,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Register Account on XMTP Network
    Register {
        #[clap(long)]
        seed_phrase: Option<String>,
    },
    CreateGroup {
        #[clap(value_enum, default_value_t = Permissions::EveryoneIsAdmin)]
        permissions: Permissions,
    },
    // List conversations on the registered wallet
    ListGroups {},
    /// Send Message
    Send {
        #[arg(value_name = "Group ID")]
        group_id: String,
        #[arg(value_name = "Message")]
        msg: String,
    },
    GroupInfo {
        #[arg(value_name = "Group ID")]
        group_id: String,
    },
    ListGroupMessages {
        #[arg(value_name = "Group ID")]
        group_id: String,
    },
    AddGroupMembers {
        #[arg(value_name = "Group ID")]
        group_id: String,
        #[clap(short, long, value_parser, num_args = 1.., value_delimiter = ' ')]
        account_addresses: Vec<String>,
    },
    RemoveGroupMembers {
        #[arg(value_name = "Group ID")]
        group_id: String,
        #[clap(short, long, value_parser, num_args = 1.., value_delimiter = ' ')]
        account_addresses: Vec<String>,
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
    let cli = Cli::parse();
    if cli.json {
        crate::json_logger::start(log::LevelFilter::Info);
    } else {
        femme::with_level(femme::LevelFilter::Info);
    }
    info!("Starting CLI Client....");

    if let Commands::Register { seed_phrase } = &cli.command {
        info!("Register");
        if let Err(e) = register(&cli, seed_phrase.clone()).await {
            error!("Registration failed: {:?}", e)
        }
        return;
    }

    match &cli.command {
        #[allow(unused_variables)]
        Commands::Register { seed_phrase } => {
            unreachable!()
        }
        Commands::Info {} => {
            info!("Info");
            let client = create_client(&cli, IdentityStrategy::CachedOnly)
                .await
                .unwrap();
            let installation_id = hex::encode(client.installation_public_key());
            info!("identity info", { command_output: true, account_address: client.inbox_id(), installation_id: installation_id });
        }
        Commands::ListGroups {} => {
            info!("List Groups");
            let client = create_client(&cli, IdentityStrategy::CachedOnly)
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
                group.sync(&client).await.expect("error syncing group");
            }
            let serializable_group_list = group_list
                .iter()
                .map(Into::into)
                .collect::<Vec<SerializableGroup>>();

            info!(
                "group members",
                {
                    command_output: true,
                    groups: make_value(&serializable_group_list),
                }
            );
        }
        Commands::Send { group_id, msg } => {
            info!("Sending message to group", { group_id: group_id, message: msg });
            let client = create_client(&cli, IdentityStrategy::CachedOnly)
                .await
                .unwrap();
            info!("Inbox ID is: {}", client.inbox_id());
            let group = get_group(&client, hex::decode(group_id).expect("group id decode"))
                .await
                .expect("failed to get group");
            send(group, msg.clone(), &client).await.unwrap();
            info!("sent message", { command_output: true, group_id: group_id, message: msg });
        }
        Commands::ListGroupMessages { group_id } => {
            info!("Recv");
            let client = create_client(&cli, IdentityStrategy::CachedOnly)
                .await
                .unwrap();

            let group = get_group(&client, hex::decode(group_id).expect("group id decode"))
                .await
                .expect("failed to get group");

            let messages = group.find_messages(None, None, None, None, None).unwrap();
            if cli.json {
                let json_serializable_messages = messages
                    .iter()
                    .map(SerializableMessage::from_stored_message)
                    .collect::<Vec<_>>();
                info!("messages", { command_output: true, messages: make_value(&json_serializable_messages), group_id: group_id });
            } else {
                let messages =
                    format_messages(messages, client.inbox_id()).expect("failed to get messages");
                info!(
                    "====== Group {} ======\n{}",
                    hex::encode(group.group_id),
                    messages
                )
            }
        }
        Commands::AddGroupMembers {
            group_id,
            account_addresses,
        } => {
            let client = create_client(&cli, IdentityStrategy::CachedOnly)
                .await
                .unwrap();

            let group = get_group(&client, hex::decode(group_id).expect("group id decode"))
                .await
                .expect("failed to get group");

            group
                .add_members(account_addresses.clone(), &client)
                .await
                .expect("failed to add member");

            info!(
                "Successfully added {} to group {}",
                account_addresses.join(", "), group_id, { command_output: true, group_id: group_id}
            );
        }
        Commands::RemoveGroupMembers {
            group_id,
            account_addresses,
        } => {
            let client = create_client(&cli, IdentityStrategy::CachedOnly)
                .await
                .unwrap();

            let group = get_group(&client, hex::decode(group_id).expect("group id decode"))
                .await
                .expect("failed to get group");

            group
                .remove_members(account_addresses.clone(), &client)
                .await
                .expect("failed to add member");

            info!(
                "Successfully removed {} from group {}",
                account_addresses.join(", "), group_id, { command_output: true }
            );
        }
        Commands::CreateGroup { permissions } => {
            let group_permissions = match permissions {
                Permissions::EveryoneIsAdmin => {
                    xmtp_mls::groups::PreconfiguredPolicies::EveryoneIsAdmin
                }
                Permissions::GroupCreatorIsAdmin => {
                    xmtp_mls::groups::PreconfiguredPolicies::GroupCreatorIsAdmin
                }
            };
            let client = create_client(&cli, IdentityStrategy::CachedOnly)
                .await
                .unwrap();

            let group = client
                .create_group(Some(group_permissions))
                .expect("failed to create group");
            let group_id = hex::encode(group.group_id);
            info!("Created group {}", group_id, { command_output: true, group_id: group_id})
        }
        Commands::GroupInfo { group_id } => {
            let client = create_client(&cli, IdentityStrategy::CachedOnly)
                .await
                .unwrap();
            let group = &client
                .group(hex::decode(group_id).expect("bad group id"))
                .expect("group not found");
            group.sync(&client).await.unwrap();
            let serializable: SerializableGroup = group.into();
            info!("Group {}", group_id, { command_output: true, group_id: group_id, group_info: make_value(&serializable) })
        }
        Commands::Clear {} => {
            fs::remove_file(cli.db.unwrap()).unwrap();
        }
    }
}

async fn create_client(cli: &Cli, account: IdentityStrategy) -> Result<Client, CliError> {
    let msg_store = get_encrypted_store(&cli.db).unwrap();
    let mut builder = ClientBuilder::new(account).store(msg_store);

    if cli.local {
        info!("Using local network");
        builder = builder
            .network(Network::Local("http://localhost:5556"))
            .api_client(
                ApiClient::create("http://localhost:5556".into(), false)
                    .await
                    .unwrap(),
            );
    } else {
        info!("Using dev network");
        builder = builder.network(Network::Dev).api_client(
            ApiClient::create("https://grpc.dev.xmtp.network:443".into(), true)
                .await
                .unwrap(),
        );
    }

    builder.build().await.map_err(CliError::ClientBuilder)
}

async fn register(cli: &Cli, maybe_seed_phrase: Option<String>) -> Result<(), CliError> {
    let w: Wallet = if let Some(seed_phrase) = maybe_seed_phrase {
        Wallet::LocalWallet(
            MnemonicBuilder::<English>::default()
                .phrase(seed_phrase.as_str())
                .build()
                .unwrap(),
        )
    } else {
        Wallet::LocalWallet(LocalWallet::new(&mut rng()))
    };

    let client = create_client(
        cli,
        IdentityStrategy::CreateIfNotFound(w.get_address(), None),
    )
    .await?;
    if let Err(e) = client
        .register_identity(client.identity().get_signature_request().unwrap()) // TODO: remove `.unwrap()`. What should [Identity::request_signature()] return?
        .await
    {
        error!("Initialization Failed: {}", e.to_string());
        panic!("Could not init");
    };
    info!("Registered identity", {account_address: client.inbox_id(), installation_id: hex::encode(client.installation_public_key()), command_output: true});

    Ok(())
}

async fn get_group(client: &Client, group_id: Vec<u8>) -> Result<MlsGroup, CliError> {
    client.sync_welcomes().await?;
    let group = client.group(group_id)?;
    group
        .sync(client)
        .await
        .map_err(|_| CliError::Generic("failed to sync group".to_string()))?;

    Ok(group)
}

async fn send(group: MlsGroup, msg: String, client: &Client) -> Result<(), CliError> {
    let mut buf = Vec::new();
    TextCodec::encode(msg.clone())
        .unwrap()
        .encode(&mut buf)
        .unwrap();
    group.send_message(buf.as_slice(), client).await.unwrap();
    Ok(())
}

fn format_messages(
    messages: Vec<StoredGroupMessage>,
    my_account_address: String,
) -> Result<String, CliError> {
    let mut output: Vec<String> = vec![];

    for msg in messages {
        let text = maybe_get_text(&msg);
        if text.is_none() {
            continue;
        }
        let sender = if msg.sender_account_address == my_account_address {
            "Me".to_string()
        } else {
            msg.sender_account_address
        };

        let msg_line = format!(
            "[{:>15} ] {}:   {}",
            pretty_delta(now_ns() as u64, msg.sent_at_ns as u64),
            sender,
            text.expect("already checked")
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
