//! App Argument Options
use clap::{Args, Parser, Subcommand, ValueEnum};
use clap_verbosity_flag::{InfoLevel, Verbosity};
use color_eyre::eyre;
use std::path::PathBuf;
use xmtp_configuration::PAYER_WRITE_FILTER;
use xxhash_rust::xxh3;
mod types;
pub use types::*;
use xmtp_api_d14n::{ClientBundle, MessageBackendBuilder, ReadWriteClient};
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
    /// Emit CSV metric lines (latency_seconds, throughput_events, event)
    /// to stdout. Off by default for clean CLI output.
    #[arg(long)]
    pub metrics: bool,
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
    Test(TestOpts),
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
    pub app: bool,
}

#[derive(ValueEnum, Debug, Copy, Clone)]
pub enum ExportEntityKind {
    Group,
    Message,
    Identity,
    GroupTopics,
    IdentityTopics,
    KeyPackageTopics,
    WelcomeMessageTopics,
}

/// Export information to JSON
#[derive(Args, Debug)]
pub struct ExportOpts {
    /// Entity to export
    #[arg(long, short)]
    pub entity: ExportEntityKind,
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

#[derive(ValueEnum, Debug, Copy, Clone)]
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

/// Log format for stdout output
#[derive(ValueEnum, Debug, Clone, Default)]
pub enum LogFormat {
    /// Human-readable, colored in terminals
    #[default]
    Text,
    /// Structured JSON (for Docker/Datadog)
    Json,
}

/// specify the log output
#[derive(Args, Debug)]
pub struct LogOptions {
    /// Stdout log format: "text" (default, colored in terminals) or "json" (for Docker/Datadog).
    /// Can also be set via XDBG_LOG_FORMAT env var.
    #[arg(long, env = "XDBG_LOG_FORMAT", default_value = "text")]
    pub log_format: LogFormat,
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
    /// Connect reads directly to a single xmtpd node for D14n, bypassing MultiNodeClient
    /// gateway discovery. Writes still route through --xmtpd-gateway-url.
    /// Requires --d14n.
    #[arg(long, requires = "d14n")]
    pub d14n_host: Option<url::Url>,
    /// Use the perf gateway (closest-node selection) instead of the default gateway.
    /// Requires --d14n.
    #[arg(short, long, requires = "d14n")]
    pub perf: bool,
    /// enable the v3 -> d14n cutover client
    #[arg(short = 'm', long, conflicts_with_all = &["d14n"])]
    pub enable_migration: bool,
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

        if self.perf {
            debug_assert!(self.d14n, "--perf requires --d14n");
            return match self.backend {
                Dev => Ok((*crate::constants::XMTP_DEV_PERF_GATEWAY).clone()),
                Staging => Ok((*crate::constants::XMTP_STAGING_PERF_GATEWAY).clone()),
                Production => Ok((*crate::constants::XMTP_PRODUCTION_PERF_GATEWAY).clone()),
                Local => Ok((*crate::constants::XMTP_LOCAL_PERF_GATEWAY).clone()),
            };
        }

        match (self.backend, self.d14n, self.enable_migration) {
            (Dev, false, false) => eyre::bail!("No gateway for V3"),
            (Staging, false, false) => eyre::bail!("No gateway for V3"),
            (Production, false, false) => eyre::bail!("No gateway for V3"),
            (Local, false, false) => eyre::bail!("No gateway for V3"),
            (Dev, true, false) => Ok((*crate::constants::XMTP_DEV_GATEWAY).clone()),
            (Staging, true, false) => Ok((*crate::constants::XMTP_STAGING_GATEWAY).clone()),
            (Production, true, false) => Ok((*crate::constants::XMTP_PRODUCTION_GATEWAY).clone()),
            (Local, true, false) => Ok((*crate::constants::XMTP_LOCAL_GATEWAY).clone()),
            (Local, _, true) => Ok((*crate::constants::XMTP_LOCAL_GATEWAY).clone()),
            (Dev, _, true) => Ok((*crate::constants::XMTP_DEV_GATEWAY).clone()),
            (Staging, _, true) => Ok((*crate::constants::XMTP_STAGING_GATEWAY).clone()),
            (Production, _, true) => Ok((*crate::constants::XMTP_PRODUCTION_GATEWAY).clone()),
        }
    }

