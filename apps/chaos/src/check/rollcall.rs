use std::collections::BTreeSet;

use xmtp_mls::diagnostics::RetryBudgets;

use super::{
    Finding, InstanceSnapshot, RollCall, Verdict, checkpoint, membership::MembershipLedger,
};

pub(super) fn evaluate(
    instances: &[InstanceSnapshot],
    ledger: &MembershipLedger,
    calls: &[RollCall],
    budgets: &RetryBudgets,
    findings: &mut Vec<Finding>,
) {
    for call in calls {
        let owed: Vec<_> = instances
            .iter()
            .filter(|instance| {
                call.expected_installations
                    .contains(&instance.installation_id)
                    && ledger.owed(&call.group_id, &instance.installation_id)
            })
            .collect();
        if owed
            .iter()
            .any(|instance| !checkpoint::instance_complete(instance))
        {
            continue;
        }
        let disagree = findings.iter().any(|finding| {
            finding.verdict == Verdict::Fork && finding.group_id.as_ref() == Some(&call.group_id)
        });
        let mut seen = BTreeSet::new();
        for instance in owed {
            if !seen.insert(&instance.installation_id) {
                continue;
            }
            let sync = call.sync_received.contains(&instance.installation_id);
            let stream = call.stream_received.contains(&instance.installation_id);
            let missing = if !sync {
                Some("sync")
            } else if call
                .expected_stream_installations
                .contains(&instance.installation_id)
                && !stream
            {
                Some("stream")
            } else {
                None
            };
            let Some(delivery) = missing else { continue };
            let verdict = if disagree {
                Verdict::Fork
            } else if call.elapsed_ms > budgets.barrier_ms {
                Verdict::Brick
            } else {
                Verdict::Stall
            };
            findings.push(Finding::group(
                verdict,
                instance.instance,
                &call.group_id,
                format!(
                    "Roll-call token {} from {} is missing by {delivery}; waited {}ms, budget {}ms",
                    call.token, call.sender_installation, call.elapsed_ms, budgets.barrier_ms
                ),
            ));
        }
    }
}
