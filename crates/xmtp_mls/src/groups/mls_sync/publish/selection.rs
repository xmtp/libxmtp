//! Select pending membership proposals for one outgoing commit.

use super::*;
use crate::groups::app_data::component_source::{ComponentSourceError, read_from_app_data_dict};
use crate::groups::group_permissions::MembershipPolicy;
use openmls::group::QueuedProposal;
use openmls::messages::proposals::AppDataUpdateOperation;
use tls_codec::{Deserialize, VLBytes};
use xmtp_id::key_package::VerifiedKeyPackageV2;
use xmtp_mls_common::app_data::{
    component_id::ComponentId, components::tls_map_components::GroupMembershipComponent,
    typed::Component,
};
use xmtp_mls_common::tls_map::{TlsMapDelta, TlsMapError, TlsMapMutation};

/// A read-only snapshot. Dependency resolution never retains a mutable MLS group.
pub(super) struct SelectionRequirements {
    proposals: Vec<QueuedProposal>,
    membership_bytes: Vec<u8>,
    old: GroupMembership,
    own_update: Option<UpdateGroupMembershipIntentData>,
    readds: HashSet<Vec<u8>>,
    can_replace: HashSet<Vec<u8>>,
    leaves: HashMap<LeafNodeIndex, Vec<u8>>,
}

#[derive(Default)]
pub(super) struct ProposalSelection {
    pub active: bool,
    omitted: HashSet<usize>,
    replacements: Vec<KeyPackage>,
}

fn membership_payload(proposal: &QueuedProposal) -> Option<&[u8]> {
    match proposal.proposal() {
        Proposal::AppDataUpdate(update)
            if update.component_id() == ComponentId::GROUP_MEMBERSHIP.as_u16() =>
        {
            match update.operation() {
                AppDataUpdateOperation::Update(payload) => Some(payload.as_slice()),
                AppDataUpdateOperation::Remove => None,
            }
        }
        _ => None,
    }
}

impl SelectionRequirements {
    pub(super) fn capture(
        group: &OpenMlsGroup,
        intent: &StoredGroupIntent,
    ) -> Result<Option<Self>, GroupError> {
        if matches!(
            intent.kind,
            IntentKind::SendMessage | IntentKind::ProposeMemberUpdate
        ) {
            return Ok(None);
        }
        let proposals: Vec<_> = group.pending_proposals().cloned().collect();
        if !proposals.iter().any(|proposal| {
            matches!(proposal.proposal(), Proposal::Add(_) | Proposal::Remove(_))
                || membership_payload(proposal).is_some()
        }) {
            return Ok(None);
        }
        let policies =
            crate::groups::group_permissions::policy_set_from_dictionary(group.extensions())
                .map_err(CommitValidationError::GroupMutablePermissions)?;
        let seed = crate::groups::app_data::component_source::read_group_metadata_from_dict(group)?
            .ok_or(GroupError::InvalidGroupMembership)?;
        let mutable = crate::groups::app_data::component_source::extract_group_mutable_metadata_capability_aware(group)?;
        let member = group
            .member_at(group.own_leaf_index())
            .ok_or(GroupError::InvalidGroupMembership)?;
        let credential = BasicCredential::try_from(member.credential)?;
        let inbox_id = parse_credential(credential.identity())?;
        let actor = crate::groups::validated_commit::CommitParticipant {
            is_creator: inbox_id == seed.creator_inbox_id,
            is_admin: mutable.is_admin(&inbox_id),
            is_super_admin: mutable.is_super_admin(&inbox_id),
            inbox_id,
            installation_id: member.signature_key,
        };
        let mut can_replace = HashSet::new();
        for proposal in &proposals {
            let Proposal::Add(add) = proposal.proposal() else {
                continue;
            };
            let credential =
                BasicCredential::try_from(add.key_package().leaf_node().credential().clone())?;
            let inbox_id = parse_credential(credential.identity())?;
            let subject = Inbox {
                inbox_id: inbox_id.clone(),
                is_creator: false,
                is_admin: false,
                is_super_admin: false,
                proposer: Some(actor.clone()),
            };
            let dm_add = seed.dm_members.as_ref().is_some_and(|dm| {
                (dm.dm_member_one
                    .as_ref()
                    .is_some_and(|member| member.inbox_id == inbox_id)
                    || dm
                        .dm_member_two
                        .as_ref()
                        .is_some_and(|member| member.inbox_id == inbox_id))
                    && inbox_id != actor.inbox_id
            });
            if policies
                .policies
                .add_member_policy
                .evaluate(&actor, &subject)
                || dm_add
            {
                can_replace.insert(
                    add.key_package()
                        .leaf_node()
                        .signature_key()
                        .as_slice()
                        .to_vec(),
                );
            }
        }
        Ok(Some(Self {
            proposals,
            membership_bytes: read_from_app_data_dict(ComponentId::GROUP_MEMBERSHIP, group)
                .ok_or(GroupError::InvalidGroupMembership)?,
            old: extract_group_membership(group.extensions())?,
            own_update: (intent.kind == IntentKind::UpdateGroupMembership)
                .then(|| UpdateGroupMembershipIntentData::try_from(intent.data.as_slice()))
                .transpose()?,
            readds: if intent.kind == IntentKind::ReaddInstallations {
                ReaddInstallationsIntentData::try_from(intent.data.as_slice())?
                    .readded_installations
                    .into_iter()
                    .collect()
            } else {
                HashSet::new()
            },
            can_replace,
            leaves: group
                .members()
                .map(|member| (member.index, member.signature_key))
                .collect(),
        }))
    }

