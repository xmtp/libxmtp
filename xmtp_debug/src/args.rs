//! App Argument Options
use clap::{Args, Parser, Subcommand, ValueEnum};
use clap_verbosity_flag::{InfoLevel, Verbosity};
use color_eyre::eyre;
use std::path::PathBuf;
use xmtp_configuration::{MULTI_NODE_TIMEOUT_MS, PAYER_WRITE_FILTER};
use xxhash_rust::xxh3;
mod types;
use std::time::Duration;
pub use types::*;
use xmtp_api_d14n::{MessageBackendBuilder, MiddlewareBuilder, ReadWriteClient};
use xmtp_api_grpc::GrpcClient;
use xmtp_proto::{
    api::Client,
    prelude::{ApiBuilder, NetConnectConfig},
};

/// Debug & Generate data on the XMTP Network
#[derive(Parser, Debug)]
pub struct AppOpts {
    // Print Version
    #[arg(long)]
    pub version: bool,
    #[command(subcommand)]
    pub cmd: Option<Commands>,
    #[command(flatten)]
    pub log: LogOptions,
    #[command(flatten)]
    pub backend: BackendOpts,
    /// Clear ALL local app data & state kept by xdbg
    /// Runs at the end of execution, so operations will still be carried out
    #[arg(long)]
    pub clear: bool,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    Generate(Generate),
    Modify(Modify),
    Inspect(Inspect),
    Send(Send),
    #[command(subcommand)]
    Query(Query),
    Info(InfoOpts),
    Export(ExportOpts),
    Stream(StreamOpts),
}

/// Send Data on the network
#[derive(Args, Debug)]
pub struct Send {
    pub action: ActionKind,
    pub data: String,
    pub group_id: GroupId,
}

#[derive(ValueEnum, Debug, Clone)]
pub enum ActionKind {
    Message,
}

/// Generate Groups/Messages/Users
#[derive(Args, Debug)]
pub struct Generate {
    /// Specify an entity to generate
    #[arg(value_enum, long, short)]
    pub entity: EntityKind,
    /// How many entities to generate
    #[arg(long, short)]
    pub amount: usize,
    /// Specify amount of random identities to invite to group
    #[arg(long)]
    pub invite: Option<usize>,
    #[command(flatten)]
    pub message_opts: MessageGenerateOpts,
    /// Maximum number of concurrent tasks to use during generation.
    /// Defaults to the number of available CPU cores if not specified.
    #[arg(long, short, default_value_t = Concurrency::default())]
    pub concurrency: Concurrency,
    /// enable reading publishes from the backend
    /// _NOTE:_ feature is experimental
    #[arg(long, short)]
    pub ryow: bool,
}

#[derive(Args, Debug, Clone)]
pub struct MessageGenerateOpts {
    /// Continuously generate & send messages
    #[arg(long, short)]
    pub r#loop: bool,
    /// Interval to send messages on (default every second)
    #[arg(long, short, default_value_t = MillisecondInterval::default())]
    pub interval: MillisecondInterval,
    /// Max variable message size, in words.
    #[arg(long, short, default_value = "100")]
    pub max_message_size: u32,
    /// on every interval, adds a new member to the group and changes the group description in
    /// addition to sending a message
    #[arg(long, short)]
    pub add_and_change_description: bool,
    /// on every interval, changes the group description in addition to sending a message
    #[arg(long, short)]
    pub change_description: bool,
    /// specify how many identities to add up to
    /// requires `add_or_change_description`.
    /// does nothing unless add_or_change_description is set
    #[arg(long, short, default_value = "100")]
    pub add_up_to: u32,
}

/// Modify state of local clients & groups
#[derive(Args, Debug)]
pub struct Modify {
    /// action to take
    #[arg(value_enum)]
    pub action: MemberModificationKind,

    /// group to modify
    pub group_id: GroupId,

    /// InboxID to add or remove
    #[arg(long, short)]
    pub inbox_id: Option<InboxId>,
}

#[derive(ValueEnum, Debug, Clone)]
pub enum MemberModificationKind {
    /// Remove a member from a group
    Remove,
    /// Add a random member to a group
    AddRandom,
    /// Add an external id the group
    AddExternal,
}

