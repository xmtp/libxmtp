use std::collections::{HashMap, HashSet};

use thiserror::Error;

use super::{entity::Entity, hashes::generate_xid, EntityRole};

#[derive(Debug, Error, PartialEq)]
pub enum StateError {
    #[error("Not found")]
    NotFound,
    #[error("Replay detected")]
    ReplayDetected,
}

#[derive(Clone, Debug)]
pub struct AssociationState {
    pub xid: String,
    pub current_entities: HashMap<String, Entity>,
    pub recovery_address: String,
    pub seen_events: HashSet<String>,
}

impl AssociationState {
    pub fn add(&self, entity: Entity) -> Self {
        let mut new_state = self.clone();
        let _ = new_state.current_entities.insert(entity.id.clone(), entity);

        new_state
    }

    pub fn set_recovery_address(&self, recovery_address: String) -> Self {
        let mut new_state = self.clone();
        new_state.recovery_address = recovery_address;

        new_state
    }

    pub fn get(&self, id: &String) -> Option<Entity> {
        self.current_entities.get(id).map(|e| e.clone())
    }

    pub fn mark_event_seen(&self, event_hash: String) -> Self {
        let mut new_state = self.clone();
        new_state.seen_events.insert(event_hash);

        new_state
    }

    pub fn has_seen(&self, event_hash: &String) -> bool {
        self.seen_events.contains(event_hash)
    }

    pub fn remove(&self, entity_id: String) -> Self {
        let mut new_state = self.clone();
        let _ = new_state.current_entities.remove(&entity_id);

        new_state
    }

    pub fn entities(&self) -> Vec<Entity> {
        self.current_entities.values().cloned().collect()
    }

    pub fn entities_by_parent(&self, parent_id: &String) -> Vec<Entity> {
        self.current_entities
            .values()
            .filter(|e| e.added_by_entity == Some(parent_id.clone()))
            .cloned()
            .collect()
    }

    pub fn entities_by_role(&self, role: EntityRole) -> Vec<Entity> {
        self.current_entities
            .values()
            .filter(|e| e.role == role)
            .cloned()
            .collect()
    }

    pub fn new(account_address: String, nonce: u32) -> Self {
        let xid = generate_xid(&account_address, &nonce);
        let new_entity = Entity::new(EntityRole::Address, account_address.clone(), None);
        Self {
            current_entities: {
                let mut entities = HashMap::new();
                entities.insert(account_address.clone(), new_entity);
                entities
            },
            seen_events: HashSet::new(),
            recovery_address: account_address,
            xid,
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
        let new_entity = Entity::default();
        let with_add = starting_state.add(new_entity.clone());
        assert!(with_add.get(&new_entity.id).is_some());
        assert!(starting_state.get(&new_entity.id).is_none());
    }
}