    fn membership(&self, omitted: &mut HashSet<usize>) -> Result<GroupMembership, GroupError> {
        let mut bytes = self.membership_bytes.clone();
        for (index, proposal) in self.proposals.iter().enumerate() {
            if !omitted.contains(&index)
                && let Some(payload) = membership_payload(proposal)
            {
                match GroupMembershipComponent::apply_update_payload(payload, Some(&bytes))
                    .map_err(ComponentSourceError::from)
                {
                    Ok(next) => bytes = next,
                    Err(ComponentSourceError::TlsMapApply(
                        TlsMapError::KeyExists | TlsMapError::KeyNotFound,
                    )) => {
                        // Each proposal was accepted against the committed
                        // dictionary. Concurrent Insert/Delete deltas can conflict.
                        // Keep one whole delta, then recompute its tree changes.
                        omitted.insert(index);
                    }
                    Err(error) => return Err(error.into()),
                }
            }
        }
        let membership = membership_from_app_data_bytes(&bytes)?;
        Ok(self
            .own_update
            .as_ref()
            .map_or(membership.clone(), |update| {
                update.apply_to_group_membership(&membership)
            }))
    }

    /// Drop a whole authenticated membership delta. Never rewrite another
    /// author's payload or apply only some of its inbox changes.
    fn omit_dependencies(
        &self,
        inbox: &str,
        omitted: &mut HashSet<usize>,
    ) -> Result<bool, GroupError> {
        let mut changed = false;
        for (index, proposal) in self.proposals.iter().enumerate() {
            let Some(payload) = membership_payload(proposal) else {
                continue;
            };
            let delta =
                TlsMapDelta::<xmtp_mls_common::inbox_id::InboxId, VLBytes>::tls_deserialize_exact(
                    payload,
                )?;
            if delta.mutations.iter().any(|mutation| {
                let key = match mutation {
                    TlsMapMutation::Insert { key, .. }
                    | TlsMapMutation::Update { key, .. }
                    | TlsMapMutation::Delete { key } => key,
                };
                key.to_hex() == inbox
            }) {
                changed |= omitted.insert(index);
            }
        }
        Ok(changed)
    }
}

