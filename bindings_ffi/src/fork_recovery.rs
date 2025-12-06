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
#[derive(uniffi::Record, Debug)]
pub struct FfiForkRecoveryOpts {
    // These two params are used to roll out the fork recovery feature.
    pub enable_recovery_requests: FfiForkRecoveryPolicy,
    pub groups_to_request_recovery: Vec<String>,
    // Emergency switch to disable fork recovery responses (do not set in normal operation)
    pub disable_recovery_responses: Option<bool>,
    // Interval at which to run the fork recovery worker (do not set in normal operation)
    // After a 'bad' commit is made, fork recovery can be expected to take up to 4x this interval
    // to complete end-to-end, assuming both the super admin and forked installation are online
    // and streaming welcomes.
    // This can be overridden for end-to-end/manual testing purposes.
    // If the interval duration is less than the time it takes for a single tick of the worker to
    // complete, the worker will wait for the first tick to complete before the next tick begins.
    // Worth considering if any strange behavior is observed with low intervals.
    pub worker_interval_ns: Option<u64>,
}

impl From<FfiForkRecoveryOpts> for ForkRecoveryOpts {
    fn from(opts: FfiForkRecoveryOpts) -> Self {
        Self {
            enable_recovery_requests: opts.enable_recovery_requests.into(),
            groups_to_request_recovery: opts.groups_to_request_recovery,
            disable_recovery_responses: opts.disable_recovery_responses.unwrap_or(false),
            worker_interval_ns: opts.worker_interval_ns,
        }
    }
}
