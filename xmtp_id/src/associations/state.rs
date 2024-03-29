use std::collections::{HashMap, HashSet};

use thiserror::Error;

use super::{entity::Entity, EntityRole};

#[derive(Debug, Error)]
pub enum StateError {
    #[error("Not found")]
    NotFound,
    #[error("Replay detected")]
    ReplayDetected,
}

#[derive(Clone, Debug)]
pub struct AssociationState {
    pub current_entities: HashMap<String, Entity>,
    // Stores the entity as it was at the time it was added
    pub entities_by_event: HashMap<String, Entity>,
    pub revoked_association_hashes: HashSet<String>,
    pub allowlisted_association_hashes: HashSet<String>,
    pub recovery_address: Option<String>,
}

impl AssociationState {
    pub fn add(&self, entity: Entity, event_hash: String) -> Result<Self, StateError> {
        self.replay_check(&event_hash)?;
        let mut new_state = self.clone();
        let _ = new_state
            .entities_by_event
            .insert(event_hash, entity.clone());
        let _ = new_state.current_entities.insert(entity.id.clone(), entity);

        Ok(new_state)
    }

    pub fn set_recovery_address(&self, recovery_address: String) -> Self {
        let mut new_state = self.clone();
        new_state.recovery_address = Some(recovery_address);

        new_state
    }

    pub fn get(&self, id: &String) -> Option<Entity> {
        self.current_entities.get(id).map(|e| e.clone())
    }

    pub fn has_seen(&self, event_hash: &String) -> bool {
        self.entities_by_event.contains_key(event_hash)
    }

    fn replay_check(&self, event_hash: &String) -> Result<(), StateError> {
        if self.has_seen(event_hash) {
            return Err(StateError::ReplayDetected);
        }

        Ok(())
    }

    pub fn apply_revocation(
        &self,
        revoked_association_hash: String,
        allowlisted_association_hashes: Vec<String>,
    ) -> Self {
        let mut new_state = self.clone();
        let _ = new_state
            .revoked_association_hashes
            .insert(revoked_association_hash);
        new_state
            .allowlisted_association_hashes
            .extend(allowlisted_association_hashes);

        new_state
    }

    pub fn was_association_revoked(&self, association_hash: &String) -> bool {
        self.revoked_association_hashes.contains(association_hash)
    }

    pub fn entities(&self) -> Vec<Entity> {
        self.current_entities.values().cloned().collect()
    }

    pub fn entities_by_role(&self, role: EntityRole) -> Vec<Entity> {
        self.current_entities
            .values()
            .filter(|e| e.role == role)
            .cloned()
            .collect()
    }

    pub fn new() -> Self {
        Self {
            current_entities: HashMap::new(),
            entities_by_event: HashMap::new(),
            revoked_association_hashes: HashSet::new(),
            allowlisted_association_hashes: HashSet::new(),
            recovery_address: None,
        }
    }
}

impl Default for AssociationState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::associations::test_utils::rand_string;

    use super::*;

    #[test]
    fn can_add_remove() {
        let starting_state = AssociationState::new();
        let new_entity = Entity::default();
        let with_add = starting_state
            .add(new_entity.clone(), rand_string())
            .unwrap();
        assert!(with_add.get(&new_entity.id).is_some());
        assert!(starting_state.get(&new_entity.id).is_none());
    }
}
