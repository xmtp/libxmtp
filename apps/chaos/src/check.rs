mod checkpoint;
mod membership;
mod rollcall;
mod state;
mod verdict;

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use xmtp_mls::diagnostics::{CheckpointSnapshot, GroupSnapshot, RetryBudgets};

pub use membership::MembershipRecord;
pub use verdict::{CheckResult, Finding, Verdict};

pub(crate) fn checkpoint_complete(checkpoint: &CheckpointSnapshot) -> bool {
    checkpoint.failure.is_none() && checkpoint.topics.iter().all(checkpoint::complete)
}

pub(crate) fn group_checkpoint_complete(instance: &InstanceSnapshot, group_id: &str) -> bool {
    checkpoint::group_complete(instance, group_id)
}

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
    pending_calls: BTreeMap<String, RollCall>,
    last_observed_ms: Option<u64>,
    deferred_receivers: BTreeMap<(String, String), Option<u64>>,
    token_generations: BTreeMap<(String, String), u64>,
}

// One batch per group fits the maximum population and group count with room to spare.
const MAX_PENDING_ROLLCALLS: usize = 256;

impl Oracle {
    pub fn new(strict: bool, budgets: RetryBudgets) -> Self {
        Self {
            strict,
            budgets,
            checkpoints: checkpoint::CheckpointTracker::default(),
            membership: membership::MembershipLedger::default(),
            pending_calls: BTreeMap::new(),
            last_observed_ms: None,
            deferred_receivers: BTreeMap::new(),
            token_generations: BTreeMap::new(),
        }
    }

    pub fn pending_rollcalls(&self) -> Vec<RollCall> {
        self.pending_calls.values().cloned().collect()
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
        if let Some(previous) = self.last_observed_ms {
            for call in self.pending_calls.values_mut() {
                call.elapsed_ms = call
                    .elapsed_ms
                    .saturating_add(elapsed_ms.saturating_sub(previous));
            }
        }
        self.last_observed_ms = Some(elapsed_ms);
        for call in rollcall {
            self.pending_calls
                .entry(call.token.clone())
                .and_modify(|pending| {
                    pending
                        .sync_received
                        .extend(call.sync_received.iter().cloned());
                    pending
                        .stream_received
                        .extend(call.stream_received.iter().cloned());
                    pending.elapsed_ms = pending.elapsed_ms.max(call.elapsed_ms);
                })
                .or_insert_with(|| call.clone());
        }
        if self.pending_calls.len() > MAX_PENDING_ROLLCALLS {
            findings.push(Finding {
                verdict: Verdict::Harness, instance: None, group_id: None, topic: None,
                detail: format!("Pending roll-call limit exceeded: {} > {MAX_PENDING_ROLLCALLS}; obligations were preserved", self.pending_calls.len()),
                repeated_rounds: 0, escalated: false,
            });
        }
        let calls = self.pending_rollcalls();
        let mut resumed_elapsed = BTreeMap::new();
        let mut ended_memberships = BTreeSet::new();
        for call in &calls {
            for instance in instances.iter().filter(|instance| {
                call.expected_installations
                    .contains(&instance.installation_id)
            }) {
                let key = (call.token.clone(), instance.installation_id.clone());
                if let Some(generation) = self
                    .membership
                    .generation(&call.group_id, &instance.installation_id)
                {
                    let original = *self
                        .token_generations
                        .entry(key.clone())
                        .or_insert(generation);
                    if generation != original {
                        ended_memberships.insert(key.clone());
                    }
                }
                if !checkpoint::group_complete(instance, &call.group_id) {
                    self.deferred_receivers.insert(key, None);
                } else if let Some(resumed) = self.deferred_receivers.get_mut(&key) {
                    let start = *resumed.get_or_insert(elapsed_ms);
                    resumed_elapsed.insert(key, elapsed_ms.saturating_sub(start));
                }
            }
        }
        rollcall::evaluate(
            instances,
            &self.membership,
            &calls,
            &self.budgets,
            &resumed_elapsed,
            &ended_memberships,
            &mut findings,
        );
        self.pending_calls.retain(|_, call| {
            call.expected_installations.iter().any(|installation| {
                self.membership.owed(&call.group_id, installation)
                    && !ended_memberships.contains(&(call.token.clone(), installation.clone()))
                    && (!call.sync_received.contains(installation)
                        || (call.expected_stream_installations.contains(installation)
                            && !call.stream_received.contains(installation)))
            })
        });
        self.deferred_receivers
            .retain(|(token, _), _| self.pending_calls.contains_key(token));
        self.token_generations
            .retain(|(token, _), _| self.pending_calls.contains_key(token));
        CheckResult::new(findings, self.membership.records(), self.strict)
    }
}

#[cfg(test)]
mod tests;
