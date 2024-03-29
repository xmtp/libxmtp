mod entity;
mod state;
#[cfg(test)]
mod test_utils;

pub use self::entity::{Entity, EntityRole};
pub use self::state::{AssociationState, StateError};
use sha2::{Digest, Sha256};

use thiserror::Error;

const ALLOWED_CREATE_ENTITY_ROLES: [EntityRole; 2] = [EntityRole::LegacyKey, EntityRole::Address];

#[derive(Debug, Error)]
pub enum SignatureError {
    #[error("Signature validation failed")]
    Invalid,
}

#[derive(Debug, Error)]
pub enum AssociationError {
    #[error("Error creating association {0}")]
    Generic(String),
    #[error("Multiple create operations detect")]
    MultipleCreate,
    #[error("Signature validation failed {0}")]
    Signature(#[from] SignatureError),
    #[error("State update failed")]
    StateError(#[from] StateError),
    #[error("Missing existing member")]
    MissingExistingMember,
    #[error("Signature not allowed for role {0:?} {1:?}")]
    SignatureNotAllowed(EntityRole, SignatureKind),
    #[error("Added by revoked member")]
    AddedByRevokedMember,
    #[error("Replay detected")]
    Replay,
    #[error("No recovery address")]
    NoRecoveryAddress,
}

#[derive(Clone, Debug)]
pub enum SignatureKind {
    Erc191,
    Erc1271,
    InstallationKey,
    LegacyKey,
}

pub trait Signature {
    fn recover_signer(&self) -> Result<String, SignatureError>;
    fn signature_kind(&self) -> SignatureKind;
    fn text(&self) -> String;
}

pub trait LogEntry {
    fn update_state(
        &self,
        existing_state: AssociationState,
    ) -> Result<AssociationState, AssociationError>;
    fn hash(&self) -> String;
}

pub struct CreateXidEntry {
    pub nonce: u32,
    pub signature: Box<dyn Signature>,
    pub recovery_address: String,
    pub entity_role: EntityRole,
}

impl LogEntry for CreateXidEntry {
    fn update_state(
        &self,
        existing_state: AssociationState,
    ) -> Result<AssociationState, AssociationError> {
        // Verify that the existing state is empty
        if !existing_state.entities().is_empty() {
            return Err(AssociationError::MultipleCreate);
        }

        // This verifies that the signature is valid
        let signer_address = self.signature.recover_signer()?;
        if !ALLOWED_CREATE_ENTITY_ROLES.contains(&self.entity_role) {
            return Err(AssociationError::Generic("invalid entity role".to_string()));
        }

        let signature_kind = self.signature.signature_kind();
        if !allowed_signature_for_role(&self.entity_role, &signature_kind) {
            return Err(AssociationError::SignatureNotAllowed(
                self.entity_role.clone(),
                signature_kind,
            ));
        }

        let entity = Entity::new(self.entity_role.clone(), signer_address, false);
        Ok(existing_state
            .set_recovery_address(self.recovery_address.clone())
            .add(entity, self.hash())?)
    }

    fn hash(&self) -> String {
        // Once we have real signatures the nonce and the recovery address should become part of the text
        let inputs = format!(
            "{}{}{}",
            self.nonce,
            self.signature.text(),
            self.recovery_address
        );

        sha256_string(inputs)
    }
}

pub struct AddAssociationEntry {
    pub nonce: u32,
    pub new_member_role: EntityRole,
    pub existing_member_signature: Box<dyn Signature>,
    pub new_member_signature: Box<dyn Signature>,
}

impl AddAssociationEntry {
    pub fn new_member_address(&self) -> String {
        self.new_member_signature.recover_signer().unwrap()
    }
}

impl LogEntry for AddAssociationEntry {
    fn update_state(
        &self,
        existing_state: AssociationState,
    ) -> Result<AssociationState, AssociationError> {
        let association_hash = self.hash();
        if existing_state.has_seen(&association_hash) {
            return Err(AssociationError::Replay);
        }

        // Recovery address has to be set
        if existing_state.recovery_address.is_none() {
            return Err(AssociationError::NoRecoveryAddress);
        }

        let new_member_address = self.new_member_signature.recover_signer()?;
        let existing_member_address = self.existing_member_signature.recover_signer()?;
        if new_member_address == existing_member_address {
            return Err(AssociationError::Generic("tried to add self".to_string()));
        }

        // Get the current version of the entity that added this new entry. If it has been revoked and added back, it will now be unrevoked
        let existing_entity = existing_state
            .get(&existing_member_address)
            .ok_or(AssociationError::MissingExistingMember)?;

        if existing_entity.is_revoked {
            // The entity that added this member is currently revoked. Check if this particular association is allowlisted
            if !existing_state
                .allowlisted_association_hashes
                .contains(&association_hash)
            {
                return Err(AssociationError::AddedByRevokedMember);
            }
        }

        // Make sure that the signature type lines up with the role
        if !allowed_signature_for_role(
            &self.new_member_role,
            &self.new_member_signature.signature_kind(),
        ) {
            return Err(AssociationError::SignatureNotAllowed(
                self.new_member_role.clone(),
                self.new_member_signature.signature_kind(),
            ));
        }

        // Check to see if the new member was revoked
        let is_new_member_revoked = existing_state.was_association_revoked(&association_hash);
        let new_member = Entity::new(
            self.new_member_role.clone(),
            new_member_address,
            is_new_member_revoked,
        );

        println!(
            "Adding new entity to state {:?} with hash {}",
            &new_member, &association_hash
        );

        Ok(existing_state.add(new_member, association_hash)?)
    }

    fn hash(&self) -> String {
        let inputs = format!(
            "{}{:?}{}{}",
            self.nonce,
            self.new_member_role,
            self.existing_member_signature.text(),
            self.new_member_signature.text()
        );
        sha256_string(inputs)
    }
}

pub struct RevokeAssociationEntry {
    pub nonce: u32,
    pub recovery_address_signature: Box<dyn Signature>,
    pub revoked_association_hash: String,
    pub allowed_child_hashes: Vec<String>,
}

impl LogEntry for RevokeAssociationEntry {
    fn update_state(
        &self,
        existing_state: AssociationState,
    ) -> Result<AssociationState, AssociationError> {
        // Don't need to check for replay here since revocation is idempotent
        let recovery_signer = self.recovery_address_signature.recover_signer()?;
        // Make sure there is a recovery address set on the state
        let state_recovery_address = existing_state
            .recovery_address
            .clone()
            .ok_or(AssociationError::NoRecoveryAddress)?;

        // Ensure this message is signed by the recovery address
        if recovery_signer != state_recovery_address {
            return Err(AssociationError::MissingExistingMember);
        }

        // Actually apply the revocation
        Ok(existing_state.apply_revocation(
            self.revoked_association_hash.clone(),
            self.allowed_child_hashes.clone(),
        ))
    }

    fn hash(&self) -> String {
        let inputs = format!(
            "{}{}{}{}",
            self.nonce,
            self.recovery_address_signature.text(),
            self.revoked_association_hash,
            self.allowed_child_hashes.join(",")
        );
        sha256_string(inputs)
    }
}

pub struct ChangeRecoveryAddressEntry {
    pub nonce: u32,
    pub recovery_address_signature: Box<dyn Signature>,
    pub new_recovery_address: String,
}

pub enum RecoveryLogEntry {
    CreateXid(CreateXidEntry),
    RevokeAssociation(RevokeAssociationEntry),
}

impl LogEntry for RecoveryLogEntry {
    fn update_state(
        &self,
        existing_state: AssociationState,
    ) -> Result<AssociationState, AssociationError> {
        match self {
            RecoveryLogEntry::CreateXid(create_xid) => create_xid.update_state(existing_state),
            RecoveryLogEntry::RevokeAssociation(revoke_association) => {
                revoke_association.update_state(existing_state)
            }
        }
    }

    fn hash(&self) -> String {
        match self {
            RecoveryLogEntry::CreateXid(create_xid) => create_xid.hash(),
            RecoveryLogEntry::RevokeAssociation(revoke_association) => revoke_association.hash(),
        }
    }
}

pub fn apply_updates(
    initial_state: AssociationState,
    associations: Vec<AddAssociationEntry>,
) -> AssociationState {
    associations.iter().fold(initial_state, |state, update| {
        match update.update_state(state.clone()) {
            Ok(new_state) => new_state,
            Err(err) => {
                println!("invalid entry {}", err);
                state
            }
        }
    })
}

pub fn get_initial_state(recovery_log: Vec<RecoveryLogEntry>) -> AssociationState {
    recovery_log
        .iter()
        .fold(AssociationState::new(), |state, update| {
            match update.update_state(state.clone()) {
                Ok(new_state) => new_state,
                Err(err) => {
                    println!("invalid entry {}", err);
                    state
                }
            }
        })
}

pub fn get_state(
    recovery_log: Vec<RecoveryLogEntry>,
    association_updates: Vec<AddAssociationEntry>,
) -> AssociationState {
    let state = get_initial_state(recovery_log);
    println!("Initial state {:?}", state);
    apply_updates(state, association_updates)
}

fn sha256_string(input: String) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();
    format!("{:x}", result)
}

// Ensure that the type of signature matches the new entity's role.
pub fn allowed_signature_for_role(role: &EntityRole, signature_kind: &SignatureKind) -> bool {
    match role {
        EntityRole::Address => match signature_kind {
            SignatureKind::Erc191 => true,
            SignatureKind::Erc1271 => true,
            SignatureKind::InstallationKey => false,
            SignatureKind::LegacyKey => false,
        },
        EntityRole::LegacyKey => match signature_kind {
            SignatureKind::Erc191 => false,
            SignatureKind::Erc1271 => false,
            SignatureKind::InstallationKey => false,
            SignatureKind::LegacyKey => true,
        },
        EntityRole::Installation => match signature_kind {
            SignatureKind::Erc191 => false,
            SignatureKind::Erc1271 => false,
            SignatureKind::InstallationKey => true,
            SignatureKind::LegacyKey => false,
        },
    }
}

/**
 * Revocation properties
 * 1. Revoking an association will mark the entity added as revoked
 * 2. Revoking an association will prevent new associations from being created with an `existing_entity_signature` from the revoked entity
 * 3. Entities created with an `existing_entity_signature` of a revoked entity can be protected from revocation if they were added before the revocation
 * 4. Revoked entities can be re-added with a new signature, so long as a new nonce is included in the signature
 * 5. A revocation payload can be added to a subset of the association log. When this happens, all entities present in the subset will have the same revocation status that they have in the full log.
 */

#[cfg(test)]
mod tests {
    use self::test_utils::{rand_string, rand_u32};

