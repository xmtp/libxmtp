//! Client configuration values that every layer reads.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceSyncMode {
    Disabled,
    Enabled,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ForkRecoveryPolicy {
    None,
    AllowlistedGroups,
    All,
}

#[derive(Clone, Debug)]
pub struct ForkRecoveryOpts {
    pub enable_recovery_requests: ForkRecoveryPolicy,
    pub groups_to_request_recovery: Vec<String>,
    pub disable_recovery_responses: bool,
    pub worker_interval_ns: Option<u64>,
}

impl Default for ForkRecoveryOpts {
    fn default() -> Self {
        Self {
            enable_recovery_requests: ForkRecoveryPolicy::None,
            groups_to_request_recovery: Vec::new(),
            disable_recovery_responses: false,
            worker_interval_ns: None,
        }
    }
}

#[derive(Clone)]
pub struct DeviceSync {
    pub(crate) mode: DeviceSyncMode,
}
