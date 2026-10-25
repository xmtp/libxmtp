use super::handle::{Handle, InvalidHandle, MAX_HANDLE_CLAIMS};
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
    /// Already a member.
    ///
    /// The member being added is already a member of the inbox. Not retryable.
    #[error("The new member is already a member of the inbox")]
    AlreadyMember,
    /// Empty update.
    ///
    /// The identity update carries no actions. Not retryable.
    #[error("Identity update has no actions")]
    EmptyUpdate,
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
    #[error(transparent)]
    #[error_code(inherit)]
    InvalidHandle(#[from] InvalidHandle),
    /// Claim not alone.
    ///
    /// An identity update that claims a handle carries another action. Not retryable.
    #[error("an identity update that claims a handle must carry no other action")]
    ClaimNotAlone,
    /// Handle claim limit reached.
    ///
    /// The inbox's log already holds the most handle claims it can hold. Not retryable.
    #[error("the inbox has reached the limit of handle claims")]
    HandleClaimLimit,
    /// Recovery not member.
    ///
    /// The new recovery identifier is not a member of the inbox. Not retryable.
    #[error("the new recovery identifier is not a member of the inbox")]
    RecoveryNotMember,
}

pub trait IdentityAction: Send {
    fn update_state(
        &self,
        existing_state: Option<AssociationState>,
        client_timestamp_ns: u64,
    ) -> Result<AssociationState, AssociationError>;
    /// The replay key of every signature the action carries: its signer and signature text.
    fn replay_keys(&self) -> Vec<Vec<u8>>;
    // implements: IDENT-090
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

