//! A commit keeps installations accounted for.
//!
//! A key is unaccounted for an inbox when the inbox's referenced identity
//! state associates it, no leaf has it as its signature key, and the inbox's
//! entry does not list it as failed. A leaf under any inbox counts, because
//! a key associated with two inboxes has a leaf under only one of them and a
//! sender may not list a key with a leaf as failed. A commit may keep a key
//! unaccounted that already was, but may not leave a new one. Receivers and
//! the commit builder both call [`validate_membership_accounting`], so they
//! cannot disagree.

use std::collections::{BTreeSet, HashMap, HashSet};

use openmls::{
    group::{MlsGroup as OpenMlsGroup, StagedCommit},
    prelude::{LeafNode, LeafNodeIndex, Sender},
};

use crate::{
    commit::{CommitRuleError, inbox_id_from_credential},
    group_membership::{FailedInstallations, GroupMembership},
};

/// A commit's membership change, as the accounting rule reads it.
pub struct MembershipChange<'a> {
    /// The membership entries before the commit.
    pub old: &'a GroupMembership,
    /// The membership entries after the commit.
    pub new: &'a GroupMembership,
    /// Each entry's `failed_installations` before the commit.
    pub old_failed: &'a FailedInstallations,
    /// Each entry's `failed_installations` after the commit.
    pub new_failed: &'a FailedInstallations,
    /// Each leaf's signature key and the inbox its credential names, before
    /// the commit.
    pub leaves_before: &'a HashMap<Vec<u8>, String>,
    /// The same after the commit.
    pub leaves_after: &'a HashMap<Vec<u8>, String>,
}

impl MembershipChange<'_> {
    /// The inboxes that are members after the commit and whose entry,
    /// failed list, or leaves the commit changes. A leaf belongs to the inbox
    /// its credential names.
    fn touched(&self) -> BTreeSet<&str> {
        let by_inbox = |leaves: &HashMap<Vec<u8>, String>| {
            let mut keys: HashMap<String, BTreeSet<Vec<u8>>> = HashMap::new();
            for (key, inbox) in leaves {
                keys.entry(inbox.clone()).or_default().insert(key.clone());
            }
            keys
        };
        let (before, after) = (by_inbox(self.leaves_before), by_inbox(self.leaves_after));
        self.new
            .members
            .iter()
            .filter(|(inbox, sequence_id)| {
                self.old.get(inbox) != Some(sequence_id)
                    || self.old_failed.of(inbox) != self.new_failed.of(inbox)
                    || before.get(*inbox) != after.get(*inbox)
            })
            .map(|(inbox, _)| inbox.as_str())
            .collect()
    }

    /// The identity states [`validate_membership_accounting`] reads: each
    /// touched inbox at its nonzero entry before and after the commit.
    pub fn references(&self) -> BTreeSet<(&str, u64)> {
        self.touched()
            .into_iter()
            .flat_map(|inbox| {
                [self.old.get(inbox), self.new.get(inbox)]
                    .into_iter()
                    .flatten()
                    .filter(|sequence_id| **sequence_id != 0)
                    .map(move |sequence_id| (inbox, *sequence_id))
            })
            .collect()
    }
}

/// The keys of `associated` that have no leaf and are not in `failed`.
fn unaccounted<'k>(
    associated: &'k HashSet<Vec<u8>>,
    leaves: &HashMap<Vec<u8>, String>,
    failed: Option<&HashSet<Vec<u8>>>,
) -> HashSet<&'k [u8]> {
    associated
        .iter()
        .filter(|key| !leaves.contains_key(*key))
        .filter(|key| !failed.is_some_and(|failed| failed.contains(*key)))
        .map(Vec::as_slice)
        .collect()
}

