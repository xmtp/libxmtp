//! Bounded JSON messages between the supervisor and each installation process.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

pub(crate) const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;
pub(crate) const RPC_TIMEOUT_SECS: u64 = 90;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct InstanceConfig {
    pub slot: usize,
    pub inbox_index: usize,
    pub database: PathBuf,
    pub database_key: String,
    pub wallet_key: String,
    pub endpoint: String,
    pub seed: u64,
    pub stream_owner: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum Operation {
    Create { members: Vec<String> },
    Add { group: String, inbox: String },
    Remove { group: String, inbox: String },
    Readd { group: String, inbox: String },
    Metadata { group: String, value: String },
    Send { group: String, token: String },
    PendingSend { group: String, token: String },
    Sync { group: String },
    SyncAll,
    NewInstallation { inbox_index: usize },
    RestartStream,
    Consent { group: String, state: u8 },
    UpdateInstallations { group: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub(crate) enum Command {
    Operation { operation: Operation },
    Checkpoint,
    Snapshot,
    Counters,
    Tokens { tokens: Vec<String> },
    Publish { group: String },
    Stream { enabled: bool },
    Disk { kind: String, duration_ms: u64 },
    Disconnect { duration_ms: u64 },
    ClearFaults,
    Drain,
    Shutdown,
}

impl Command {
    /// Trace names contain no request data or message content.
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Operation { operation } => match operation {
                Operation::Create { .. } => "create",
                Operation::Add { .. } => "add",
                Operation::Remove { .. } => "remove",
                Operation::Readd { .. } => "readd",
                Operation::Metadata { .. } => "metadata",
                Operation::Send { .. } => "send",
                Operation::PendingSend { .. } => "pending_send",
                Operation::Sync { .. } => "sync",
                Operation::SyncAll => "sync_all",
                Operation::NewInstallation { .. } => "new_installation",
                Operation::RestartStream => "restart_stream",
                Operation::Consent { .. } => "consent",
                Operation::UpdateInstallations { .. } => "update_installations",
            },
            Self::Checkpoint => "checkpoint",
            Self::Snapshot => "snapshot",
            Self::Counters => "counters",
            Self::Tokens { .. } => "tokens",
            Self::Publish { .. } => "publish",
            Self::Stream { .. } => "stream",
            Self::Disk { .. } => "disk",
            Self::Disconnect { .. } => "disconnect",
            Self::ClearFaults => "clear_faults",
            Self::Drain => "drain",
            Self::Shutdown => "shutdown",
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Request {
    pub id: u64,
    pub command: Command,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Response {
    pub id: u64,
    pub value: Option<Value>,
    pub error: Option<String>,
}
