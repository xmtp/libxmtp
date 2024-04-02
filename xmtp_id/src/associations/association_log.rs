use super::entity::{Entity, EntityRole};
use super::hashes::{generate_xid, sha256_string};
use super::signature::{Signature, SignatureError, SignatureKind};
use super::state::{AssociationState, StateError};

use thiserror::Error;

// const ALLOWED_CREATE_ENTITY_ROLES: [EntityRole; 2] = [EntityRole::LegacyKey, EntityRole::Address];

#[derive(Debug, Error, PartialEq)]
pub enum AssociationError {
    #[error("Error creating association {0}")]
    Generic(String),
    #[error("Multiple create operations detected")]
    MultipleCreate,
    #[error("XID not yet created")]
    NotCreated,
    #[error("Signature validation failed {0}")]
    Signature(#[from] SignatureError),
    #[error("State update failed")]
    StateError(#[from] StateError),
    #[error("Missing existing member")]
    MissingExistingMember,
    #[error("Legacy key is only allowed to be associated using a legacy signature with nonce 0")]
    LegacySignatureReuse,
    #[error("Signature not allowed for role {0:?} {1:?}")]
    SignatureNotAllowed(EntityRole, SignatureKind),
    #[error("Replay detected")]
    Replay,
}

pub trait LogEntry {
    fn update_state(
        &self,
        existing_state: Option<AssociationState>,
    ) -> Result<AssociationState, AssociationError>;
    fn hash(&self) -> String;
}

pub struct CreateXid {
    pub nonce: u32,
    pub account_address: String,
    pub initial_association: AddAssociation,
}

impl LogEntry for CreateXid {
    fn update_state(
        &self,
        existing_state: Option<AssociationState>,
    ) -> Result<AssociationState, AssociationError> {
        if existing_state.is_some() {
            return Err(AssociationError::MultipleCreate);
        }

        let account_address = self.account_address.clone();

        let initial_state = AssociationState::new(account_address, self.nonce);
        let new_state = self.initial_association.update_state(Some(initial_state))?;

        Ok(new_state.mark_event_seen(self.hash()))
    }

    fn hash(&self) -> String {
        // Once we have real signatures the nonce and the recovery address should become part of the text
        let inputs = format!(
            "{}{}{}",
            self.nonce,
            self.account_address,
            self.initial_association.hash()
        );

        sha256_string(inputs)
    }
}

pub struct AddAssociation {
    pub client_timestamp_ns: u32,
    pub new_member_role: EntityRole,
    pub new_member_signature: Box<dyn Signature>,
    pub existing_member_signature: Box<dyn Signature>,
}

impl AddAssociation {
    pub fn new_member_address(&self) -> String {
        self.new_member_signature.recover_signer().unwrap()
    }
}

impl LogEntry for AddAssociation {
    fn update_state(
        &self,
        maybe_existing_state: Option<AssociationState>,
    ) -> Result<AssociationState, AssociationError> {
        let existing_state = maybe_existing_state.ok_or(AssociationError::NotCreated)?;

        // Catch replays per-association
        // The real hash function should probably just be the signature text, but since that's stubbed out I have some more inputs
        let association_hash = self.hash();
        if existing_state.has_seen(&association_hash) {
            return Err(AssociationError::Replay);
        }

        let new_member_address = self.new_member_signature.recover_signer()?;
        let existing_member_address = self.existing_member_signature.recover_signer()?;
        if new_member_address == existing_member_address {
            return Err(AssociationError::Generic("tried to add self".to_string()));
        }

        if self.new_member_role == EntityRole::LegacyKey {
            if existing_state.xid != generate_xid(&existing_member_address, &0) {
                return Err(AssociationError::LegacySignatureReuse);
            }
        }

        // Find the existing entity that authorized this add
        let existing_entity = existing_state
            .get(&existing_member_address)
            .ok_or(AssociationError::MissingExistingMember)?;

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

        let new_member = Entity::new(
            self.new_member_role.clone(),
            new_member_address,
            Some(existing_entity.id),
        );

        println!(
            "Adding new entity to state {:?} with hash {}",
            &new_member, &association_hash
        );

        Ok(existing_state.add(new_member).mark_event_seen(self.hash()))
    }

    fn hash(&self) -> String {
        let inputs = format!(
            "{}{:?}{}{}",
            self.client_timestamp_ns,
            self.new_member_role,
            self.existing_member_signature.text(),
            self.new_member_signature.text()
        );
        sha256_string(inputs)
    }
}

pub struct RevokeAssociation {
    pub client_timestamp_ns: u32,
    pub recovery_address_signature: Box<dyn Signature>,
    pub revoked_member: String,
}

impl LogEntry for RevokeAssociation {
    fn update_state(
        &self,
        maybe_existing_state: Option<AssociationState>,
    ) -> Result<AssociationState, AssociationError> {
        let existing_state = maybe_existing_state.ok_or(AssociationError::NotCreated)?;
        // Don't need to check for replay here since revocation is idempotent
        let recovery_signer = self.recovery_address_signature.recover_signer()?;
        // Make sure there is a recovery address set on the state
        let state_recovery_address = existing_state.recovery_address.clone();

        // Ensure this message is signed by the recovery address
        if recovery_signer != state_recovery_address {
            return Err(AssociationError::MissingExistingMember);
        }

        let installations_to_remove: Vec<Entity> = existing_state
            .entities_by_parent(&self.revoked_member)
            .into_iter()
            // Only remove children if they are installations
            .filter(|child| child.role == EntityRole::Installation)
            .collect();

        // Actually apply the revocation to the parent
        let new_state = existing_state.remove(self.revoked_member.clone());

        Ok(installations_to_remove
            .iter()
            .fold(new_state, |state, installation| {
                state.remove(installation.id.clone())
            })
            .mark_event_seen(self.hash()))
    }

    fn hash(&self) -> String {
        let inputs = format!(
            "{}{}{}",
            self.client_timestamp_ns,
            self.recovery_address_signature.text(),
            self.revoked_member,
        );
        sha256_string(inputs)
    }
}

pub enum AssociationEvent {
    CreateXid(CreateXid),
    AddAssociation(AddAssociation),
    RevokeAssociation(RevokeAssociation),
}

impl LogEntry for AssociationEvent {
    fn update_state(
        &self,
        existing_state: Option<AssociationState>,
    ) -> Result<AssociationState, AssociationError> {
        match self {
            AssociationEvent::CreateXid(event) => event.update_state(existing_state),
            AssociationEvent::AddAssociation(event) => event.update_state(existing_state),
            AssociationEvent::RevokeAssociation(event) => event.update_state(existing_state),
        }
    }

    fn hash(&self) -> String {
        match self {
            AssociationEvent::CreateXid(event) => event.hash(),
            AssociationEvent::AddAssociation(event) => event.hash(),
            AssociationEvent::RevokeAssociation(event) => event.hash(),
        }
    }
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
