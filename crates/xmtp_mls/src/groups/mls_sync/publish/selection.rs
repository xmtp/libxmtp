//! Select the pending proposals one outgoing commit carries: the membership
//! selection, and the omission of proposals no commit can carry.

use super::*;
use crate::groups::app_data::Committable;
use crate::groups::group_permissions::MembershipPolicy;
use crate::identity_updates::{
    IdentityDependencyError, InstallationDiffError, get_installation_diff_local,
    required_leaf_removals,
};
use openmls::group::QueuedProposal;
use openmls::messages::proposals::AppDataUpdateOperation;
use tls_codec::{Deserialize, VLBytes};
use xmtp_id::key_package::VerifiedKeyPackageV2;
use xmtp_mls_common::app_data::component_source::{ComponentSourceError, read_from_app_data_dict};
use xmtp_mls_common::app_data::{
    component_id::ComponentId, components::tls_map_components::GroupMembershipComponent,
    typed::Component,
};
use xmtp_mls_common::tls_map::{TlsMapDelta, TlsMapError, TlsMapMutation};
use xmtp_mls_validation::{
    commit::{CommitRuleError, extract_group_membership_with_failures, membership_entries},
    group_membership::FailedInstallations,
};

/// A read-only snapshot. Dependency resolution never retains a mutable MLS group.
pub(super) struct SelectionRequirements {
    proposals: Vec<QueuedProposal>,
    membership_bytes: Vec<u8>,
    old: GroupMembership,
    old_failed: FailedInstallations,
    own_update: Option<UpdateGroupMembershipIntentData>,
    readds: HashSet<Vec<u8>>,
    can_replace: HashSet<Vec<u8>>,
    is_super_admin: bool,
    /// Each leaf's installation and inbox.
    leaves: HashMap<LeafNodeIndex, (Vec<u8>, String)>,
}

#[derive(Default)]
pub(super) struct ProposalSelection {
    pub active: bool,
    omitted: HashSet<usize>,
    replacements: Vec<KeyPackage>,
}

fn membership_operation(proposal: &QueuedProposal) -> Option<&AppDataUpdateOperation> {
    match proposal.proposal() {
        Proposal::AppDataUpdate(update)
            if update.component_id() == ComponentId::GROUP_MEMBERSHIP.as_u16() =>
        {
            Some(update.operation())
        }
        _ => None,
    }
}

fn membership_payload(proposal: &QueuedProposal) -> Option<&[u8]> {
    match membership_operation(proposal)? {
        AppDataUpdateOperation::Update(payload) => Some(payload.as_slice()),
        AppDataUpdateOperation::Remove => None,
    }
}

