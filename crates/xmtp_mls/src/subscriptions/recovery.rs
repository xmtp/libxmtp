//! A recovery budget belongs to one public stream, never to the shared receiver.

use super::{incoming::IncomingError, local_delivery::LocalDeliveryError};
use std::sync::Arc;
use xmtp_common::{
    RetryableError,
    time::{Duration, Instant},
};

const MAX_FAILURES: u64 = 10;
const MAX_OUTAGE: Duration = Duration::from_secs(10 * 60);
pub(crate) const HEALTHY_PERIOD: Duration = Duration::from_secs(30);
pub(crate) const RECOVERY_POLL: Duration = Duration::from_secs(1);

/// Monotonic failure count and continuous accepted-registration interval.
/// A socket open alone does not start the healthy interval.
#[derive(Clone, Debug, Default)]
pub(crate) struct RecoverySnapshot {
    pub(crate) terminal: Option<RecoveryFailure>,
    pub(crate) idle: bool,
    pub(crate) failures: u64,
    pub(crate) healthy_since: Option<Instant>,
    pub(crate) outage_since: Option<Instant>,
    pub(crate) healthy_generation: u64,
    pub(crate) healthy_failure_baseline: u64,
    pub(crate) error: Option<Arc<IncomingError>>,
}

#[derive(Clone, Debug)]
pub(crate) enum RecoveryFailure {
    Exhausted {
        attempts: u32,
        source: Option<Arc<IncomingError>>,
    },
    Terminal(Arc<IncomingError>),
}

impl RecoveryFailure {
    pub(crate) fn error(&self) -> LocalDeliveryError {
        match self {
            Self::Exhausted { attempts, source } => LocalDeliveryError::NetworkRecoveryExhausted {
                attempts: *attempts,
                source: source.clone(),
            },
            Self::Terminal(error) => LocalDeliveryError::NetworkFailure(error.clone()),
        }
    }
}

pub(crate) struct RecoveryBudget {
    baseline: u64,
    opened_at: Instant,
    healthy_generation: u64,
    outage_since: Option<Instant>,
}

impl RecoveryBudget {
    pub(crate) fn new(snapshot: &RecoverySnapshot, now: Instant) -> Self {
        Self {
            baseline: snapshot.failures,
            opened_at: now,
            healthy_generation: snapshot.healthy_generation,
            outage_since: Some(now),
        }
    }

