//! Registry reconciliation and removed-member cleanup carried by every
//! commit this client builds.
//!
//! A commit registers the catalogue entries its group lacks and deletes the
//! per-inbox map keys and admin-list key of each inbox it removes, whether
//! the removal is its own or a swept pending proposal.
//! [`membership_upkeep`] returns those changes as inline proposals.
//! Each one passes the receiver's own validation against the post-commit
//! membership before it is kept; a change the committer may not make is
//! dropped, so upkeep never blocks the commit it rides on. Receivers require
//! none of it.

use std::collections::{BTreeSet, HashMap, HashSet};

use openmls::{
    group::MlsGroup as OpenMlsGroup,
    messages::proposals::{AppDataUpdateProposal, Proposal},
};
use prost::Message;
use tls_codec::{Deserialize, Serialize, VLBytes};
use xmtp_configuration::ApplicationComponentDefinition;
use xmtp_mls_common::{
    app_data::{
        component_id::ComponentId,
        component_registry::ComponentRegistry,
        component_source::{component_type, read_from_app_data_dict},
        creation::catalogue_registry_entries,
    },
    inbox_id::InboxId,
    tls_map::{TlsMap, TlsMapDelta},
    tls_set::{TlsSet, TlsSetDelta},
};
use xmtp_mls_validation::commit::{
    AppDataUpdateInCommit, extract_commit_participant, read_committed_metadata,
    validate_app_data_update_sequence,
};
use xmtp_proto::{types::ConversationType, xmtp::mls::message_contents::ComponentType};

use super::{load_component_registry, pending_app_data_updates};
use crate::groups::GroupError;

type States = HashMap<ComponentId, Option<Vec<u8>>>;

/// The inline `AppDataUpdate` proposals this client adds to a commit, in
/// addition to the group's pending proposals.
// implements: META-067, GMOD-039
pub(crate) fn membership_upkeep(
    group: &OpenMlsGroup,
    catalogue: &[ApplicationComponentDefinition],
) -> Result<Vec<Proposal>, GroupError> {
    let registry = load_component_registry(group)?;
    let (immutable, mutable) = read_committed_metadata(group)?;
    let committer =
        extract_commit_participant(&group.own_leaf_index(), group, &immutable, &mutable)?;
    let mut states: States = pending_app_data_updates(group)?
        .into_iter()
        .flatten()
        .map(|(id, value)| (ComponentId::from(id), value))
        .collect();
    let state = |states: &States, id| {
        states
            .get(&id)
            .cloned()
            .unwrap_or_else(|| read_from_app_data_dict(id, group))
    };

    let committed_members = inbox_keys(read_from_app_data_dict(
        ComponentId::GROUP_MEMBERSHIP,
        group,
    ));
    let members = inbox_keys(state(&states, ComponentId::GROUP_MEMBERSHIP));
    let removed: Vec<InboxId> = committed_members.difference(&members).copied().collect();
    let members: HashSet<InboxId> = members.into_iter().collect();

    let mut candidates = Vec::new();
    if let Some(pending) = state(&states, ComponentId::COMPONENT_REGISTRY)
        .and_then(|bytes| ComponentRegistry::from_bytes(&bytes).ok())
    {
        let missing = unregistered(
            catalogue,
            immutable.conversation_type,
            [&registry, &pending],
        );
        if !missing.mutations.is_empty() {
            candidates.push((
                ComponentId::COMPONENT_REGISTRY,
                missing.tls_serialize_detached()?,
            ));
        }
    }
    // Types come from the committed registry, not a pending one: sender and
    // receiver decode every update in a commit against the pre-commit
    // registry, so a map delete for a type changed in this commit would be
    // applied as a scalar replacement.
    for (id, metadata) in registry.iter().flatten() {
        let Some(snapshot) = state(&states, id) else {
            continue;
        };
        let payload = match component_type(id)
            .or(ComponentType::try_from(metadata.component_type).ok())
        {
            Some(ComponentType::TlsMapInboxIdBytes | ComponentType::TlsMapInboxIdString)
                if id != ComponentId::GROUP_MEMBERSHIP =>
            {
                let Ok(map) = TlsMap::<InboxId, VLBytes>::tls_deserialize_exact(snapshot) else {
                    continue;
                };
                let delta = removed
                    .iter()
                    .filter(|inbox| map.contains_key(inbox))
                    .fold(TlsMapDelta::<InboxId, VLBytes>::new(), |delta, inbox| {
                        delta.delete(*inbox)
                    });
                (!delta.mutations.is_empty()).then(|| delta.tls_serialize_detached())
            }
            _ if id == ComponentId::ADMIN_LIST => {
                let Ok(set) = TlsSet::<InboxId>::tls_deserialize_exact(snapshot) else {
                    continue;
                };
                let delta = removed
                    .iter()
                    .filter(|inbox| set.contains(inbox))
                    .fold(TlsSetDelta::new(), |delta, inbox| delta.remove(*inbox));
                (!delta.mutations.is_empty()).then(|| delta.tls_serialize_detached())
            }
            _ => None,
        };
        if let Some(payload) = payload {
            candidates.push((id, payload?));
        }
    }

    let mut proposals = Vec::new();
    for (id, payload) in candidates {
        let proposal = AppDataUpdateProposal::update(id.as_u16(), payload);
        let update = AppDataUpdateInCommit {
            component_id: id,
            operation: proposal.operation(),
            actor: (&committer).into(),
            proposer_inbox_id: &committer.inbox_id,
        };
        match validate_app_data_update_sequence(
            [update],
            |id| state(&states, id),
            &registry,
            immutable.dm_members.as_ref(),
            Some(&members),
        ) {
            Ok(post) => {
                states.extend(post);
                proposals.push(Proposal::AppDataUpdate(Box::new(proposal)));
            }
            Err(error) => {
                tracing::info!(%id, %error, "commit omits unauthorized upkeep")
            }
        }
    }
    Ok(proposals)
}

