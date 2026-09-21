use std::collections::BTreeSet;

use xmtp_db::group::GroupMembershipState;
use xmtp_mls::diagnostics::GroupSnapshot;

use super::{Finding, InstanceSnapshot, Verdict, checkpoint, membership::MembershipLedger};

pub(super) fn agrees(left: &GroupSnapshot, right: &GroupSnapshot) -> bool {
    let members = |group: &GroupSnapshot| -> BTreeSet<(String, String)> {
        group
            .members
            .iter()
            .map(|member| (member.inbox_id.clone(), member.installation_id.clone()))
            .collect()
    };
    left.epoch == right.epoch
        && left.epoch_authenticator == right.epoch_authenticator
        && left.metadata == right.metadata
        && members(left) == members(right)
}

pub(super) fn evaluate(
    instances: &[InstanceSnapshot],
    ledger: &MembershipLedger,
    findings: &mut Vec<Finding>,
) {
    for group_id in ledger.group_ids() {
        let owed: Vec<_> = instances
            .iter()
            .filter(|instance| ledger.owed(group_id, &instance.installation_id))
            .collect();
        // A pending Welcome or processor retry can explain both absent and unequal local state.
        if owed
            .iter()
            .any(|instance| !checkpoint::instance_complete(instance))
        {
            continue;
        }
        let mut reference: Option<(&InstanceSnapshot, &GroupSnapshot)> = None;
        let mut seen = BTreeSet::new();
        for instance in owed {
            if !seen.insert(&instance.installation_id) {
                continue;
            }
            let Some(group) = instance
                .groups
                .iter()
                .find(|group| group.group_id == group_id)
            else {
                findings.push(Finding::group(
                    Verdict::Brick,
                    instance.instance,
                    group_id,
                    "An owed installation has no group state after the checkpoint (lost Welcome)",
                ));
                continue;
            };
            if !group.active || group.membership_state == GroupMembershipState::Restored {
                findings.push(Finding::group(Verdict::Brick, instance.instance, group_id,
                    "An owed installation has not joined; inactive or Restored state is not removal evidence"));
                continue;
            }
            if let Some((other, state)) = reference {
                if !agrees(state, group) {
                    findings.push(Finding::group(Verdict::Fork, instance.instance, group_id,
                        format!("State disagrees with installation {} at the checkpoint (epoch {} versus {})",
                            other.installation_id, state.epoch, group.epoch)));
                }
            } else {
                reference = Some((instance, group));
            }
            if group.maybe_forked || group.is_commit_log_forked == Some(true) {
                findings.push(Finding::group(
                    Verdict::Warn,
                    instance.instance,
                    group_id,
                    format!(
                        "SDK fork flags: maybe_forked={}, is_commit_log_forked={:?}",
                        group.maybe_forked, group.is_commit_log_forked
                    ),
                ));
            }
        }
    }
}
