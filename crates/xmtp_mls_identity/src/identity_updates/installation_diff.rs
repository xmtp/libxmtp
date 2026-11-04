//! Installation diffs between two association states, and the leaves a membership
//! change must remove.

use super::*;

#[derive(Debug)]
pub struct InstallationDiff {
    /// Each added installation key, mapped to the inboxes whose identity states add it.
    pub added_installations: HashMap<Vec<u8>, HashSet<String>>,
    pub removed_installations: HashSet<Vec<u8>>,
}

/// Compare exact verified snapshots without network or signature-verifier calls.
/// A creation placeholder has an empty identity baseline. It never selects the
/// receiver's latest identity state. New membership references must be nonzero.
/// A missing proof does not stop the scan, so `Need` names every missing proof.
pub fn get_installation_diff_local(
    conn: &impl DbQuery,
    old_membership: &GroupMembership,
    new_membership: &GroupMembership,
    membership_diff: &MembershipDiff<'_>,
) -> Result<InstallationDiff, IdentityDependencyError> {
    let mut added_installations = HashMap::new();
    let mut removed_installations = HashSet::new();
    let mut missing = MissingProofs::default();
    for inbox_id in membership_diff
        .added_inboxes
        .iter()
        .chain(&membership_diff.updated_inboxes)
    {
        let final_state = missing.read(
            conn,
            &IdentityRequirement {
                inbox_id: (*inbox_id).clone(),
                sequence_id: new_membership.get(inbox_id).copied().unwrap_or(0),
            },
        )?;
        let initial_state = match old_membership.get(inbox_id) {
            None | Some(0) => None,
            Some(sequence_id) => Some(missing.read(
                conn,
                &IdentityRequirement {
                    inbox_id: (*inbox_id).clone(),
                    sequence_id: *sequence_id,
                },
            )?),
        };
        let diff = match (final_state, initial_state) {
            (Some(final_state), None) => final_state.as_diff(),
            (Some(final_state), Some(Some(initial_state))) => initial_state.diff(&final_state),
            (None, _) | (_, Some(None)) => continue,
        };
        for installation in diff.new_installations() {
            added_installations
                .entry(installation)
                .or_insert_with(HashSet::new)
                .insert((*inbox_id).clone());
        }
        removed_installations.extend(diff.removed_installations());
    }
    for inbox_id in &membership_diff.removed_inboxes {
        let Some(state) = missing.read(
            conn,
            &IdentityRequirement {
                inbox_id: (*inbox_id).clone(),
                sequence_id: old_membership.get(inbox_id).copied().unwrap_or(0),
            },
        )?
        else {
            continue;
        };
        removed_installations.extend(state.installation_ids());
    }
    missing.check()?;
    Ok(InstallationDiff {
        added_installations,
        removed_installations,
    })
}

/// Required removals include leafed identity removals and stale leaves of
/// every inbox whose membership entry advances or disappears.
// implements: GMOD-046
pub fn required_leaf_removals(
    conn: &impl DbQuery,
    old: &GroupMembership,
    new: &GroupMembership,
    diff_removals: &HashSet<Vec<u8>>,
    leaf_owners: &HashMap<Vec<u8>, String>,
) -> Result<HashSet<Vec<u8>>, IdentityDependencyError> {
    let mut required: HashSet<_> = diff_removals
        .iter()
        .filter(|key| leaf_owners.contains_key(*key))
        .cloned()
        .collect();
    let mut by_inbox: HashMap<&str, Vec<&Vec<u8>>> = HashMap::new();
    for (key, inbox) in leaf_owners {
        by_inbox.entry(inbox).or_default().push(key);
    }
    let mut missing = MissingProofs::default();
    for (inbox, keys) in by_inbox {
        match (old.get(inbox), new.get(inbox)) {
            (Some(_), None) => required.extend(keys.into_iter().cloned()),
            (before, Some(after)) if *after != 0 && before != Some(after) => {
                let Some(state) = missing.read(
                    conn,
                    &IdentityRequirement {
                        inbox_id: inbox.to_string(),
                        sequence_id: *after,
                    },
                )?
                else {
                    continue;
                };
                let associated: HashSet<_> = state.installation_ids().into_iter().collect();
                required.extend(
                    keys.into_iter()
                        .filter(|key| !associated.contains(*key))
                        .cloned(),
                );
            }
            _ => {}
        }
    }
    missing.check()?;
    Ok(required)
}