    use super::*;

    struct MockSignature {
        is_valid: bool,
        signer_identity: String,
        signature_kind: SignatureKind,
    }

    impl MockSignature {
        pub fn new_boxed(
            is_valid: bool,
            signer_identity: String,
            signature_kind: SignatureKind,
        ) -> Box<Self> {
            Box::new(Self {
                is_valid,
                signer_identity,
                signature_kind,
            })
        }
    }

    impl Default for AddAssociationEntry {
        fn default() -> Self {
            return Self {
                nonce: rand_u32(),
                new_member_role: EntityRole::Address,
                existing_member_signature: MockSignature::new_boxed(
                    true,
                    rand_string(),
                    SignatureKind::Erc191,
                ),
                new_member_signature: MockSignature::new_boxed(
                    true,
                    rand_string(),
                    SignatureKind::Erc191,
                ),
            };
        }
    }

    impl Default for CreateXidEntry {
        fn default() -> Self {
            let signer = rand_string();
            return Self {
                nonce: rand_u32(),
                signature: MockSignature::new_boxed(true, signer.clone(), SignatureKind::Erc191),
                recovery_address: signer,
                entity_role: EntityRole::Address,
            };
        }
    }

    impl Default for RevokeAssociationEntry {
        fn default() -> Self {
            let signer = rand_string();
            return Self {
                nonce: rand_u32(),
                recovery_address_signature: MockSignature::new_boxed(
                    true,
                    signer,
                    SignatureKind::Erc191,
                ),
                revoked_association_hash: rand_string(),
                allowed_child_hashes: vec![],
            };
        }
    }