impl<Context: XmtpSharedContext> MlsGroup<Context> {
    pub(super) async fn resolve_proposal_selection(
        &self,
        requirements: &SelectionRequirements,
        own_changes: Option<&MembershipDiffWithKeyPackages>,
    ) -> Result<ProposalSelection, GroupError> {
        let mut selection = ProposalSelection {
            active: true,
            ..Default::default()
        };
        if let Some(update) = &requirements.own_update {
            let mut target = update.apply_to_group_membership(&requirements.old);
            if let Some(changes) = own_changes {
                target
                    .failed_installations
                    .extend(changes.failed_installations.iter().cloned());
                target.failed_installations.sort_unstable();
                target.failed_installations.dedup();
            }
            let mut same_request = HashSet::new();
            for (index, proposal) in requirements.proposals.iter().enumerate() {
                if let Some(payload) = membership_payload(proposal) {
                    let bytes = GroupMembershipComponent::apply_update_payload(
                        payload,
                        Some(&requirements.membership_bytes),
                    )
                    .map_err(ComponentSourceError::from)?;
                    let mut membership = membership_from_app_data_bytes(&bytes)?;
                    membership.failed_installations.sort_unstable();
                    membership.failed_installations.dedup();
                    if membership == target {
                        same_request.insert(index);
                    }
                }
            }
            let diff = requirements.old.diff(&target);
            for inbox in diff.added_inboxes.iter().chain(&diff.removed_inboxes) {
                // This request supplies its own Insert/Delete. Including another
                // signed delta for that inbox would repeat the dictionary change.
                requirements.omit_dependencies(inbox, &mut selection.omitted)?;
            }
            // An accepted delta for this exact request can be committed by
            // reference. Re-signing it would repeat the same proposal reference.
            selection
                .omitted
                .retain(|index| !same_request.contains(index));
        }
        let mut pending = Vec::new();
        for (index, proposal) in requirements.proposals.iter().enumerate() {
            if let Proposal::Add(add) = proposal.proposal() {
                let package = add.key_package();
                let credential =
                    BasicCredential::try_from(package.leaf_node().credential().clone())?;
                pending.push((
                    index,
                    package.leaf_node().signature_key().as_slice().to_vec(),
                    parse_credential(credential.identity())?,
                    package,
                ));
            }
        }
        let inboxes: HashSet<_> = pending
            .iter()
            .map(|(_, _, inbox, _)| inbox.as_str())
            .collect();
        let inboxes: Vec<_> = inboxes.into_iter().collect();
        let identity = IdentityUpdates::new(&self.context);
        let mut active: HashMap<String, HashSet<Vec<u8>>> = HashMap::new();
        if !inboxes.is_empty() {
            load_identity_updates(self.context.api(), &self.context.db(), &inboxes).await?;
            for inbox in inboxes {
                active.insert(
                    inbox.to_string(),
                    identity
                        .get_association_state(&self.context.db(), inbox, None)
                        .await?
                        .installation_ids()
                        .into_iter()
                        .collect(),
                );
            }
        }
        // A failed request is an error, not evidence that a retained package is obsolete.
        let requested = pending
            .iter()
            .filter(|(_, id, inbox, _)| active.get(inbox).is_some_and(|ids| ids.contains(id)))
            .map(|(_, id, _, _)| id.clone())
            .collect::<HashSet<_>>();
        #[allow(unused_mut)]
        let mut packages = if requested.is_empty() {
            HashMap::new()
        } else {
            MlsStore::new(self.context.clone())
                .get_key_packages_for_installation_ids(requested.into_iter().collect())
                .await?
        };
        #[cfg(any(test, feature = "test-utils"))]
        inject_failed_installations_for_test(&mut packages, &mut Vec::new()).await;
        let mut reusable = HashMap::new();
        for (index, id, _, retained) in &pending {
            let Some(Ok(current)) = packages.get(id) else {
                continue;
            };
            let bytes = retained.tls_serialize_detached()?;
            if bytes == current.inner.tls_serialize_detached()?
                && VerifiedKeyPackageV2::from_bytes(
                    &openmls_rust_crypto::RustCrypto::default(),
                    &bytes,
                )
                .is_ok()
            {
                reusable.entry(id.clone()).or_insert(*index);
            }
        }
        loop {
            // Each retry omits another complete membership delta. Recompute
            // tree proposals because removing a delta can restore an earlier value.
            selection
                .omitted
                .retain(|index| membership_payload(&requirements.proposals[*index]).is_some());
            let mut target = requirements.membership(&mut selection.omitted)?;
            if requirements.own_update.is_some()
                && let Some(changes) = own_changes
            {
                // Keep the normal partial-success rule for an inbox with some
                // unusable installations. The own membership delta records them.
                target
                    .failed_installations
                    .extend(changes.failed_installations.iter().cloned());
            }
            let diff = identity
                .get_installation_diff(
                    &self.context.db(),
                    &self.group_id,
                    &requirements.old,
                    &target,
                    &requirements.old.diff(&target),
                )
                .await?;
            let mut changed = false;
            let mut replacements = Vec::new();
            let mut included = HashSet::new();
            for (index, id, inbox, _) in &pending {
                let desired = (diff.added_installations.contains(id)
                    && !target.failed_installations.contains(id))
                    || requirements.readds.contains(id);
                if !desired || !active.get(inbox).is_some_and(|ids| ids.contains(id)) {
                    selection.omitted.insert(*index);
                    if desired {
                        changed |= requirements.omit_dependencies(inbox, &mut selection.omitted)?;
                        if !changed && requirements.own_update.is_some() {
                            return Err(GroupError::InvalidGroupMembership);
                        }
                    }
                    continue;
                }
                let current = match packages.get(id) {
                    Some(Ok(current)) => {
                        let credential = BasicCredential::try_from(
                            current.inner.leaf_node().credential().clone(),
                        )?;
                        (current.installation_id() == *id
                            && parse_credential(credential.identity())? == *inbox)
                            .then_some(current)
                    }
                    _ => None,
                };
                let Some(current) = current else {
                    selection.omitted.insert(*index);
                    let omitted_dependency =
                        requirements.omit_dependencies(inbox, &mut selection.omitted)?;
                    changed |= omitted_dependency;
                    if !omitted_dependency && requirements.own_update.is_some() {
                        return Err(GroupError::InvalidPublicKeys(vec![id.clone()]));
                    }
                    continue;
                };
                // Prefer an accepted proposal with the complete current package,
                // even if an obsolete duplicate appeared earlier in network order.
                if let Some(selected) = reusable.get(id) {
                    if index != selected {
                        selection.omitted.insert(*index);
                    }
                    continue;
                }
                if !requirements.can_replace.contains(id) {
                    selection.omitted.insert(*index);
                    changed |= requirements.omit_dependencies(inbox, &mut selection.omitted)?;
                    if !changed && requirements.own_update.is_some() {
                        return Err(CommitValidationError::InsufficientPermissions.into());
                    }
                    continue;
                }
                if !included.insert(id.clone()) {
                    selection.omitted.insert(*index);
                } else {
                    selection.omitted.insert(*index);
                    replacements.push(current.inner.clone());
                }
            }
            if changed {
                continue;
            }
            for (index, proposal) in requirements.proposals.iter().enumerate() {
                if let Proposal::Remove(remove) = proposal.proposal()
                    && requirements.leaves.get(&remove.removed()).is_none_or(|id| {
                        !diff.removed_installations.contains(id)
                            && !requirements.readds.contains(id)
                    })
                {
                    selection.omitted.insert(index);
                }
            }
            selection.replacements = replacements;
            return Ok(selection);
        }
    }
}

