//! [`AssociationState`] describes a single point in time for an Inbox where it contains a set of
//! associated [`MemberIdentifier`]'s, which may be one of [`MemberKind::Address`]
//! or[`MemberKind::Installation`]. A diff between two states can be calculated to determine
//! a change of membership between two periods of time. [XIP-46](https://github.com/xmtp/XIPs/pull/53)

use std::{
    collections::{HashMap, HashSet},
    fmt::{Debug, Write},
};

use prost::Message;
use xmtp_db::association_state::StoredAssociationState;
use xmtp_proto::{
    xmtp::identity::associations::AssociationState as AssociationStateProto, ConversionError,
};

use super::{
    ident,
    member::{Identifier, Member},
    AssociationError, MemberIdentifier, MemberKind,
};
use crate::InboxIdRef;

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
    pub(crate) seen_signatures: HashSet<Vec<u8>>,
}

impl std::fmt::Debug for AssociationState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut members = String::new();
        for member in self.members().keys() {
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

        write!(
            f,
            "AssociationState {{ inbox_id: {}, members: {}, recovery: {}, seen_signatures: {} }}",
            self.inbox_id, members, self.recovery_identifier, signatures
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
                ))
            }
        };
        Ok(ident)
    }
}

impl TryFrom<StoredAssociationState> for AssociationState {
    type Error = ConversionError;

    fn try_from(stored_state: StoredAssociationState) -> Result<Self, Self::Error> {
        AssociationStateProto::decode(stored_state.state.as_slice())?.try_into()
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

    pub fn set_recovery_identifier(&self, recovery_identifier: Identifier) -> Self {
        let mut new_state = self.clone();
        new_state.recovery_identifier = recovery_identifier;

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

    pub fn members(&self) -> Vec<Member> {
        let mut sorted_members: Vec<_> = state.members().values().cloned().collect();
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
        self.members()
            .values()
            .filter(|e| e.added_by_entity.eq(&Some(parent_id.clone())))
            .cloned()
            .collect()
    }

    pub fn members_by_kind(&self, kind: MemberKind) -> Vec<Member> {
        self.members()
            .values()
            .filter(|e| e.kind() == kind)
            .cloned()
            .collect()
    }

    pub fn identifiers(&self) -> Vec<Identifier> {
        self.members()
            .values()
            .cloned()
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
            .members()
            .keys()
            .filter(|new_member_identifier| !self.members.contains_key(new_member_identifier))
            .cloned()
            .collect();

        let removed_members: Vec<MemberIdentifier> = self
            .members()
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
            new_members: self.members().keys().cloned().collect(),
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
            inbox_id,
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

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