        AssociationState::new(
            account_address,
            self.nonce,
            self.initial_identifier_signature.chain_id,
        )
    }

    fn replay_keys(&self) -> Vec<Vec<u8>> {
        vec![self.initial_identifier_signature.replay_key().to_vec()]
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
    // implements: IDENT-040, IDENT-042
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

        // Re-adding a member would rewrite its adder and spend a log slot
        if existing_state.get(&self.new_member_identifier).is_some() {
            return Err(AssociationError::AlreadyMember);
        }
        // A re-added recovery identifier must sign on the chain its role records.
        verify_signer_chain(&existing_state, &self.new_member_signature)?;

        allowed_signature_for_kind(
            &self.new_member_identifier.kind(),
            &self.new_member_signature.kind,
        )?;

        verify_signer_chain(&existing_state, &self.existing_member_signature)?;
        let existing_member = existing_state.get(existing_member_identifier);

        let existing_entity_id = match existing_member {
            // If there is an existing member of the XID, use that member's ID
            Some(member) => member.identifier.clone(),
            None => {
                // Get the recovery address from the state as a MemberIdentifier
                let recovery_identifier = existing_state.recovery_identifier().clone().into();

                // Check if it is a signature from the recovery address, which is allowed to add members
                if *existing_member_identifier != recovery_identifier {
                    return Err(AssociationError::MissingExistingMember);
                }
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
            self.existing_member_signature.replay_key().to_vec(),
            self.new_member_signature.replay_key().to_vec(),
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

        verify_signer_chain(&existing_state, &self.recovery_identifier_signature)?;

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
        vec![self.recovery_identifier_signature.replay_key().to_vec()]
    }
}

/// ChangeRecoveryAddress Action
#[derive(Debug, Clone)]
pub struct ChangeRecoveryIdentity {
    pub recovery_identifier_signature: VerifiedSignature,
    pub new_recovery_identifier: Identifier,
}

impl IdentityAction for ChangeRecoveryIdentity {
    /// Moves recovery to a current member, which takes that member's chain id. The signer and
    /// chain checks run before the membership check, so a signature from the wrong signer or
    /// chain is never reported as a membership problem.
    // implements: IDENT-091
    fn update_state(
        &self,
        existing_state: Option<AssociationState>,
        _client_timestamp_ns: u64,
    ) -> Result<AssociationState, AssociationError> {
        let existing_state = existing_state.ok_or(AssociationError::NotCreated)?;
        self.replay_check(&existing_state)?;
        verify_signer_chain(&existing_state, &self.recovery_identifier_signature)?;

        let recovery_signer = &self.recovery_identifier_signature.signer;
        if existing_state.recovery_identifier() != recovery_signer {
            return Err(AssociationError::MissingExistingMember);
        }

        let chain_id = existing_state
            .get(&self.new_recovery_identifier.clone().into())
            .ok_or(AssociationError::RecoveryNotMember)?
            .added_on_chain_id;
        Ok(existing_state.set_recovery_identifier(self.new_recovery_identifier.clone(), chain_id))
    }

    fn replay_keys(&self) -> Vec<Vec<u8>> {
        vec![self.recovery_identifier_signature.replay_key().to_vec()]
    }
}

/// ClaimHandle Action
///
/// `handle` is the text as carried. The grammar check runs when the action is
/// applied, so a claim outside the grammar fails as an invalid identity update.
#[derive(Debug, Clone)]
pub struct ClaimHandle {
    pub handle: String,
    pub signature: VerifiedSignature,
}

impl IdentityAction for ClaimHandle {
    /// Applies the claim: the grammar, a signer that is a current member on the
    /// chain it was added on, and the claim limit. The backend's uniqueness,
    /// name, and interval checks are not applied here, so a stored log always
    /// replays.
    // implements: IDENT-078, IDENT-080, IDENT-081, IDENT-082
    fn update_state(
        &self,
        existing_state: Option<AssociationState>,
        _client_timestamp_ns: u64,
    ) -> Result<AssociationState, AssociationError> {
        let existing_state = existing_state.ok_or(AssociationError::NotCreated)?;
        self.replay_check(&existing_state)?;
        let handle = Handle::parse(&self.handle)?;

        // A recovery identifier that is not a member cannot sign a claim.
        let signer = existing_state
            .get(&self.signature.signer)
            .ok_or(AssociationError::MissingExistingMember)?;
        verify_signer_chain(&existing_state, &self.signature)?;
        allowed_signature_for_kind(&signer.kind(), &self.signature.kind)?;

        if existing_state.handle_claims() >= MAX_HANDLE_CLAIMS {
            return Err(AssociationError::HandleClaimLimit);
        }

        Ok(existing_state.claim_handle(handle))
    }

    fn replay_keys(&self) -> Vec<Vec<u8>> {
        vec![self.signature.replay_key().to_vec()]
    }
}

/// All possible Action types that can be used inside an `IdentityUpdate`
#[derive(Debug, Clone)]
pub enum Action {
    CreateInbox(CreateInbox),
    AddAssociation(AddAssociation),
    RevokeAssociation(RevokeAssociation),
    ChangeRecoveryIdentity(ChangeRecoveryIdentity),
    ClaimHandle(ClaimHandle),
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
            Action::ClaimHandle(event) => event.update_state(existing_state, client_timestamp_ns),
        }
    }

    fn replay_keys(&self) -> Vec<Vec<u8>> {
        match self {
            Action::CreateInbox(event) => event.replay_keys(),
            Action::AddAssociation(event) => event.replay_keys(),
            Action::RevokeAssociation(event) => event.replay_keys(),
            Action::ChangeRecoveryIdentity(event) => event.replay_keys(),
            Action::ClaimHandle(event) => event.replay_keys(),
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

    /// Whether this update carries a `ClaimHandle`. An update that carries one
    /// and applies carries no other action.
    pub fn claims_handle(&self) -> bool {
        self.actions
            .iter()
            .any(|action| matches!(action, Action::ClaimHandle(_)))
    }
}

impl IdentityAction for IdentityUpdate {
    // implements: IDENT-001, IDENT-002, IDENT-003, IDENT-079
    fn update_state(
        &self,
        existing_state: Option<AssociationState>,
        _client_timestamp_ns: u64,
    ) -> Result<AssociationState, AssociationError> {
        if self.actions.is_empty() {
            return Err(AssociationError::EmptyUpdate);
        }
        // Checked before any action applies, so one update cannot pass the
        // backend's interval between claims for several claims.
        if self.actions.len() > 1 && self.claims_handle() {
            return Err(AssociationError::ClaimNotAlone);
        }
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
            SignatureKind::Erc191 | SignatureKind::Erc1271
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

/// Rejects a signature whose chain id differs from the one recorded for its signer: the
/// recovery role's chain when the signer is the recovery identifier, member or not, and
/// otherwise the chain a current member was added on. A signer with neither record is not
/// checked.
///
/// While the recovery identifier is a member, both records hold the same chain: creation sets
/// both from one signature, a recovery change copies the member's chain, a member's chain never
/// changes, and this check holds a re-added recovery identifier to the role's chain.
// implements: IDENT-046, IDENT-092
fn verify_signer_chain(
    state: &AssociationState,
    signature: &VerifiedSignature,
) -> Result<(), AssociationError> {
    let expected = if *state.recovery_identifier() == signature.signer {
        Some(state.recovery_chain_id)
    } else {
        state
            .get(&signature.signer)
            .map(|member| member.added_on_chain_id)
    };
    match expected {
        Some(expected) if expected != signature.chain_id => Err(AssociationError::ChainIdMismatch(
            expected.unwrap_or(0),
            signature.chain_id.unwrap_or(0),
        )),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        InboxOwner,
        associations::{
            AccountId, apply_update,
            builder::SignatureRequestBuilder,
            get_state,
            test_utils::{
                MockSmartContractSignatureVerifier, WalletTestExt, add_installation_key_signature,
                add_wallet_signature, ecdsa_negated_s_alias, ecdsa_recovery_byte_alias,
                p256_negated_s_alias,
            },
            unverified::{UnverifiedAction, UnverifiedIdentityUpdate, UnverifiedSignature},
        },
        scw_verifier::{SmartContractSignatureVerifier, ValidationResponse, VerifierError},
        utils::passkey::PasskeyUser,
    };
    use alloy::primitives::{BlockNumber, Bytes};
    use alloy::signers::local::PrivateKeySigner;
    use xmtp_common::rand_vec;
    use xmtp_cryptography::XmtpInstallationCredential;

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
                UnverifiedAction::ClaimHandle(a) => alias(&mut a.signature),
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
    // verifies: IDENT-090
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

    /// A signature by `signer` with a fresh replay key, for actions built by hand.
    fn signed_by(
        signer: &MemberIdentifier,
        kind: SignatureKind,
        chain_id: Option<u64>,
    ) -> VerifiedSignature {
        VerifiedSignature::new(signer.clone(), kind, rand_vec::<32>(), chain_id)
    }

    /// A wallet that created an inbox with ERC-191 may later verify at the same address through
    /// ERC-1271. The contract verifier accepts the new signatures, but the member and the recovery
    /// role hold no chain id, so association validation rejects them: before adding a replacement
    /// installation, before revoking one, and after the wallet is revoked as a member.
    #[xmtp_common::test(unwrap_try = true)]
    // verifies: IDENT-046
    async fn erc191_created_inbox_rejects_later_erc1271_recovery_actions() {
        let wallet = PrivateKeySigner::random();
        let wallet_id = wallet.member_identifier();
        let inbox_id = wallet.get_inbox_id(0);
        let created = sign(
            SignatureRequestBuilder::new(&inbox_id).create_inbox(wallet.identifier(), 0),
            &[&wallet],
        )
        .await;
        let created = get_state([verify(&created).await])?;
        assert_eq!(created.get(&wallet_id).unwrap().added_on_chain_id, None);

        let old_installation = XmtpInstallationCredential::new();
        let old_id = MemberIdentifier::installation(old_installation.public_bytes().to_vec());
        let mut old_add = SignatureRequestBuilder::new(&inbox_id)
            .add_association(old_id.clone(), wallet_id.clone())
            .build();
        add_wallet_signature(&mut old_add, &wallet).await;
        add_installation_key_signature(&mut old_add, &old_installation).await;
        let old_add = old_add.build_identity_update()?;
        let state = apply_update(created, verify(&old_add).await)?;

        let account_id = AccountId::new_evm(1, wallet_id.clone().to_eth_address().unwrap());
        let scw_signed =
            async |builder: SignatureRequestBuilder,
                   bytes: Vec<u8>,
                   installation: Option<&XmtpInstallationCredential>| {
                let mut request = builder.build();
                request
                    .add_signature(
                        UnverifiedSignature::new_smart_contract_wallet(
                            bytes,
                            account_id.clone(),
                            1,
                        ),
                        MockSmartContractSignatureVerifier::new(true),
                    )
                    .await
                    .unwrap();
                if let Some(installation) = installation {
                    add_installation_key_signature(&mut request, installation).await;
                }
                request
                    .build_identity_update()
                    .unwrap()
                    .to_verified(MockSmartContractSignatureVerifier::new(true))
                    .await
                    .unwrap()
            };

        let replacement = XmtpInstallationCredential::new();
        let replacement_id = MemberIdentifier::installation(replacement.public_bytes().to_vec());
        let scw_add = scw_signed(
            SignatureRequestBuilder::new(&inbox_id)
                .add_association(replacement_id.clone(), wallet_id.clone()),
            vec![1, 2, 3],
            Some(&replacement),
        )
        .await;
        let result = apply_update(state.clone(), scw_add);
        assert!(
            matches!(result, Err(AssociationError::ChainIdMismatch(0, 1))),
            "{result:?}"
        );

        let scw_revoke = scw_signed(
            SignatureRequestBuilder::new(&inbox_id)
                .revoke_association(wallet_id.clone(), old_id.clone()),
            vec![4, 5, 6],
            None,
        )
        .await;
        let result = apply_update(state.clone(), scw_revoke);
        assert!(
            matches!(result, Err(AssociationError::ChainIdMismatch(0, 1))),
            "{result:?}"
        );

        // The same actions remain possible when the wallet can still sign with ERC-191.
        let mut eoa_add = SignatureRequestBuilder::new(&inbox_id)
            .add_association(replacement_id.clone(), wallet_id.clone())
            .build();
        add_wallet_signature(&mut eoa_add, &wallet).await;
        add_installation_key_signature(&mut eoa_add, &replacement).await;
        let eoa_add = eoa_add.build_identity_update()?;
        let with_replacement = apply_update(state.clone(), verify(&eoa_add).await)?;
        assert!(with_replacement.get(&replacement_id).is_some());

        let eoa_revoke = sign(
            SignatureRequestBuilder::new(&inbox_id)
                .revoke_association(wallet_id.clone(), old_id.clone()),
            &[&wallet],
        )
        .await;
        let without_old = apply_update(state, verify(&eoa_revoke).await)?;
        assert!(without_old.get(&old_id).is_none());

        // Revoked as a member, the wallet keeps the recovery role and its missing chain id.
        let other = PrivateKeySigner::random();
        let other_id = other.member_identifier();
        let add_other = sign(
            SignatureRequestBuilder::new(&inbox_id)
                .add_association(other_id.clone(), wallet_id.clone()),
            &[&wallet, &other],
        )
        .await;
        let revoke_self = sign(
            SignatureRequestBuilder::new(&inbox_id)
                .revoke_association(wallet_id.clone(), wallet_id.clone()),
            &[&wallet],
        )
        .await;
        let with_other = apply_update(without_old, verify(&add_other).await)?;
        let recovery_only = apply_update(with_other, verify(&revoke_self).await)?;
        assert!(recovery_only.get(&wallet_id).is_none());

        let scw_revoke_other = scw_signed(
            SignatureRequestBuilder::new(&inbox_id)
                .revoke_association(wallet_id.clone(), other_id.clone()),
            vec![7, 8, 9],
            None,
        )
        .await;
        let result = apply_update(recovery_only.clone(), scw_revoke_other);
        assert!(
            matches!(result, Err(AssociationError::ChainIdMismatch(0, 1))),
            "{result:?}"
        );
        let eoa_revoke_other = sign(
            SignatureRequestBuilder::new(&inbox_id).revoke_association(wallet_id, other_id.clone()),
            &[&wallet],
        )
        .await;
        let without_other = apply_update(recovery_only, verify(&eoa_revoke_other).await)?;
        assert!(without_other.get(&other_id).is_none());
    }

    /// Accepts the recovery wallet's chain-1 history (bytes 1, 2, or 4 at block 1) and a chain-2
    /// signature (bytes 42, 43, 44 at block 42) that an attacker's contract at the same address
    /// produces.
    struct RecoveryContractVerifier;

    #[xmtp_common::async_trait]
    impl SmartContractSignatureVerifier for RecoveryContractVerifier {
        async fn is_valid_signature(
            &self,
            account_id: AccountId,
            _hash: [u8; 32],
            signature: Bytes,
            block_number: Option<BlockNumber>,
        ) -> Result<ValidationResponse, VerifierError> {
            let bytes: &[u8] = signature.as_ref();
            let is_historical_chain_one = account_id.get_chain_id() == "eip155:1"
                && block_number == Some(1)
                && matches!(bytes, [1] | [2] | [4]);
            let is_attacker_chain_two = account_id.get_chain_id() == "eip155:2"
                && block_number == Some(42)
                && bytes == [42, 43, 44];
            Ok(ValidationResponse {
                is_valid: is_historical_chain_one || is_attacker_chain_two,
                block_number,
                error: None,
            })
        }
    }

    /// A chain-1 signature by `recovery`, with the bytes `[marker]`, over `text`.
    async fn chain_one_signature(
        recovery: &Identifier,
        marker: u8,
        text: &str,
    ) -> VerifiedSignature {
        VerifiedSignature::from_smart_contract_wallet(
            text,
            RecoveryContractVerifier,
            &[marker],
            AccountId::new_evm(1, recovery.to_string()),
            &mut Some(1),
        )
        .await
        .unwrap()
    }

    /// A chain-1 signature of the recovery wallet's history. Each marker signs its own text, so
    /// each has its own replay key.
    async fn historical_recovery_signature(recovery: &Identifier, marker: u8) -> VerifiedSignature {
        chain_one_signature(
            recovery,
            marker,
            &format!("historical recovery action {marker}"),
        )
        .await
    }

    /// A recovery identifier revoked as a member keeps its role and the chain the inbox was
    /// created on. A contract at the same address on another chain cannot change recovery, add a
    /// member, or revoke one, alone or in one update with a recovery change. Chain-1 signatures
    /// still revoke a member and still move recovery to a member.
    #[xmtp_common::test(unwrap_try = true)]
    // verifies: IDENT-012, IDENT-044, IDENT-046
    async fn recovery_only_erc1271_rejects_cross_chain_signatures() {
        let recovery = Identifier::rand_ethereum();
        let victim = Identifier::rand_ethereum();
        let attacker = Identifier::rand_ethereum();
        let inbox_id = recovery.inbox_id(0)?;
        let recovery_member: MemberIdentifier = recovery.clone().into();
        let victim_member: MemberIdentifier = victim.clone().into();
        let attacker_member: MemberIdentifier = attacker.clone().into();
        let update = |actions: Vec<Action>| IdentityUpdate::new_test(actions, inbox_id.clone());

        let created = get_state([update(vec![Action::CreateInbox(CreateInbox {
            nonce: 0,
            account_identifier: recovery.clone(),
            initial_identifier_signature: historical_recovery_signature(&recovery, 1).await,
        })])])?;
        let with_victim = apply_update(
            created,
            update(vec![Action::AddAssociation(AddAssociation {
                existing_member_signature: historical_recovery_signature(&recovery, 2).await,
                new_member_identifier: victim_member.clone(),
                new_member_signature: signed_by(&victim_member, SignatureKind::Erc191, None),
            })]),
        )?;
        let recovery_only = apply_update(
            with_victim,
            update(vec![Action::RevokeAssociation(RevokeAssociation {
                recovery_identifier_signature: historical_recovery_signature(&recovery, 4).await,
                revoked_member: recovery_member.clone(),
            })]),
        )?;
        assert_eq!(recovery_only.recovery_identifier(), &recovery);
        assert!(recovery_only.get(&recovery_member).is_none());

        // The attacker can validate this address on chain 2. Chain 1 rejects the same bytes.
        let signature_bytes = [42, 43, 44];
        let chain_one = VerifiedSignature::from_smart_contract_wallet(
            "recovery action",
            RecoveryContractVerifier,
            &signature_bytes,
            AccountId::new_evm(1, recovery.to_string()),
            &mut Some(42),
        )
        .await;
        assert!(chain_one.is_err());
        let chain_two = VerifiedSignature::from_smart_contract_wallet(
            "recovery action",
            RecoveryContractVerifier,
            &signature_bytes,
            AccountId::new_evm(2, recovery.to_string()),
            &mut Some(42),
        )
        .await?;

        let change_to_attacker = || {
            Action::ChangeRecoveryIdentity(ChangeRecoveryIdentity {
                recovery_identifier_signature: chain_two.clone(),
                new_recovery_identifier: attacker.clone(),
            })
        };
        let revoke_victim = |signature: VerifiedSignature| {
            Action::RevokeAssociation(RevokeAssociation {
                recovery_identifier_signature: signature,
                revoked_member: victim_member.clone(),
            })
        };
        let attempts = [
            vec![change_to_attacker()],
            vec![Action::AddAssociation(AddAssociation {
                existing_member_signature: chain_two.clone(),
                new_member_identifier: attacker_member.clone(),
                new_member_signature: signed_by(&attacker_member, SignatureKind::Erc191, None),
            })],
            vec![revoke_victim(chain_two.clone())],
            // A recovery change would let the new recovery signer revoke in the same update.
            vec![
                change_to_attacker(),
                revoke_victim(signed_by(&attacker_member, SignatureKind::Erc191, None)),
            ],
        ];
        for actions in attempts {
            let result = apply_update(recovery_only.clone(), update(actions));
            assert!(
                matches!(result, Err(AssociationError::ChainIdMismatch(1, 2))),
                "{result:?}"
            );
        }

        let revoked = apply_update(
            recovery_only.clone(),
            update(vec![revoke_victim(
                chain_one_signature(&recovery, 1, "chain one revoke").await,
            )]),
        )?;
        assert!(revoked.get(&victim_member).is_none());
        let moved = apply_update(
            recovery_only,
            update(vec![Action::ChangeRecoveryIdentity(
                ChangeRecoveryIdentity {
                    recovery_identifier_signature: chain_one_signature(
                        &recovery,
                        2,
                        "chain one recovery change",
                    )
                    .await,
                    new_recovery_identifier: victim.clone(),
                },
            )]),
        )?;
        assert_eq!(moved.recovery_identifier(), &victim);
    }

    /// Only the recovery identifier moves recovery, and only to a current member; the role takes
    /// that member's chain. The role keeps the chain after the member is revoked, so a signature
    /// from the same address on another chain is rejected and one from the recorded chain
    /// applies.
    #[xmtp_common::test(unwrap_try = true)]
    // verifies: IDENT-091
    fn recovery_moves_only_to_a_member() {
        let creator = Identifier::rand_ethereum();
        let creator_member: MemberIdentifier = creator.clone().into();
        let wallet = Identifier::rand_ethereum();
        let wallet_member: MemberIdentifier = wallet.clone().into();
        let inbox_id = creator.inbox_id(0)?;
        let update = |action| IdentityUpdate::new_test(vec![action], inbox_id.clone());
        let change_to = |new_recovery: &Identifier| {
            Action::ChangeRecoveryIdentity(ChangeRecoveryIdentity {
                recovery_identifier_signature: signed_by(
                    &creator_member,
                    SignatureKind::Erc191,
                    None,
                ),
                new_recovery_identifier: new_recovery.clone(),
            })
        };

        let created = get_state([update(Action::CreateInbox(CreateInbox {
            nonce: 0,
            account_identifier: creator.clone(),
            initial_identifier_signature: signed_by(&creator_member, SignatureKind::Erc191, None),
        }))])?;
        let state = apply_update(
            created,
            update(Action::AddAssociation(AddAssociation {
                existing_member_signature: signed_by(&creator_member, SignatureKind::Erc191, None),
                new_member_identifier: wallet_member.clone(),
                new_member_signature: signed_by(&wallet_member, SignatureKind::Erc1271, Some(5)),
            })),
        )?;

        let result = apply_update(
            state.clone(),
            update(change_to(&Identifier::rand_ethereum())),
        );
        assert!(
            matches!(result, Err(AssociationError::RecoveryNotMember)),
            "{result:?}"
        );
        // A member that is not the recovery identifier cannot move recovery, and the signer
        // check runs before the membership check.
        for new_recovery in [&wallet, &Identifier::rand_ethereum()] {
            let result = apply_update(
                state.clone(),
                update(Action::ChangeRecoveryIdentity(ChangeRecoveryIdentity {
                    recovery_identifier_signature: signed_by(
                        &wallet_member,
                        SignatureKind::Erc1271,
                        Some(5),
                    ),
                    new_recovery_identifier: new_recovery.clone(),
                })),
            );
            assert!(
                matches!(result, Err(AssociationError::MissingExistingMember)),
                "{result:?}"
            );
        }

        let moved = apply_update(state, update(change_to(&wallet)))?;
        assert_eq!(moved.recovery_identifier(), &wallet);
        assert_eq!(moved.recovery_chain_id, Some(5));
        let recovery_only = apply_update(
            moved,
            update(Action::RevokeAssociation(RevokeAssociation {
                recovery_identifier_signature: signed_by(
                    &wallet_member,
                    SignatureKind::Erc1271,
                    Some(5),
                ),
                revoked_member: wallet_member.clone(),
            })),
        )?;
        assert!(recovery_only.get(&wallet_member).is_none());

        let revoke_creator_on = |chain_id| {
            update(Action::RevokeAssociation(RevokeAssociation {
                recovery_identifier_signature: signed_by(
                    &wallet_member,
                    SignatureKind::Erc1271,
                    Some(chain_id),
                ),
                revoked_member: creator_member.clone(),
            }))
        };
        let result = apply_update(recovery_only.clone(), revoke_creator_on(6));
        assert!(
            matches!(result, Err(AssociationError::ChainIdMismatch(5, 6))),
            "{result:?}"
        );
        let revoked = apply_update(recovery_only, revoke_creator_on(5))?;
        assert!(revoked.get(&creator_member).is_none());
    }

    /// The recovery identifier, revoked as a member, can be added again only on the chain its
    /// role records, so the member and the role never hold two chains. Any other member can be
    /// added again on another chain.
    #[xmtp_common::test(unwrap_try = true)]
    // verifies: IDENT-092
    fn readded_recovery_keeps_its_chain() {
        let recovery = Identifier::rand_ethereum();
        let recovery_member: MemberIdentifier = recovery.clone().into();
        let wallet = MemberIdentifier::rand_ethereum();
        let other = MemberIdentifier::rand_ethereum();
        let inbox_id = recovery.inbox_id(0)?;
        let update = |actions: Vec<Action>| IdentityUpdate::new_test(actions, inbox_id.clone());
        let add_by_wallet = |member: &MemberIdentifier, chain_id| {
            Action::AddAssociation(AddAssociation {
                existing_member_signature: signed_by(&wallet, SignatureKind::Erc191, None),
                new_member_identifier: member.clone(),
                new_member_signature: signed_by(member, SignatureKind::Erc1271, Some(chain_id)),
            })
        };
        let revoke = |member: &MemberIdentifier| {
            Action::RevokeAssociation(RevokeAssociation {
                recovery_identifier_signature: signed_by(
                    &recovery_member,
                    SignatureKind::Erc1271,
                    Some(1),
                ),
                revoked_member: member.clone(),
            })
        };

        let created = get_state([update(vec![Action::CreateInbox(CreateInbox {
            nonce: 0,
            account_identifier: recovery.clone(),
            initial_identifier_signature: signed_by(
                &recovery_member,
                SignatureKind::Erc1271,
                Some(1),
            ),
        })])])?;
        let state = apply_update(
            created,
            update(vec![
                Action::AddAssociation(AddAssociation {
                    existing_member_signature: signed_by(
                        &recovery_member,
                        SignatureKind::Erc1271,
                        Some(1),
                    ),
                    new_member_identifier: wallet.clone(),
                    new_member_signature: signed_by(&wallet, SignatureKind::Erc191, None),
                }),
                add_by_wallet(&other, 1),
                revoke(&recovery_member),
                revoke(&other),
            ]),
        )?;

        let result = apply_update(
            state.clone(),
            update(vec![add_by_wallet(&recovery_member, 2)]),
        );
        assert!(
            matches!(result, Err(AssociationError::ChainIdMismatch(1, 2))),
            "{result:?}"
        );
        let readded = apply_update(
            state.clone(),
            update(vec![add_by_wallet(&recovery_member, 1)]),
        )?;
        assert_eq!(readded.get(&recovery_member)?.added_on_chain_id, Some(1));
        let other_readded = apply_update(state, update(vec![add_by_wallet(&other, 3)]))?;
        assert_eq!(other_readded.get(&other)?.added_on_chain_id, Some(3));
    }

    /// A revoked passkey's add, republished with the passkey's `s` negated and the recovery
    /// wallet's recovery byte re-encoded, is the same add and must not re-add the passkey.
    #[xmtp_common::test]
    // verifies: IDENT-090
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

    /// A revoked wallet's public ERC-191 add, re-wrapped as ERC-6492 signatures that the
    /// verifier's EOA fallback accepts, has the same signers and signature text as the original.
    /// It is a replay, so it does not re-add the wallet.
    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_common::test(unwrap_try = true)]
    // verifies: IDENT-090
    async fn erc6492_rewrap_of_erc191_add_is_replay() {
        use crate::{scw_verifier::RpcSmartContractWalletVerifier, utils::test::docker_provider};
        use alloy::providers::Provider;

        let chain = docker_provider().await;
        let recovery = chain.owner0.clone();
        let member = chain.owner1.clone();
        let (_, revoked_member, add) = add_then_revoke(&recovery, &member).await;
        // With the recovery wallet no longer a member, no member record binds its chain.
        let recovery_id = recovery.member_identifier();
        let revoke_recovery = sign(
            SignatureRequestBuilder::new(recovery.get_inbox_id(0))
                .revoke_association(recovery_id.clone(), recovery_id),
            &[&recovery],
        )
        .await;
        let revoked = apply_update(revoked_member, verify(&revoke_recovery).await)?;

        let chain_id = chain.provider.get_chain_id().await?;
        let block_number = chain.provider.get_block_number().await?;
        let mut replay = add.clone();
        let UnverifiedAction::AddAssociation(add_action) = &mut replay.actions[0] else {
            panic!("the signed update must add a member");
        };
        let wrap_eoa = |signature: &mut UnverifiedSignature, address| {
            let UnverifiedSignature::RecoverableEcdsa(ecdsa) = signature else {
                panic!("the original add must carry an ERC-191 signature");
            };
            *signature = UnverifiedSignature::new_smart_contract_wallet(
                ecdsa.signature_bytes.clone(),
                AccountId::new_evm(chain_id, format!("{address:?}")),
                block_number,
            );
        };
        wrap_eoa(
            &mut add_action.existing_member_signature,
            recovery.address(),
        );
        wrap_eoa(&mut add_action.new_member_signature, member.address());
        assert_ne!(replay, add);

        let verifier = RpcSmartContractWalletVerifier::new_from_provider(chain.provider);
        let result = apply_update(revoked, replay.to_verified(verifier).await?);
        assert!(
            matches!(result, Err(AssociationError::Replay)),
            "{result:?}"
        );
    }

    /// `builder`'s request signed by smart contract wallets on chain 1, with one signature of
    /// the given bytes for each signer. The mock verifier accepts any bytes.
    async fn scw_sign(
        builder: SignatureRequestBuilder,
        signers: &[(&Identifier, &[u8])],
    ) -> UnverifiedIdentityUpdate {
        let mut request = builder.build();
        for (signer, bytes) in signers {
            let signature = UnverifiedSignature::new_smart_contract_wallet(
                bytes.to_vec(),
                AccountId::new_evm(1, signer.to_string()),
                1,
            );
            request
                .add_signature(signature, MockSmartContractSignatureVerifier::new(true))
                .await
                .unwrap();
        }
        request.build_identity_update().unwrap()
    }

    /// A revoked smart contract wallet's add, republished with both ERC-6492 signatures in
    /// another encoding that the wallet contracts accept, has the same signers and signature text
    /// as the original. It is a replay, so it does not re-add the wallet.
    #[xmtp_common::test(unwrap_try = true)]
    // verifies: IDENT-090
    async fn erc6492_reencoding_is_replay() {
        let accepts = MockSmartContractSignatureVerifier::new(true);
        let recovery = Identifier::rand_ethereum();
        let member = Identifier::rand_ethereum();
        let member_id: MemberIdentifier = member.clone().into();
        let inbox_id = recovery.inbox_id(0)?;
        let create = scw_sign(
            SignatureRequestBuilder::new(&inbox_id).create_inbox(recovery.clone(), 0),
            &[(&recovery, &[1])],
        )
        .await;
        let add = scw_sign(
            SignatureRequestBuilder::new(&inbox_id)
                .add_association(member_id.clone(), recovery.clone().into()),
            &[(&recovery, &[2]), (&member, &[3])],
        )
        .await;
        let revoke = scw_sign(
            SignatureRequestBuilder::new(&inbox_id)
                .revoke_association(recovery.clone().into(), member_id),
            &[(&recovery, &[4])],
        )
        .await;
        let state = get_state([create.to_verified(&accepts).await?])?;
        let state = apply_update(state, add.to_verified(&accepts).await?)?;
        let revoked = apply_update(state, revoke.to_verified(&accepts).await?)?;

        let replay = reencode(&add, |signature| {
            if let UnverifiedSignature::SmartContractWallet(scw) = signature {
                scw.signature_bytes.push(0);
            }
        });
        assert_ne!(replay, add);
        let result = apply_update(revoked, replay.to_verified(&accepts).await?);
        assert!(
            matches!(result, Err(AssociationError::Replay)),
            "{result:?}"
        );
    }
}