/// Reject a commit that leaves a key unaccounted for an inbox when the key
/// was not unaccounted for that inbox before the commit. Before the commit,
/// an inbox that was not a member or whose entry was 0 has no unaccounted
/// key.
///
/// `associated` returns the installation keys an inbox's identity state at a
/// sequence id associates with it. Callers read every one of
/// [`MembershipChange::references`] first and hold the commit when one is
/// missing; an inbox with a missing state is not judged.
// implements: GMOD-057
pub fn validate_membership_accounting<'s>(
    change: &MembershipChange<'_>,
    associated: impl Fn(&str, u64) -> Option<&'s HashSet<Vec<u8>>>,
) -> Result<(), CommitRuleError> {
    for inbox in change.touched() {
        // An entry of 0 references no identity state, so nothing is
        // associated with the inbox.
        let Some(&after) = change
            .new
            .get(inbox)
            .filter(|sequence_id| **sequence_id != 0)
        else {
            continue;
        };
        let Some(associated_after) = associated(inbox, after) else {
            continue;
        };
        let before = match change.old.get(inbox) {
            None | Some(0) => HashSet::new(),
            Some(&sequence_id) => match associated(inbox, sequence_id) {
                Some(associated_before) => unaccounted(
                    associated_before,
                    change.leaves_before,
                    change.old_failed.of(inbox),
                ),
                None => continue,
            },
        };
        let after = unaccounted(
            associated_after,
            change.leaves_after,
            change.new_failed.of(inbox),
        );
        if !after.is_subset(&before) {
            return Err(CommitRuleError::UnaccountedInstallation(inbox.to_string()));
        }
    }
    Ok(())
}

