use crate::worker::FfiWorkerKind;
use xmtp_mls::worker::WorkerConfig;

/// A single per-worker interval override.
#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiWorkerIntervalOverride {
    pub kind: FfiWorkerKind,
    pub interval_ns: u64,
}

/// A single per-worker jitter override (nanoseconds).
#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiWorkerJitterOverride {
    pub kind: FfiWorkerKind,
    pub jitter_ns: u64,
}

/// Tuning for the background worker scheduler. All fields optional; the empty
/// record preserves default behavior (all workers enabled, const intervals,
/// no jitter).
#[derive(uniffi::Record, Debug, Clone, Default)]
pub struct FfiWorkerConfig {
    /// Global default interval for all workers, in nanoseconds.
    pub default_interval_ns: Option<u64>,
    /// Per-worker interval overrides (nanoseconds).
    pub worker_intervals_ns: Vec<FfiWorkerIntervalOverride>,
    /// Per-worker jitter overrides (nanoseconds).
    pub worker_jitters_ns: Vec<FfiWorkerJitterOverride>,
    /// Workers to disable. Anything not listed stays enabled.
    pub disabled_workers: Vec<FfiWorkerKind>,
}

impl From<FfiWorkerConfig> for WorkerConfig {
    fn from(o: FfiWorkerConfig) -> Self {
        let mut cfg = WorkerConfig {
            default_interval_ns: o.default_interval_ns,
            ..Default::default()
        };
        for ov in o.worker_intervals_ns {
            cfg.interval_overrides
                .insert(ov.kind.into(), ov.interval_ns);
        }
        for ov in o.worker_jitters_ns {
            cfg.jitter_overrides.insert(ov.kind.into(), ov.jitter_ns);
        }
        for k in o.disabled_workers {
            cfg.enabled.insert(k.into(), false);
        }
        cfg
    }
}
