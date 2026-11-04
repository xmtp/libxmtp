//! Expected membership diffs: the installation changes an identity update allows, and
//! the commit checks against them.

use super::*;

/// Membership changes derived from the exact old and proposed identity proofs.
pub(super) struct ExpectedDiff {
    /// The membership before this commit. The commit cannot change it.
    pub(super) old_group_membership: GroupMembership,
    /// Proposed inbox sequences. They cannot rewrite the old proof requirements.
    pub(super) new_group_membership: GroupMembership,
    pub(super) old_failed: FailedInstallations,
    pub(super) new_failed: FailedInstallations,
    /// Installation changes authorized by those exact identity snapshots.
    pub(super) expected_installation_diff: InstallationDiff,
    pub(super) added_inboxes: Vec<Inbox>,
    pub(super) removed_inboxes: Vec<Inbox>,
}

impl ExpectedDiff {
    /// Derive installation changes from cached proofs on the caller's connection.
    /// Missing exact proofs return `Need`; this method does not fetch identities.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn from_staged_commit_with_proposers(
        conn: &impl DbQuery,
        staged_commit: &StagedCommit,
        openmls_group: &OpenMlsGroup,
        envelope_sequence: u64,
        added_inbox_proposers: &HashMap<String, CommitParticipant>,
        removed_inbox_proposers: &HashMap<String, CommitParticipant>,
    ) -> Result<Self, CommitValidationError> {
        let extensions = openmls_group.extensions();
        let (immutable_metadata, mutable_metadata) = read_committed_metadata(openmls_group)
            .map_err(CommitValidationError::installed_state)?;

        let expected_diff = Self::extract_expected_diff_with_proposers(
            conn,
            staged_commit,
            envelope_sequence,
            extensions,
            &immutable_metadata,
            &mutable_metadata,
            added_inbox_proposers,
            removed_inbox_proposers,
        )?;

        Ok(expected_diff)
    }

    /// Generates an expected diff with proposer attribution for each inbox change.
    /// This is used when validating commits with proposals from multiple members.
    #[allow(clippy::too_many_arguments)]
    fn extract_expected_diff_with_proposers(
        conn: &impl DbQuery,
        staged_commit: &StagedCommit,
        envelope_sequence: u64,
        existing_group_extensions: &Extensions<GroupContext>,
        immutable_metadata: &GroupMetadata,
        mutable_metadata: &GroupMutableMetadata,
        added_inbox_proposers: &HashMap<String, CommitParticipant>,
        removed_inbox_proposers: &HashMap<String, CommitParticipant>,
    ) -> Result<ExpectedDiff, CommitValidationError> {
        let (old_group_membership, old_failed) =
            extract_group_membership_with_failures(existing_group_extensions)
                .map_err(CommitValidationError::installed_state)?;
        let (new_group_membership, new_failed) =
            extract_group_membership_with_failures(staged_commit.group_context().extensions())?;
        validate_identity_sequence_order(&new_group_membership, envelope_sequence)?;
        let membership_diff = old_group_membership.diff(&new_group_membership);

        validate_membership_diff(
            &old_group_membership,
            &new_group_membership,
            &membership_diff,
        )?;
        validate_sync_membership(immutable_metadata, &new_group_membership)?;

        let added_inboxes = membership_diff
            .added_inboxes
            .iter()
            .map(|inbox_id| {
                let proposer = added_inbox_proposers
                    .get(inbox_id.as_str())
                    .cloned()
                    .ok_or(CommitValidationError::Rule(
                        CommitRuleError::ProposerNotFound,
                    ))?;
                Ok(build_inbox_with_proposer(
                    inbox_id,
                    immutable_metadata,
                    mutable_metadata,
                    proposer,
                ))
            })
            .collect::<Result<Vec<Inbox>, CommitValidationError>>()?;

        let removed_inboxes = membership_diff
            .removed_inboxes
            .iter()
            .map(|inbox_id| {
                let proposer = removed_inbox_proposers
                    .get(inbox_id.as_str())
                    .cloned()
                    .ok_or(CommitValidationError::Rule(
                        CommitRuleError::ProposerNotFound,
                    ))?;
                Ok(build_inbox_with_proposer(
                    inbox_id,
                    immutable_metadata,
                    mutable_metadata,
                    proposer,
                ))
            })
            .collect::<Result<Vec<Inbox>, CommitValidationError>>()?;

        let expected_installation_diff = get_installation_diff_local(
            conn,
            &old_group_membership,
            &new_group_membership,
            &membership_diff,
        )?;

        Ok(ExpectedDiff {
            old_group_membership,
            new_group_membership,
            old_failed,
            new_failed,
            expected_installation_diff,
            added_inboxes,
            removed_inboxes,
        })
    }
}