/// Each leaf's signature key and the inbox its credential names after
/// `staged_commit`, given the same for the group before it.
pub fn leaf_owners_after_commit(
    staged_commit: &StagedCommit,
    openmls_group: &OpenMlsGroup,
    committer: LeafNodeIndex,
    before: &HashMap<Vec<u8>, String>,
) -> Result<HashMap<Vec<u8>, String>, CommitRuleError> {
    /// Replace the leaf at `old`, if any, with `leaf`.
    fn replace(
        after: &mut HashMap<Vec<u8>, String>,
        openmls_group: &OpenMlsGroup,
        old: Option<LeafNodeIndex>,
        leaf: &LeafNode,
    ) -> Result<(), CommitRuleError> {
        if let Some(member) = old.and_then(|index| openmls_group.member_at(index)) {
            after.remove(&member.signature_key);
        }
        after.insert(
            leaf.signature_key().as_slice().to_vec(),
            inbox_id_from_credential(leaf.credential())?,
        );
        Ok(())
    }

    let mut after = before.clone();
    // OpenMLS applies updates, then removes, then adds, then the path.
    for update in staged_commit.update_proposals() {
        if let Sender::Member(index) = update.sender() {
            replace(
                &mut after,
                openmls_group,
                Some(*index),
                update.update_proposal().leaf_node(),
            )?;
        }
    }
    for remove in staged_commit.remove_proposals() {
        if let Some(member) = openmls_group.member_at(remove.remove_proposal().removed()) {
            after.remove(&member.signature_key);
        }
    }
    for add in staged_commit.add_proposals() {
        let leaf = add.add_proposal().key_package().leaf_node();
        replace(&mut after, openmls_group, None, leaf)?;
    }
    if let Some(leaf) = staged_commit.update_path_leaf_node() {
        replace(&mut after, openmls_group, Some(committer), leaf)?;
    }
    Ok(after)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALIX: &str = "alix";
    const BO: &str = "bo";

    fn key(byte: u8) -> Vec<u8> {
        vec![byte; 32]
    }

    fn membership(entries: &[(&str, u64)]) -> GroupMembership {
        let mut membership = GroupMembership::new();
        for (inbox, sequence_id) in entries {
            membership.add(inbox.to_string(), *sequence_id);
        }
        membership
    }

    fn failed(entries: &[(&str, &[u8])]) -> FailedInstallations {
        let mut failed = FailedInstallations::default();
        for (inbox, byte) in entries {
            failed
                .0
                .entry(inbox.to_string())
                .or_default()
                .extend(byte.iter().map(|byte| key(*byte)));
        }
        failed
    }

    fn leaves(entries: &[(u8, &str)]) -> HashMap<Vec<u8>, String> {
        entries
            .iter()
            .map(|(byte, inbox)| (key(*byte), inbox.to_string()))
            .collect()
    }

    /// The identity states, keyed by inbox and sequence id.
    struct States(HashMap<(String, u64), HashSet<Vec<u8>>>);

    impl States {
        fn new(entries: &[(&str, u64, &[u8])]) -> Self {
            Self(
                entries
                    .iter()
                    .map(|(inbox, sequence_id, keys)| {
                        (
                            (inbox.to_string(), *sequence_id),
                            keys.iter().map(|byte| key(*byte)).collect(),
                        )
                    })
                    .collect(),
            )
        }

        fn validate(&self, change: &MembershipChange<'_>) -> Result<(), CommitRuleError> {
            validate_membership_accounting(change, |inbox, sequence_id| {
                self.0.get(&(inbox.to_string(), sequence_id))
            })
        }
    }

    /// One commit's inputs, owned so a test can describe it in one place.
    struct Commit {
        old: GroupMembership,
        new: GroupMembership,
        old_failed: FailedInstallations,
        new_failed: FailedInstallations,
        leaves_before: HashMap<Vec<u8>, String>,
        leaves_after: HashMap<Vec<u8>, String>,
    }

    impl Commit {
        fn change(&self) -> MembershipChange<'_> {
            MembershipChange {
                old: &self.old,
                new: &self.new,
                old_failed: &self.old_failed,
                new_failed: &self.new_failed,
                leaves_before: &self.leaves_before,
                leaves_after: &self.leaves_after,
            }
        }
    }

    fn rejected(result: Result<(), CommitRuleError>, inbox: &str) -> bool {
        matches!(result, Err(CommitRuleError::UnaccountedInstallation(i)) if i == inbox)
    }

    /// Bo's entry rises to a state that associates a new key, which the
    /// commit neither adds nor lists as failed. Listing it as failed
    /// accounts for it.
    // verifies: GMOD-057
    #[xmtp_common::test(unwrap_try = true)]
    fn a_new_key_needs_a_leaf_or_a_failure() {
        let states = States::new(&[(ALIX, 1, &[1]), (BO, 1, &[2]), (BO, 2, &[2, 3])]);
        let mut commit = Commit {
            old: membership(&[(ALIX, 1), (BO, 1)]),
            new: membership(&[(ALIX, 1), (BO, 2)]),
            old_failed: failed(&[]),
            new_failed: failed(&[]),
            leaves_before: leaves(&[(1, ALIX), (2, BO)]),
            leaves_after: leaves(&[(1, ALIX), (2, BO)]),
        };
        assert!(rejected(states.validate(&commit.change()), BO));
        commit.new_failed = failed(&[(BO, &[3])]);
        states.validate(&commit.change())?;
    }

    /// A key that was unaccounted before the commit may stay so.
    // verifies: GMOD-057
    #[xmtp_common::test(unwrap_try = true)]
    fn a_pre_existing_unaccounted_key_is_carried_forward() {
        let states = States::new(&[(BO, 1, &[2, 3]), (BO, 2, &[2, 3])]);
        let commit = Commit {
            old: membership(&[(BO, 1)]),
            new: membership(&[(BO, 2)]),
            old_failed: failed(&[]),
            new_failed: failed(&[]),
            leaves_before: leaves(&[(2, BO)]),
            leaves_after: leaves(&[(2, BO)]),
        };
        states.validate(&commit.change())?;
    }

    /// An added inbox has no unaccounted key before the commit, so every
    /// key its state associates needs a leaf or a failure.
    // verifies: GMOD-057
    #[xmtp_common::test(unwrap_try = true)]
    fn a_new_inbox_missing_a_key_is_rejected() {
        let states = States::new(&[(ALIX, 1, &[1]), (BO, 1, &[2, 3])]);
        let commit = Commit {
            old: membership(&[(ALIX, 1)]),
            new: membership(&[(ALIX, 1), (BO, 1)]),
            old_failed: failed(&[]),
            new_failed: failed(&[]),
            leaves_before: leaves(&[(1, ALIX)]),
            leaves_after: leaves(&[(1, ALIX), (2, BO)]),
        };
        assert!(rejected(states.validate(&commit.change()), BO));
    }

    /// Dropping a failure of a key that is still associated and leafless
    /// leaves it unaccounted, even at the same sequence id.
    // verifies: GMOD-057
    #[xmtp_common::test(unwrap_try = true)]
    fn a_dropped_failure_is_rejected() {
        let states = States::new(&[(BO, 1, &[2, 3])]);
        let commit = Commit {
            old: membership(&[(BO, 1)]),
            new: membership(&[(BO, 1)]),
            old_failed: failed(&[(BO, &[3])]),
            new_failed: failed(&[(BO, &[])]),
            leaves_before: leaves(&[(2, BO)]),
            leaves_after: leaves(&[(2, BO)]),
        };
        assert!(rejected(states.validate(&commit.change()), BO));
    }

    /// A key the new state no longer associates is not unaccounted, even
    /// when its leaf and failure are both gone.
    // verifies: GMOD-057
    #[xmtp_common::test(unwrap_try = true)]
    fn a_revoked_key_is_ignored() {
        let states = States::new(&[(BO, 1, &[2, 3, 4]), (BO, 2, &[2])]);
        let commit = Commit {
            old: membership(&[(BO, 1)]),
            new: membership(&[(BO, 2)]),
            old_failed: failed(&[(BO, &[4])]),
            new_failed: failed(&[(BO, &[])]),
            leaves_before: leaves(&[(2, BO), (3, BO)]),
            leaves_after: leaves(&[(2, BO)]),
        };
        states.validate(&commit.change())?;
    }

    /// A readd removes and re-adds the same key, which keeps its leaf.
    // verifies: GMOD-057
    #[xmtp_common::test(unwrap_try = true)]
    fn a_readd_is_accepted() {
        let states = States::new(&[(BO, 1, &[2, 3])]);
        let mut commit = Commit {
            old: membership(&[(BO, 1)]),
            new: membership(&[(BO, 1)]),
            old_failed: failed(&[(BO, &[])]),
            new_failed: failed(&[(BO, &[])]),
            leaves_before: leaves(&[(2, BO), (3, BO)]),
            leaves_after: leaves(&[(2, BO), (3, BO)]),
        };
        states.validate(&commit.change())?;
        // A readd whose package could not be fetched lists the key instead.
        commit.leaves_after = leaves(&[(2, BO)]);
        assert!(rejected(states.validate(&commit.change()), BO));
        commit.new_failed = failed(&[(BO, &[3])]);
        states.validate(&commit.change())?;
    }

    /// A key associated with two inboxes is accounted for both by its leaf
    /// under one of them, whichever inbox's entry the commit adds or raises.
    // verifies: GMOD-057
    #[xmtp_common::test(unwrap_try = true)]
    fn a_shared_key_with_a_leaf_under_another_inbox_is_accounted() {
        let states = States::new(&[(ALIX, 1, &[1, 3]), (BO, 1, &[2]), (BO, 2, &[2, 3])]);
        let commit = Commit {
            old: membership(&[(ALIX, 1), (BO, 1)]),
            new: membership(&[(ALIX, 1), (BO, 2)]),
            old_failed: failed(&[]),
            new_failed: failed(&[]),
            leaves_before: leaves(&[(1, ALIX), (2, BO), (3, ALIX)]),
            leaves_after: leaves(&[(1, ALIX), (2, BO), (3, ALIX)]),
        };
        states.validate(&commit.change())?;
        let added = Commit {
            old: membership(&[(ALIX, 1)]),
            new: membership(&[(ALIX, 1), (BO, 2)]),
            old_failed: failed(&[]),
            new_failed: failed(&[]),
            leaves_before: leaves(&[(1, ALIX), (3, ALIX)]),
            leaves_after: leaves(&[(1, ALIX), (2, BO), (3, ALIX)]),
        };
        states.validate(&added.change())?;
    }

    /// An entry of 0 before the commit has no unaccounted key, so raising
    /// it must account for every key.
    // verifies: GMOD-057
    #[xmtp_common::test(unwrap_try = true)]
    fn an_entry_of_zero_before_the_commit_is_treated_as_empty() {
        let states = States::new(&[(ALIX, 1, &[1, 4])]);
        let commit = Commit {
            old: membership(&[(ALIX, 0)]),
            new: membership(&[(ALIX, 1)]),
            old_failed: failed(&[]),
            new_failed: failed(&[]),
            leaves_before: leaves(&[(1, ALIX)]),
            leaves_after: leaves(&[(1, ALIX)]),
        };
        assert!(rejected(states.validate(&commit.change()), ALIX));
        assert_eq!(
            commit.change().references(),
            BTreeSet::from([(ALIX, 1)]),
            "an entry of 0 references no state"
        );
    }

    /// A rotated leaf key leaves the old key unaccounted when the state still
    /// associates it, and an untouched inbox is not judged.
    // verifies: GMOD-057
    #[xmtp_common::test(unwrap_try = true)]
    fn a_rotated_leaf_key_must_stay_accounted() {
        let states = States::new(&[(ALIX, 1, &[1, 4]), (BO, 1, &[2, 9])]);
        let commit = Commit {
            old: membership(&[(ALIX, 1), (BO, 1)]),
            new: membership(&[(ALIX, 1), (BO, 1)]),
            old_failed: failed(&[(ALIX, &[]), (BO, &[])]),
            new_failed: failed(&[(ALIX, &[]), (BO, &[])]),
            leaves_before: leaves(&[(1, ALIX), (2, BO)]),
            leaves_after: leaves(&[(4, ALIX), (2, BO)]),
        };
        assert_eq!(commit.change().references(), BTreeSet::from([(ALIX, 1)]));
        assert!(rejected(states.validate(&commit.change()), ALIX));
    }
}
