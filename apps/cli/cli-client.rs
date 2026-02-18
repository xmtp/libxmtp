#![recursion_limit = "256"]
/*
XLI is a Commandline client using XMTPv3.
*/

mod debug;
mod pretty;
mod serializable;

use crate::serializable::{SerializableGroup, SerializableMessage};
use alloy::signers::local::{MnemonicBuilder, PrivateKeySigner, coins_bip39::English};
use clap::{Parser, Subcommand, ValueEnum};
use color_eyre::eyre::eyre;
use debug::DebugCommands;
use futures::future::join_all;
use owo_colors::OwoColorize;
use prost::Message;
use serializable::maybe_get_text;
use std::iter::Iterator;
use std::sync::Arc;
use std::{fs, path::PathBuf, time::Duration};
use thiserror::Error;
use tracing::Dispatch;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::field::MakeExt;
use tracing_subscriber::{
    Registry,
    fmt::{format, time},
    layer::SubscriberExt,
    prelude::*,
};
use valuable::Valuable;
use xmtp_api_d14n::MessageBackendBuilder;
use xmtp_api_d14n::protocol::XmtpQuery;
use xmtp_common::time::now_ns;
use xmtp_configuration::{
    DeviceSyncUrls, GrpcUrlsDev, GrpcUrlsLocal, GrpcUrlsProduction, GrpcUrlsStaging,
};
use xmtp_content_types::{ContentCodec, text::TextCodec};
use xmtp_cryptography::signature::IdentifierValidationError;
use xmtp_cryptography::signature::SignatureError;
use xmtp_db::NativeDb;
use xmtp_db::group::GroupQueryArgs;
use xmtp_db::group_message::MsgQueryArgs;
use xmtp_db::{
    EncryptedMessageStore, EncryptionKey, StorageError, group_message::StoredGroupMessage,
};
use xmtp_id::associations::unverified::UnverifiedSignature;
use xmtp_id::associations::{AssociationError, AssociationState, Identifier, MemberKind};
use xmtp_mls::XmtpApi;
use xmtp_mls::XmtpApiClient;
use xmtp_mls::context::XmtpMlsLocalContext;
use xmtp_mls::context::XmtpSharedContext;
use xmtp_mls::groups::GroupError;
use xmtp_mls::groups::send_message_opts::SendMessageOptsBuilder;
use xmtp_mls::{InboxOwner, identity::IdentityStrategy};
use xmtp_mls::{builder::ClientBuilderError, client::ClientError};
use xmtp_proto::types::ApiIdentifier;

#[macro_use]
extern crate tracing;

pub type MlsContext =
    Arc<XmtpMlsLocalContext<XmtpApiClient, xmtp_db::DefaultStore, xmtp_db::DefaultMlsStore>>;
type Client = xmtp_mls::client::Client<MlsContext>;
type RustMlsGroup = xmtp_mls::groups::MlsGroup<MlsContext>;

#[derive(clap::ValueEnum, Clone, Default, Debug, serde::Serialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
enum Env {
    #[default]
    Local,
    Dev,
    Staging,
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
    LocalWallet(PrivateKeySigner),
}

impl InboxOwner for Wallet {
    fn get_identifier(&self) -> Result<Identifier, IdentifierValidationError> {
        match self {
            Wallet::LocalWallet(w) => w.get_identifier(),
        }
    }