    pub fn network_url(&self) -> url::Url {
        if let Some(n) = &self.url {
            return n.clone();
        }
        self.backend.to_network_url(self.d14n)
    }

    pub fn connect(&self) -> eyre::Result<crate::DbgClientApi> {
        let mut builder = MessageBackendBuilder::default();
        let bundle = self.client_bundle()?;
        Ok(builder.from_bundle(bundle)?)
    }

    pub fn client_bundle(&self) -> eyre::Result<xmtp_mls::XmtpClientBundle> {
        let network = self.network_url();
        let mut builder = ClientBundle::builder();
        builder.v3_host(network.as_str());
        if self.enable_migration {
            let xmtpd_gateway_host = self.xmtpd_gateway_url()?;
            trace!(url = %network, xmtpd_gateway = %xmtpd_gateway_host, "create grpc");
            return Ok(builder.gateway_host(xmtpd_gateway_host.as_str()).build()?);
        }
        if self.d14n {
            let xmtpd_gateway_host = self.xmtpd_gateway_url()?;
            Ok(builder
                .maybe_xmtpd_host(self.d14n_host.clone())
                .gateway_host(xmtpd_gateway_host.as_str())
                .build_d14n()?)
        } else {
            trace!(url = %network, "create grpc");
            Ok(builder.build_v3()?)
        }
    }

    pub fn xmtpd(&self) -> eyre::Result<impl Client> {
        let mut gateway_client_builder = GrpcClient::builder();
        gateway_client_builder.set_host(self.xmtpd_gateway_url()?);
        let gateway_client = gateway_client_builder.build()?;
        let multi_node = xmtp_api_d14n::middleware::MultiNodeClient::builder()
            .gateway_client(gateway_client.clone())
            .node_client_template(GrpcClient::builder())
            .build()?;

        let rw = ReadWriteClient::builder()
            .read(multi_node)
            .write(gateway_client)
            .filter(PAYER_WRITE_FILTER)
            .build()?;
        Ok(rw)
    }
}

