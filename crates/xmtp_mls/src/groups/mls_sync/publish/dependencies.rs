//! Immutable requirements for outgoing MLS preparation.

use super::*;
use crate::identity_updates::{IdentityRequirement, resolve_identity_requirements};
use std::collections::HashMap;
use xmtp_mls_validation::commit::inbox_id_from_credential;

/// Immutable inputs copied under the writer before any dependency request.
pub(super) struct PublishRequirements {
    /// Intent contents that must still match when preparation resumes.
    pub intent: StoredGroupIntent,
    /// Committed MLS state that must still match after dependency resolution.
    pub base: PreparedBase,
    /// The part of a membership refresh being tried against the envelope limit.
    pub membership_update: Option<UpdateGroupMembershipIntentData>,
    pub partial_membership_update: bool,
    pub fail_refresh_additions: bool,
    old_membership: Option<GroupMembership>,
    new_membership: Option<GroupMembership>,
    add_inboxes: Vec<String>,
    /// Installation keys to readd, mapped to the inbox their current leaf names.
    /// A leaf whose credential does not parse has no owner, so its package fails.
    readd_installations: Option<HashMap<Vec<u8>, HashSet<String>>>,
    selection: Option<Box<super::selection::SelectionRequirements>>,
}

impl PublishRequirements {
    /// Copy requirements without retaining the mutable group outside its transaction.
    pub(super) fn capture(
        group: &OpenMlsGroup,
        intent: &StoredGroupIntent,
    ) -> Result<Self, GroupError> {
        let mut requirements = Self {
            intent: intent.clone(),
            base: PreparedBase::capture(group)?,
            membership_update: None,
            partial_membership_update: false,
            fail_refresh_additions: false,
            old_membership: None,
            new_membership: None,
            add_inboxes: Vec::new(),
            readd_installations: None,
            selection: super::selection::SelectionRequirements::capture(group, intent)?
                .map(Box::new),
        };
        match intent.kind {
            IntentKind::UpdateGroupMembership => {
                let old = extract_group_membership(group.extensions())?;
                let data = UpdateGroupMembershipIntentData::try_from(intent.data.as_slice())?;
                requirements.new_membership = Some(data.apply_to_group_membership(&old));
                requirements.membership_update = Some(data);
                requirements.old_membership = Some(old);
            }
            IntentKind::ProposeMemberUpdate => {
                let data = ProposeMemberUpdateIntentData::try_from(intent.data.as_slice())?;
                requirements.old_membership = Some(extract_group_membership(group.extensions())?);
                requirements.add_inboxes = data.add_inbox_ids;
                // Proposal preparation fetches keys before applying removals.
            }
            IntentKind::ReaddInstallations => {
                let data = ReaddInstallationsIntentData::try_from(intent.data.as_slice())?;
                let mut readd_installations = HashMap::new();
                for member in group.members().filter(|member| {
                    member.index != group.own_leaf_index()
                        && data.readded_installations.contains(&member.signature_key)
                }) {
                    let owners = inbox_id_from_credential(&member.credential)
                        .into_iter()
                        .collect();
                    readd_installations.insert(member.signature_key, owners);
                }
                requirements.readd_installations = Some(readd_installations);
            }
            _ => {}
        }
        Ok(requirements)
    }
}

#[derive(Default)]
/// Resolved inputs that can be used only after the original MLS base is rechecked.
pub(super) struct PublishDependencies {
    /// Verified installation changes and fetched key packages for this membership plan.
    pub changes: Option<MembershipDiffWithKeyPackages>,
    /// Explicit identity sequence values selected during outgoing dependency resolution.
    pub latest_sequence_ids: HashMap<String, i64>,
    memberships: Option<(GroupMembership, GroupMembership)>,
    pub selection: super::selection::ProposalSelection,
    /// Whether preparation omitted pending proposals no commit can carry.
    pub omitted_uncommittable: bool,
    /// A fitting child refresh saved in place of the original desired target.
    pub prepared_intent: Option<StoredGroupIntent>,
}

impl PublishDependencies {
    pub(super) fn take_changes(&mut self) -> Result<MembershipDiffWithKeyPackages, GroupError> {
        self.changes
            .take()
            .ok_or_else(|| OutgoingPreparationError::InvalidPreparedAttempt.into())
    }

    /// Require cached proofs for the exact membership snapshots under the writer.
    pub(super) fn validate_local(
        &self,
        storage: &impl XmtpMlsStorageProvider,
    ) -> Result<(), GroupError> {
        if let Some((old, new)) = &self.memberships {
            crate::identity_updates::get_installation_diff_local(
                &storage.db(),
                old,
                new,
                &old.diff(new),
            )
            .map_err(crate::identity_updates::InstallationDiffError::from)?;
        }
        Ok(())
    }
}

