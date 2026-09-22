use std::collections::BTreeSet;

use xmtp_db::group::GroupMembershipState;
use xmtp_mls::diagnostics::{
    BarrierCause, BarrierFailure, BarrierTopicSnapshot, CheckpointSnapshot, CommitSnapshot,
    GroupSnapshot, MemberSnapshot, RetryBudgets,
};

use super::{InstanceSnapshot, Oracle, RollCall, Verdict};

fn group() -> GroupSnapshot {
    GroupSnapshot {
        group_id: "07070707070707070707070707070707".into(),
        epoch: 2,
        epoch_authenticator: "state".into(),
        members: ["a", "b"]
            .into_iter()
            .map(|id| MemberSnapshot {
                inbox_id: format!("inbox-{id}"),
                installation_id: id.into(),
            })
            .collect(),
        metadata: "metadata".into(),
        membership_state: GroupMembershipState::Allowed,
        active: true,
        maybe_forked: false,
        is_commit_log_forked: Some(false),
        cursor: 10,
        commits: Vec::new(),
        omitted_commit_count: 0,
    }
}

fn population() -> Vec<InstanceSnapshot> {
    ["a", "b"]
        .into_iter()
        .enumerate()
        .map(|(instance, id)| InstanceSnapshot {
            instance,
            inbox_id: format!("inbox-{id}"),
            installation_id: id.into(),
            stream_owner: true,
            groups: vec![group()],
            checkpoint: CheckpointSnapshot {
                group_ids: vec!["07070707070707070707070707070707".into()],
                topics: vec![BarrierTopicSnapshot {
                    topic: hex::encode(xmtp_proto::types::Topic::new_group_message([7; 16])),
                    target: Some(10),
                    received: 10,
                    processed: 10,
                    unresolved_welcomes: Vec::new(),
                    inactive: false,
                    cause: None,
                }],
                failure: None,
            },
        })
        .collect()
}

fn oracle() -> Oracle {
    Oracle::new(false, RetryBudgets::default())
}

fn missing_stream_call() -> RollCall {
    RollCall {
        group_id: group().group_id,
        token: "deferred-token".into(),
        sender_installation: "a".into(),
        expected_installations: BTreeSet::from(["a".into(), "b".into()]),
        expected_stream_installations: BTreeSet::from(["a".into(), "b".into()]),
        sync_received: BTreeSet::from(["a".into(), "b".into()]),
        stream_received: BTreeSet::from(["a".into()]),
        elapsed_ms: RetryBudgets::default().barrier_ms + 1,
    }
}

fn block_topic(instance: &mut InstanceSnapshot, unrelated: bool) {
    let mut pending = instance.checkpoint.topics[0].clone();
    pending.processed = 9;
    pending.cause = Some(BarrierCause::Blocked {
        code: "unsupported_version".into(),
    });
    if unrelated {
        pending.topic = hex::encode(xmtp_proto::types::Topic::new_group_message([8; 16]));
        instance.checkpoint.topics.push(pending);
    } else {
        instance.checkpoint.topics[0] = pending;
    }
    instance.checkpoint.failure = Some(BarrierFailure::Blocked);
}

#[xmtp_common::test(unwrap_try = true)]
async fn unrelated_blocker_does_not_hide_group_divergence() {
    let mut instances = population();
    block_topic(&mut instances[1], true);
    instances[1].groups[0].epoch_authenticator = "diverged".into();
    let result = oracle().evaluate(1, 0, &instances, &[]);
    assert_eq!(result.verdict, Verdict::Fork);
    assert!(result.violation);
}