/// An insert for each catalogue entry eligible for `conversation_type`
/// whose ID has no raw key in any of `registries`. A present entry is never
/// replaced, even when this build cannot read it.
// implements: META-067
fn unregistered(
    catalogue: &[ApplicationComponentDefinition],
    conversation_type: ConversationType,
    registries: [&ComponentRegistry; 2],
) -> TlsMapDelta<ComponentId, VLBytes> {
    catalogue_registry_entries(catalogue, conversation_type)
        .filter(|(id, _)| registries.iter().all(|registry| !registry.contains_raw(id)))
        .fold(TlsMapDelta::new(), |delta, (id, metadata)| {
            delta.insert(id, VLBytes::new(metadata.encode_to_vec()))
        })
}

/// The inbox keys of a `GROUP_MEMBERSHIP` snapshot; empty when absent or
/// malformed.
fn inbox_keys(snapshot: Option<Vec<u8>>) -> BTreeSet<InboxId> {
    snapshot
        .and_then(|bytes| TlsMap::<InboxId, VLBytes>::tls_deserialize_exact(bytes).ok())
        .map(|map| map.keys().copied().collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use xmtp_configuration::{ComponentPermissions, MetadataPolicy};
    use xmtp_proto::xmtp::mls::message_contents::metadata_policy::MetadataBasePolicy;

    fn definition(component_id: u16, in_groups: bool) -> ApplicationComponentDefinition {
        let allow = Some(MetadataPolicy::Base(MetadataBasePolicy::Allow as i32));
        ApplicationComponentDefinition {
            component_id,
            name: format!("app.{component_id:x}"),
            component_type: ComponentType::Bytes as i32,
            permissions: ComponentPermissions {
                insert: allow.clone(),
                update: allow.clone(),
                delete: allow,
            },
            in_groups,
            in_dms: !in_groups,
        }
    }

    fn registry(entries: &[(u16, Vec<u8>)]) -> ComponentRegistry {
        let map = TlsMap::from_pairs(
            entries
                .iter()
                .map(|(id, raw)| (ComponentId::new(*id), VLBytes::new(raw.clone()))),
        );
        ComponentRegistry::from_bytes(&map.tls_serialize_detached().unwrap()).unwrap()
    }

    /// Reconciliation inserts only what neither the committed nor the pending
    /// registry holds. An entry this build cannot read still counts as
    /// present, so no client replaces it with its own backend copy.
    #[xmtp_common::test(unwrap_try = true)]
    // verifies: META-067
    fn unregistered_skips_every_raw_key() {
        let catalogue = [0xC001, 0xC002, 0xC003]
            .map(|id| definition(id, true))
            .into_iter()
            .chain([definition(0xC004, false)])
            .collect::<Vec<_>>();
        let entries: Vec<_> =
            catalogue_registry_entries(&catalogue, ConversationType::Group).collect();
        let raw = |id: u16| {
            entries
                .iter()
                .find(|(entry, _)| entry.as_u16() == id)
                .map(|(_, metadata)| metadata.encode_to_vec())
                .unwrap()
        };
        let committed = registry(&[(0xC001, vec![0xFF])]);
        let pending = registry(&[(0xC001, vec![0xFF]), (0xC002, raw(0xC002))]);

        assert_eq!(
            unregistered(&catalogue, ConversationType::Group, [&committed, &pending]),
            TlsMapDelta::new().insert(ComponentId::new(0xC003), VLBytes::new(raw(0xC003))),
        );
        assert!(
            unregistered(&catalogue, ConversationType::Sync, [&committed, &pending])
                .mutations
                .is_empty()
        );
    }
}
