//! [`AssociationState`] describes a single point in time for an Inbox where it contains a set of
//! associated [`MemberIdentifier`]'s, which may be one of [`MemberKind::Address`]
//! or[`MemberKind::Installation`]. A diff between two states can be calculated to determine
//! a change of membership between two periods of time. [XIP-46](https://github.com/xmtp/XIPs/pull/53)

use super::{
    AssociationError, Handle, MemberIdentifier, MemberKind, ident,
    member::{Identifier, Member},
};
use crate::InboxIdRef;
use std::{
    collections::{HashMap, HashSet},
    fmt::{Debug, Write},
};

#[derive(Debug, Clone)]
pub struct AssociationStateDiff {
    pub new_members: Vec<MemberIdentifier>,
    pub removed_members: Vec<MemberIdentifier>,
}

#[derive(Debug)]
pub struct Installation {
    pub id: Vec<u8>,
    pub client_timestamp_ns: Option<u64>,
}

impl AssociationStateDiff {
    pub fn new_installations(&self) -> Vec<Vec<u8>> {
        self.new_members
            .iter()
            .filter_map(|member| match member {
                MemberIdentifier::Installation(ident::Installation(key)) => Some(key.clone()),
                _ => None,
            })
            .collect()
    }

    pub fn removed_installations(&self) -> Vec<Vec<u8>> {
        self.removed_members
            .iter()
            .filter_map(|member| match member {
                MemberIdentifier::Installation(ident::Installation(key)) => Some(key.clone()),
                _ => None,
            })
            .collect()
    }
}

#[derive(Clone)]
pub struct AssociationState {
    pub(crate) inbox_id: String,
    pub(crate) members: HashMap<MemberIdentifier, Member>,
    pub(crate) recovery_identifier: Identifier,
    /// The chain id recorded for the recovery role. It survives the recovery identifier's
    /// revocation as a member, and equals its member chain id while it is a member.
    pub(crate) recovery_chain_id: Option<u64>,
    pub(crate) seen_signatures: HashSet<Vec<u8>>,
    /// Claimed handles in claim order; the last is the display handle.
    pub(crate) handles: Vec<Handle>,
    /// The number of `ClaimHandle` actions applied to produce this state.
    pub(crate) handle_claims: u32,
}

impl std::fmt::Debug for AssociationState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut members = String::new();
        for member in self.members.keys() {
            write!(members, "{:?}", member)?;
            write!(members, ",")?;
        }

        let mut signatures = String::new();
        for signature in self.seen_signatures.iter() {
            write!(
                signatures,
                "{}",
                xmtp_common::fmt::truncate_hex(hex::encode(signature))
            )?;
            write!(signatures, ",")?;
        }

        // A handle names an inbox, so a log line carries only the count.
        write!(
            f,
            "AssociationState {{ inbox_id: {}, members: {}, recovery: {}, recovery_chain_id: {:?}, seen_signatures: {}, handles: {}, handle_claims: {} }}",
            self.inbox_id,
            members,
            self.recovery_identifier,
            self.recovery_chain_id,
            signatures,
            self.handles.len(),
            self.handle_claims
        )
    }
}

impl TryFrom<MemberIdentifier> for Identifier {
    type Error = AssociationError;
    fn try_from(ident: MemberIdentifier) -> Result<Self, Self::Error> {
        let ident = match ident {
            MemberIdentifier::Ethereum(eth) => Self::Ethereum(eth),
            MemberIdentifier::Passkey(passkey) => Self::Passkey(passkey),
            MemberIdentifier::Installation(_) => {
                return Err(AssociationError::NotIdentifier(
                    "Installation Keys".to_string(),
                ));
            }
        };
        Ok(ident)
    }
}

impl AssociationState {
    pub fn add(&self, member: Member) -> Self {
        let mut new_state = self.clone();
        let _ = new_state.members.insert(member.identifier.clone(), member);

        new_state
    }

    pub fn remove(&self, identifier: &MemberIdentifier) -> Self {
        let mut new_state = self.clone();
        let _ = new_state.members.remove(identifier);

        new_state
    }

    /// The state with `recovery_identifier` holding the recovery role on `chain_id`.
    pub fn set_recovery_identifier(
        &self,
        recovery_identifier: Identifier,
        chain_id: Option<u64>,
    ) -> Self {
        let mut new_state = self.clone();
        new_state.recovery_identifier = recovery_identifier;
        new_state.recovery_chain_id = chain_id;

        new_state
    }

    pub fn get(&self, identifier: &MemberIdentifier) -> Option<&Member> {
        self.members.get(identifier)
    }

    pub fn add_seen_signatures(&self, signatures: Vec<Vec<u8>>) -> Self {
        let mut new_state = self.clone();
        new_state.seen_signatures.extend(signatures);

        new_state
    }

    pub fn has_seen(&self, signature: &Vec<u8>) -> bool {
        self.seen_signatures.contains(signature)
    }

    /// The state after a claim of `handle`: `handle` moves to the end of the
    /// handles, or is added there, and the claim count rises by one. Members,
    /// the recovery identifier, and chain ids are unchanged.
    pub fn claim_handle(&self, handle: Handle) -> Self {
        let mut new_state = self.clone();
        new_state.handles.retain(|held| *held != handle);
        new_state.handles.push(handle);
        new_state.handle_claims += 1;

        new_state
    }

