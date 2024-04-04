use std::collections::{HashMap, HashSet};

use super::{hashes::generate_inbox_id, member::Member, MemberIdentifier, MemberKind};

#[derive(Clone, Debug)]
pub struct AssociationState {
    inbox_id: String,
    members: HashMap<MemberIdentifier, Member>,
    recovery_address: String,
    seen_signatures: HashSet<Vec<u8>>,
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
}