/// Inspect Local State
#[derive(Args, Debug)]
pub struct Inspect {
    /// The InboxId of the Client to Inspect
    pub inbox_id: InboxId,

    /// Kind of inspection to perform
    pub kind: InspectionKind,
}

#[derive(ValueEnum, Default, Debug, Clone)]
pub enum InspectionKind {
    /// Inspect the associations this client has
    Associations,
    /// Inspect the groups this client is apart of
    #[default]
    Groups,
}

/// Query for Information about a Group or Message or User
#[derive(Subcommand, Debug, Clone)]
pub enum Query {
    Identity(Identity),
    FetchKeyPackages(FetchKeyPackages),
    BatchQueryCommitLog(BatchQueryCommitLog),
    /// Get all keypackages for each installation id in the app db
    AllKeyPackages,
}

#[derive(Args, Debug, Clone)]
pub struct Identity {
    pub inbox_id: InboxId,
}

#[derive(Args, Debug, Clone)]
pub struct FetchKeyPackages {
    pub installation_keys: Vec<String>,
}

#[derive(Args, Debug, Clone)]
pub struct BatchQueryCommitLog {
    pub group_ids: Vec<String>,
    #[arg(long)]
    pub skip_unspecified: bool,
}

/// Print information about the local generated state
#[derive(Args, Debug)]
pub struct InfoOpts {
    /// Show a random identity
    #[arg(long)]
    pub random: bool,
    /// Show information about the app
    #[arg(long)]
    pub app: bool, // /// Show information about a specific identity, like its id and storage
                   // pub identity: IdentityInfo
}

/// Export information to JSON
#[derive(Args, Debug)]
pub struct ExportOpts {
    /// Entity to export
    #[arg(long, short)]
    pub entity: EntityKind,
    /// File to write to
    #[arg(long, short)]
    pub out: Option<PathBuf>,
}

/// Stream messages and conversations
#[derive(Args, Debug)]
pub struct StreamOpts {
    /// Indicate the Inbox to stream messages from.
    /// Defaults to a randomly chosen identity
    #[arg(long, short)]
    pub inbox: Option<InboxId>,
    /// Indicate the kind of stream.
    #[arg(long, short)]
    pub kind: StreamKind,
    /// Indicate format that should be used.
    #[arg(long, short)]
    pub format: FormatKind,
    /// optionally indicate a file to write to.
    /// Defaults to stdout
    #[arg(long, short)]
    pub out: Option<PathBuf>,
}

#[derive(ValueEnum, Debug, Default, Clone, Copy)]
pub enum FormatKind {
    /// output in a JSON Format
    Json,
    /// output in a CSV Format
    #[default]
    Csv,
}

#[derive(ValueEnum, Debug, Default, Clone, Copy)]
pub enum StreamKind {
    /// Stream only new conversations for this inbox id
    Conversations,
    /// Stream only messages for this inbox id
    #[default]
    Messages,
}

#[derive(ValueEnum, Debug, Clone)]
pub enum EntityKind {
    Group,
    Message,
    Identity,
}

impl std::fmt::Display for EntityKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use EntityKind::*;
        match self {
            Group => write!(f, "group"),
            Message => write!(f, "message"),
            Identity => write!(f, "identity"),
        }
    }
}

/// specify the log output
#[derive(Args, Debug)]
pub struct LogOptions {
    /// Output libxmtp logs into file with a structured, ndJSON format
    #[arg(long)]
    pub json: bool,
    /// Output libxmtp into file with logfmt format
    #[arg(long)]
    pub logfmt: bool,
    /// Output libxmtp logs to file in a human-readable format
    #[arg(long)]
    pub human: bool,
    /// Show key-value fields. Default on for JSON & logfmt and off for human
    #[arg(short, long, action)]
    pub show_fields: bool,
    /// Specify verbosity of logs, default ERROR
    #[command(flatten)]
    pub verbose: Verbosity<InfoLevel>,
}

