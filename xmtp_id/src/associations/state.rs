use std::collections::{HashMap, HashSet};

use super::{hashes::generate_inbox_id, member::Member, MemberIdentifier, MemberKind};

#[derive(Debug, Clone)]
pub struct AssociationStateDiff {
    pub new_members: Vec<MemberIdentifier>,
    pub removed_members: Vec<MemberIdentifier>,
}

#[derive(Clone, Debug)]
pub struct AssociationState {
    pub(crate) inbox_id: String,
    pub(crate) members: HashMap<MemberIdentifier, Member>,
    pub(crate) recovery_address: String,
    pub(crate) seen_signatures: HashSet<Vec<u8>>,
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

    pub fn set_recovery_address(&self, recovery_address: String) -> Self {
        let mut new_state = self.clone();
        new_state.recovery_address = recovery_address;

        new_state
    }

    pub fn get(&self, identifier: &MemberIdentifier) -> Option<Member> {
        self.members.get(identifier).cloned()
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
        self.members.values().cloned().collect()
    }

    pub fn inbox_id(&self) -> &String {
        &self.inbox_id
    }

    pub fn recovery_address(&self) -> &String {
        &self.recovery_address
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

    pub fn new(account_address: String, nonce: u64) -> Self {
        let inbox_id = generate_inbox_id(&account_address, &nonce);
        let identifier = MemberIdentifier::Address(account_address.clone());
        let new_member = Member::new(identifier.clone(), None);
        Self {
            members: {
                let mut members = HashMap::new();
                members.insert(identifier, new_member);
                members
            },
            seen_signatures: HashSet::new(),
            recovery_address: account_address,
            inbox_id,
        }
    }
}

impl From<AssociationState> for AssociationStateDiff {
    fn from(state: AssociationState) -> Self {
        AssociationStateDiff {
            new_members: state.members.keys().cloned().collect(),
            removed_members: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::associations::test_utils::rand_string;

    use super::*;

    #[test]
    fn can_add_remove() {
        let starting_state = AssociationState::new(rand_string(), 0);
        let new_entity = Member::default();
        let with_add = starting_state.add(new_entity.clone());
        assert!(with_add.get(&new_entity.identifier).is_some());
        assert!(starting_state.get(&new_entity.identifier).is_none());
    }

    #[test]
    fn can_diff() {
        let starting_state = AssociationState::new(rand_string(), 0);
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