impl ProposalSelection {
    /// This subset exists only in the writer transaction. Preparation restores
    /// all original objects and their order before it saves the exact attempt.
    pub(super) fn apply(
        &self,
        group: &mut OpenMlsGroup,
        storage: &impl XmtpMlsStorageProvider,
        signer: &impl openmls_traits::signatures::Signer,
    ) -> Result<Vec<Vec<u8>>, GroupError> {
        let proposals: Vec<_> = group.pending_proposals().cloned().collect();
        for index in &self.omitted {
            remove_proposal(group, storage, &proposals[*index])?;
        }
        let provider = XmtpOpenMlsProviderRef::new(storage);
        let mut payloads = Vec::new();
        for package in &self.replacements {
            let (message, _) = group
                .propose_add_member(&provider, signer, package)
                .map_err(GroupError::ProposeAddMember)?;
            payloads.push(message.tls_serialize_detached()?);
        }
        Ok(payloads)
    }
}

pub(super) fn restore_proposals(
    group: &mut OpenMlsGroup,
    storage: &impl XmtpMlsStorageProvider,
    originals: &[QueuedProposal],
) -> Result<(), GroupError> {
    let current: Vec<_> = group.pending_proposals().cloned().collect();
    for proposal in &current {
        remove_proposal(group, storage, proposal)?;
    }
    for proposal in originals {
        group.store_pending_proposal(storage, proposal.clone())?;
    }
    Ok(())
}

fn remove_proposal(
    group: &mut OpenMlsGroup,
    storage: &impl XmtpMlsStorageProvider,
    proposal: &QueuedProposal,
) -> Result<(), GroupError> {
    group
        .remove_pending_proposal(storage, proposal.proposal_reference_ref())
        .map_err(|error| match error {
            openmls::group::RemoveProposalError::Storage(error) => GroupError::SqlKeyStore(error),
            openmls::group::RemoveProposalError::ProposalNotFound => {
                OutgoingPreparationError::InvalidPreparedAttempt.into()
            }
        })
}