    impl Signature for MockSignature {
        fn signature_kind(&self) -> SignatureKind {
            self.signature_kind.clone()
        }

        fn recover_signer(&self) -> Result<String, SignatureError> {
            match self.is_valid {
                true => Ok(self.signer_identity.clone()),
                false => Err(SignatureError::Invalid),
            }
        }

        fn text(&self) -> String {
            self.signer_identity.clone()
        }
    }

    fn init_recovery_log() -> (Vec<RecoveryLogEntry>, String) {
        let create_request = CreateXidEntry::default();
        let creator_address = create_request.signature.recover_signer().unwrap();
        let entries = vec![RecoveryLogEntry::CreateXid(create_request)];

        (entries, creator_address)
    }

    #[test]
    fn test_create_and_add() {
        let create_request = CreateXidEntry::default();
        let creator_address = create_request.signature.recover_signer().unwrap();
        let recovery_log = vec![RecoveryLogEntry::CreateXid(create_request)];
        let mut state = get_state(recovery_log, vec![]);
        assert_eq!(state.entities().len(), 1);

        let add_installation_entry = AddAssociationEntry {
            existing_member_signature: MockSignature::new_boxed(
                true,
                creator_address,
                SignatureKind::Erc191,
            ),
            ..AddAssociationEntry::default()
        };
        state = apply_updates(state, vec![add_installation_entry]);
        assert_eq!(state.entities().len(), 2);
    }

