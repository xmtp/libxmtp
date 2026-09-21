use std::collections::{BTreeMap, BTreeSet};

use xmtp_mls::diagnostics::{BarrierCause, BarrierTopicSnapshot, RetryBudgets};
use xmtp_proto::types::{Topic, TopicKind};

use super::{Finding, InstanceSnapshot, Verdict};

const STALL_ESCALATION_ROUNDS: u64 = 3;

#[derive(Default)]
pub(super) struct CheckpointTracker {
    pending: BTreeMap<(String, String), Pending>,
}

struct Pending {
    round: u64,
    repeats: u64,
    cause: String,
    last_progress_ms: u64,
    received: u64,
    processed: u64,
    unresolved: BTreeSet<u64>,
}

pub(super) fn complete(topic: &BarrierTopicSnapshot) -> bool {
    let welcome = match hex::decode(&topic.topic) {
        Ok(bytes) => {
            matches!(Topic::parse(&bytes), Ok(parsed) if parsed.kind() == TopicKind::WelcomeMessagesV1)
        }
        Err(_) => false,
    };
    (topic.inactive && topic.cause.is_none())
        || topic.target.is_some_and(|target| {
            topic.cause.is_none()
                && topic.received >= target
                && (welcome || topic.processed >= target)
                && topic.unresolved_welcomes.iter().all(|id| *id > target)
        })
}

pub(super) fn instance_complete(instance: &InstanceSnapshot) -> bool {
    instance.checkpoint.failure.is_none() && instance.checkpoint.topics.iter().all(complete)
}

impl CheckpointTracker {
    pub(super) fn evaluate(
        &mut self,
        round: u64,
        elapsed_ms: u64,
        instances: &[InstanceSnapshot],
        budgets: &RetryBudgets,
        findings: &mut Vec<Finding>,
    ) {
        let mut present = BTreeSet::new();
        for instance in instances {
            let mut incomplete = false;
            for topic in &instance.checkpoint.topics {
                if complete(topic) {
                    continue;
                }
                incomplete = true;
                let key = (instance.installation_id.clone(), topic.topic.clone());
                if !present.insert(key.clone()) {
                    continue;
                }
                let cause = format!("{:?}", topic.cause);
                let unresolved: BTreeSet<_> = topic.unresolved_welcomes.iter().copied().collect();
                let pending = self.pending.entry(key).or_insert_with(|| Pending {
                    round,
                    repeats: 0,
                    cause: cause.clone(),
                    last_progress_ms: elapsed_ms,
                    received: topic.received,
                    processed: topic.processed,
                    unresolved: unresolved.clone(),
                });
                let receipt_progress = matches!(
                    topic.cause,
                    Some(BarrierCause::ReceiptPending | BarrierCause::TargetPending)
                ) && topic.received > pending.received;
                let progress = receipt_progress
                    || topic.processed > pending.processed
                    || !pending.unresolved.is_subset(&unresolved);
                if progress || pending.cause != cause {
                    pending.last_progress_ms = elapsed_ms;
                }
                if pending.round != round || pending.repeats == 0 {
                    pending.repeats =
                        if pending.round.saturating_add(1) == round && pending.cause == cause {
                            pending.repeats.saturating_add(1)
                        } else {
                            1
                        };
                }
                pending.round = round;
                pending.cause = cause.clone();
                pending.received = topic.received;
                pending.processed = topic.processed;
                pending.unresolved = unresolved;
                let budget_ms = match &topic.cause {
                    Some(BarrierCause::Blocked { .. }) => budgets.blocked_welcome_ms,
                    Some(
                        BarrierCause::Receiver {
                            retryable: false, ..
                        }
                        | BarrierCause::Storage {
                            retryable: false, ..
                        },
                    ) => budgets.permanent_receiver_ms,
                    _ => budgets.barrier_ms,
                };
                let idle_ms = elapsed_ms.saturating_sub(pending.last_progress_ms);
                let verdict = if matches!(topic.cause, Some(BarrierCause::InvalidTopic)) {
                    Verdict::Harness
                } else if idle_ms > budget_ms && !progress {
                    Verdict::Brick
                } else {
                    Verdict::Stall
                };
                findings.push(Finding {
                    verdict,
                    instance: Some(instance.instance),
                    group_id: None,
                    topic: Some(topic.topic.clone()),
                    detail: format!(
                        "{cause}: H={:?} F={} P={} unresolved={:?}; no progress for {idle_ms}ms, budget {budget_ms}ms",
                        topic.target, topic.received, topic.processed, topic.unresolved_welcomes
                    ),
                    repeated_rounds: pending.repeats,
                    escalated: pending.repeats >= STALL_ESCALATION_ROUNDS,
                });
            }
            if !incomplete && instance.checkpoint.failure.is_some() {
                findings.push(Finding {
                    verdict: Verdict::Harness,
                    instance: Some(instance.instance),
                    group_id: None,
                    topic: None,
                    detail: "Checkpoint failed without an incomplete topic obligation".to_owned(),
                    repeated_rounds: 0,
                    escalated: false,
                });
            }
        }
        self.pending.retain(|key, _| present.contains(key));
    }
}
