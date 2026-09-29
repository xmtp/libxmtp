#[derive(Clone, Debug, Default, uniffi::Enum)]
pub enum StorageLocation {
    #[default]
    Default,
    InMemory,
    Directory(String),
    Path(String),
}

#[derive(Clone, Debug, Default, uniffi::Record)]
pub struct StorageOptions {
    pub location: StorageLocation,
    #[uniffi(default = None)]
    pub label: Option<String>,
    #[cfg(not(target_arch = "wasm32"))]
    #[uniffi(default = None)]
    pub encryption_key: Option<Vec<u8>>,
    #[uniffi(default = None)]
    pub pool: Option<StoragePoolOptions>,
    #[uniffi(default = false)]
    pub single_connection: bool,
}

#[derive(Clone, Debug, Default, uniffi::Record)]
pub struct StoragePoolOptions {
    #[uniffi(default = None)]
    pub min: Option<u32>,
    #[uniffi(default = None)]
    pub max: Option<u32>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct RegistrationOptions {
    #[uniffi(default = true)]
    pub auto: bool,
    #[uniffi(default = None)]
    pub nonce: Option<u64>,
}

impl Default for RegistrationOptions {
    fn default() -> Self {
        Self {
            auto: true,
            nonce: None,
        }
    }
}

#[derive(Clone, Debug, Default, uniffi::Enum)]
pub enum ForkRecoveryPolicy {
    #[default]
    None,
    AllowlistedGroups,
    All,
}

#[derive(Clone, Debug, Default, uniffi::Record)]
pub struct ForkRecoveryOptions {
    pub policy: ForkRecoveryPolicy,
    #[uniffi(default)]
    pub groups: Vec<crate::ConversationId>,
    #[uniffi(default = false)]
    pub disable_responses: bool,
    #[uniffi(default = None)]
    pub worker_interval_ns: Option<u64>,
}

impl TryFrom<ForkRecoveryOptions> for ForkRecoveryOpts {
    type Error = XmtpError;

    fn try_from(value: ForkRecoveryOptions) -> Result<Self, Self::Error> {
        use xmtp_mls::builder::ForkRecoveryPolicy as CorePolicy;
        Ok(Self {
            enable_recovery_requests: match value.policy {
                ForkRecoveryPolicy::None => CorePolicy::None,
                ForkRecoveryPolicy::AllowlistedGroups => CorePolicy::AllowlistedGroups,
                ForkRecoveryPolicy::All => CorePolicy::All,
            },
            groups_to_request_recovery: value
                .groups
                .into_iter()
                .map(crate::ConversationId::into_checked)
                .collect::<Result<_, _>>()?,
            disable_recovery_responses: value.disable_responses,
            worker_interval_ns: value.worker_interval_ns,
        })
    }
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum WorkerKind {
    DeviceSync,
    DisappearingMessages,
    CommitLog,
    TaskRunner,
    ConfigurationRefresh,
    HmacEpoch,
}

impl From<WorkerKind> for xmtp_mls::worker::WorkerKind {
    fn from(value: WorkerKind) -> Self {
        use xmtp_mls::worker::WorkerKind as CoreKind;
        match value {
            WorkerKind::DeviceSync => CoreKind::DeviceSync,
            WorkerKind::DisappearingMessages => CoreKind::DisappearingMessages,
            WorkerKind::CommitLog => CoreKind::CommitLog,
            WorkerKind::TaskRunner => CoreKind::TaskRunner,
            WorkerKind::ConfigurationRefresh => CoreKind::ConfigurationRefresh,
            WorkerKind::HmacEpoch => CoreKind::HmacEpoch,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct WorkerInterval {
    pub kind: WorkerKind,
    pub interval_ns: Option<u64>,
    pub jitter_ns: Option<u64>,
    pub enabled: Option<bool>,
}

#[derive(Clone, Debug, Default, uniffi::Record)]
pub struct WorkerOptions {
    #[uniffi(default = None)]
    pub default_interval_ns: Option<u64>,
    #[uniffi(default)]
    pub intervals: Vec<WorkerInterval>,
}

impl From<WorkerOptions> for xmtp_mls::worker::WorkerConfig {
    fn from(value: WorkerOptions) -> Self {
        let mut config = Self {
            default_interval_ns: value.default_interval_ns,
            ..Default::default()
        };
        for entry in value.intervals {
            let kind = entry.kind.into();
            if let Some(interval) = entry.interval_ns {
                config.interval_overrides.insert(kind, interval);
            }
            if let Some(jitter) = entry.jitter_ns {
                config.jitter_overrides.insert(kind, jitter);
            }
            if let Some(enabled) = entry.enabled {
                config.enabled.insert(kind, enabled);
            }
        }
        config
    }
}

#[xmtp_macro::callback_error]
#[derive(Clone, Debug, thiserror::Error, uniffi::Error)]
pub enum PreAuthenticateError {
    #[error("pre-authenticate callback failed")]
    Failed,
}

impl From<uniffi::UnexpectedUniFFICallbackError> for PreAuthenticateError {
    fn from(_: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Failed
    }
}

// Foreign traits need `with_foreign`, which `sdk_export` cannot emit.
#[uniffi::export(with_foreign)]
#[xmtp_common::async_trait]
pub trait PreAuthenticate: MaybeSend + MaybeSync + 'static {
    async fn run(&self) -> Result<(), PreAuthenticateError>;
}

#[derive(Clone, Default, uniffi::Record)]
pub struct ClientHandlers {
    #[uniffi(default = None)]
    pub pre_authenticate: Option<Arc<dyn PreAuthenticate>>,
}

#[derive(Clone, uniffi::Record)]
pub struct ClientOptions {
    /// Omission uses empty connection options, as the old field default did.
    #[uniffi(default = None)]
    pub backend: Option<BackendSource>,
    pub storage: StorageOptions,
    #[uniffi(default = true)]
    pub device_sync: bool,
    /// Permit startup from stored state when the backend is unavailable.
    #[uniffi(default = false)]
    pub allow_offline: bool,
    #[uniffi(default)]
    pub registration: RegistrationOptions,
    #[uniffi(default = None)]
    pub fork_recovery: Option<ForkRecoveryOptions>,
    #[uniffi(default = None)]
    pub workers: Option<WorkerOptions>,
    #[uniffi(default = None)]
    pub handlers: Option<ClientHandlers>,
}

impl Default for ClientOptions {
    fn default() -> Self {
        Self {
            backend: None,
            storage: StorageOptions::default(),
            device_sync: true,
            allow_offline: false,
            registration: RegistrationOptions::default(),
            fork_recovery: None,
            workers: None,
            handlers: None,
        }
    }
}

impl ClientOptions {
    /// Returns the core fork recovery options, or `InvalidArgument` if a
    /// conversation ID is malformed.
    pub(super) fn fork_recovery_opts(&self) -> Result<Option<ForkRecoveryOpts>, XmtpError> {
        self.fork_recovery
            .clone()
            .map(ForkRecoveryOpts::try_from)
            .transpose()
    }
}