    #[test]
    fn create_and_add_chained() {
        let (recovery_log, creator_address) = init_recovery_log();
        let add_first_association = AddAssociationEntry {
            existing_member_signature: MockSignature::new_boxed(
                true,
                creator_address,
                SignatureKind::Erc191,
            ),
            ..AddAssociationEntry::default()
        };
        let first_association_address = add_first_association
            .new_member_signature
            .recover_signer()
            .unwrap();

        let add_second_association = AddAssociationEntry {
            existing_member_signature: MockSignature::new_boxed(
                true,
                first_association_address.clone(),
                SignatureKind::Erc191,
            ),
            ..AddAssociationEntry::default()
        };

        let state = get_state(
            recovery_log,
            vec![add_first_association, add_second_association],
        );

        assert_eq!(state.entities().len(), 3);
        assert_eq!(
            state.get(&first_association_address).unwrap().is_revoked,
            false
        );
        assert_eq!(
            state.get(&first_association_address).unwrap().id,
            first_association_address
        );
    }

    #[test]
    fn add_from_revoked() {
        let (mut recovery_log, creator_address) = init_recovery_log();
        let add_association = AddAssociationEntry {
            existing_member_signature: MockSignature::new_boxed(
                true,
                creator_address.clone(),
                SignatureKind::Erc191,
            ),
            ..AddAssociationEntry::default()
        };

        recovery_log.push(RecoveryLogEntry::RevokeAssociation(
            RevokeAssociationEntry {
                recovery_address_signature: MockSignature::new_boxed(
                    true,
                    // Creator address is the recovery address, so this is valid
                    creator_address,
                    SignatureKind::Erc191,
                ),
                revoked_association_hash: add_association.hash(),
                // Not setting any allowed children here, since this doesn't have any
                ..Default::default()
            },
        ));

        let add_another_association = AddAssociationEntry {
            existing_member_signature: MockSignature::new_boxed(
                true,
                add_association.new_member_address(),
                SignatureKind::Erc191,
            ),
            ..AddAssociationEntry::default()
        };
        let second_new_member_address = add_another_association.new_member_address();

        let state = get_state(recovery_log, vec![add_association, add_another_association]);
        assert_eq!(state.entities().len(), 2);
        assert!(state.get(&second_new_member_address).is_none())
    }

