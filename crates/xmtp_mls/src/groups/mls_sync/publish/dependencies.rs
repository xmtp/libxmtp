//! Immutable requirements for outgoing MLS preparation.

use super::*;
use std::collections::{BTreeMap, HashMap};
use xmtp_mls_common::app_data::component_id::ComponentId;

/// Immutable inputs copied under the writer before any dependency request.
pub(super) struct PublishRequirements {
    /// Intent contents that must still match when preparation resumes.
    pub intent: StoredGroupIntent,
    /// Committed MLS state that must still match after dependency resolution.
    pub base: PreparedBase,
    old_membership: Option<GroupMembership>,
    new_membership: Option<GroupMembership>,
    add_inboxes: Vec<String>,
    remove_inboxes: Vec<String>,
    readd_installations: Option<HashSet<Vec<u8>>>,
    bootstrap_extensions: Option<Extensions<GroupContext>>,
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
            old_membership: None,
            new_membership: None,
            add_inboxes: Vec::new(),
            remove_inboxes: Vec::new(),
            readd_installations: None,
            bootstrap_extensions: None,
        };
        match intent.kind {
            IntentKind::UpdateGroupMembership => {
                let old = extract_group_membership(group.extensions())?;
                let data = UpdateGroupMembershipIntentData::try_from(intent.data.as_slice())?;
                requirements.new_membership = Some(data.apply_to_group_membership(&old));
                requirements.old_membership = Some(old);
            }
            IntentKind::ProposeMemberUpdate => {
                let data = ProposeMemberUpdateIntentData::try_from(intent.data.as_slice())?;
                requirements.old_membership = Some(extract_group_membership(group.extensions())?);
                requirements.add_inboxes = data.add_inbox_ids;
                // Proposal preparation fetches keys before applying removals.
            }
            IntentKind::CommitPendingProposals => {
                let membership = extract_group_membership(group.extensions())?;
                for proposal in group.pending_proposals() {
                    match proposal.proposal() {
                        Proposal::Add(add) => {
                            let credential = BasicCredential::try_from(
                                add.key_package().leaf_node().credential().clone(),
                            )?;
                            let inbox_id = parse_credential(credential.identity())?;
                            if membership.get(&inbox_id).is_none()
                                && !requirements.add_inboxes.contains(&inbox_id)
                            {
                                requirements.add_inboxes.push(inbox_id);
                            }
                        }
                        Proposal::Remove(remove) => {
                            if let Some(member) = group.member_at(remove.removed()) {
                                let credential = BasicCredential::try_from(member.credential)?;
                                requirements
                                    .remove_inboxes
                                    .push(parse_credential(credential.identity())?);
                            }
                        }
                        _ => {}
                    }
                }
                requirements.old_membership = Some(membership);
            }
            IntentKind::ReaddInstallations => {
                let data = ReaddInstallationsIntentData::try_from(intent.data.as_slice())?;
                requirements.readd_installations = Some(
                    group
                        .members()
                        .filter(|member| {
                            member.index != group.own_leaf_index()
                                && data.readded_installations.contains(&member.signature_key)
                        })
                        .map(|member| member.signature_key)
                        .collect(),
                );
            }
            IntentKind::BootstrapMigration
                if !crate::groups::app_data::is_migrated_group(group) =>
            {
                requirements.bootstrap_extensions = Some(group.extensions().clone());
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
    /// Canonical bootstrap values synthesized from the captured legacy extensions.
    pub bootstrap_components: Option<BTreeMap<ComponentId, Vec<u8>>>,
    memberships: Option<(GroupMembership, GroupMembership)>,
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
        _context: &impl XmtpSharedContext,
        storage: &impl XmtpMlsStorageProvider,
        _group_id: GroupId,
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
            load_identity_updates(self.context.api(), &self.context.db(), &inboxes).await?;
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
            for inbox_id in &requirements.remove_inboxes {
                new.remove(inbox_id);
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
                installations.clone(),
                failed_installations,
            ));
        }
        if let Some(extensions) = &requirements.bootstrap_extensions {
            dependencies.bootstrap_components = Some(
                crate::groups::app_data::migration::synthesize_initial_component_values_from_extensions(
                    &self.context, extensions,
                ).await?,
            );
        }
        Ok(dependencies)
    }
}
