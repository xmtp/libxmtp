//! Per-round deltas from structured counters in each child process.
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Default, PartialEq, Eq, Serialize)]
pub(crate) struct RoundCounters {
    pub own_commit_epoch_conflicts: u64,
    pub welcome_retries: u64,
    pub welcome_retries_by_cause: BTreeMap<String, u64>,
    pub maybe_committed: u64,
}

#[derive(Default)]
struct ProcessCounters {
    process_id: u64,
    own_commit_epoch_conflicts: u64,
    welcome_retries: BTreeMap<String, u64>,
    maybe_committed: u64,
}

impl ProcessCounters {
    fn from_snapshot(snapshot: &Value) -> Self {
        let contention = &snapshot["contention"];
        Self {
            process_id: snapshot["process_id"].as_u64().unwrap_or_default(),
            own_commit_epoch_conflicts: contention["own_commit_epoch_conflicts"]
                .as_u64()
                .unwrap_or_default(),
            welcome_retries: contention["welcome_retries"]
                .as_object()
                .into_iter()
                .flatten()
                .filter_map(|(cause, count)| count.as_u64().map(|count| (cause.clone(), count)))
                .collect(),
            maybe_committed: snapshot["disk"]["maybe_committed"]
                .as_u64()
                .unwrap_or_default(),
        }
    }
}

#[derive(Default)]
pub(crate) struct Counters {
    previous: BTreeMap<usize, ProcessCounters>,
}

impl Counters {
    /// Observe once before the first round to exclude setup activity.
    /// A new process starts from zero; its first observation counts in full.
    pub fn observe(&mut self, snapshots: &[(usize, Value)]) -> RoundCounters {
        let mut round = RoundCounters::default();
        for (slot, snapshot) in snapshots {
            let current = ProcessCounters::from_snapshot(snapshot);
            let old = self.previous.remove(slot).unwrap_or_default();
            let previous = if current.process_id == old.process_id {
                old
            } else {
                ProcessCounters::default()
            };
            round.own_commit_epoch_conflicts = round.own_commit_epoch_conflicts.saturating_add(
                current
                    .own_commit_epoch_conflicts
                    .saturating_sub(previous.own_commit_epoch_conflicts),
            );
            round.maybe_committed = round.maybe_committed.saturating_add(
                current
                    .maybe_committed
                    .saturating_sub(previous.maybe_committed),
            );
            for (cause, count) in &current.welcome_retries {
                let delta = count.saturating_sub(
                    previous
                        .welcome_retries
                        .get(cause)
                        .copied()
                        .unwrap_or_default(),
                );
                round.welcome_retries = round.welcome_retries.saturating_add(delta);
                if delta > 0 {
                    let total = round
                        .welcome_retries_by_cause
                        .entry(cause.clone())
                        .or_default();
                    *total = total.saturating_add(delta);
                }
            }
            self.previous.insert(*slot, current);
        }
        round
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn snapshot(pid: u64, conflicts: u64, retries: u64, maybe_committed: u64) -> Value {
        json!({
            "process_id": pid,
            "contention": {
                "own_commit_epoch_conflicts": conflicts,
                "welcome_retries": {"missing_dependency": retries},
            },
            "disk": {"maybe_committed": maybe_committed},
        })
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn counts_deltas_by_cause_across_installations() {
        let mut counters = Counters::default();
        counters.observe(&[(0, snapshot(100, 4, 2, 3)), (1, snapshot(101, 2, 3, 1))]);
        let round = counters.observe(&[(0, snapshot(100, 6, 4, 4)), (1, snapshot(101, 3, 6, 1))]);
        assert_eq!(round.own_commit_epoch_conflicts, 3);
        assert_eq!(round.welcome_retries, 5);
        assert_eq!(round.welcome_retries_by_cause["missing_dependency"], 5);
        assert_eq!(round.maybe_committed, 1);
        assert_eq!(
            counters.observe(&[(0, snapshot(100, 6, 4, 4)), (1, snapshot(101, 3, 6, 1))]),
            RoundCounters::default()
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn process_restart_preserves_the_new_process_counts() {
        let mut counters = Counters::default();
        counters.observe(&[(0, snapshot(100, 40, 20, 30))]);
        let restarted = counters.observe(&[(0, snapshot(200, 2, 3, 4))]);
        assert_eq!(restarted.own_commit_epoch_conflicts, 2);
        assert_eq!(restarted.welcome_retries, 3);
        assert_eq!(restarted.welcome_retries_by_cause["missing_dependency"], 3);
        assert_eq!(restarted.maybe_committed, 4);
        let next = counters.observe(&[(0, snapshot(200, 3, 5, 4))]);
        assert_eq!(next.own_commit_epoch_conflicts, 1);
        assert_eq!(next.welcome_retries, 2);
        assert_eq!(next.maybe_committed, 0);
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn missing_process_does_not_reset_its_baseline() {
        let mut counters = Counters::default();
        counters.observe(&[(0, snapshot(100, 4, 2, 3))]);
        assert_eq!(counters.observe(&[]), RoundCounters::default());
        let round = counters.observe(&[(0, snapshot(100, 6, 4, 4))]);
        assert_eq!(round.own_commit_epoch_conflicts, 2);
        assert_eq!(round.welcome_retries, 2);
        assert_eq!(round.maybe_committed, 1);
    }
}