    fn sign(&self, text: &str) -> Result<UnverifiedSignature, SignatureError> {
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

    let grpc: XmtpApiClient = match (cli.testnet, &cli.env) {
        (true, Env::Local) => MessageBackendBuilder::default()
            .v3_host(GrpcUrlsLocal::NODE)
            .gateway_host(GrpcUrlsLocal::GATEWAY)
            .is_secure(false)
            .build()?,
        (true, Env::Production) => MessageBackendBuilder::default()
            .v3_host(GrpcUrlsProduction::NODE)
            .gateway_host(GrpcUrlsProduction::GATEWAY)
            .is_secure(true)
            .build()?,
        (true, Env::Staging) => MessageBackendBuilder::default()
            .v3_host(GrpcUrlsStaging::NODE)
            .gateway_host(GrpcUrlsStaging::GATEWAY)
            .is_secure(true)
            .build()?,
        (true, Env::Dev) => MessageBackendBuilder::default()
            .v3_host(GrpcUrlsDev::NODE)
            .gateway_host(GrpcUrlsDev::GATEWAY)
            .is_secure(true)
            .build()?,
        (false, Env::Local) => MessageBackendBuilder::default()
            .v3_host(GrpcUrlsLocal::NODE)
            .is_secure(false)
            .build()?,
        (false, Env::Dev) | (false, Env::Staging) => MessageBackendBuilder::default()
            .v3_host(GrpcUrlsDev::NODE)
            .is_secure(true)
            .build()?,
        (false, Env::Production) => MessageBackendBuilder::default()
            .v3_host(GrpcUrlsProduction::NODE)
            .is_secure(true)
            .build()?,
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
                inbox_id = client.context.inbox_id(),
                recovery_address = format!("{recovery:?}"),
                installation_ids = &ids.as_value(),
                addresses = &addrs.as_value(),
                "identity info",
            );
        }
        Commands::ListGroups {} => {
            info!("List Groups");
            client
                .sync_welcomes()
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
            info!("Inbox ID is: {}", client.context.inbox_id());
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
                let messages = format_messages(messages, client.context.inbox_id().to_string())
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
                .add_members_by_identity(&address_to_identity(account_addresses))
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
                .remove_members_by_identity(&address_to_identity(account_addresses))
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
                Permissions::EveryoneIsAdmin => xmtp_mls::groups::PreconfiguredPolicies::Default,
                Permissions::GroupCreatorIsAdmin => {
                    xmtp_mls::groups::PreconfiguredPolicies::AdminsOnly
                }
            };
            let group = client
                .create_group(Some(group_permissions.to_policy_set()), None)
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
                .group(&hex::decode(group_id).expect("bad group id"))
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
        Commands::Clear {} => {
            fs::remove_file(cli.db.ok_or(eyre!("DB Missing"))?)?;
        }
        Commands::Debug(debug_commands) => {
            debug::handle_debug(&client, debug_commands).await.unwrap();
        }
        Commands::GetInboxId { account_address } => {
            let ident = address_to_identity(&[account_address]).pop().unwrap();
            let api_ident: ApiIdentifier = ident.into();
            let mapping = client
                .context
                .api()
                .get_inbox_ids(vec![api_ident.clone()])
                .await?;
            let inbox_id = mapping.get(&api_ident).unwrap();
            info!("Inbox_id {inbox_id}");
        }
    }

    Ok(())
}

async fn create_client<C: XmtpApi + Clone + XmtpQuery + 'static>(
    cli: &Cli,
    account: IdentityStrategy,
    grpc: C,
) -> Result<
    xmtp_mls::client::Client<
        Arc<XmtpMlsLocalContext<C, xmtp_db::DefaultStore, xmtp_db::DefaultMlsStore>>,
    >,
    CliError,
> {
    let msg_store = get_encrypted_store(&cli.db).await?;
    let builder = xmtp_mls::Client::builder(account).store(msg_store);
    let mut builder = builder.api_clients(grpc.clone(), grpc);

    builder = match (cli.testnet, &cli.env) {
        (false, Env::Local) => builder.device_sync_server_url(DeviceSyncUrls::LOCAL_ADDRESS),
        (false, Env::Dev) => builder.device_sync_server_url(DeviceSyncUrls::DEV_ADDRESS),
        _ => builder,
    };

    let client = builder
        .with_remote_verifier()?
        .default_mls_store()?
        .build()
        .await
        .map_err(CliError::ClientBuilder)?;

    Ok(client)
}