    pub(crate) fn check(
        &mut self,
        snapshot: &RecoverySnapshot,
        now: Instant,
    ) -> Result<(), RecoveryFailure> {
        if let Some(failure) = &snapshot.terminal {
            return Err(failure.clone());
        }
        if snapshot.idle {
            self.baseline = snapshot.failures;
            self.outage_since = None;
            return Ok(());
        }
        // The controller records healthy intervals even when the application
        // holds an item and does not poll this stream.
        if snapshot.healthy_generation > self.healthy_generation {
            self.healthy_generation = snapshot.healthy_generation;
            self.baseline = self.baseline.max(snapshot.healthy_failure_baseline);
            self.outage_since = None;
        }
        let failures = snapshot.failures.saturating_sub(self.baseline);
        if failures > 0
            && let Some(error) = &snapshot.error
            && !error.is_retryable()
        {
            tracing::warn!(
                code = error.code(),
                "stream network recovery stopped after a terminal error"
            );
            return Err(RecoveryFailure::Terminal(error.clone()));
        }
        // The transport keepalive detects a silent wire. All registrations must
        // remain accepted for this interval; application messages are not required.
        if snapshot
            .healthy_since
            .is_some_and(|since| now.saturating_duration_since(since) >= HEALTHY_PERIOD)
        {
            self.baseline = snapshot.failures;
            self.outage_since = None;
            return Ok(());
        }
        let started = self
            .outage_since
            .get_or_insert_with(|| snapshot.outage_since.unwrap_or(now).max(self.opened_at));
        if failures >= MAX_FAILURES || now.saturating_duration_since(*started) >= MAX_OUTAGE {
            tracing::warn!(
                attempts = failures,
                elapsed_ms = now.saturating_duration_since(*started).as_millis() as u64,
                cause = snapshot.error.as_ref().map(|error| error.code()),
                "stream network recovery budget exhausted"
            );
            return Err(RecoveryFailure::Exhausted {
                attempts: failures.min(u32::MAX as u64) as u32,
                source: snapshot.error.clone(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // verifies: PROC-038
    #[xmtp_common::test(unwrap_try = true)]
    fn one_stream_survives_twelve_separate_outages() {
        let start = Instant::now();
        let mut now = start;
        let mut snapshot = RecoverySnapshot::default();
        let mut budget = RecoveryBudget::new(&snapshot, now);
        for _ in 0..12 {
            snapshot.failures += MAX_FAILURES - 1;
            snapshot.healthy_since = None;
            budget.check(&snapshot, now)?;
            snapshot.healthy_since = Some(now);
            now += HEALTHY_PERIOD;
            budget.check(&snapshot, now)?;
        }
        assert!(snapshot.failures > MAX_FAILURES);
    }

    // verifies: PROC-038
    #[xmtp_common::test(unwrap_try = true)]
    fn a_paused_consumer_keeps_the_observed_outage_deadline() {
        let now = Instant::now();
        let mut snapshot = RecoverySnapshot {
            healthy_since: Some(now),
            ..Default::default()
        };
        let mut budget = RecoveryBudget::new(&snapshot, now);
        budget.check(&snapshot, now + HEALTHY_PERIOD)?;
        snapshot.healthy_since = None;
        snapshot.outage_since = Some(now + HEALTHY_PERIOD);
        snapshot.failures = 1;
        assert!(
            budget
                .check(&snapshot, now + HEALTHY_PERIOD + MAX_OUTAGE)
                .is_err()
        );
    }

    // verifies: PROC-038
    #[xmtp_common::test(unwrap_try = true)]
    fn healthy_reset_survives_a_pause_that_also_spans_the_next_outage() {
        let now = Instant::now();
        let mut snapshot = RecoverySnapshot::default();
        let mut budget = RecoveryBudget::new(&snapshot, now);
        snapshot.failures = MAX_FAILURES - 1;
        budget.check(&snapshot, now)?;
        // The controller observed a healthy interval while the app held an
        // item. A later outage has already removed healthy_since.
        snapshot.healthy_generation = 1;
        snapshot.healthy_failure_baseline = snapshot.failures;
        snapshot.failures += 1;
        snapshot.outage_since = Some(now + MAX_OUTAGE * 2);
        budget.check(&snapshot, now + MAX_OUTAGE * 2 + Duration::from_secs(1))?;
        snapshot.failures += MAX_FAILURES - 1;
        assert!(
            budget
                .check(&snapshot, now + MAX_OUTAGE * 2 + Duration::from_secs(2))
                .is_err()
        );
    }

    // verifies: PROC-038, PROC-039
    #[xmtp_common::test(unwrap_try = true)]
    fn an_exhausted_episode_stays_terminal_after_health_returns() {
        let now = Instant::now();
        let mut snapshot = RecoverySnapshot::default();
        let mut controller_budget = RecoveryBudget::new(&snapshot, now);
        let mut consumer_budget = RecoveryBudget::new(&snapshot, now);
        snapshot.failures = MAX_FAILURES;
        snapshot.terminal = Some(controller_budget.check(&snapshot, now).unwrap_err());
        snapshot.healthy_generation = 1;
        snapshot.healthy_failure_baseline = MAX_FAILURES;
        snapshot.healthy_since = Some(now);
        assert!(
            consumer_budget
                .check(&snapshot, now + HEALTHY_PERIOD)
                .is_err()
        );
        // A new lease copies the shared receiver counters, not the old lease's
        // terminal outcome. The old stream cannot poison its replacement.
        snapshot.terminal = None;
        let mut replacement = RecoveryBudget::new(&snapshot, now + HEALTHY_PERIOD);
        replacement.check(&snapshot, now + HEALTHY_PERIOD)?;
    }

    // verifies: PROC-039
    #[xmtp_common::test(unwrap_try = true)]
    fn new_stream_has_fresh_budget_during_the_same_outage() {
        let now = Instant::now();
        let mut snapshot = RecoverySnapshot::default();
        let mut old = RecoveryBudget::new(&snapshot, now);
        snapshot.failures = MAX_FAILURES;
        assert!(old.check(&snapshot, now).is_err());
        let mut replacement = RecoveryBudget::new(&snapshot, now);
        replacement.check(&snapshot, now)?;
        snapshot.failures += MAX_FAILURES - 1;
        replacement.check(&snapshot, now)?;
        snapshot.failures += 1;
        assert!(replacement.check(&snapshot, now).is_err());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn hung_open_expires_without_an_attempt_event() {
        let now = Instant::now();
        let snapshot = RecoverySnapshot::default();
        let mut budget = RecoveryBudget::new(&snapshot, now);
        budget.check(&snapshot, now + MAX_OUTAGE - Duration::from_millis(1))?;
        assert!(budget.check(&snapshot, now + MAX_OUTAGE).is_err());
    }

    // verifies: PROC-038
    #[xmtp_common::test(unwrap_try = true)]
    fn empty_interest_does_not_expire_during_an_unrelated_outage() {
        let now = Instant::now();
        let snapshot = RecoverySnapshot {
            idle: true,
            ..Default::default()
        };
        let mut budget = RecoveryBudget::new(&snapshot, now);
        budget.check(&snapshot, now + MAX_OUTAGE * 2)?;
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn flapping_does_not_reset_but_quiet_registered_connection_does() {
        let now = Instant::now();
        let mut snapshot = RecoverySnapshot::default();
        let mut budget = RecoveryBudget::new(&snapshot, now);
        for failures in 1..MAX_FAILURES {
            snapshot.failures = failures;
            snapshot.healthy_since = Some(now);
            budget.check(&snapshot, now + HEALTHY_PERIOD - Duration::from_millis(1))?;
        }
        budget.check(&snapshot, now + HEALTHY_PERIOD)?;
        snapshot.healthy_since = None;
        snapshot.failures += MAX_FAILURES - 1;
        budget.check(&snapshot, now + HEALTHY_PERIOD)?;
        snapshot.failures += 1;
        assert!(budget.check(&snapshot, now + HEALTHY_PERIOD).is_err());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn consumers_do_not_share_an_outage_deadline() {
        let now = Instant::now();
        let snapshot = RecoverySnapshot::default();
        let mut first = RecoveryBudget::new(&snapshot, now);
        let mut second = RecoveryBudget::new(&snapshot, now + Duration::from_secs(60));
        assert!(first.check(&snapshot, now + MAX_OUTAGE).is_err());
        second.check(&snapshot, now + MAX_OUTAGE)?;
    }
}
