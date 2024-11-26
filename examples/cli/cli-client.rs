#![recursion_limit = "256"]
/*
XLI is a Commandline client using XMTPv3.
*/

mod debug;
mod pretty;
mod serializable;

use std::iter::Iterator;
use std::{fs, path::PathBuf, time::Duration};

use crate::serializable::{SerializableGroup, SerializableMessage};
use clap::{Parser, Subcommand, ValueEnum};
use color_eyre::eyre::eyre;
use debug::DebugCommands;
use ethers::signers::{coins_bip39::English, LocalWallet, MnemonicBuilder};
use futures::future::join_all;
use owo_colors::OwoColorize;
use prost::Message;
use serializable::maybe_get_text;
use thiserror::Error;
use tracing::Dispatch;
use tracing_subscriber::field::MakeExt;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::{
    fmt::{format, time},
    layer::SubscriberExt,
    prelude::*,
    Registry,
};
use valuable::Valuable;
use xmtp_api_grpc::grpc_api_helper::Client as ClientV3;
use xmtp_api_grpc::replication_client::ClientV4;
use xmtp_cryptography::{
    signature::{RecoverableSignature, SignatureError},
    utils::rng,
};
use xmtp_id::associations::unverified::{UnverifiedRecoverableEcdsaSignature, UnverifiedSignature};
use xmtp_id::associations::{generate_inbox_id, AssociationError, AssociationState, MemberKind};
use xmtp_mls::groups::device_sync::DeviceSyncContent;
use xmtp_mls::groups::scoped_client::ScopedGroupClient;
use xmtp_mls::groups::GroupError;
use xmtp_mls::storage::group::GroupQueryArgs;
use xmtp_mls::storage::group_message::{GroupMessageKind, MsgQueryArgs};
use xmtp_mls::XmtpApi;
use xmtp_mls::{
    builder::ClientBuilderError,
    client::ClientError,
    codecs::{text::TextCodec, ContentCodec},
    groups::{device_sync::MessageHistoryUrls, GroupMetadataOptions},
    identity::IdentityStrategy,
    storage::{
        group_message::StoredGroupMessage, EncryptedMessageStore, EncryptionKey, StorageError,
        StorageOption,
    },
    utils::time::now_ns,
    InboxOwner,
};
use xmtp_proto::xmtp::mls::message_contents::DeviceSyncKind;

#[macro_use]
extern crate tracing;

type Client = xmtp_mls::client::Client<Box<dyn XmtpApi>>;
type MlsGroup = xmtp_mls::groups::MlsGroup<Client>;

