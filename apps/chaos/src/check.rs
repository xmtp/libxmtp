mod checkpoint;
mod membership;
mod rollcall;
mod state;
mod verdict;

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use xmtp_mls::diagnostics::{CheckpointSnapshot, GroupSnapshot, RetryBudgets};

pub use membership::MembershipRecord;
pub use verdict::{CheckResult, Finding, Verdict};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstanceSnapshot {
    pub instance: usize,
    pub inbox_id: String,
    pub installation_id: String,
    pub stream_owner: bool,
    pub groups: Vec<GroupSnapshot>,
    pub checkpoint: CheckpointSnapshot,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RollCall {
    pub group_id: String,
    pub token: String,
    pub sender_installation: String,
    /// Membership when this token was sent. Later joiners do not owe old messages.
    pub expected_installations: BTreeSet<String>,
    pub expected_stream_installations: BTreeSet<String>,
    pub sync_received: BTreeSet<String>,
    pub stream_received: BTreeSet<String>,
    pub elapsed_ms: u64,
}

pub struct Oracle {
    strict: bool,
    budgets: RetryBudgets,
    checkpoints: checkpoint::CheckpointTracker,
    membership: membership::MembershipLedger,
}

impl Oracle {
    pub fn new(strict: bool, budgets: RetryBudgets) -> Self {
        Self {
            strict,
            budgets,
            checkpoints: checkpoint::CheckpointTracker::default(),
            membership: membership::MembershipLedger::default(),
        }
    }

    pub fn evaluate(
        &mut self,
        round: u64,
        elapsed_ms: u64,
        instances: &[InstanceSnapshot],
        rollcall: &[RollCall],
    ) -> CheckResult {
        let mut findings = Vec::new();
        self.checkpoints
            .evaluate(round, elapsed_ms, instances, &self.budgets, &mut findings);
        self.membership.observe(instances, &mut findings);
        state::evaluate(instances, &self.membership, &mut findings);
        rollcall::evaluate(
            instances,
            &self.membership,
            rollcall,
            &self.budgets,
            &mut findings,
        );
        CheckResult::new(findings, self.membership.records(), self.strict)
    }
}

#[cfg(test)]
mod tests;
