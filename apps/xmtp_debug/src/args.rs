//! App Argument Options
use clap::{Args, Parser, Subcommand, ValueEnum};
use clap_verbosity_flag::{InfoLevel, Verbosity};
use color_eyre::eyre;
use std::path::PathBuf;
use xxhash_rust::xxh3;
mod types;
pub use types::*;
use xmtp_api_backend::MessageBackendBuilder;
use xmtp_proto::types::GroupId;

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
    Test(TestOpts),
    Healthcheck(HealthcheckOpts),
    Sync(SyncOpts),
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
}

#[derive(Args, Copy, Debug, Clone)]
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
    /// Query the server-side welcome queue for every installation
    /// known to redb (across all binary-version partitions). Bypasses
    /// libxmtp's `sync_welcomes` so you see the raw server response —
    /// useful for diagnosing "welcome was published but recipient
    /// sync returned 0" scenarios.
    Welcomes,
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
    /// Append `openmls_kv=trace` to file-log filter to capture SqlKeyStore K/V spans.
    #[arg(long)]
    pub trace_openmls_kv: bool,
}

/// Backend connection options.
#[derive(Args, Clone, Debug)]
pub struct BackendOpts {
    /// Required self-hosted backend URL.
    #[arg(short, long)]
    pub url: url::Url,
}

impl BackendOpts {
    pub fn hash(&self) -> u64 {
        xxh3::xxh3_64(self.url.as_str().as_bytes())
    }

    pub fn connect(&self) -> eyre::Result<crate::DbgClientApi> {
        Ok(MessageBackendBuilder::default()
            .host(self.url.as_str())
            .build()?)
    }
}

impl From<&BackendOpts> for u64 {
    fn from(value: &BackendOpts) -> Self {
        value.hash()
    }
}

impl From<BackendOpts> for u64 {
    fn from(value: BackendOpts) -> Self {
        value.hash()
    }
}

impl From<BackendOpts> for url::Url {
    fn from(value: BackendOpts) -> Self {
        value.url
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
}

#[derive(ValueEnum, Debug, Clone)]
pub enum TestScenario {
    /// Measure message stream delivery latency (sender → receiver)
    MessageVisibility,
    /// Measure group sync latency after N messages
    GroupSync,
}

/// Cross-version libxmtp health check.
/// Runs every user-visible protocol op against the local xdbg state,
/// validates that all clients converge, and exits non-zero on any failure.
#[derive(Args, Debug)]
pub struct HealthcheckOpts {
    /// Skip mutating ops; only reads, sends, and validators run.
    /// Primary is reused from existing_clients instead of registered.
    #[arg(long)]
    pub read_only: bool,
}

/// Walk identities loaded from redb, run `sync_welcomes` + per-group
/// `sync` on each, and reconcile redb's `GroupStore` / `MessageStore`
/// against libxmtp's SQLite. Useful for catching up local state when
/// other xdbg invocations have mutated the network.
///
/// Honors `--strict-versioning` — only syncs identities visible to
/// the current binary version when the flag is set.
#[derive(Args, Debug)]
pub struct SyncOpts {}
