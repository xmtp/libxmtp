use super::member::{HasMemberKind, Identifier, Member, MemberIdentifier, MemberKind};
use super::serialization::DeserializationError;
use super::signature::{SignatureError, SignatureKind};
use super::state::AssociationState;
use super::verified_signature::VerifiedSignature;
use thiserror::Error;
use xmtp_common::ErrorCode;

#[derive(Debug, Error, ErrorCode)]
pub enum AssociationError {
    /// Generic association error.
    ///
    /// Unclassified association error. Not retryable.
    #[error("Error creating association {0}")]
    Generic(String),
    /// Multiple create operations.
    ///
    /// Duplicate inbox creation detected. Not retryable.
    #[error("Multiple create operations detected")]
    MultipleCreate,
    /// XID not yet created.
    ///
    /// Operating on inbox that doesn't exist yet. Not retryable.
    #[error("XID not yet created")]
    NotCreated,
    #[error("Signature validation failed {0}")]
    #[error_code(inherit)]
    Signature(#[from] SignatureError),
    /// Member not allowed.
    ///
    /// Member kind cannot add the specified kind. Not retryable.
    #[error("Member of kind {0} not allowed to add {1}")]
    MemberNotAllowed(MemberKind, MemberKind),
    /// Missing existing member.
    ///
    /// Required signer not found or signer identity mismatch. Not retryable.
    #[error("Missing existing member")]
    MissingExistingMember,
    /// Legacy signature reuse.
    ///
    /// Legacy delegated signature used in disallowed context. Not retryable.
    #[error("Legacy key is only allowed to be associated using a legacy signature with nonce 0")]
    LegacySignatureReuse,
    /// New member ID signature mismatch.
    ///
    /// Signer doesn't match new member identifier. Not retryable.
    #[error("The new member identifier does not match the signer")]
    NewMemberIdSignatureMismatch,
    /// Wrong Inbox ID.
    ///
    /// Incorrect inbox_id in association. Not retryable.
    #[error("Wrong inbox_id specified on association")]
    WrongInboxId,
    /// Signature not allowed.
    ///
    /// Signature type not permitted for this role. Not retryable.
    #[error("Signature not allowed for role {0:?} {1:?}")]
    SignatureNotAllowed(String, String),
    /// Replay detected.
    ///
    /// Replayed identity update detected. Not retryable.
    #[error("Replay detected")]
    Replay,
    #[error("Deserialization error {0}")]
    #[error_code(inherit)]
    Deserialization(#[from] DeserializationError),
    /// Missing identity update.
    ///
    /// Required identity update not provided. Not retryable.
    #[error("Missing identity update")]
    MissingIdentityUpdate,
    /// Chain ID mismatch.
    ///
    /// Smart contract wallet chain ID changed. Not retryable.
    #[error("Wrong chain id. Initially added with {0} but now signing from {1}")]
    ChainIdMismatch(u64, u64),
    /// Invalid account address.
    ///
    /// Address is not 42-char hex starting with 0x. Not retryable.
    #[error("Invalid account address: Must be 42 hex characters, starting with '0x'.")]
    InvalidAccountAddress,
    /// Not an identifier.
    ///
    /// Value is not a valid public identifier. Not retryable.
    #[error("{0} are not a public identifier")]
    NotIdentifier(String),
    #[error(transparent)]
    #[error_code(inherit)]
    Convert(#[from] xmtp_proto::ConversionError),
}

pub trait IdentityAction: Send {
    fn update_state(
        &self,
        existing_state: Option<AssociationState>,
        client_timestamp_ns: u64,
    ) -> Result<AssociationState, AssociationError>;
    /// The canonical replay key of every signature the action carries (see [`VerifiedSignature`]).
    fn replay_keys(&self) -> Vec<Vec<u8>>;
    // implements: IDENT-050
    fn replay_check(&self, state: &AssociationState) -> Result<(), AssociationError> {
        if self.replay_keys().iter().any(|key| state.has_seen(key)) {
            return Err(AssociationError::Replay);
        }
        Ok(())
    }
}

/// CreateInbox Action
#[derive(Debug, Clone)]
pub struct CreateInbox {
    pub nonce: u64,
    pub account_identifier: Identifier,
    pub initial_identifier_signature: VerifiedSignature,
}

impl IdentityAction for CreateInbox {
    // implements: IDENT-011, IDENT-012
    fn update_state(
        &self,
        existing_state: Option<AssociationState>,
        _client_timestamp_ns: u64,
    ) -> Result<AssociationState, AssociationError> {
        if existing_state.is_some() {
            return Err(AssociationError::MultipleCreate);
        }

        let account_address = self.account_identifier.clone();
        let recovered_signer = self.initial_identifier_signature.signer.clone();
        if recovered_signer != account_address {
            return Err(AssociationError::MissingExistingMember);
        }

        allowed_signature_for_kind(
            &self.account_identifier.kind(),
            &self.initial_identifier_signature.kind,
        )?;

        if self.initial_identifier_signature.kind == SignatureKind::LegacyDelegated
            && self.nonce != 0
        {
            return Err(AssociationError::LegacySignatureReuse);
        }

        AssociationState::new(
            account_address,
            self.nonce,
            self.initial_identifier_signature.chain_id,
        )
    }

    fn replay_keys(&self) -> Vec<Vec<u8>> {
        vec![self.initial_identifier_signature.replay_key.clone()]
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
    // implements: IDENT-042
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
        let existing_member_identifier = existing_member_identifier.clone();
        let identifier: Option<Identifier> = existing_member_identifier.clone().into();
        if let Some(identifier) = identifier
            && (is_legacy_signature(&self.new_member_signature)
                || is_legacy_signature(&self.existing_member_signature))
            && existing_state.inbox_id() != identifier.inbox_id(0)?
        {
            return Err(AssociationError::LegacySignatureReuse);
        }

        allowed_signature_for_kind(
            &self.new_member_identifier.kind(),
            &self.new_member_signature.kind,
        )?;

        let existing_member = existing_state.get(&existing_member_identifier);

        if let Some(member) = existing_member {
            verify_chain_id_matches(member, &self.existing_member_signature)?;
        }

        let existing_entity_id = match existing_member {
            // If there is an existing member of the XID, use that member's ID
            Some(member) => member.identifier.clone(),
            None => {
                // Get the recovery address from the state as a MemberIdentifier
                let recovery_identifier = existing_state.recovery_identifier().clone().into();

                // Check if it is a signature from the recovery address, which is allowed to add members
                if existing_member_identifier != recovery_identifier {
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
            self.new_member_signature.chain_id,
        );

        Ok(existing_state.add(new_member))
    }

    fn replay_keys(&self) -> Vec<Vec<u8>> {
        vec![
            self.existing_member_signature.replay_key.clone(),
            self.new_member_signature.replay_key.clone(),
        ]
    }
}

/// RevokeAssociation Action
#[derive(Debug, Clone)]
pub struct RevokeAssociation {
    pub recovery_identifier_signature: VerifiedSignature,
    pub revoked_member: MemberIdentifier,
}

impl IdentityAction for RevokeAssociation {
    // implements: IDENT-043, IDENT-044
    fn update_state(
        &self,
        maybe_existing_state: Option<AssociationState>,
        _client_timestamp_ns: u64,
    ) -> Result<AssociationState, AssociationError> {
        let existing_state = maybe_existing_state.ok_or(AssociationError::NotCreated)?;
        self.replay_check(&existing_state)?;

        // Ensure that the new signature is on the same chain as the signature to create the account
        let existing_member = existing_state.get(&self.recovery_identifier_signature.signer);
        if let Some(member) = existing_member {
            verify_chain_id_matches(member, &self.recovery_identifier_signature)?;
        }

        if is_legacy_signature(&self.recovery_identifier_signature) {
            return Err(AssociationError::SignatureNotAllowed(
                MemberKind::Ethereum.to_string(),
                SignatureKind::LegacyDelegated.to_string(),
            ));
        }
        // Don't need to check for replay here since revocation is idempotent
        let recovery_signer = &self.recovery_identifier_signature.signer;
        // Make sure there is a recovery address set on the state
        let state_recovery_identifier: MemberIdentifier =
            existing_state.recovery_identifier.clone().into();

        if *recovery_signer != state_recovery_identifier {
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

    fn replay_keys(&self) -> Vec<Vec<u8>> {
        vec![self.recovery_identifier_signature.replay_key.clone()]
    }
}

/// ChangeRecoveryAddress Action
#[derive(Debug, Clone)]
pub struct ChangeRecoveryIdentity {
    pub recovery_identifier_signature: VerifiedSignature,
    pub new_recovery_identifier: Identifier,
}

impl IdentityAction for ChangeRecoveryIdentity {
    fn update_state(
        &self,
        existing_state: Option<AssociationState>,
        _client_timestamp_ns: u64,
    ) -> Result<AssociationState, AssociationError> {
        let existing_state = existing_state.ok_or(AssociationError::NotCreated)?;
        self.replay_check(&existing_state)?;

        let existing_member = existing_state.get(&self.recovery_identifier_signature.signer);
        if let Some(member) = existing_member {
            verify_chain_id_matches(member, &self.recovery_identifier_signature)?;
        }

        if is_legacy_signature(&self.recovery_identifier_signature) {
            return Err(AssociationError::SignatureNotAllowed(
                MemberKind::Ethereum.to_string(),
                SignatureKind::LegacyDelegated.to_string(),
            ));
        }

        let recovery_signer = &self.recovery_identifier_signature.signer;
        if existing_state.recovery_identifier() != recovery_signer {
            return Err(AssociationError::MissingExistingMember);
        }

        Ok(existing_state.set_recovery_identifier(self.new_recovery_identifier.clone()))
    }

    fn replay_keys(&self) -> Vec<Vec<u8>> {
        vec![self.recovery_identifier_signature.replay_key.clone()]
    }
}

/// All possible Action types that can be used inside an `IdentityUpdate`
#[derive(Debug, Clone)]
pub enum Action {
    CreateInbox(CreateInbox),
    AddAssociation(AddAssociation),
    RevokeAssociation(RevokeAssociation),
    ChangeRecoveryIdentity(ChangeRecoveryIdentity),
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
            Action::ChangeRecoveryIdentity(event) => {
                event.update_state(existing_state, client_timestamp_ns)
            }
        }
    }

    fn replay_keys(&self) -> Vec<Vec<u8>> {
        match self {
            Action::CreateInbox(event) => event.replay_keys(),
            Action::AddAssociation(event) => event.replay_keys(),
            Action::RevokeAssociation(event) => event.replay_keys(),
            Action::ChangeRecoveryIdentity(event) => event.replay_keys(),
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

    /// Get the signature kind used to create this inbox if this update contains a CreateInbox action.
    /// Returns None if there is no CreateInbox action in this update.
    ///
    /// This is useful for determining whether an identity was created with a Smart Contract Wallet (Erc1271)
    /// or an Externally Owned Account/EOA (Erc191) signature
    pub fn creation_signature_kind(&self) -> Option<SignatureKind> {
        self.actions.iter().find_map(|action| match action {
            Action::CreateInbox(create_inbox) => {
                Some(create_inbox.initial_identifier_signature.kind.clone())
            }
            _ => None,
        })
    }
}

impl IdentityAction for IdentityUpdate {
    // implements: IDENT-002, IDENT-003
    fn update_state(
        &self,
        existing_state: Option<AssociationState>,
        _client_timestamp_ns: u64,
    ) -> Result<AssociationState, AssociationError> {
        let mut state = existing_state;
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
        Ok(new_state.add_seen_signatures(self.replay_keys()))
    }

    fn replay_keys(&self) -> Vec<Vec<u8>> {
        self.actions
            .iter()
            .flat_map(|action| action.replay_keys())
            .collect()
    }
}

#[allow(clippy::borrowed_box)]
fn is_legacy_signature(signature: &VerifiedSignature) -> bool {
    signature.kind == SignatureKind::LegacyDelegated
}

// implements: IDENT-041
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
// implements: IDENT-031
fn allowed_signature_for_kind(
    role: &MemberKind,
    signature_kind: &SignatureKind,
) -> Result<(), AssociationError> {
    let is_ok = match role {
        MemberKind::Ethereum => matches!(
            signature_kind,
            SignatureKind::Erc191 | SignatureKind::Erc1271 | SignatureKind::LegacyDelegated
        ),
        MemberKind::Installation => matches!(signature_kind, SignatureKind::InstallationKey),
        MemberKind::Passkey => matches!(signature_kind, SignatureKind::P256),
    };

    if !is_ok {
        return Err(AssociationError::SignatureNotAllowed(
            role.to_string(),
            signature_kind.to_string(),
        ));
    }

    Ok(())
}

fn verify_chain_id_matches(
    member: &Member,
    signature: &VerifiedSignature,
) -> Result<(), AssociationError> {
    if member.added_on_chain_id != signature.chain_id {
        return Err(AssociationError::ChainIdMismatch(
            member.added_on_chain_id.unwrap_or(0),
            signature.chain_id.unwrap_or(0),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        InboxOwner,
        associations::{
            apply_update,
            builder::SignatureRequestBuilder,
            get_state,
            test_utils::{
                MockSmartContractSignatureVerifier, WalletTestExt, ecdsa_negated_s_alias,
                ecdsa_recovery_byte_alias, p256_negated_s_alias,
            },
            unverified::{UnverifiedAction, UnverifiedIdentityUpdate, UnverifiedSignature},
        },
        utils::passkey::PasskeyUser,
    };
    use alloy::signers::local::PrivateKeySigner;

    async fn sign(
        builder: SignatureRequestBuilder,
        owners: &[&dyn InboxOwner],
    ) -> UnverifiedIdentityUpdate {
        let mut request = builder.build();
        for owner in owners {
            let signature = owner.sign(&request.signature_text()).unwrap();
            request
                .add_signature(signature, MockSmartContractSignatureVerifier::new(false))
                .await
                .unwrap();
        }
        request.build_identity_update().unwrap()
    }

    async fn verify(update: &UnverifiedIdentityUpdate) -> IdentityUpdate {
        update
            .to_verified(MockSmartContractSignatureVerifier::new(false))
            .await
            .unwrap()
    }

    /// `update` with every signature rewritten by `alias`; what was signed is untouched.
    fn reencode(
        update: &UnverifiedIdentityUpdate,
        alias: impl Fn(&mut UnverifiedSignature),
    ) -> UnverifiedIdentityUpdate {
        let mut update = update.clone();
        for action in &mut update.actions {
            match action {
                UnverifiedAction::CreateInbox(a) => alias(&mut a.initial_identifier_signature),
                UnverifiedAction::AddAssociation(a) => {
                    alias(&mut a.existing_member_signature);
                    alias(&mut a.new_member_signature);
                }
                UnverifiedAction::RevokeAssociation(a) => {
                    alias(&mut a.recovery_identifier_signature)
                }
                UnverifiedAction::ChangeRecoveryAddress(a) => {
                    alias(&mut a.recovery_identifier_signature)
                }
            }
        }
        update
    }

    fn recovery_byte_form(signature: &mut UnverifiedSignature) {
        if let UnverifiedSignature::RecoverableEcdsa(s) = signature {
            s.signature_bytes = ecdsa_recovery_byte_alias(&s.signature_bytes);
        }
    }

    fn negated_s_form(signature: &mut UnverifiedSignature) {
        match signature {
            UnverifiedSignature::RecoverableEcdsa(s) => {
                s.signature_bytes = ecdsa_negated_s_alias(&s.signature_bytes)
            }
            UnverifiedSignature::Passkey(s) => s.signature = p256_negated_s_alias(&s.signature),
            _ => {}
        }
    }

    /// Adds `member` with `owner`'s signature, then revokes it. Returns the state just before the
    /// add, the state after the revoke, and the signed add for replaying.
    async fn add_then_revoke(
        recovery: &PrivateKeySigner,
        member: &dyn InboxOwner,
    ) -> (AssociationState, AssociationState, UnverifiedIdentityUpdate) {
        let inbox_id = recovery.get_inbox_id(0);
        let recovery_id = recovery.identifier();
        let member_id: MemberIdentifier = member.get_identifier().unwrap().into();
        let create = sign(
            SignatureRequestBuilder::new(&inbox_id).create_inbox(recovery_id.clone(), 0),
            &[recovery],
        )
        .await;
        let add = sign(
            SignatureRequestBuilder::new(&inbox_id)
                .add_association(member_id.clone(), recovery_id.clone().into()),
            &[recovery, member],
        )
        .await;
        let revoke = sign(
            SignatureRequestBuilder::new(&inbox_id)
                .revoke_association(recovery_id.into(), member_id.clone()),
            &[recovery],
        )
        .await;

        let created = get_state([verify(&create).await]).unwrap();
        let added = apply_update(created.clone(), verify(&add).await).unwrap();
        assert!(added.get(&member_id).is_some());
        let revoked = apply_update(added, verify(&revoke).await).unwrap();
        assert!(revoked.get(&member_id).is_none());
        (created, revoked, add)
    }

    /// Replays `add` re-encoded by `alias` after the revoke. The re-encoding is a valid add in its
    /// own right, yet shares its replay keys with the original, so it is rejected and the member
    /// stays revoked.
    async fn assert_alias_is_replay(
        created: &AssociationState,
        revoked: &AssociationState,
        add: &UnverifiedIdentityUpdate,
        member: &MemberIdentifier,
        alias: impl Fn(&mut UnverifiedSignature),
    ) {
        let replay = reencode(add, alias);
        assert_ne!(&replay, add, "the alias must change the signature bytes");
        let replay = verify(&replay).await;
        let fresh = apply_update(created.clone(), replay.clone()).unwrap();
        assert!(fresh.get(member).is_some(), "the alias is a valid add");

        let result = apply_update(revoked.clone(), replay);
        assert!(
            matches!(result, Err(AssociationError::Replay)),
            "{result:?}"
        );
        assert!(revoked.get(member).is_none());
    }

    /// A revoked wallet's add, republished with its wallet signatures moved between the 0/1 and
    /// 27/28 recovery-byte forms or with `s` negated, is the same add and must not re-add the
    /// wallet.
    #[xmtp_common::test]
    // verifies: IDENT-050
    async fn identity_replay_aliases_wallet_add_after_revoke() {
        let recovery = PrivateKeySigner::random();
        let member = PrivateKeySigner::random();
        let member_id = member.member_identifier();
        let (created, revoked, add) = add_then_revoke(&recovery, &member).await;

        assert_alias_is_replay(&created, &revoked, &add, &member_id, recovery_byte_form).await;
        assert_alias_is_replay(&created, &revoked, &add, &member_id, negated_s_form).await;
        assert_alias_is_replay(&created, &revoked, &add, &member_id, |s| {
            negated_s_form(s);
            recovery_byte_form(s);
        })
        .await;
    }

    /// A revoked passkey's add, republished with the passkey's `s` negated and the recovery
    /// wallet's recovery byte re-encoded, is the same add and must not re-add the passkey.
    #[xmtp_common::test]
    // verifies: IDENT-050
    async fn identity_replay_aliases_passkey_add_after_revoke() {
        let recovery = PrivateKeySigner::random();
        let passkey = PasskeyUser::new().await;
        let member_id: MemberIdentifier = passkey.identifier().into();
        let (created, revoked, add) = add_then_revoke(&recovery, &passkey).await;

        assert_alias_is_replay(&created, &revoked, &add, &member_id, |s| match s {
            UnverifiedSignature::Passkey(_) => negated_s_form(s),
            _ => recovery_byte_form(s),
        })
        .await;
    }
}