// this decides the folder/prefix for network
// each network gets an isolated folder/database for redb and also sqlite clients
// if the numbers are the same, clients will conflict on network
// custom network URLS are hashed with xxh3_64
impl<'a> From<&'a BackendOpts> for u64 {
    fn from(value: &'a BackendOpts) -> Self {
        use BackendKind::*;

        if let Some(ref url) = value.url {
            xxh3::xxh3_64(url.as_str().as_bytes())
        } else {
            match (value.backend, value.d14n, value.enable_migration) {
                (Production, false, false) => 2,
                (Staging, false, false) => 1,
                (Dev, false, false) => 1,
                (Local, false, false) => 0,
                (Production, true, false) => 5,
                (Staging, true, false) => 6,
                (Dev, true, false) => 4,
                (Local, true, false) => 3,
                // Migration cases, where the client is both d14n and v3
                (Local, _, true) => 7,
                (Dev, _, true) => 8,
                (Staging, _, true) => 9,
                (Production, _, true) => 10,
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
    pub fn to_network_url(self, d14n: bool) -> url::Url {
        use BackendKind::*;
        match (self, d14n) {
            (Dev, false) => (*crate::constants::XMTP_DEV).clone(),
            (Staging, false) => (*crate::constants::XMTP_STAGING).clone(),
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
        value.to_network_url(false)
    }
}

/// Test scenarios for e2e latency measurement
#[derive(Args, Debug)]
pub struct TestOpts {
    /// Test scenario to run
    #[arg(value_enum)]
    pub scenario: TestScenario,
    /// Number of iterations
    #[arg(long, short, default_value = "1")]
    pub iterations: usize,
    /// Number of messages for group-sync scenario
    #[arg(long, short, default_value = "10")]
    pub message_count: usize,
    /// V4/D14N replication node URL for migration-latency scenario.
    /// Must be a D14N node (e.g. https://grpc.testnet.xmtp.network:443),
    /// NOT the payer gateway — the gateway doesn't serve QueryEnvelopes reads.
    #[arg(long)]
    pub v4_node_url: Option<url::Url>,
    /// Timeout in seconds for waiting for migrated message on V4 (default 120)
    #[arg(long, default_value = "120")]
    pub migration_timeout: u64,
    /// Number of messages to send for content-parity scenario (default 5)
    #[arg(long, default_value = "5")]
    pub parity_messages: usize,
    /// Number of messages to send for wallet-continuity scenario (default 5)
    #[arg(long, default_value = "5")]
    pub continuity_messages: usize,
}

#[derive(ValueEnum, Debug, Clone)]
pub enum TestScenario {
    /// Measure message stream delivery latency (sender → receiver)
    MessageVisibility,
    /// Measure group sync latency after N messages
    GroupSync,
    /// Measure V3→V4 migration latency (write to V3, poll V4)
    MigrationLatency,
    /// Validate V3→V4 content parity (write structured payloads, diff on V4)
    ContentParity,
    /// Validate wallet continuity: V3 data readable on V4 with same wallet
    WalletContinuity,
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

    #[test]
    fn perf_requires_d14n() {
        let opts = parse_backend_args(&["--perf"]);
        assert!(opts.is_err(), "--perf without --d14n should fail");
    }

    #[test]
    fn perf_with_d14n_is_valid() {
        let opts = parse_backend_args(&["--perf", "--d14n"]);
        assert!(opts.is_ok());
        let backend = opts.unwrap();
        assert!(backend.perf);
        assert!(backend.d14n);
    }

    #[test]
    fn perf_with_d14n_and_backend_is_valid() {
        let opts = parse_backend_args(&["--perf", "--d14n", "--backend", "staging"]);
        assert!(opts.is_ok());
        let backend = opts.unwrap();
        let url = backend.xmtpd_gateway_url().unwrap();
        assert!(
            url.as_str().contains("payer-perf"),
            "perf flag should select perf gateway, got: {url}"
        );
    }

    #[test]
    fn explicit_gateway_url_overrides_perf() {
        // --xmtpd-gateway-url conflicts with --backend, so we use --url to
        // avoid that conflict and verify the explicit URL takes precedence
        // over the perf gateway resolution
        let opts = parse_backend_args(&[
            "--perf",
            "--d14n",
            "--url",
            "http://localhost:5050",
            "--xmtpd-gateway-url",
            "http://custom:5052",
        ]);
        assert!(opts.is_ok());
        let backend = opts.unwrap();
        assert!(backend.perf, "perf flag should be set");
        let url = backend.xmtpd_gateway_url().unwrap();
        assert_eq!(
            url.as_str(),
            "http://custom:5052/",
            "explicit --xmtpd-gateway-url should override perf gateway"
        );
    }

    #[test]
    fn metrics_flag_defaults_false() {
        let opts = AppOpts::try_parse_from(["xdbg"]).expect("parses with no args");
        assert!(!opts.metrics, "--metrics should default to false");
    }

    #[test]
    fn metrics_flag_parses_when_present() {
        let opts = AppOpts::try_parse_from(["xdbg", "--metrics"]).expect("parses with --metrics");
        assert!(opts.metrics, "--metrics should set opts.metrics to true");
    }

    #[test]
    fn metrics_flag_coexists_with_enable_migration_short_flag() {
        // -m is --enable-migration on BackendOpts; --metrics must not collide.
        let opts = AppOpts::try_parse_from(["xdbg", "--metrics", "-m"])
            .expect("--metrics and -m should coexist");
        assert!(opts.metrics);
        assert!(opts.backend.enable_migration);
    }
}
