use xmtp_mls::builder::{ForkRecoveryOpts, ForkRecoveryPolicy};

#[derive(uniffi::Enum, Debug)]
pub enum FfiForkRecoveryPolicy {
    None,
    AllowlistedGroups,
    All,
}

impl From<FfiForkRecoveryPolicy> for ForkRecoveryPolicy {
    fn from(policy: FfiForkRecoveryPolicy) -> Self {
        match policy {
            FfiForkRecoveryPolicy::None => ForkRecoveryPolicy::None,
            FfiForkRecoveryPolicy::AllowlistedGroups => ForkRecoveryPolicy::AllowlistedGroups,
            FfiForkRecoveryPolicy::All => ForkRecoveryPolicy::All,
        }
    }
}

// Please see docs for more information.
// It is recommended to control fork recovery via the enable_recovery_requests policy.
// `disable_recovery_responses` is an emergency switch to disable fork recovery responses
// that should not be set to `true` in normal operation.
#[derive(uniffi::Record, Debug)]
pub struct FfiForkRecoveryOpts {
    pub enable_recovery_requests: FfiForkRecoveryPolicy,
    pub groups_to_request_recovery: Vec<String>,
    pub disable_recovery_responses: Option<bool>,
}

impl From<FfiForkRecoveryOpts> for ForkRecoveryOpts {
    fn from(opts: FfiForkRecoveryOpts) -> Self {
        Self {
            enable_recovery_requests: opts.enable_recovery_requests.into(),
            groups_to_request_recovery: opts.groups_to_request_recovery,
            disable_recovery_responses: opts.disable_recovery_responses.unwrap_or(false),
        }
    }
}