impl SelectionRequirements {
    pub(super) fn capture(
        group: &OpenMlsGroup,
        intent: &StoredGroupIntent,
    ) -> Result<Option<Self>, GroupError> {
        if !commits_proposals(intent.kind) {
            return Ok(None);
        }
        let proposals: Vec<_> = group.pending_proposals().cloned().collect();
        if !proposals.iter().any(|proposal| {
            matches!(proposal.proposal(), Proposal::Add(_) | Proposal::Remove(_))
                || membership_operation(proposal).is_some()
        }) {
            return Ok(None);
        }
        let policies =
            crate::groups::group_permissions::policy_set_from_dictionary(group.extensions())
                .map_err(|e| {
                    CommitValidationError::Rule(CommitRuleError::GroupMutablePermissions(e))
                })?;
        let seed =
            xmtp_mls_common::app_data::component_source::read_group_metadata_from_dict(group)?
                .ok_or(GroupError::InvalidGroupMembership)?;
        let mutable = xmtp_mls_common::app_data::component_source::extract_group_mutable_metadata_capability_aware(group)?;
        let member = group
            .member_at(group.own_leaf_index())
            .ok_or(GroupError::InvalidGroupMembership)?;
        let credential = BasicCredential::try_from(member.credential)?;
        let inbox_id = parse_credential(credential.identity())?;
        let actor = xmtp_mls_validation::commit::CommitParticipant {
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
        let (old, old_failed) = extract_group_membership_with_failures(group.extensions())?;
        Ok(Some(Self {
            proposals,
            membership_bytes: read_from_app_data_dict(ComponentId::GROUP_MEMBERSHIP, group)
                .ok_or(GroupError::InvalidGroupMembership)?
                .to_vec(),
            old,
            old_failed,
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
            is_super_admin: actor.is_super_admin,
            leaves: group
                .members()
                .map(|member| {
                    let credential = BasicCredential::try_from(member.credential)?;
                    Ok((
                        member.index,
                        (
                            member.signature_key,
                            parse_credential(credential.identity())?,
                        ),
                    ))
                })
                .collect::<Result<_, GroupError>>()?,
        }))
    }

    fn membership(
        &self,
        omitted: &mut HashSet<usize>,
        conflicts: &mut HashSet<usize>,
        own_payload: Option<&[u8]>,
        same_request: &HashSet<usize>,
    ) -> Result<(GroupMembership, FailedInstallations), GroupError> {
        let mut bytes = self.membership_bytes.clone();
        for (index, proposal) in self.proposals.iter().enumerate() {
            if matches!(
                membership_operation(proposal),
                Some(AppDataUpdateOperation::Remove)
            ) {
                // An authorized standalone Remove cannot form a valid commit:
                // the membership component is required. Omit the whole proposal.
                omitted.insert(index);
                continue;
            }
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
                        // Retry this delta if an earlier invalid delta is later omitted.
                        // Otherwise the first valid delta keeps precedence.
                        conflicts.insert(index);
                    }
                    Err(error) => return Err(error.into()),
                }
            }
        }
        // The builder reuses an equivalent accepted delta. Otherwise it appends
        // its own encoded delta, which can be empty after a competing commit.
        if let Some(payload) = own_payload
            && !same_request
                .iter()
                .any(|index| !omitted.contains(index) && !conflicts.contains(index))
        {
            bytes = GroupMembershipComponent::apply_update_payload(payload, Some(&bytes))
                .map_err(ComponentSourceError::from)?;
        }
        Ok(membership_entries(&bytes)?)
    }

    /// Whether a membership delta writes an entry this client cannot decode,
    /// writes a committed inbox below its entry, adds an inbox at sequence 0,
    /// or names a sequence id no identity update can have. Proposal admission
    /// checks authority and the map codec, not entry contents, so a stale
    /// refresh can be accepted. No commit that keeps such a write can be
    /// validated.
    fn cannot_commit(&self, proposal: &QueuedProposal) -> Result<bool, GroupError> {
        use xmtp_proto::xmtp::mls::message_contents::{
            GroupMembershipEntry, group_membership_entry,
        };
        let Some(payload) = membership_payload(proposal) else {
            return Ok(false);
        };
        let delta =
            TlsMapDelta::<xmtp_mls_common::inbox_id::InboxId, VLBytes>::tls_deserialize_exact(
                payload,
            )?;
        for mutation in &delta.mutations {
            let (TlsMapMutation::Insert { key, value } | TlsMapMutation::Update { key, value }) =
                mutation
            else {
                continue;
            };
            // An error here would block every commit of pending proposals.
            // Omitting the whole delta is always a valid selection.
            let Ok(GroupMembershipEntry {
                version: Some(group_membership_entry::Version::V1(entry)),
            }) = GroupMembershipEntry::decode(value.as_slice())
            else {
                return Ok(true);
            };
            // An Insert of a committed inbox can follow a Delete in the same delta.
            // An added entry needs a resolved identity reference, never 0.
            if i64::try_from(entry.sequence_id).is_err()
                || self
                    .old
                    .get(key.to_hex())
                    .map_or(entry.sequence_id == 0, |&committed| {
                        entry.sequence_id < committed
                    })
            {
                return Ok(true);
            }
        }
        Ok(false)
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
        // implements: GMOD-038
        // Omit each whole stale delta before any other use of its entries.
        // The selection loop recomputes the Add and Remove proposals that
        // depended on it.
        for (index, proposal) in requirements.proposals.iter().enumerate() {
            if requirements.cannot_commit(proposal)? {
                selection.omitted.insert(index);
            }
        }
        // Readd package resolution includes only current, non-own leaves.
        // Pending proposals must use the same usable targets as the builder;
        // a requested installation without a package must keep its leaf.
        let usable_readds: HashSet<_> = own_changes
            .into_iter()
            .flat_map(|changes| &changes.new_key_packages)
            .map(|package| package.leaf_node().signature_key().as_slice())
            .filter(|key| requirements.readds.contains(*key))
            .collect();
        let owners: HashMap<_, _> = requirements.leaves.values().cloned().collect();
        let mut same_request = HashSet::new();
        let mut own_inboxes = HashSet::new();
        let mut own_removals = HashSet::new();
        let own_payload = if let Some(update) = &requirements.own_update {
            let mut target = update.apply_to_group_membership(&requirements.old);
            let no_removals = HashSet::new();
            own_removals = required_leaf_removals(
                &self.context.db(),
                &requirements.old,
                &target,
                own_changes.map_or(&no_removals, |changes| &changes.removed_installations),
                &owners,
            )
            .map_err(InstallationDiffError::from)?;
            // Publication writes the same failed list.
            let (fetch_failures, added) = own_changes.map_or((&[][..], &[][..]), |changes| {
                (
                    changes.failed_installations.as_slice(),
                    changes.new_key_packages.as_slice(),
                )
            });
            target.failed_installations = update_group_membership::leafless_failures(
                std::mem::take(&mut target.failed_installations)
                    .into_iter()
                    .chain(fetch_failures.iter().cloned()),
                &requirements
                    .leaves
                    .values()
                    .map(|(installation, _)| installation.clone())
                    .collect(),
                added,
            );
            let payload = update_group_membership::build_membership_delta(
                &self.context.db(),
                &requirements.membership_bytes,
                &requirements.old,
                &target,
            )?;
            let target_bytes = GroupMembershipComponent::apply_update_payload(
                &payload,
                Some(&requirements.membership_bytes),
            )
            .map_err(ComponentSourceError::from)?;
            let (target, target_failed) = membership_entries(&target_bytes)?;
            for (index, proposal) in requirements.proposals.iter().enumerate() {
                if selection.omitted.contains(&index) {
                    continue;
                }
                if let Some(payload) = membership_payload(proposal) {
                    let bytes = GroupMembershipComponent::apply_update_payload(
                        payload,
                        Some(&requirements.membership_bytes),
                    )
                    .map_err(ComponentSourceError::from)?;
                    let (membership, failed) = membership_entries(&bytes)?;
                    if membership.members == target.members && failed == target_failed {
                        same_request.insert(index);
                    }
                }
            }
            let delta =
                TlsMapDelta::<xmtp_mls_common::inbox_id::InboxId, VLBytes>::tls_deserialize_exact(
                    &payload,
                )?;
            for mutation in &delta.mutations {
                let key = match mutation {
                    TlsMapMutation::Insert { key, .. }
                    | TlsMapMutation::Update { key, .. }
                    | TlsMapMutation::Delete { key } => key,
                };
                // An own Update also conflicts with a pending Delete. Use the
                // exact emitted mutations, not absolute requested membership.
                let inbox = key.to_hex();
                requirements.omit_dependencies(&inbox, &mut selection.omitted)?;
                own_inboxes.insert(inbox);
            }
            // An accepted delta for this exact request can be committed by
            // reference. Re-signing it would repeat the same proposal reference.
            selection
                .omitted
                .retain(|index| !same_request.contains(index));
            Some(payload)
        } else {
            None
        };
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
            load_identity_updates_for_client(&self.context, &self.context.db(), &inboxes).await?;
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
        inject_test_key_packages(&mut packages);
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
        let leaf_inboxes: HashSet<_> = owners.values().map(String::as_str).collect();
        loop {
            // Each retry omits another complete membership delta. Recompute
            // tree proposals because removing a delta can restore an earlier value.
            selection
                .omitted
                .retain(|index| membership_operation(&requirements.proposals[*index]).is_some());
            let mut conflicts = HashSet::new();
            let (target, new_failed) = requirements.membership(
                &mut selection.omitted,
                &mut conflicts,
                own_payload.as_deref(),
                &same_request,
            )?;
            let inbox_diff = requirements.old.diff(&target);
            let diff = match identity
                .get_installation_diff(
                    &self.context.db(),
                    &self.group_id,
                    &requirements.old,
                    &target,
                    &inbox_diff,
                )
                .await
            {
                Ok(diff) => diff,
                // The backend holds no identity update at this reference, so
                // no commit that keeps it can be validated.
                Err(InstallationDiffError::IdentityDependency(
                    IdentityDependencyError::MissingReference(requirement),
                )) => {
                    if requirements
                        .omit_dependencies(&requirement.inbox_id, &mut selection.omitted)?
                    {
                        continue;
                    }
                    return Err(InstallationDiffError::from(
                        IdentityDependencyError::MissingReference(requirement),
                    )
                    .into());
                }
                Err(error) => return Err(error.into()),
            };
            let required = required_leaf_removals(
                &self.context.db(),
                &requirements.old,
                &target,
                &diff.removed_installations,
                &owners,
            )
            .map_err(InstallationDiffError::from)?;
            let mut changed = false;
            let mut unproven: HashMap<&str, Vec<&[u8]>> = HashMap::new();
            for (inbox, key) in new_failed.unproven_new_failures(&requirements.old_failed, &owners)
            {
                unproven.entry(inbox).or_default().push(key);
            }
            for (inbox, keys) in unproven {
                let sequence = target.get(inbox).copied().unwrap_or(0);
                let invalid = if sequence == 0 {
                    true
                } else {
                    let state = identity
                        .get_association_state(&self.context.db(), inbox, Some(sequence as i64))
                        .await?;
                    let installations: HashSet<_> = state.installation_ids().into_iter().collect();
                    keys.iter().any(|key| !installations.contains(*key))
                };
                if invalid {
                    if !requirements.omit_dependencies(inbox, &mut selection.omitted)? {
                        return Err(CommitValidationError::Rule(
                            CommitRuleError::InboxValidationFailed(inbox.to_string()),
                        )
                        .into());
                    }
                    changed = true;
                }
            }
            if changed {
                continue;
            }
            let mut replacements = Vec::new();
            let mut included = HashSet::new();
            // The inboxes whose Add this commit carries.
            let mut added = HashSet::new();
            for (index, id, inbox, _) in &pending {
                let desired = (diff.added_installations.contains_key(id)
                    && !target.failed_installations.contains(id))
                    || usable_readds.contains(id.as_slice());
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
                    added.insert(inbox.as_str());
                    continue;
                }
                if !requirements.can_replace.contains(id) {
                    selection.omitted.insert(*index);
                    changed |= requirements.omit_dependencies(inbox, &mut selection.omitted)?;
                    if !changed && requirements.own_update.is_some() {
                        return Err(CommitValidationError::Rule(
                            CommitRuleError::InsufficientPermissions,
                        )
                        .into());
                    }
                    continue;
                }
                selection.omitted.insert(*index);
                if included.insert(id.clone()) {
                    replacements.push(current.inner.clone());
                }
                added.insert(inbox.as_str());
            }
            if changed {
                continue;
            }
            // The inboxes whose Remove this commit carries.
            let mut removed = HashSet::new();
            let mut removed_keys = HashSet::new();
            for (index, proposal) in requirements.proposals.iter().enumerate() {
                let Proposal::Remove(remove) = proposal.proposal() else {
                    continue;
                };
                match requirements.leaves.get(&remove.removed()) {
                    Some((id, inbox))
                        if required.contains(id) || usable_readds.contains(id.as_slice()) =>
                    {
                        removed.insert(inbox.as_str());
                        // The receiver removes failed readds from its
                        // actual-removal set, so they cannot cover this delta.
                        if !requirements.is_super_admin || !target.failed_installations.contains(id)
                        {
                            removed_keys.insert(id);
                        }
                    }
                    _ => {
                        selection.omitted.insert(index);
                    }
                }
            }
            // An own update builds these removals inline. Readds do not
            // cover them: the receiver excludes matched Adds and Removes.
            // Attribute each missing removal to the inbox update requiring it.
            // A shared key's leaf can name a different, unchanged inbox.
            let uncovered_removals: HashSet<_> = required
                .iter()
                .filter(|key| !removed_keys.contains(*key) && !own_removals.contains(*key))
                .cloned()
                .collect();
            if !uncovered_removals.is_empty() {
                for inbox in inbox_diff
                    .added_inboxes
                    .iter()
                    .chain(&inbox_diff.updated_inboxes)
                    .chain(&inbox_diff.removed_inboxes)
                {
                    let mut inbox_target = requirements.old.clone();
                    if let Some(sequence) = target.get(inbox) {
                        inbox_target.add((*inbox).clone(), *sequence);
                    } else {
                        inbox_target.remove(inbox);
                    }
                    let inbox_changes = get_installation_diff_local(
                        &self.context.db(),
                        &requirements.old,
                        &inbox_target,
                        &requirements.old.diff(&inbox_target),
                    )
                    .map_err(InstallationDiffError::from)?;
                    let inbox_removals = required_leaf_removals(
                        &self.context.db(),
                        &requirements.old,
                        &inbox_target,
                        &inbox_changes.removed_installations,
                        &owners,
                    )
                    .map_err(InstallationDiffError::from)?;
                    if !inbox_removals.is_disjoint(&uncovered_removals) {
                        changed |= requirements.omit_dependencies(inbox, &mut selection.omitted)?;
                    }
                }
            }
            // implements: GMOD-038, GMOD-047
            // Added inboxes need an Add. A removed inbox needs a Remove only
            // when it has a pre-commit leaf; leafless deletes carry their proposer.
            let uncovered = inbox_diff
                .added_inboxes
                .iter()
                .filter(|inbox| !added.contains(inbox.as_str()))
                .chain(inbox_diff.removed_inboxes.iter().filter(|inbox| {
                    leaf_inboxes.contains(inbox.as_str()) && !removed.contains(inbox.as_str())
                }))
                .filter(|inbox| !own_inboxes.contains(inbox.as_str()));
            for inbox in uncovered {
                changed |= requirements.omit_dependencies(inbox, &mut selection.omitted)?;
            }
            if changed {
                continue;
            }
            selection.omitted.extend(conflicts);
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
        if !self.omitted.is_empty() {
            let kept: Vec<_> = group
                .pending_proposals()
                .enumerate()
                .filter(|(index, _)| !self.omitted.contains(index))
                .map(|(_, proposal)| proposal.clone())
                .collect();
            replace_proposals(group, storage, &kept)?;
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

/// Replace the pending proposals with `proposals`, in order. Removing one
/// copy of a repeated reference deletes the stored proposal for every copy,
/// so the store is always rebuilt whole.
pub(super) fn replace_proposals(
    group: &mut OpenMlsGroup,
    storage: &impl XmtpMlsStorageProvider,
    proposals: &[QueuedProposal],
) -> Result<(), GroupError> {
    let current: Vec<_> = group.pending_proposals().cloned().collect();
    for proposal in &current {
        remove_proposal(group, storage, proposal)?;
    }
    for proposal in proposals {
        group.store_pending_proposal(storage, proposal.clone())?;
    }
    Ok(())
}

/// Whether preparing an intent of `kind` builds a commit of the pending proposals.
pub(super) fn commits_proposals(kind: IntentKind) -> bool {
    !matches!(
        kind,
        IntentKind::SendMessage | IntentKind::ProposeMemberUpdate
    )
}

/// Drop each pending proposal no commit can carry, and each `AppDataUpdate`
/// past the cap (see [`Committable::pending`]), and return whether any was
/// dropped. It expires with the epoch.
// implements: GMOD-047
pub(super) fn omit_uncommittable_proposals(
    group: &mut OpenMlsGroup,
    storage: &impl XmtpMlsStorageProvider,
) -> Result<bool, GroupError> {
    let kept: Vec<_> = Committable::pending(group)?
        .proposals
        .into_iter()
        .cloned()
        .collect();
    let omitted = group.pending_proposals().count() - kept.len();
    if omitted > 0 {
        tracing::warn!(
            group_id = group.group_id().as_slice().short_hex(),
            omitted,
            "omitting pending proposals no commit can carry; they expire with the epoch"
        );
        replace_proposals(group, storage, &kept)?;
    }
    Ok(omitted > 0)
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
