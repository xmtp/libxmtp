/*
XLI is a Commandline client using XMTPv3.
*/

mod json_logger;

extern crate ethers;
extern crate log;
extern crate xmtp_mls;

use std::{fs, path::PathBuf, time::Duration};

use clap::{Parser, Subcommand, ValueEnum};
use kv_log_macro::{error, info};
use prost::Message;

use serde::Serialize;
use thiserror::Error;
use xmtp_api_grpc::grpc_api_helper::Client as ApiClient;
use xmtp_cryptography::{
    signature::{RecoverableSignature, SignatureError},
    utils::{rng, seeded_rng, LocalWallet},
};
use xmtp_mls::{
    builder::{ClientBuilderError, IdentityStrategy, LegacyIdentity},
    client::ClientError,
    codecs::{text::TextCodec, ContentCodec},
    groups::MlsGroup,
    storage::{
        group_message::StoredGroupMessage, EncryptedMessageStore, EncryptionKey, StorageError,
        StorageOption,
    },
    utils::time::now_ns,
    InboxOwner, Network,
};
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

use crate::json_logger::make_value;
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
        #[clap(long = "seed", default_value_t = 0)]
        wallet_seed: u64,
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
        env_logger::init();
    }
    info!("Starting CLI Client....");

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
            let installation_id = hex::encode(client.installation_public_key());
            info!("wallet info", { command_output: true, account_address: client.account_address(), installation_id: installation_id });
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
                group.sync().await.expect("error syncing group");
                let group_id = hex::encode(group.group_id.clone());
                let members = group
                    .members()
                    .unwrap()
                    .into_iter()
                    .map(|m| m.account_address)
                    .collect::<Vec<String>>();
                info!(
                    "group members",
                    {
                        command_output: true,
                        members: make_value(&members),
                        group_id: group_id,
                    }
                );
            }
        }
        Commands::Send { group_id, msg } => {
            info!("Sending message to group", { group_id: group_id, message: msg });
            let client = create_client(&cli, IdentityStrategy::CachedOnly)
                .await
                .unwrap();
            info!("Address is: {}", client.account_address());
            let group = get_group(&client, hex::decode(group_id).expect("group id decode"))
                .await
                .expect("failed to get group");
            send(group, msg.clone()).await.unwrap();
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

            let messages = group.find_messages(None, None, None, None).unwrap();
            if cli.json {
                let json_serializable_messages = messages
                    .iter()
                    .map(SerializableMessage::from_stored_message)
                    .collect::<Vec<_>>();
                info!("messages", { command_output: true, messages: make_value(&json_serializable_messages), group_id: group_id });
            } else {
                let messages = format_messages(messages, client.account_address())
                    .expect("failed to get messages");
                info!(
                    "====== Group {} ======\n{}",
                    hex::encode(group.group_id),
                    messages
                )
            }
        }
        Commands::AddGroupMember {
            group_id,
            account_address,
        } => {
            let client = create_client(&cli, IdentityStrategy::CachedOnly)
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
                account_address, group_id, { command_output: true, group_id: group_id}
            );
        }
        Commands::RemoveGroupMember {
            group_id,
            account_address,
        } => {
            let client = create_client(&cli, IdentityStrategy::CachedOnly)
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
                account_address, group_id, { command_output: true }
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

async fn register(cli: &Cli, wallet_seed: &u64) -> Result<(), CliError> {
    let w = if wallet_seed == &0 {
        Wallet::LocalWallet(LocalWallet::new(&mut rng()))
    } else {
        Wallet::LocalWallet(LocalWallet::new(&mut seeded_rng(*wallet_seed)))
    };

    let client = create_client(
        cli,
        IdentityStrategy::CreateIfNotFound(w.get_address(), LegacyIdentity::None),
    )
    .await?;
    let signature: Option<Vec<u8>> = client.text_to_sign().map(|t| w.sign(&t).unwrap().into());

    if let Err(e) = client.register_identity(signature).await {
        error!("Initialization Failed: {}", e.to_string());
        panic!("Could not init");
    };
    info!("Registered identity", {account_address: client.account_address(), installation_id: hex::encode(client.installation_public_key()), command_output: true});

    Ok(())
}

async fn get_group(client: &Client, group_id: Vec<u8>) -> Result<MlsGroup<ApiClient>, CliError> {
    client.sync_welcomes().await?;
    let group = client.group(group_id)?;
    group
        .sync()
        .await
        .map_err(|_| CliError::Generic("failed to sync group".to_string()))?;

    Ok(group)
}

async fn send(group: MlsGroup<'_, ApiClient>, msg: String) -> Result<(), CliError> {
    let mut buf = Vec::new();
    TextCodec::encode(msg.clone())
        .unwrap()
        .encode(&mut buf)
        .unwrap();
    group.send_message(buf.as_slice()).await.unwrap();
    Ok(())
}

#[derive(Serialize, Debug, Clone)]
struct SerializableMessage {
    sender_account_address: String,
    sent_at_ns: u64,
    message_text: Option<String>,
    // content_type: String
}

impl SerializableMessage {
    fn from_stored_message(msg: &StoredGroupMessage) -> Self {
        let maybe_text = maybe_get_text(msg);
        Self {
            sender_account_address: msg.sender_account_address.clone(),
            sent_at_ns: msg.sent_at_ns as u64,
            message_text: maybe_text,
        }
    }
}

fn maybe_get_text(msg: &StoredGroupMessage) -> Option<String> {
    let contents = msg.decrypted_message_bytes.clone();
    let Ok(encoded_content) = EncodedContent::decode(contents.as_slice()) else {
        return None;
    };
    let Ok(decoded) = TextCodec::decode(encoded_content) else {
        log::warn!("Skipping over unrecognized codec");
        return None;
    };
    Some(decoded)
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