/// Specify which backend to use
#[derive(Args, Clone, Debug, Default)]
pub struct BackendOpts {
    #[arg(
        value_enum,
        short,
        long,
        conflicts_with_all = &["url", "xmtpd_gateway_url"],
        default_value_t = BackendKind::Local
    )]
    pub backend: BackendKind,
    /// URL Pointing to a backend. Conflicts with `backend`
    #[arg(short, long)]
    pub url: Option<url::Url>,
    #[arg(short, long)]
    pub xmtpd_gateway_url: Option<url::Url>,
    /// Enable the decentralization backend
    #[arg(short, long)]
    pub d14n: bool,
    /// Timeout for reading writes to the decentralized backend
    #[arg(long, short, default_value_t = default_ryow_timeout())]
    pub ryow_timeout: humantime::Duration,
}

fn default_ryow_timeout() -> humantime::Duration {
    "5s".parse::<humantime::Duration>().unwrap()
}

impl BackendOpts {
    pub fn xmtpd_gateway_url(&self) -> eyre::Result<url::Url> {
        use BackendKind::*;

        if let Some(p) = &self.xmtpd_gateway_url {
            return Ok(p.clone());
        }

        match (self.backend, self.d14n) {
            (Dev, false) => eyre::bail!("No gateway for V3"),
            (Staging, false) => eyre::bail!("No gateway for V3"),
            (Production, false) => eyre::bail!("No gateway for V3"),
            (Local, false) => eyre::bail!("No gateway for V3"),
            (Dev, true) => Ok((*crate::constants::XMTP_DEV_GATEWAY).clone()),
            (Staging, true) => Ok((*crate::constants::XMTP_STAGING_GATEWAY).clone()),
            (Production, true) => Ok((*crate::constants::XMTP_PRODUCTION_GATEWAY).clone()),
            (Local, true) => Ok((*crate::constants::XMTP_LOCAL_GATEWAY).clone()),
        }
    }

    pub fn network_url(&self) -> url::Url {
        use BackendKind::*;

        if let Some(n) = &self.url {
            return n.clone();
        }

        match (self.backend, self.d14n) {
            (Dev, false) => (*crate::constants::XMTP_DEV).clone(),
            (Staging, false) => (*crate::constants::XMTP_DEV).clone(),
            (Production, false) => (*crate::constants::XMTP_PRODUCTION).clone(),
            (Local, false) => (*crate::constants::XMTP_LOCAL).clone(),
            (Dev, true) => (*crate::constants::XMTP_DEV_D14N).clone(),
            (Staging, true) => (*crate::constants::XMTP_STAGING_D14N).clone(),
            (Production, true) => (*crate::constants::XMTP_PRODUCTION_D14N).clone(),
            (Local, true) => (*crate::constants::XMTP_LOCAL_D14N).clone(),
        }
    }

    pub fn connect(&self) -> eyre::Result<crate::DbgClientApi> {
        let network = self.network_url();
        let is_secure = network.scheme() == "https";

        let mut builder = MessageBackendBuilder::default();
        builder.v3_host(network.as_str()).is_secure(is_secure);
        if self.d14n {
            let xmtpd_gateway_host = self.xmtpd_gateway_url()?;
            trace!(url = %network, xmtpd_gateway = %xmtpd_gateway_host, is_secure, "create grpc");
            Ok(builder.gateway_host(xmtpd_gateway_host.as_str()).build()?)
        } else {
            trace!(url = %network, is_secure, "create grpc");
            Ok(builder.build()?)
        }
    }

    pub fn xmtpd(&self) -> eyre::Result<impl Client> {
        let network = self.network_url();
        let is_secure = network.scheme() == "https";

        let mut gateway_client_builder = GrpcClient::builder();
        gateway_client_builder.set_host(self.xmtpd_gateway_url()?.to_string());
        gateway_client_builder.set_tls(is_secure);
        let mut node_builder = GrpcClient::builder();
        node_builder.set_tls(is_secure);

        let mut multi_node = xmtp_api_d14n::middleware::MultiNodeClientBuilder::default();
        multi_node.set_timeout(Duration::from_millis(MULTI_NODE_TIMEOUT_MS))?;
        multi_node.set_gateway_builder(gateway_client_builder.clone())?;
        multi_node.set_node_client_builder(node_builder)?;
        let multi_node = multi_node.build()?;

        let gateway_client = gateway_client_builder.build()?;
        let rw = ReadWriteClient::builder()
            .read(multi_node)
            .write(gateway_client)
            .filter(PAYER_WRITE_FILTER)
            .build()?;
        Ok(rw)
    }
}