/// Newly listed failures must belong to the entry's inbox or one of its old leaves.
// implements: GMOD-049
pub(super) fn validate_new_failed_installations(
    conn: &impl DbQuery,
    membership: &GroupMembership,
    before: &FailedInstallations,
    after: &FailedInstallations,
    leaf_owners: &HashMap<Vec<u8>, String>,
) -> Result<(), CommitValidationError> {
    let mut by_inbox: HashMap<&str, Vec<&[u8]>> = HashMap::new();
    for (inbox, key) in after.unproven_new_failures(before, leaf_owners) {
        by_inbox.entry(inbox).or_default().push(key);
    }
    let mut missing = MissingProofs::default();
    for (inbox, keys) in by_inbox {
        let sequence_id = membership.get(inbox).copied().unwrap_or(0);
        if sequence_id == 0 {
            return Err(CommitRuleError::InboxValidationFailed(inbox.to_string()).into());
        }
        let Some(state) = missing.read(
            conn,
            &IdentityRequirement {
                inbox_id: inbox.to_string(),
                sequence_id,
            },
        )?
        else {
            continue;
        };
        let installations: HashSet<_> = state.installation_ids().into_iter().collect();
        if keys.iter().any(|key| !installations.contains(*key)) {
            return Err(CommitRuleError::InboxValidationFailed(inbox.to_string()).into());
        }
    }
    missing.check()?;
    Ok(())
}

/// Compare the list of installations added and removed in the commit to the expected diff based on the changes
/// to the inbox state.
/// Satisfies Rule 3 and Rule 7
// implements: GMOD-010, GMOD-046
pub(super) fn expected_diff_matches_commit(
    expected_diff: &InstallationDiff,
    added_installations: HashSet<Vec<u8>>,
    self_added_installations: &HashSet<Vec<u8>>,
    removed_installations: HashSet<Vec<u8>>,
    required_removals: &HashSet<Vec<u8>>,
    existing_installation_ids: &HashSet<Vec<u8>>,
) -> Result<(), CommitValidationError> {
    // Check and make sure that any added installations are either:
    // 1. In the expected diff
    // 2. Already a member of the group (for example, the group creator is already a member on the first commit)
    // 3. Proposed by a member of the inbox the leaf's credential names

    let unknown_adds = added_installations
        .into_iter()
        .filter(|installation_id| {
            !expected_diff
                .added_installations
                .contains_key(installation_id)
                && !existing_installation_ids.contains(installation_id)
                // The identity check below still needs the key in its inbox's state.
                && !self_added_installations.contains(installation_id)
        })
        .collect::<Vec<Vec<u8>>>();
    if !unknown_adds.is_empty() {
        return Err(CommitValidationError::Rule(
            CommitRuleError::UnexpectedInstallationAdded(unknown_adds),
        ));
    }

    if &removed_installations != required_removals {
        let unexpected: Vec<_> = removed_installations
            .symmetric_difference(required_removals)
            .cloned()
            .collect();

        return Err(CommitValidationError::Rule(
            CommitRuleError::UnexpectedInstallationsRemoved(unexpected),
        ));
    }

    Ok(())
}
