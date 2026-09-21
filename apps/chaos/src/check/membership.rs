use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use xmtp_mls::diagnostics::CommitSnapshot;

use super::{Finding, InstanceSnapshot, Verdict, checkpoint};

const REPORT_EVIDENCE_LIMIT: usize = 64;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MembershipRecord {
    pub group_id: String,
    pub installation_id: String,
    pub inbox_id: String,
    pub owes_membership: bool,
    pub generation: u64,
    pub removal_sequence_ids: BTreeSet<i64>,
    pub commits: Vec<CommitSnapshot>,
    pub commit_evidence_omitted: usize,
    pub removal_evidence_omitted: usize,
}

#[derive(Default)]
pub(super) struct MembershipLedger {
    records: BTreeMap<(String, String), MembershipRecord>,
}

impl MembershipLedger {
    pub(super) fn records(&self) -> Vec<MembershipRecord> {
        self.records
            .values()
            .map(|record| {
                let mut commits: Vec<_> = record
                    .commits
                    .iter()
                    .rev()
                    .take(REPORT_EVIDENCE_LIMIT)
                    .cloned()
                    .collect();
                commits.reverse();
                MembershipRecord {
                    group_id: record.group_id.clone(),
                    installation_id: record.installation_id.clone(),
                    inbox_id: record.inbox_id.clone(),
                    owes_membership: record.owes_membership,
                    generation: record.generation,
                    removal_sequence_ids: record
                        .removal_sequence_ids
                        .iter()
                        .rev()
                        .take(REPORT_EVIDENCE_LIMIT)
                        .copied()
                        .collect(),
                    commits,
                    commit_evidence_omitted: record
                        .commits
                        .len()
                        .saturating_sub(REPORT_EVIDENCE_LIMIT),
                    removal_evidence_omitted: record
                        .removal_sequence_ids
                        .len()
                        .saturating_sub(REPORT_EVIDENCE_LIMIT),
                }
            })
            .collect()
    }

    pub(super) fn owed(&self, group: &str, installation: &str) -> bool {
        self.records
            .get(&(group.to_owned(), installation.to_owned()))
            .is_some_and(|record| record.owes_membership)
    }

    pub(super) fn group_ids(&self) -> BTreeSet<&str> {
        self.records
            .values()
            .map(|record| record.group_id.as_str())
            .collect()
    }

    fn owe(&mut self, group: &str, installation: &str, inbox: &str) {
        let record = self
            .records
            .entry((group.to_owned(), installation.to_owned()))
            .or_insert_with(|| MembershipRecord {
                group_id: group.to_owned(),
                installation_id: installation.to_owned(),
                inbox_id: inbox.to_owned(),
                owes_membership: false,
                generation: 0,
                removal_sequence_ids: BTreeSet::new(),
                commits: Vec::new(),
                commit_evidence_omitted: 0,
                removal_evidence_omitted: 0,
            });
        if !record.owes_membership {
            record.owes_membership = true;
            record.generation += 1;
        }
    }

    pub(super) fn observe(&mut self, instances: &[InstanceSnapshot], findings: &mut Vec<Finding>) {
        // Keep evidence before changing membership. A rejoin can reset the SDK's fork flags.
        for instance in instances {
            for group in &instance.groups {
                let key = (group.group_id.clone(), instance.installation_id.clone());
                if !self.records.contains_key(&key) {
                    self.owe(
                        &group.group_id,
                        &instance.installation_id,
                        &instance.inbox_id,
                    );
                }
                let record = self
                    .records
                    .get_mut(&key)
                    .expect("membership record exists");
                for commit in &group.commits {
                    if commit.removed_this_installation
                        && record.removal_sequence_ids.insert(commit.sequence_id)
                    {
                        if !record.owes_membership {
                            // Two removals prove that a membership began between them.
                            record.generation += 1;
                        }
                        record.owes_membership = false;
                        record.generation += 1;
                    }
                    if !record.commits.iter().any(|entry| {
                        entry.sequence_id == commit.sequence_id
                            && entry.authenticator == commit.authenticator
                            && entry.result == commit.result
                            && entry.removed_this_installation == commit.removed_this_installation
                    }) {
                        record.commits.push(commit.clone());
                    }
                }
                if group.active {
                    self.owe(
                        &group.group_id,
                        &instance.installation_id,
                        &instance.inbox_id,
                    );
                }
            }
        }

        // Only an agreed MLS member set creates obligations for an absent installation.
        // A changed member set alone never proves that an existing obligation ended.
        let groups: BTreeSet<_> = instances
            .iter()
            .flat_map(|instance| instance.groups.iter().map(|group| group.group_id.as_str()))
            .collect();
        for group_id in groups {
            let active: Vec<_> = instances
                .iter()
                .filter(|instance| checkpoint::instance_complete(instance))
                .flat_map(|instance| instance.groups.iter())
                .filter(|group| group.group_id == group_id && group.active)
                .collect();
            let Some(first) = active.first() else {
                continue;
            };
            if active
                .iter()
                .all(|group| super::state::agrees(first, group))
            {
                for member in &first.members {
                    // New devices become owed when a ratchet-tree leaf confirms reconciliation.
                    if instances.iter().any(|instance| {
                        instance.installation_id == member.installation_id
                            && instance.inbox_id == member.inbox_id
                    }) {
                        self.owe(group_id, &member.installation_id, &member.inbox_id);
                    }
                }
            }
        }
        self.compare_history(instances, findings);
    }

    fn compare_history(&self, instances: &[InstanceSnapshot], findings: &mut Vec<Finding>) {
        let mut applied: BTreeMap<(&str, i64), (&str, &str)> = BTreeMap::new();
        for record in self.records.values() {
            for commit in &record.commits {
                // A removal retains the old authenticator and is not an attestation of the new epoch.
                if commit.removed_this_installation
                    || commit.last_authenticator.is_empty()
                    || (commit.result == xmtp_db::remote_commit_log::CommitResult::Success as i32
                        && commit.authenticator == commit.last_authenticator)
                {
                    continue;
                }
                let key = (record.group_id.as_str(), commit.sequence_id);
                if let Some((installation, authenticator)) = applied.get(&key) {
                    if *authenticator != commit.authenticator.as_str() {
                        let instance = instances
                            .iter()
                            .find(|instance| instance.installation_id == record.installation_id)
                            .map_or(0, |instance| instance.instance);
                        findings.push(Finding::group(
                            Verdict::Fork,
                            instance,
                            &record.group_id,
                            format!(
                                "Commit {} at epoch {} disagrees with installation {installation}; evidence survives membership generation {}",
                                commit.sequence_id, commit.epoch, record.generation
                            ),
                        ));
                    }
                } else {
                    applied.insert(key, (&record.installation_id, &commit.authenticator));
                }
            }
        }
    }
}