#[derive(clap::ValueEnum, Clone, Default, Debug, serde::Serialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
enum Env {
    #[default]
    Local,
    Dev,
    Production,
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
    #[clap(long, value_enum, default_value_t)]
    env: Env,
    #[clap(long, default_value_t = false)]
    json: bool,
    #[clap(long, default_value_t = false)]
    testnet: bool,
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
    RequestHistorySync {},
    ListHistorySyncMessages {},
    /// Information about the account that owns the DB
    Info {},
    Clear {},
    GetInboxId {
        #[arg(value_name = "Account Address")]
        account_address: String,
    },
    #[command(subcommand)]
    Debug(DebugCommands),
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
    #[error(transparent)]
    Association(#[from] AssociationError),
    #[error(transparent)]
    Group(#[from] GroupError),
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
async fn main() -> color_eyre::eyre::Result<()> {
    color_eyre::install()?;
    let cli = Cli::parse();
    let crate_name = env!("CARGO_PKG_NAME");
    let filter = EnvFilter::builder().parse(format!(
        "{crate_name}=INFO,xmtp_mls=INFO,xmtp_api_grpc=INFO"
    ))?;
    if cli.json {
        let fmt = tracing_subscriber::fmt::layer()
            .json()
            .flatten_event(true)
            .with_level(true)
            .with_timer(time::ChronoLocal::new("%s".into()));

        tracing_subscriber::registry().with(filter).with(fmt).init();
    } else {
        let layer = tracing_subscriber::fmt::layer()
            .without_time()
            .map_event_format(|_| pretty::PrettyTarget)
            .fmt_fields(
                format::debug_fn(|writer, field, value| {
                    if field.name() == "message" {
                        write!(writer, "{:?}", value.white())
                    } else {
                        write!(writer, "{} {:?}", field.bold(), value.white())
                    }
                })
                .delimited("\n\t"),
            );
        let subscriber = Registry::default().with(filter).with(layer);
        let _ = tracing::dispatcher::set_global_default(Dispatch::new(subscriber));
    }
    info!("Starting CLI Client....");

    let grpc: Box<dyn XmtpApi> = match (cli.testnet, &cli.env) {
        (true, Env::Local) => Box::new(
            ClientV4::create(
                "http://localhost:5050".into(),
                "http://localhost:5050".into(),
                false,
            )
            .await?,
        ),
        (true, Env::Dev) => Box::new(
            ClientV4::create(
                "https://grpc.testnet.xmtp.network:443".into(),
                "https://payer.testnet.xmtp.network:443".into(),
                true,
            )
            .await?,
        ),
        (false, Env::Local) => {
            Box::new(ClientV3::create("http://localhost:5556".into(), false).await?)
        }
        (false, Env::Dev) => {
            Box::new(ClientV3::create("https://grpc.dev.xmtp.network:443".into(), true).await?)
        }
        (false, Env::Production) => Box::new(
            ClientV3::create("https://grpc.production.xmtp.network:443".into(), true).await?,
        ),
        (true, Env::Production) => todo!("not supported"),
    };

    if let Commands::Register { seed_phrase } = &cli.command {
        info!("Register");
        if let Err(e) = register(&cli, seed_phrase.clone(), grpc).await {
            error!("Registration failed: {:?}", e)
        }
        return Ok(());
    }

    let client = create_client(&cli, IdentityStrategy::CachedOnly, grpc).await?;

    match &cli.command {
        #[allow(unused_variables)]
        Commands::Register { seed_phrase } => {
            unreachable!()
        }
        Commands::Info {} => {
            info!("Info");
            let (recovery, ids, addrs) = pretty_association_state(&client.inbox_state(true).await?);
            info!(
                command_output = true,
                inbox_id = client.inbox_id(),
                recovery_address = recovery,
                installation_ids = &ids.as_value(),
                addressess = &addrs.as_value(),
                "identity info",
            );
        }
        Commands::ListGroups {} => {
            info!("List Groups");
            let conn = client.store().conn()?;
            client
                .sync_welcomes(&conn)
                .await
                .expect("failed to sync welcomes");

            // recv(&client).await.unwrap();
            let group_list = client
                .find_groups(GroupQueryArgs::default())
                .expect("failed to list groups");
            for group in group_list.iter() {
                group.sync().await.expect("error syncing group");
            }

            let serializable_group_list = group_list
                .iter()
                .map(SerializableGroup::from)
                .collect::<Vec<_>>();
            let serializable_group_list = join_all(serializable_group_list).await;

            info!(
                command_output = true,
                groups = &serializable_group_list.as_value(),
                "group members",
            );
        }
        Commands::Send { group_id, msg } => {
            info!(
                group_id = group_id,
                message = msg,
                "Sending message to group"
            );
            info!("Inbox ID is: {}", client.inbox_id());
            let group = get_group(&client, hex::decode(group_id).expect("group id decode"))
                .await
                .expect("failed to get group");
            send(group, msg.clone()).await?;
            info!(
                command_output = true,
                group_id = group_id,
                message = msg,
                "sent message"
            );
        }
        Commands::ListGroupMessages { group_id } => {
            info!("Recv");

            let group = get_group(&client, hex::decode(group_id).expect("group id decode"))
                .await
                .expect("failed to get group");

            let messages = group.find_messages(&MsgQueryArgs::default())?;
            if cli.json {
                let json_serializable_messages = messages
                    .iter()
                    .map(SerializableMessage::from_stored_message)
                    .collect::<Vec<_>>();
                info!(
                    command_output = true,
                    messages = &json_serializable_messages.as_value(),
                    group_id = group_id,
                    "messages",
                );
            } else {
                let messages = format_messages(messages, client.inbox_id().to_string())
                    .expect("failed to get messages");
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
            let group = get_group(&client, hex::decode(group_id).expect("group id decode"))
                .await
                .expect("failed to get group");

            group
                .add_members(account_addresses)
                .await
                .expect("failed to add member");

            info!(
                command_output = true,
                group_id = group_id,
                "Successfully added {} to group {}",
                account_addresses.join(", "),
                group_id,
            );
        }
        Commands::RemoveGroupMembers {
            group_id,
            account_addresses,
        } => {
            let group = get_group(&client, hex::decode(group_id).expect("group id decode"))
                .await
                .expect("failed to get group");

            group
                .remove_members(account_addresses)
                .await
                .expect("failed to add member");

            info!(
                command_output = true,
                "Successfully removed {} from group {}",
                account_addresses.join(", "),
                group_id
            );
        }
        Commands::CreateGroup { permissions } => {
            let group_permissions = match permissions {
                Permissions::EveryoneIsAdmin => xmtp_mls::groups::PreconfiguredPolicies::AllMembers,
                Permissions::GroupCreatorIsAdmin => {
                    xmtp_mls::groups::PreconfiguredPolicies::AdminsOnly
                }
            };
            let group = client
                .create_group(
                    Some(group_permissions.to_policy_set()),
                    GroupMetadataOptions::default(),
                )
                .expect("failed to create group");
            let group_id = hex::encode(group.group_id);
            info!(
                command_output = true,
                group_id = group_id,
                "Created group {}",
                group_id
            );
        }
        Commands::GroupInfo { group_id } => {
            let group = &client
                .group(hex::decode(group_id).expect("bad group id"))
                .expect("group not found");
            group.sync().await.unwrap();
            let serializable = SerializableGroup::from(group).await;
            info!(
                command_output = true,
                group_id = group_id,
                group_info = &serializable.as_value(),
                "Group {}",
                group_id
            );
        }
        Commands::RequestHistorySync {} => {
            let conn = client.store().conn().unwrap();
            let provider = client.mls_provider().unwrap();
            client.sync_welcomes(&conn).await.unwrap();
            client.start_sync_worker(&provider).await.unwrap();
            client
                .send_sync_request(&provider, DeviceSyncKind::MessageHistory)
                .await
                .unwrap();
            info!("Sent history sync request in sync group.")
        }
        Commands::ListHistorySyncMessages {} => {
            let conn = client.store().conn()?;
            client.sync_welcomes(&conn).await?;
            let group = client.get_sync_group()?;
            let group_id_str = hex::encode(group.group_id.clone());
            group.sync().await?;
            let messages = group
                .find_messages(&MsgQueryArgs::default().kind(GroupMessageKind::Application))?;
            info!(
                group_id = group_id_str,
                messages = messages.len(),
                "Listing history sync messages"
            );
            for message in messages {
                let message_history_content =
                    serde_json::from_slice::<DeviceSyncContent>(&message.decrypted_message_bytes);

                match message_history_content {
                    Ok(DeviceSyncContent::Request(ref request)) => {
                        info!("Request: {:?}", request);
                    }
                    Ok(DeviceSyncContent::Reply(ref reply)) => {
                        info!("Reply: {:?}", reply);
                    }
                    _ => {
                        info!("Unknown message type: {:?}", message);
                    }
                }
            }
        }
        Commands::Clear {} => {
            fs::remove_file(cli.db.ok_or(eyre!("DB Missing"))?)?;
        }
        Commands::Debug(debug_commands) => {
            debug::handle_debug(&client, debug_commands).await.unwrap();
        }
        Commands::GetInboxId { account_address } => {
            let mapping = client
                .api()
                .get_inbox_ids(vec![account_address.clone()])
                .await?;
            let inbox_id = mapping.get(account_address).unwrap();
            info!("Inbox_id {inbox_id}");
        }
    }

    Ok(())
}

async fn create_client<C: XmtpApi + 'static>(
    cli: &Cli,
    account: IdentityStrategy,
    grpc: C,
) -> Result<xmtp_mls::client::Client<C>, CliError> {
    let msg_store = get_encrypted_store(&cli.db).await?;
    let mut builder = xmtp_mls::builder::ClientBuilder::<C>::new(account).store(msg_store);

    builder = builder.api_client(grpc);

    builder = match (cli.testnet, &cli.env) {
        (false, Env::Local) => builder.history_sync_url(MessageHistoryUrls::LOCAL_ADDRESS),
        (false, Env::Dev) => builder.history_sync_url(MessageHistoryUrls::DEV_ADDRESS),
        _ => builder,
    };

    let client = builder.build().await.map_err(CliError::ClientBuilder)?;

    Ok(client)
}

async fn register<C>(
    cli: &Cli,
    maybe_seed_phrase: Option<String>,
    client: C,
) -> Result<(), CliError>
where
    C: XmtpApi + 'static,
{
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

    let nonce = 0;
    let inbox_id = generate_inbox_id(&w.get_address(), &nonce)?;
    let client = create_client(
        cli,
        IdentityStrategy::new(inbox_id, w.get_address(), nonce, None),
        client,
    )
    .await?;
    let mut signature_request = client.identity().signature_request().unwrap();
    let sig_bytes: Vec<u8> = w
        .sign(signature_request.signature_text().as_str())
        .unwrap()
        .into();
    let signature =
        UnverifiedSignature::RecoverableEcdsa(UnverifiedRecoverableEcdsaSignature::new(sig_bytes));
    signature_request
        .add_signature(signature, client.scw_verifier())
        .await
        .unwrap();

    if let Err(e) = client.register_identity(signature_request).await {
        error!("Initialization Failed: {}", e.to_string());
        panic!("Could not init");
    };
    info!(
        account_address = client.inbox_id(),
        installation_id = hex::encode(client.installation_public_key()),
        command_output = true,
        "Registered identity"
    );

    Ok(())
}

async fn get_group(client: &Client, group_id: Vec<u8>) -> Result<MlsGroup, CliError> {
    let conn = client.store().conn().unwrap();
    client.sync_welcomes(&conn).await?;
    let group = client.group(group_id)?;
    group
        .sync()
        .await
        .map_err(|_| CliError::Generic("failed to sync group".to_string()))?;

    Ok(group)
}

async fn send(group: MlsGroup, msg: String) -> Result<(), CliError> {
    let mut buf = Vec::new();
    TextCodec::encode(msg.clone())
        .unwrap()
        .encode(&mut buf)
        .unwrap();
    group.send_message(buf.as_slice()).await?;
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

        let sender = if msg.sender_inbox_id == my_account_address {
            "Me".to_string()
        } else {
            msg.sender_inbox_id
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

async fn get_encrypted_store(db: &Option<PathBuf>) -> Result<EncryptedMessageStore, CliError> {
    let store = match db {
        Some(path) => {
            let s = path.as_path().to_string_lossy().to_string();
            info!("Using persistent storage: {} ", s);
            EncryptedMessageStore::new_unencrypted(StorageOption::Persistent(s)).await
        }

        None => {
            info!("Using ephemeral store");
            EncryptedMessageStore::new(StorageOption::Ephemeral, static_enc_key()).await
        }
    };

    store.map_err(|e| e.into())
}

fn pretty_delta(now: u64, then: u64) -> String {
    let f = timeago::Formatter::new();
    let diff = if now > then { now - then } else { then - now };
    f.convert(Duration::from_nanos(diff))
}

fn pretty_association_state(state: &AssociationState) -> (String, Vec<String>, Vec<String>) {
    let recovery_address = state.recovery_address().clone();
    let installation_ids = state
        .installation_ids()
        .into_iter()
        .map(hex::encode)
        .collect::<Vec<String>>();

    let addresses = state
        .members_by_kind(MemberKind::Address)
        .into_iter()
        .map(|m| m.identifier.to_address().unwrap())
        .collect::<Vec<String>>();

    (recovery_address, installation_ids, addresses)
}
