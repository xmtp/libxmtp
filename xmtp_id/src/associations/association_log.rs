use super::hashes::generate_inbox_id;
use super::member::{Member, MemberIdentifier, MemberKind};
use super::serialization::DeserializationError;
use super::signature::{SignatureError, SignatureKind};
use super::state::AssociationState;
use super::verified_signature::VerifiedSignature;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AssociationError {
    #[error("Error creating association {0}")]
    Generic(String),
    #[error("Multiple create operations detected")]
    MultipleCreate,
    #[error("XID not yet created")]
    NotCreated,
    #[error("Signature validation failed {0}")]
    Signature(#[from] SignatureError),
    #[error("Member of kind {0} not allowed to add {1}")]
    MemberNotAllowed(MemberKind, MemberKind),
    #[error("Missing existing member")]
    MissingExistingMember,
    #[error("Legacy key is only allowed to be associated using a legacy signature with nonce 0")]
    LegacySignatureReuse,
    #[error("The new member identifier does not match the signer")]
    NewMemberIdSignatureMismatch,
    #[error("Wrong inbox_id specified on association")]
    WrongInboxId,
    #[error("Signature not allowed for role {0:?} {1:?}")]
    SignatureNotAllowed(String, String),
    #[error("Replay detected")]
    Replay,
    #[error("Deserialization error {0}")]
    Deserialization(#[from] DeserializationError),
    #[error("Missing identity update")]
    MissingIdentityUpdate,
}

pub trait IdentityAction: Send + 'static {
    fn update_state(
        &self,
        existing_state: Option<AssociationState>,
        client_timestamp_ns: u64,
    ) -> Result<AssociationState, AssociationError>;
    fn signatures(&self) -> Vec<Vec<u8>>;
    fn replay_check(&self, state: &AssociationState) -> Result<(), AssociationError> {
        let signatures = self.signatures();
        for signature in signatures {
            if state.has_seen(&signature) {
                return Err(AssociationError::Replay);
            }
        }

        Ok(())
    }
}

/// CreateInbox Action
#[derive(Debug, Clone)]
pub struct CreateInbox {
    pub nonce: u64,
    pub account_address: String,
    pub initial_address_signature: VerifiedSignature,
}

impl IdentityAction for CreateInbox {
    fn update_state(
        &self,
        existing_state: Option<AssociationState>,
        _client_timestamp_ns: u64,
    ) -> Result<AssociationState, AssociationError> {
        if existing_state.is_some() {
            return Err(AssociationError::MultipleCreate);
        }

        let account_address = self.account_address.clone();
        let recovered_signer = self.initial_address_signature.signer.clone();
        if recovered_signer.ne(&MemberIdentifier::Address(
            account_address.clone().to_lowercase(),
        )) {
            return Err(AssociationError::MissingExistingMember);
        }

        allowed_signature_for_kind(&MemberKind::Address, &self.initial_address_signature.kind)?;

        if self.initial_address_signature.kind == SignatureKind::LegacyDelegated && self.nonce != 0
        {
            return Err(AssociationError::LegacySignatureReuse);
        }

        Ok(AssociationState::new(account_address, self.nonce))
    }

    fn signatures(&self) -> Vec<Vec<u8>> {
        vec![self.initial_address_signature.raw_bytes.clone()]
    }
}

/// AddAssociation Action
#[derive(Debug, Clone)]
pub struct AddAssociation {
    pub new_member_signature: VerifiedSignature,
    pub new_member_identifier: MemberIdentifier,
    pub existing_member_signature: VerifiedSignature,
}

impl IdentityAction for AddAssociation {
    fn update_state(
        &self,
        maybe_existing_state: Option<AssociationState>,
        client_timestamp_ns: u64,
    ) -> Result<AssociationState, AssociationError> {
        let existing_state = maybe_existing_state.ok_or(AssociationError::NotCreated)?;
        self.replay_check(&existing_state)?;

        // Validate the new member signature and get the recovered signer
        let new_member_address = &self.new_member_signature.signer;
        // Validate the existing member signature and get the recovedred signer
        let existing_member_identifier = &self.existing_member_signature.signer;

        if new_member_address.ne(&self.new_member_identifier) {
            return Err(AssociationError::NewMemberIdSignatureMismatch);
        }

        // You cannot add yourself
        if new_member_address == existing_member_identifier {
            return Err(AssociationError::Generic("tried to add self".to_string()));
        }

        // Only allow LegacyDelegated signatures on XIDs with a nonce of 0
        // Otherwise the client should use the regular wallet signature to create
        if (is_legacy_signature(&self.new_member_signature)
            || is_legacy_signature(&self.existing_member_signature))
            && existing_state.inbox_id().ne(&generate_inbox_id(
                existing_member_identifier.to_string().as_str(),
                &0,
            ))
        {
            return Err(AssociationError::LegacySignatureReuse);
        }

        allowed_signature_for_kind(
            &self.new_member_identifier.kind(),
            &self.new_member_signature.kind,
        )?;

        let existing_member = existing_state.get(existing_member_identifier);

        let existing_entity_id = match existing_member {
            // If there is an existing member of the XID, use that member's ID
            Some(member) => member.identifier,
            None => {
                // Get the recovery address from the state as a MemberIdentifier
                let recovery_identifier: MemberIdentifier =
                    existing_state.recovery_address().clone().into();

                // Check if it is a signature from the recovery address, which is allowed to add members
                if existing_member_identifier.ne(&recovery_identifier) {
                    return Err(AssociationError::MissingExistingMember);
                }
                // BUT, the recovery address has to be used with a real wallet signature, can't be delegated
                if is_legacy_signature(&self.existing_member_signature) {
                    return Err(AssociationError::LegacySignatureReuse);
                }
                // If it is a real wallet signature, then it is allowed to add members
                recovery_identifier
            }
        };

        // Ensure that the existing member signature is correct for the existing member type
        allowed_signature_for_kind(
            &existing_entity_id.kind(),
            &self.existing_member_signature.kind,
        )?;

        // Ensure that the new member signature is correct for the new member type
        allowed_association(
            existing_member_identifier.kind(),
            self.new_member_identifier.kind(),
        )?;

        let new_member = Member::new(
            new_member_address.clone(),
            Some(existing_entity_id),
            Some(client_timestamp_ns),
        );

        Ok(existing_state.add(new_member))
    }

    fn signatures(&self) -> Vec<Vec<u8>> {
        vec![
            self.existing_member_signature.raw_bytes.clone(),
            self.new_member_signature.raw_bytes.clone(),
        ]
    }
}

/// RevokeAssociation Action
#[derive(Debug, Clone)]
pub struct RevokeAssociation {
    pub recovery_address_signature: VerifiedSignature,
    pub revoked_member: MemberIdentifier,
}

impl IdentityAction for RevokeAssociation {
    fn update_state(
        &self,
        maybe_existing_state: Option<AssociationState>,
        _client_timestamp_ns: u64,
    ) -> Result<AssociationState, AssociationError> {
        let existing_state = maybe_existing_state.ok_or(AssociationError::NotCreated)?;
        self.replay_check(&existing_state)?;

        if is_legacy_signature(&self.recovery_address_signature) {
            return Err(AssociationError::SignatureNotAllowed(
                MemberKind::Address.to_string(),
                SignatureKind::LegacyDelegated.to_string(),
            ));
        }
        // Don't need to check for replay here since revocation is idempotent
        let recovery_signer = &self.recovery_address_signature.signer;
        // Make sure there is a recovery address set on the state
        let state_recovery_address = existing_state.recovery_address();

        // Ensure this message is signed by the recovery address
        if recovery_signer.ne(&MemberIdentifier::Address(
            state_recovery_address.clone().to_lowercase(),
        )) {
            return Err(AssociationError::MissingExistingMember);
        }

        let installations_to_remove: Vec<Member> = existing_state
            .members_by_parent(&self.revoked_member)
            .into_iter()
            // Only remove children if they are installations
            .filter(|child| child.kind() == MemberKind::Installation)
            .collect();

        // Actually apply the revocation to the parent
        let new_state = existing_state.remove(&self.revoked_member);

        Ok(installations_to_remove
            .iter()
            .fold(new_state, |state, installation| {
                state.remove(&installation.identifier)
            }))
    }

    fn signatures(&self) -> Vec<Vec<u8>> {
        vec![self.recovery_address_signature.raw_bytes.clone()]
    }
}

/// ChangeRecoveryAddress Action
#[derive(Debug, Clone)]
pub struct ChangeRecoveryAddress {
    pub recovery_address_signature: VerifiedSignature,
    pub new_recovery_address: String,
}

impl IdentityAction for ChangeRecoveryAddress {
    fn update_state(
        &self,
        existing_state: Option<AssociationState>,
        _client_timestamp_ns: u64,
    ) -> Result<AssociationState, AssociationError> {
        let existing_state = existing_state.ok_or(AssociationError::NotCreated)?;
        self.replay_check(&existing_state)?;

        if is_legacy_signature(&self.recovery_address_signature) {
            return Err(AssociationError::SignatureNotAllowed(
                MemberKind::Address.to_string(),
                SignatureKind::LegacyDelegated.to_string(),
            ));
        }

        let recovery_signer = &self.recovery_address_signature.signer;
        if recovery_signer.ne(&existing_state.recovery_address().clone().into()) {
            return Err(AssociationError::MissingExistingMember);
        }

        Ok(existing_state.set_recovery_address(self.new_recovery_address.clone()))
    }

    fn signatures(&self) -> Vec<Vec<u8>> {
        vec![self.recovery_address_signature.raw_bytes.clone()]
    }
}

/// All possible Action types that can be used inside an `IdentityUpdate`
#[derive(Debug, Clone)]
pub enum Action {
    CreateInbox(CreateInbox),
    AddAssociation(AddAssociation),
    RevokeAssociation(RevokeAssociation),
    ChangeRecoveryAddress(ChangeRecoveryAddress),
}

impl IdentityAction for Action {
    fn update_state(
        &self,
        existing_state: Option<AssociationState>,
        client_timestamp_ns: u64,
    ) -> Result<AssociationState, AssociationError> {
        match self {
            Action::CreateInbox(event) => event.update_state(existing_state, client_timestamp_ns),
            Action::AddAssociation(event) => {
                event.update_state(existing_state, client_timestamp_ns)
            }
            Action::RevokeAssociation(event) => {
                event.update_state(existing_state, client_timestamp_ns)
            }
            Action::ChangeRecoveryAddress(event) => {
                event.update_state(existing_state, client_timestamp_ns)
            }
        }
    }

    fn signatures(&self) -> Vec<Vec<u8>> {
        match self {
            Action::CreateInbox(event) => event.signatures(),
            Action::AddAssociation(event) => event.signatures(),
            Action::RevokeAssociation(event) => event.signatures(),
            Action::ChangeRecoveryAddress(event) => event.signatures(),
        }
    }
}

/// An `IdentityUpdate` contains one or more Actions that can be applied to the AssociationState
#[derive(Debug, Clone)]
pub struct IdentityUpdate {
    pub inbox_id: String,
    pub client_timestamp_ns: u64,
    pub actions: Vec<Action>,
}

impl IdentityUpdate {
    pub fn new(actions: Vec<Action>, inbox_id: String, client_timestamp_ns: u64) -> Self {
        Self {
            inbox_id,
            actions,
            client_timestamp_ns,
        }
    }
}

impl IdentityAction for IdentityUpdate {
    fn update_state(
        &self,
        existing_state: Option<AssociationState>,
        _client_timestamp_ns: u64,
    ) -> Result<AssociationState, AssociationError> {
        let mut state = existing_state.clone();
        for action in &self.actions {
            state = Some(action.update_state(state, self.client_timestamp_ns)?);
        }

        let new_state = state.ok_or(AssociationError::NotCreated)?;
        if new_state.inbox_id().ne(&self.inbox_id) {
            tracing::error!(
                "state inbox id mismatch, old: {}, new: {}",
                self.inbox_id,
                new_state.inbox_id()
            );
            return Err(AssociationError::WrongInboxId);
        }

        // After all the updates in the LogEntry have been processed, add the list of signatures to the state
        // so that the signatures can not be re-used in subsequent updates
        Ok(new_state.add_seen_signatures(self.signatures()))
    }

    fn signatures(&self) -> Vec<Vec<u8>> {
        self.actions
            .iter()
            .flat_map(|action| action.signatures())
            .collect()
    }
}

#[allow(clippy::borrowed_box)]
fn is_legacy_signature(signature: &VerifiedSignature) -> bool {
    signature.kind == SignatureKind::LegacyDelegated
}

fn allowed_association(
    existing_member_kind: MemberKind,
    new_member_kind: MemberKind,
) -> Result<(), AssociationError> {
    // The only disallowed association is an installation adding an installation
    if existing_member_kind == MemberKind::Installation
        && new_member_kind == MemberKind::Installation
    {
        return Err(AssociationError::MemberNotAllowed(
            existing_member_kind,
            new_member_kind,
        ));
    }

    Ok(())
}

// Ensure that the type of signature matches the new entity's role.
fn allowed_signature_for_kind(
    role: &MemberKind,
    signature_kind: &SignatureKind,
) -> Result<(), AssociationError> {
    let is_ok = match role {
        MemberKind::Address => match signature_kind {
            SignatureKind::Erc191 => true,
            SignatureKind::Erc1271 => true,
            SignatureKind::InstallationKey => false,
            SignatureKind::LegacyDelegated => true,
        },
        MemberKind::Installation => match signature_kind {
            SignatureKind::Erc191 => false,
            SignatureKind::Erc1271 => false,
            SignatureKind::InstallationKey => true,
            SignatureKind::LegacyDelegated => false,
        },
    };

    if !is_ok {
        return Err(AssociationError::SignatureNotAllowed(
            role.to_string(),
            signature_kind.to_string(),
        ));
    }

    Ok(())
}