    /// Every handle the inbox has claimed, in claim order, with a handle
    /// claimed again at the position of its last claim.
    pub fn handles(&self) -> &[Handle] {
        &self.handles
    }

    /// The number of `ClaimHandle` actions applied to produce this state.
    /// A claim of a handle the inbox already holds counts.
    pub fn handle_claims(&self) -> u32 {
        self.handle_claims
    }

    /// The inbox's current handle: the last one it claimed, or `None` when it
    /// has claimed none.
    pub fn display_handle(&self) -> Option<&Handle> {
        self.handles.last()
    }

    /// The handles other than the display handle, the most recently claimed
    /// first.
    pub fn other_handles(&self) -> impl Iterator<Item = &Handle> {
        self.handles.iter().rev().skip(1)
    }

    pub fn members(&self) -> Vec<Member> {
        let mut sorted_members: Vec<_> = self.members.values().cloned().collect();
        sorted_members.sort_by_key(|m| m.client_timestamp_ns.unwrap_or(u64::MAX));
        sorted_members
    }

    pub fn inbox_id(&self) -> InboxIdRef<'_> {
        &self.inbox_id
    }

    pub fn recovery_identifier(&self) -> &Identifier {
        &self.recovery_identifier
    }

    pub fn members_by_parent(&self, parent_id: &MemberIdentifier) -> Vec<Member> {
        self.members
            .values()
            .filter(|e| e.added_by_entity.eq(&Some(parent_id.clone())))
            .cloned()
            .collect()
    }

    pub fn members_by_kind(&self, kind: MemberKind) -> Vec<Member> {
        self.members
            .values()
            .filter(|e| e.kind() == kind)
            .cloned()
            .collect()
    }

    pub fn identifiers(&self) -> Vec<Identifier> {
        let mut address_members: Vec<_> = self.members.values().cloned().collect();

        address_members.sort_by_key(|m| m.client_timestamp_ns.unwrap_or(u64::MAX));

        address_members
            .into_iter()
            .filter_map(|member| match member.identifier {
                MemberIdentifier::Ethereum(eth) => Some(Identifier::Ethereum(eth)),
                MemberIdentifier::Passkey(pk) => Some(Identifier::Passkey(pk)),
                _ => None,
            })
            .collect()
    }

    pub fn installation_ids(&self) -> Vec<Vec<u8>> {
        self.members_by_kind(MemberKind::Installation)
            .into_iter()
            .filter_map(|member| match member.identifier {
                MemberIdentifier::Installation(ident::Installation(key)) => Some(key),
                _ => None,
            })
            .collect()
    }

    pub fn installations(&self) -> Vec<Installation> {
        self.members()
            .into_iter()
            .filter_map(|member| match member.identifier {
                MemberIdentifier::Installation(ident::Installation(id)) => Some(Installation {
                    id,
                    client_timestamp_ns: member.client_timestamp_ns,
                }),
                _ => None,
            })
            .collect()
    }

    pub fn diff(&self, new_state: &Self) -> AssociationStateDiff {
        let new_members: Vec<MemberIdentifier> = new_state
            .members
            .keys()
            .filter(|new_member_identifier| !self.members.contains_key(new_member_identifier))
            .cloned()
            .collect();

        let removed_members: Vec<MemberIdentifier> = self
            .members
            .keys()
            .filter(|existing_member_identifier| {
                !new_state.members.contains_key(existing_member_identifier)
            })
            .cloned()
            .collect();

        AssociationStateDiff {
            new_members,
            removed_members,
        }
    }

    /// Converts the [`AssociationState`] to a diff that represents all members
    /// of the inbox at the current state.
    pub fn as_diff(&self) -> AssociationStateDiff {
        AssociationStateDiff {
            new_members: self.members.keys().cloned().collect(),
            removed_members: vec![],
        }
    }

    pub fn new(
        account_identifier: Identifier,
        nonce: u64,
        chain_id: Option<u64>,
    ) -> Result<Self, AssociationError> {
        let member_identifier: MemberIdentifier = account_identifier.clone().into();

        let inbox_id = account_identifier.inbox_id(nonce)?;
        let new_member = Member::new(member_identifier.clone(), None, None, chain_id);
        Ok(Self {
            members: HashMap::from_iter([(member_identifier, new_member)]),
            seen_signatures: HashSet::new(),
            recovery_identifier: account_identifier,
            recovery_chain_id: chain_id,
            inbox_id,
            handles: Vec::new(),
            handle_claims: 0,
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn can_add_remove() {
        let starting_state = AssociationState::new(Identifier::rand_ethereum(), 0, None).unwrap();
        let new_entity = Member::default();
        let with_add = starting_state.add(new_entity.clone());
        assert!(with_add.get(&new_entity.identifier).is_some());
        assert!(starting_state.get(&new_entity.identifier).is_none());
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn can_diff() {
        let starting_state = AssociationState::new(Identifier::rand_ethereum(), 0, None).unwrap();
        let entity_1 = Member::default();
        let entity_2 = Member::default();
        let entity_3 = Member::default();

        let state_1 = starting_state.add(entity_1.clone()).add(entity_2.clone());
        let state_2 = state_1.remove(&entity_1.identifier).add(entity_3.clone());

        let diff = state_1.diff(&state_2);

        assert_eq!(diff.new_members, vec![entity_3.identifier]);
        assert_eq!(diff.removed_members, vec![entity_1.identifier]);
    }
}
