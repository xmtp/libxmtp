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
use xmtp_api_d14n::{ClientBundle, MessageBackendBuilder, MiddlewareBuilder, ReadWriteClient};
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
    /// Exit non-zero on the first per-operation error instead of logging
    /// and continuing. Useful in `git bisect run` sessions where a single
    /// failed send/sync should mark the commit bad.
    #[arg(long)]
    pub fail_fast: bool,
    /// Hide identities created by other xdbg binary versions. By default
    /// every identity (regardless of which xdbg version created it) is
    /// visible. With this flag, only identities created by this exact
    /// binary version are visible. Writes are always partitioned by
    /// version regardless of the flag.
    #[arg(long)]
    pub strict_versioning: bool,
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
    Healthcheck(HealthcheckOpts),
    Sync(SyncOpts),
}

/// Cross-version libxmtp health check.
/// Runs every user-visible protocol op against the local xdbg state,
/// validates that all clients converge, and exits non-zero on any failure.
#[derive(Args, Debug)]
pub struct HealthcheckOpts {}

/// Walk identities loaded from redb, run `sync_welcomes` + per-group
/// `sync` on each, and reconcile redb's `GroupStore` / `MessageStore`
/// against libxmtp's SQLite. Useful for catching up local state when
/// other xdbg invocations have mutated the network.
///
/// Honors `--strict-versioning` — only syncs identities visible to
/// the current binary version when the flag is set.
#[derive(Args, Debug)]
pub struct SyncOpts {}

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

    /// InboxID to add or remove (ignored for `add-from-redb`)
    #[arg(long, short)]
    pub inbox_id: Option<InboxId>,

    /// For `add-from-redb`: which version_hash partitions to pull
    /// identities from.
    #[arg(long, value_enum, default_value_t = IncludeVersions::All)]
    pub include_versions: IncludeVersions,

    /// For `add-from-redb`: also promote each newly-added inbox to
    /// super-admin via `update_admin_list(AddSuper, inbox)`.
    #[arg(long)]
    pub promote_super_admin: bool,
}

#[derive(ValueEnum, Debug, Clone, PartialEq, Eq)]
pub enum MemberModificationKind {
    /// Remove a member from a group
    Remove,
    /// Add a random member to a group
    AddRandom,
    /// Add an external id the group
    AddExternal,
    /// Add identities loaded from redb. Uses `--include-versions` and
    /// `--promote-super-admin`. The positional `--inbox-id` is ignored.
    AddFromRedb,
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncludeVersions {
    /// Only identities created by this exact xdbg binary version.
    #[value(name = "self")]
    Self_,
    /// Every version EXCEPT this binary's version.
    Other,
    /// All versions (default).
    All,
}

impl std::fmt::Display for IncludeVersions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IncludeVersions::Self_ => write!(f, "self"),
            IncludeVersions::Other => write!(f, "other"),
            IncludeVersions::All => write!(f, "all"),
        }
    }
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

    pub fn client_bundle(&self) -> eyre::Result<xmtp_mls::XmtpClientBundle> {
        let network = self.network_url();
        let is_secure = network.scheme() == "https";

        let mut builder = ClientBundle::builder();
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

    #[test]
    fn fail_fast_defaults_false() {
        let opts = AppOpts::try_parse_from(["xdbg"]).expect("parses with no args");
        assert!(!opts.fail_fast, "--fail-fast should default to false");
    }

    #[test]
    fn fail_fast_parses_when_present() {
        let opts =
            AppOpts::try_parse_from(["xdbg", "--fail-fast"]).expect("parses with --fail-fast");
        assert!(opts.fail_fast);
    }

    #[test]
    fn fail_fast_has_no_short_alias() {
        // Asserts that we don't allocate `-f` (or any short) for this flag,
        // so future subcommands remain free to use short flags. `xdbg -f`
        // should fail to parse.
        let opts = AppOpts::try_parse_from(["xdbg", "-f"]);
        assert!(opts.is_err(), "--fail-fast should not have a short alias");
    }

    #[test]
    fn parses_with_strict_versioning() {
        let opts = AppOpts::try_parse_from(["xdbg", "--strict-versioning"])
            .expect("parses with --strict-versioning");
        assert!(
            opts.strict_versioning,
            "strict_versioning flag should be set"
        );
    }

    #[test]
    fn strict_versioning_defaults_off() {
        let opts = AppOpts::try_parse_from(["xdbg"]).expect("parses with no args");
        assert!(
            !opts.strict_versioning,
            "strict_versioning should default to false"
        );
    }

    #[test]
    fn strict_versioning_has_no_short_alias() {
        // `-s` is the natural short for `strict_versioning` but is taken by
        // `--show-fields`. Confirm that `-s` does NOT set `strict_versioning`
        // (i.e. `--strict-versioning` has no short alias of its own).
        let opts = AppOpts::try_parse_from(["xdbg", "-s"]).expect("-s parses via --show-fields");
        assert!(
            !opts.strict_versioning,
            "--strict-versioning should not have a short alias"
        );
    }

    #[test]
    fn parses_sync_subcommand() {
        let opts = AppOpts::try_parse_from(["xdbg", "sync"]).expect("parses `xdbg sync`");
        assert!(matches!(opts.cmd, Some(Commands::Sync(_))));
    }

    #[test]
    fn parses_modify_add_from_redb() {
        let opts = AppOpts::try_parse_from([
            "xdbg",
            "modify",
            "add-from-redb",
            "aabbccddeeff00112233445566778899",
            "--include-versions",
            "other",
            "--promote-super-admin",
        ])
        .expect("parses modify add-from-redb");
        if let Some(Commands::Modify(m)) = &opts.cmd {
            assert!(matches!(m.action, MemberModificationKind::AddFromRedb));
            assert_eq!(m.include_versions, IncludeVersions::Other);
            assert!(m.promote_super_admin);
        } else {
            panic!("expected Modify variant");
        }
    }

    #[test]
    fn include_versions_defaults_to_all() {
        let opts = AppOpts::try_parse_from([
            "xdbg",
            "modify",
            "add-from-redb",
            "aabbccddeeff00112233445566778899",
        ])
        .expect("parses without --include-versions");
        if let Some(Commands::Modify(m)) = &opts.cmd {
            assert_eq!(m.include_versions, IncludeVersions::All);
            assert!(!m.promote_super_admin);
        } else {
            panic!("expected Modify variant");
        }
    }
}