    #[test]
    fn add_from_re_added() {
        let (mut recovery_log, creator_address) = init_recovery_log();

        let add_association = AddAssociationEntry {
            existing_member_signature: MockSignature::new_boxed(
                true,
                creator_address.clone(),
                SignatureKind::Erc191,
            ),
            ..AddAssociationEntry::default()
        };

        let new_member_address = add_association.new_member_address();

        recovery_log.push(RecoveryLogEntry::RevokeAssociation(
            RevokeAssociationEntry {
                recovery_address_signature: MockSignature::new_boxed(
                    true,
                    // Creator address is the recovery address, so this is valid
                    creator_address.clone(),
                    SignatureKind::Erc191,
                ),
                revoked_association_hash: add_association.hash(),
                // Not setting any allowed children here, since this doesn't have any
                ..Default::default()
            },
        ));

        let add_same_member_back = AddAssociationEntry {
            nonce: rand_u32(),
            existing_member_signature: MockSignature::new_boxed(
                true,
                creator_address.clone(),
                SignatureKind::Erc191,
            ),
            new_member_signature: MockSignature::new_boxed(
                true,
                new_member_address.clone(),
                SignatureKind::Erc191,
            ),
            ..Default::default()
        };

        let state = get_state(recovery_log, vec![add_association, add_same_member_back]);
        assert_eq!(state.get(&new_member_address).unwrap().is_revoked, false)
    }

    #[test]
    fn protect_children_from_revocation() {
        let (mut recovery_log, creator_address) = init_recovery_log();

        let add_association = AddAssociationEntry {
            existing_member_signature: MockSignature::new_boxed(
                true,
                creator_address.clone(),
                SignatureKind::Erc191,
            ),
            ..AddAssociationEntry::default()
        };

        let add_child = AddAssociationEntry {
            existing_member_signature: MockSignature::new_boxed(
                true,
                add_association.new_member_address(),
                SignatureKind::Erc191,
            ),
            ..AddAssociationEntry::default()
        };

        let add_grandchild = AddAssociationEntry {
            existing_member_signature: MockSignature::new_boxed(
                true,
                add_child.new_member_address(),
                SignatureKind::Erc191,
            ),
            ..AddAssociationEntry::default()
        };

        recovery_log.push(RecoveryLogEntry::RevokeAssociation(
            RevokeAssociationEntry {
                recovery_address_signature: MockSignature::new_boxed(
                    true,
                    // Creator address is the recovery address, so this is valid
                    creator_address.clone(),
                    SignatureKind::Erc191,
                ),
                revoked_association_hash: add_association.hash(),
                allowed_child_hashes: vec![add_child.hash()],
                // Not setting any allowed children here, since this doesn't have any
                ..Default::default()
            },
        ));

        let first_member_address = add_association.new_member_address();
        let first_child_address = add_child.new_member_address();
        let grandchild_address = add_grandchild.new_member_address();

        let state = get_state(
            recovery_log,
            vec![add_association, add_child, add_grandchild],
        );
        assert_eq!(state.get(&first_member_address).unwrap().is_revoked, true);
        assert_eq!(state.get(&first_child_address).unwrap().is_revoked, false);
        assert_eq!(state.get(&grandchild_address).unwrap().is_revoked, false);
    }

    #[test]
    fn fail_if_ancestor_missing() {
        let (recovery_log, creator_address) = init_recovery_log();

        let add_association = AddAssociationEntry {
            existing_member_signature: MockSignature::new_boxed(
                true,
                creator_address.clone(),
                SignatureKind::Erc191,
            ),
            ..AddAssociationEntry::default()
        };

        let add_child = AddAssociationEntry {
            existing_member_signature: MockSignature::new_boxed(
                true,
                add_association.new_member_address(),
                SignatureKind::Erc191,
            ),
            ..AddAssociationEntry::default()
        };

        let add_grandchild = AddAssociationEntry {
            existing_member_signature: MockSignature::new_boxed(
                true,
                add_child.new_member_address(),
                SignatureKind::Erc191,
            ),
            ..AddAssociationEntry::default()
        };

        let grandchild_address = add_grandchild.new_member_address();

        let state = get_state(
            recovery_log,
            // Deliberately omitting the add_child, which is necessary here
            vec![add_association, add_grandchild],
        );

        assert_eq!(state.get(&grandchild_address).is_none(), true);
    }
}