#[xmtp_common::test(unwrap_try = true)]
async fn unrelated_blocker_does_not_hide_missing_stream_delivery() {
    let mut instances = population();
    block_topic(&mut instances[1], true);
    let result = oracle().evaluate(1, 0, &instances, &[missing_stream_call()]);
    assert_eq!(result.verdict, Verdict::Brick);
    assert!(
        result
            .findings
            .iter()
            .any(|finding| finding.detail.contains("missing by stream"))
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn deferred_rollcall_survives_rounds_and_gets_a_budget_after_unblocking() {
    let mut instances = population();
    block_topic(&mut instances[1], false);
    let mut oracle = oracle();
    let first = oracle.evaluate(1, 0, &instances, &[missing_stream_call()]);
    assert_eq!(first.verdict, Verdict::Stall);
    assert!(!first.violation);
    assert_eq!(oracle.pending_rollcalls().len(), 1);
    let instances = population();
    let resumed = oracle.evaluate(2, 1, &instances, &[]);
    assert_eq!(resumed.verdict, Verdict::Stall);
    assert!(!resumed.violation);
    let overdue = oracle.evaluate(3, RetryBudgets::default().barrier_ms + 2, &instances, &[]);
    assert_eq!(overdue.verdict, Verdict::Brick);
    assert!(
        overdue
            .findings
            .iter()
            .any(|finding| finding.detail.contains("deferred-token"))
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn deferred_rollcall_merges_later_receipts_and_clears_the_obligation() {
    let mut instances = population();
    block_topic(&mut instances[1], false);
    let mut oracle = oracle();
    let mut call = missing_stream_call();
    oracle.evaluate(1, 0, &instances, &[call.clone()]);
    call.stream_received.insert("b".into());
    assert_eq!(
        oracle.evaluate(2, 1, &population(), &[call]).verdict,
        Verdict::Pass
    );
    assert!(oracle.pending_rollcalls().is_empty());
}

#[xmtp_common::test(unwrap_try = true)]
async fn removal_and_readd_end_deferred_token_obligations_but_new_tokens_are_owed() {
    let mut instances = population();
    block_topic(&mut instances[1], false);
    let mut oracle = oracle();
    oracle.evaluate(1, 0, &instances, &[missing_stream_call()]);
    let mut instances = population();
    instances[1].groups[0]
        .commits
        .push(commit(11, "removed-state", true));
    assert_eq!(
        oracle.evaluate(2, 1, &instances, &[]).verdict,
        Verdict::Pass
    );
    assert!(oracle.pending_rollcalls().is_empty());
    let mut fresh = missing_stream_call();
    fresh.token = "new-membership-token".into();
    assert_eq!(
        oracle.evaluate(3, 2, &instances, &[fresh]).verdict,
        Verdict::Brick
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn pending_receiver_does_not_hide_missing_delivery_to_a_complete_receiver() {
    let mut instances = population();
    block_topic(&mut instances[1], false);
    let mut call = missing_stream_call();
    call.stream_received.clear();
    let result = oracle().evaluate(1, 0, &instances, &[call]);
    assert_eq!(result.verdict, Verdict::Brick);
    assert!(
        result
            .findings
            .iter()
            .any(|finding| { finding.verdict == Verdict::Brick && finding.instance == Some(0) })
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn different_group_prefixes_with_unrelated_blocking_are_not_a_fork() {
    let mut instances = population();
    block_topic(&mut instances[1], true);
    instances[1].groups[0].cursor = 11;
    instances[1].groups[0].epoch_authenticator = "later-state".into();
    instances[1].checkpoint.topics[0].target = Some(11);
    instances[1].checkpoint.topics[0].received = 11;
    instances[1].checkpoint.topics[0].processed = 11;
    let result = oracle().evaluate(1, 0, &instances, &[]);
    assert_eq!(result.verdict, Verdict::Stall);
    assert!(!result.violation);
}

#[xmtp_common::test(unwrap_try = true)]
async fn pending_rollcall_cap_reports_harness_without_discarding_evidence() {
    let calls: Vec<_> = (0..=super::MAX_PENDING_ROLLCALLS)
        .map(|index| {
            let mut call = missing_stream_call();
            call.token = format!("token-{index}");
            call.elapsed_ms = 0;
            call
        })
        .collect();
    let mut oracle = oracle();
    assert_eq!(
        oracle.evaluate(1, 0, &population(), &calls).verdict,
        Verdict::Harness
    );
    assert_eq!(oracle.pending_rollcalls().len(), calls.len());
}

fn commit(sequence_id: i64, authenticator: &str, removed: bool) -> CommitSnapshot {
    CommitSnapshot {
        sequence_id,
        epoch: 2,
        authenticator: authenticator.into(),
        last_authenticator: "previous".into(),
        result: 1,
        commit_type: removed.then(|| "RemovedFromGroup".into()),
        removed_this_installation: removed,
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn disagreeing_checkpoint_is_fork() {
    let mut instances = population();
    instances[1].groups[0].epoch_authenticator = "other".into();
    let result = oracle().evaluate(1, 0, &instances, &[]);
    assert_eq!(result.verdict, Verdict::Fork);
    assert!(result.violation);
}

#[xmtp_common::test(unwrap_try = true)]
async fn missing_owed_installation_is_brick() {
    let mut instances = population();
    instances[1].groups.clear();
    let result = oracle().evaluate(1, 0, &instances, &[]);
    assert_eq!(result.verdict, Verdict::Brick);
    assert!(
        result
            .membership
            .iter()
            .any(|record| { record.installation_id == "b" && record.owes_membership })
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn restored_state_does_not_discharge_membership() {
    let mut instances = population();
    instances[1].groups[0].active = false;
    instances[1].groups[0].membership_state = GroupMembershipState::Restored;
    assert_eq!(
        oracle().evaluate(1, 0, &instances, &[]).verdict,
        Verdict::Brick
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn blocked_welcome_escalates_without_early_brick() {
    let mut instances = population();
    instances[1].groups.clear();
    instances[1].checkpoint.failure = Some(BarrierFailure::Blocked);
    let topic = &mut instances[1].checkpoint.topics[0];
    topic.processed = 9;
    topic.unresolved_welcomes = vec![10];
    topic.cause = Some(BarrierCause::Blocked {
        code: "unsupported_welcome".into(),
    });
    let budgets = RetryBudgets::default();
    let mut oracle = Oracle::new(false, budgets);
    for round in 1..=3 {
        let result = oracle.evaluate(round, round - 1, &instances, &[]);
        assert_eq!(result.verdict, Verdict::Stall);
        assert!(!result.violation);
        assert_eq!(result.findings[0].escalated, round == 3);
    }
    let result = oracle.evaluate(4, budgets.blocked_welcome_ms + 1, &instances, &[]);
    assert_eq!(result.verdict, Verdict::Brick);
}

#[xmtp_common::test(unwrap_try = true)]
async fn completed_welcome_uses_receipt_and_unresolved_ids_without_processed_prefix() {
    use xmtp_proto::types::{InstallationId, Topic};

    let mut instances = population();
    let welcome = &mut instances[0].checkpoint.topics[0];
    welcome.topic = hex::encode(Topic::new_welcome_message(InstallationId::from([7; 32])));
    welcome.processed = 0;
    // Work after H is outside this fixed checkpoint.
    welcome.unresolved_welcomes = vec![11];
    assert_eq!(
        oracle().evaluate(1, 0, &instances, &[]).verdict,
        Verdict::Pass
    );
    instances[0].checkpoint.topics[0]
        .unresolved_welcomes
        .push(10);
    assert_eq!(
        oracle().evaluate(1, 0, &instances, &[]).verdict,
        Verdict::Stall
    );
    instances[0].checkpoint.topics[0]
        .unresolved_welcomes
        .clear();
    instances[0].checkpoint.topics[0].topic = hex::encode(Topic::new_group_message([7; 16]));
    assert_eq!(
        oracle().evaluate(1, 0, &instances, &[]).verdict,
        Verdict::Stall
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn receipt_progress_resets_the_liveness_budget() {
    let mut instances = population();
    instances[0].checkpoint.topics[0].received = 7;
    instances[0].checkpoint.topics[0].processed = 6;
    instances[0].checkpoint.topics[0].cause = Some(BarrierCause::ReceiptPending);
    let budgets = RetryBudgets::default();
    let mut oracle = Oracle::new(false, budgets);
    oracle.evaluate(1, 0, &instances, &[]);
    instances[0].checkpoint.topics[0].received = 8;
    let result = oracle.evaluate(2, budgets.barrier_ms + 1, &instances, &[]);
    assert_eq!(result.verdict, Verdict::Stall);
}

#[xmtp_common::test(unwrap_try = true)]
async fn new_receipts_do_not_hide_a_stuck_processor() {
    let mut instances = population();
    instances[0].checkpoint.topics[0].processed = 6;
    instances[0].checkpoint.topics[0].cause = Some(BarrierCause::ProcessingPending);
    let budgets = RetryBudgets::default();
    let mut oracle = Oracle::new(false, budgets);
    oracle.evaluate(1, 0, &instances, &[]);
    instances[0].checkpoint.topics[0].received = 11;
    let result = oracle.evaluate(2, budgets.barrier_ms + 1, &instances, &[]);
    assert_eq!(result.verdict, Verdict::Brick);
}

#[xmtp_common::test(unwrap_try = true)]
async fn divergent_member_sets_never_prove_removal() {
    let mut instances = population();
    let mut oracle = oracle();
    assert_eq!(
        oracle.evaluate(1, 0, &instances, &[]).verdict,
        Verdict::Pass
    );
    instances[0].groups[0]
        .members
        .retain(|member| member.installation_id == "a");
    let result = oracle.evaluate(2, 1, &instances, &[]);
    assert_eq!(result.verdict, Verdict::Fork);
    assert!(
        result
            .membership
            .iter()
            .all(|record| record.owes_membership)
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn removal_and_readd_preserve_commit_evidence() {
    let mut instances = population();
    instances[0].groups[0]
        .commits
        .push(commit(8, "first", false));
    instances[1].groups[0]
        .commits
        .push(commit(8, "first", false));
    let mut oracle = oracle();
    oracle.evaluate(1, 0, &instances, &[]);
    instances[0].groups[0]
        .members
        .retain(|member| member.installation_id == "a");
    instances[1].groups[0].active = false;
    // A removal completes the barrier even if later group traffic is inaccessible.
    instances[1].checkpoint.topics[0].inactive = true;
    instances[1].checkpoint.topics[0].processed = 9;
    instances[1].groups[0]
        .commits
        .push(commit(9, "first", true));
    let result = oracle.evaluate(2, 1, &instances, &[]);
    assert_eq!(result.verdict, Verdict::Pass);
    let removed = result
        .membership
        .iter()
        .find(|record| record.installation_id == "b")
        .unwrap();
    assert!(!removed.owes_membership);
    let generation = removed.generation;
    instances[0].groups[0] = group();
    instances[1].groups[0] = group();
    // A reset SDK log must not erase an older disagreement that is discovered later.
    instances[0].groups[0]
        .commits
        .push(commit(8, "other", false));
    let result = oracle.evaluate(3, 2, &instances, &[]);
    assert_eq!(result.verdict, Verdict::Fork);
    let rejoined = result
        .membership
        .iter()
        .find(|record| record.installation_id == "b")
        .unwrap();
    assert!(rejoined.owes_membership);
    assert_eq!(rejoined.generation, generation + 1);
    assert!(
        rejoined
            .commits
            .iter()
            .any(|commit| commit.sequence_id == 8)
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn new_installation_requires_a_confirmed_leaf() {
    let mut instances = population();
    let mut new = instances[1].clone();
    new.instance = 2;
    new.installation_id = "b-second".into();
    new.groups.clear();
    instances.push(new);
    let mut oracle = oracle();
    assert_eq!(
        oracle.evaluate(1, 0, &instances, &[]).verdict,
        Verdict::Pass
    );
    for instance in &mut instances[..2] {
        instance.groups[0].members.push(MemberSnapshot {
            inbox_id: "inbox-b".into(),
            installation_id: "b-second".into(),
        });
    }
    assert_eq!(
        oracle.evaluate(2, 1, &instances, &[]).verdict,
        Verdict::Brick
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn strict_flags_are_violations_without_claiming_state_divergence() {
    let mut instances = population();
    instances[0].groups[0].maybe_forked = true;
    let normal = oracle().evaluate(1, 0, &instances, &[]);
    assert_eq!(normal.verdict, Verdict::Warn);
    assert!(!normal.violation);
    let strict = Oracle::new(true, RetryBudgets::default()).evaluate(1, 0, &instances, &[]);
    assert_eq!(strict.verdict, Verdict::Warn);
    assert!(strict.violation);
}

#[xmtp_common::test(unwrap_try = true)]
async fn shared_database_requires_only_the_stream_owner() {
    let mut instances = population();
    instances[1].stream_owner = false;
    let mut shared = instances[1].clone();
    shared.instance = 2;
    shared.stream_owner = true;
    instances.push(shared);
    let mut call = RollCall {
        group_id: "07070707070707070707070707070707".into(),
        token: "token".into(),
        sender_installation: "a".into(),
        expected_installations: BTreeSet::from(["a".into(), "b".into()]),
        expected_stream_installations: BTreeSet::from(["a".into(), "b".into()]),
        sync_received: BTreeSet::from(["a".into(), "b".into()]),
        stream_received: BTreeSet::from(["a".into(), "b".into()]),
        elapsed_ms: RetryBudgets::default().barrier_ms + 1,
    };
    let mut oracle = oracle();
    assert_eq!(
        oracle.evaluate(1, 0, &instances, &[call.clone()]).verdict,
        Verdict::Pass
    );
    call.stream_received.remove("b");
    let result = oracle.evaluate(2, 1, &instances, &[call]);
    assert_eq!(result.verdict, Verdict::Brick);
    assert_eq!(result.findings.len(), 1);
}

#[xmtp_common::test(unwrap_try = true)]
async fn installation_joined_after_rollcall_does_not_owe_earlier_tokens() {
    let mut instances = population();
    let mut new = instances[1].clone();
    new.instance = 2;
    new.installation_id = "b-second".into();
    instances.push(new);
    for instance in &mut instances {
        instance.groups[0].members.push(MemberSnapshot {
            inbox_id: "inbox-b".into(),
            installation_id: "b-second".into(),
        });
    }
    let mut call = RollCall {
        group_id: "07070707070707070707070707070707".into(),
        token: "token".into(),
        sender_installation: "a".into(),
        expected_installations: BTreeSet::from(["a".into(), "b".into()]),
        expected_stream_installations: BTreeSet::from(["a".into(), "b".into()]),
        sync_received: BTreeSet::from(["a".into(), "b".into()]),
        stream_received: BTreeSet::from(["a".into(), "b".into()]),
        elapsed_ms: RetryBudgets::default().barrier_ms + 1,
    };
    let mut oracle = oracle();
    assert_eq!(
        oracle.evaluate(1, 0, &instances, &[call.clone()]).verdict,
        Verdict::Pass
    );
    call.expected_installations.insert("b-second".into());
    call.expected_stream_installations.insert("b-second".into());
    assert_eq!(
        oracle.evaluate(2, 1, &instances, &[call]).verdict,
        Verdict::Brick
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn reports_bound_history_without_erasing_generation_or_fork_evidence() {
    let mut instances = population();
    instances[1].groups[0]
        .commits
        .push(commit(1, "original", false));
    for instance in &mut instances {
        instance.groups[0]
            .commits
            .extend((2..=131).map(|sequence| commit(sequence, "removed", true)));
    }
    let mut oracle = oracle();
    let result = oracle.evaluate(1, 0, &instances, &[]);
    assert_eq!(result.verdict, Verdict::Pass);
    let before = result
        .membership
        .iter()
        .find(|record| record.installation_id == "b")
        .unwrap();
    assert_eq!(before.commits.len(), 64);
    assert_eq!(before.commit_evidence_omitted, 67);
    assert_eq!(before.removal_sequence_ids.len(), 64);
    assert_eq!(before.removal_evidence_omitted, 66);
    assert_eq!(before.commits.first().unwrap().sequence_id, 68);
    assert!(before.owes_membership);
    let generation = before.generation;

    // The old record is absent from both the report and the next SDK snapshot.
    instances[0].groups[0].commits = vec![commit(1, "different", false)];
    instances[1].groups[0].commits = vec![commit(132, "removed", true)];
    let result = oracle.evaluate(2, 1, &instances, &[]);
    assert_eq!(result.verdict, Verdict::Fork);
    let after = result
        .membership
        .iter()
        .find(|record| record.installation_id == "b")
        .unwrap();
    assert_eq!(after.generation, generation + 2);
    assert_eq!(after.commits.len(), 64);
    assert_eq!(after.commit_evidence_omitted, 68);
    assert_eq!(after.removal_evidence_omitted, 67);
}