impl<'a> From<&'a BackendOpts> for u64 {
    fn from(value: &'a BackendOpts) -> Self {
        use BackendKind::*;

        if let Some(ref url) = value.url {
            xxh3::xxh3_64(url.as_str().as_bytes())
        } else {
            match (value.backend, value.d14n) {
                (Production, false) => 2,
                (Staging, false) => 1,
                (Dev, false) => 1,
                (Local, false) => 0,
                (Production, true) => 5,
                (Staging, true) => 6,
                (Dev, true) => 4,
                (Local, true) => 3,
            }
        }
    }
}

impl From<BackendOpts> for u64 {
    fn from(value: BackendOpts) -> Self {
        (&value).into()
    }
}

impl From<BackendOpts> for url::Url {
    fn from(value: BackendOpts) -> Self {
        let BackendOpts {
            backend, url, d14n, ..
        } = value;
        url.unwrap_or(backend.to_network_url(d14n))
    }
}

#[derive(ValueEnum, Debug, Copy, Clone, Default)]
pub enum BackendKind {
    Dev,
    Staging,
    Production,
    #[default]
    Local,
}

impl BackendKind {
    fn to_network_url(self, d14n: bool) -> url::Url {
        use BackendKind::*;
        match (self, d14n) {
            (Dev, false) => (*crate::constants::XMTP_DEV).clone(),
            (Staging, false) => (*crate::constants::XMTP_DEV).clone(),
            (Production, false) => (*crate::constants::XMTP_PRODUCTION).clone(),
            (Local, false) => (*crate::constants::XMTP_LOCAL).clone(),
            (Dev, true) => (*crate::constants::XMTP_DEV_D14N).clone(),
            (Staging, true) => (*crate::constants::XMTP_STAGING_D14N).clone(),
            (Production, true) => (*crate::constants::XMTP_PRODUCTION_D14N).clone(),
            (Local, true) => (*crate::constants::XMTP_LOCAL_D14N).clone(),
        }
    }
}

impl From<BackendKind> for url::Url {
    fn from(value: BackendKind) -> Self {
        use BackendKind::*;
        match value {
            Dev => (*crate::constants::XMTP_DEV).clone(),
            Staging => (*crate::constants::XMTP_DEV).clone(),
            Production => (*crate::constants::XMTP_PRODUCTION).clone(),
            Local => (*crate::constants::XMTP_LOCAL).clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn parse_backend_args(args: &[&str]) -> Result<BackendOpts, clap::Error> {
        AppOpts::try_parse_from(std::iter::once("test").chain(args.iter().copied()))
            .map(|app| app.backend)
    }

    #[test]
    fn backend_only_is_valid() {
        let opts = parse_backend_args(&["--backend", "local"]);
        assert!(opts.is_ok());
    }

    #[test]
    fn url_and_gateway_url_is_valid() {
        let opts = parse_backend_args(&[
            "--url",
            "http://localhost:5050",
            "--xmtpd-gateway-url",
            "http://localhost:5052",
        ]);
        assert!(opts.is_ok());
    }

    #[test]
    fn backend_and_url_is_invalid() {
        let opts = parse_backend_args(&["--backend", "local", "--url", "http://localhost:5050"]);
        assert!(opts.is_err());
    }

    #[test]
    fn backend_and_gateway_url_is_invalid() {
        let opts = parse_backend_args(&[
            "--backend",
            "local",
            "--xmtpd-gateway-url",
            "http://localhost:5052",
        ]);
        assert!(opts.is_err());
    }

    #[test]
    fn url_only_is_valid_but_maybe_warning() {
        let opts = parse_backend_args(&["--url", "http://localhost:5050"]);
        assert!(opts.is_ok());
    }

    #[test]
    fn gateway_url_only_is_valid_but_maybe_warning() {
        let opts = parse_backend_args(&["--xmtpd-gateway-url", "http://localhost:5052"]);
        assert!(opts.is_ok());
    }

    #[test]
    fn backend_and_both_urls_is_invalid() {
        let opts = parse_backend_args(&[
            "--backend",
            "local",
            "--url",
            "http://localhost:5050",
            "--xmtpd-gateway-url",
            "http://localhost:5052",
        ]);
        assert!(opts.is_err());
    }
}