impl<Context: XmtpSharedContext> MlsGroup<Context> {
    /// Retry only still-raised inboxes. The original intent remains the desired
    /// target until every part lands, so its caller waits for the whole refresh.
    pub(super) fn smaller_membership_refresh(
        &self,
        requirements: &PublishRequirements,
    ) -> Result<Option<PublishRequirements>, GroupError> {
        let Some(mut data) = requirements.membership_update.clone() else {
            return Ok(None);
        };
        let Some(old) = &requirements.old_membership else {
            return Ok(None);
        };
        // Explicit add_members calls can also advance existing inboxes.
        // Match the sender's inline-Add classification, not the intent's
        // marker for entries that must never be inserted after removal.
        let mut refreshed: Vec<_> = data
            .membership_updates
            .iter()
            .filter(|(inbox, after)| old.get(*inbox).is_some_and(|before| *after > before))
            .map(|(inbox, _)| inbox.clone())
            .collect();
        refreshed.sort_unstable();
        let (partial, fail_additions) = if refreshed.len() > 1 {
            for inbox in refreshed.drain(refreshed.len() / 2..) {
                data.membership_updates.remove(&inbox);
            }
            data.refreshed_members
                .retain(|inbox| data.membership_updates.contains_key(inbox));
            (true, false)
        } else if refreshed.len() == 1 && !requirements.fail_refresh_additions {
            (requirements.partial_membership_update, true)
        } else {
            return Ok(None);
        };
        self.with_group_snapshot(|group| {
            if PreparedBase::capture(group)? != requirements.base {
                return Err(OutgoingPreparationError::StateChanged.into());
            }
            let mut part = requirements.intent.clone();
            part.data = Vec::from(data).into();
            let mut next = PublishRequirements::capture(group, &part)?;
            next.intent = requirements.intent.clone();
            next.partial_membership_update = partial;
            next.fail_refresh_additions = fail_additions;
            Ok(Some(next))
        })
    }

    /// Resolve external inputs after the snapshot transaction has ended.
    /// The caller must recheck the intent and MLS base before using the result.
    pub(super) async fn resolve_publish_dependencies(
        &self,
        requirements: &PublishRequirements,
    ) -> Result<PublishDependencies, GroupError> {
        let mut dependencies = PublishDependencies::default();
        if !requirements.add_inboxes.is_empty() {
            let inboxes = requirements
                .add_inboxes
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            load_identity_updates_for_client(&self.context, &self.context.db(), &inboxes).await?;
            dependencies.latest_sequence_ids =
                self.context.db().get_latest_sequence_id(&inboxes)?;
        }
        if let Some(old) = &requirements.old_membership {
            let mut new = requirements
                .new_membership
                .clone()
                .unwrap_or_else(|| old.clone());
            for inbox_id in &requirements.add_inboxes {
                let sequence = dependencies
                    .latest_sequence_ids
                    .get(inbox_id)
                    .copied()
                    .ok_or(GroupError::MissingSequenceId)?;
                new.add(inbox_id.clone(), sequence as u64);
            }
            if requirements.intent.kind == IntentKind::UpdateGroupMembership
                || !requirements.add_inboxes.is_empty()
            {
                dependencies.changes = Some(
                    calculate_membership_changes_with_keypackages(
                        &self.context,
                        &self.group_id,
                        &new,
                        old,
                    )
                    .await?,
                );
                dependencies.memberships = Some((old.clone(), new));
            }
        }
        if let Some(installations) = &requirements.readd_installations {
            let mut new_installations = Vec::new();
            let mut new_key_packages = Vec::new();
            let mut failed_installations = Vec::new();
            get_keypackages_for_installation_ids(
                &self.context,
                installations.clone(),
                &mut new_installations,
                &mut new_key_packages,
                &mut failed_installations,
            )
            .await?;
            dependencies.changes = Some(MembershipDiffWithKeyPackages::new(
                new_installations,
                new_key_packages,
                installations.keys().cloned().collect(),
                failed_installations,
            ));
        }
        if requirements.fail_refresh_additions
            && let (Some(old), Some(changes)) =
                (&requirements.old_membership, &mut dependencies.changes)
        {
            let mut failed = HashSet::new();
            changes.new_key_packages.retain(|package| {
                let suppress = inbox_id_from_credential(package.leaf_node().credential())
                    .is_ok_and(|inbox| old.get(&inbox).is_some());
                if suppress {
                    failed.insert(package.leaf_node().signature_key().as_slice().to_vec());
                }
                !suppress
            });
            changes
                .new_installations
                .retain(|installation| !failed.contains(&installation.installation_key));
            changes.failed_installations.extend(failed);
        }
        if let Some((_, target)) = &dependencies.memberships
            && (!target.failed_installations.is_empty()
                || dependencies
                    .changes
                    .as_ref()
                    .is_some_and(|changes| !changes.failed_installations.is_empty()))
        {
            // The membership delta assigns failed keys to their owning inbox.
            // Its proof reads include unchanged inboxes at their exact target
            // sequences; prepare those snapshots before entering the writer.
            let proofs = target
                .members
                .iter()
                .filter(|(_, sequence_id)| **sequence_id != 0)
                .map(|(inbox_id, sequence_id)| IdentityRequirement {
                    inbox_id: inbox_id.clone(),
                    sequence_id: *sequence_id,
                })
                .collect();
            resolve_identity_requirements(&self.context, &proofs)
                .await
                .map_err(CommitValidationError::from)?;
        }
        if let Some(selection) = &requirements.selection {
            dependencies.selection = self
                .resolve_proposal_selection(selection, dependencies.changes.as_ref())
                .await?;
        }
        Ok(dependencies)
    }
}