async fn register<C>(
    cli: &Cli,
    maybe_seed_phrase: Option<String>,
    client: C,
) -> Result<(), CliError>
where
    C: Clone + XmtpApi + XmtpQuery + 'static,
{
    let w: Wallet = if let Some(seed_phrase) = maybe_seed_phrase {
        Wallet::LocalWallet(
            MnemonicBuilder::<English>::default()
                .phrase(seed_phrase.as_str())
                .build()
                .unwrap(),
        )
    } else {
        Wallet::LocalWallet(PrivateKeySigner::random())
    };

    let nonce = 0;
    let ident = w.get_identifier().expect("Wallet address is invalid");
    let inbox_id = ident.inbox_id(nonce)?;
    let client = create_client(
        cli,
        IdentityStrategy::new(inbox_id, ident, nonce, None),
        client,
    )
    .await?;
    let mut signature_request = client.identity().signature_request().unwrap();
    let signature = w.sign(&signature_request.signature_text()).unwrap();
    signature_request
        .add_signature(signature, client.scw_verifier())
        .await
        .unwrap();

    if let Err(e) = client.register_identity(signature_request).await {
        error!("Initialization Failed: {}", e.to_string());
        panic!("Could not init");
    };
    info!(
        account_address = client.context.inbox_id(),
        installation_id = hex::encode(client.installation_public_key()),
        command_output = true,
        "Registered identity"
    );

    Ok(())
}

async fn get_group(client: &Client, group_id: Vec<u8>) -> Result<RustMlsGroup, CliError> {
    client.sync_welcomes().await?;
    let group = client.group(&group_id)?;
    group
        .sync()
        .await
        .map_err(|_| CliError::Generic("failed to sync group".to_string()))?;

    Ok(group)
}

async fn send(group: RustMlsGroup, msg: String) -> Result<(), CliError> {
    let mut buf = Vec::new();
    TextCodec::encode(msg.clone())
        .unwrap()
        .encode(&mut buf)
        .unwrap();
    group
        .send_message(
            buf.as_slice(),
            SendMessageOptsBuilder::default()
                .should_push(true)
                .build()
                .unwrap(),
        )
        .await?;
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
    [2u8; 32].into()
}

async fn get_encrypted_store(
    db: &Option<PathBuf>,
) -> Result<EncryptedMessageStore<NativeDb>, CliError> {
    let store = match db {
        Some(path) => {
            let s = path.as_path().to_string_lossy().to_string();
            info!("Using persistent storage: {} ", s);
            let db = NativeDb::builder().persistent(s).build_unencrypted()?;
            EncryptedMessageStore::new(db)?
        }

        None => {
            info!("Using ephemeral store");
            let db = NativeDb::builder()
                .key(static_enc_key())
                .ephemeral()
                .build()?;
            EncryptedMessageStore::new(db)?
        }
    };

    Ok(store)
}

fn pretty_delta(now: u64, then: u64) -> String {
    let f = timeago::Formatter::new();
    let diff = now.abs_diff(then);
    f.convert(Duration::from_nanos(diff))
}

fn pretty_association_state(state: &AssociationState) -> (Identifier, Vec<String>, Vec<String>) {
    let recovery_ident = state.recovery_identifier().clone();
    let installation_ids = state
        .installation_ids()
        .into_iter()
        .map(hex::encode)
        .collect::<Vec<String>>();

    let addresses = state
        .members_by_kind(MemberKind::Ethereum)
        .into_iter()
        .map(|m| m.identifier.to_eth_address().unwrap())
        .collect::<Vec<String>>();

    (recovery_ident, installation_ids, addresses)
}

fn address_to_identity(addresses: &[impl AsRef<str>]) -> Vec<Identifier> {
    addresses
        .iter()
        .map(|addr| Identifier::eth(addr.as_ref()).expect("Eth address is invalid"))
        .collect()
}
