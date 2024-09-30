//! Builders for creating a [`SignatureRequest`] with a [`PendingIdentityAction`] for an external SDK/Library, which can then be
//! resolved into an [`IdentityUpdate`]. An [`IdentityUpdate`] may be used for updating the state
//! of an XMTP ID according to [XIP-46](https://github.com/xmtp/XIPs/pull/53)

use std::collections::{HashMap, HashSet};

use crate::{scw_verifier::SmartContractSignatureVerifier, utils::now_ns};
use thiserror::Error;

use super::{
    unsigned_actions::{
        SignatureTextCreator, UnsignedAction, UnsignedAddAssociation,
        UnsignedChangeRecoveryAddress, UnsignedCreateInbox, UnsignedIdentityUpdate,
        UnsignedRevokeAssociation,
    },
    unverified::{
        UnverifiedAction, UnverifiedAddAssociation, UnverifiedChangeRecoveryAddress,
        UnverifiedCreateInbox, UnverifiedIdentityUpdate, UnverifiedRevokeAssociation,
        UnverifiedSignature,
    },
    MemberIdentifier, MemberKind, SignatureError,
};

/// The SignatureField is used to map the signatures from a [SignatureRequest] back to the correct
/// field in an [IdentityUpdate]. It is used in the `pending_signatures` map in a [PendingIdentityAction]
#[derive(Clone, PartialEq, Hash, Eq, Debug)]
enum SignatureField {
    InitialAddress,
    ExistingMember,
    NewMember,
    RecoveryAddress,
}

#[derive(Clone, Debug)]
pub struct PendingIdentityAction {
    unsigned_action: UnsignedAction,
    pending_signatures: HashMap<SignatureField, MemberIdentifier>,
}

/// The SignatureRequestBuilder is used to collect all of the actions in
/// an IdentityUpdate, but without the signatures.
/// It outputs a SignatureRequest, which can then collect the relevant signatures and be turned into
/// an IdentityUpdate.
pub struct SignatureRequestBuilder {
    inbox_id: String,
    client_timestamp_ns: u64,
    actions: Vec<PendingIdentityAction>,
}

impl SignatureRequestBuilder {
    /// Create a new IdentityUpdateBuilder for the given `inbox_id`
    pub fn new(inbox_id: String) -> Self {
        Self {
            inbox_id,
            client_timestamp_ns: now_ns() as u64,
            actions: vec![],
        }
    }

    /// Create a new inbox. This method must be called before any other methods or the IdentityUpdate will fail
    pub fn create_inbox(mut self, signer_identity: MemberIdentifier, nonce: u64) -> Self {
        let pending_action = PendingIdentityAction {
            unsigned_action: UnsignedAction::CreateInbox(UnsignedCreateInbox {
                account_address: signer_identity.to_string(),
                nonce,
            }),
            pending_signatures: HashMap::from([(
                SignatureField::InitialAddress,
                signer_identity.clone(),
            )]),
        };
        // Save the `PendingIdentityAction` for later
        self.actions.push(pending_action);

        self
    }

    /// Add an AddAssociation action.
    pub fn add_association(
        mut self,
        new_member_identifier: MemberIdentifier,
        existing_member_identifier: MemberIdentifier,
    ) -> Self {
        self.actions.push(PendingIdentityAction {
            unsigned_action: UnsignedAction::AddAssociation(UnsignedAddAssociation {
                new_member_identifier: new_member_identifier.clone(),
            }),
            pending_signatures: HashMap::from([
                (
                    SignatureField::ExistingMember,
                    existing_member_identifier.clone(),
                ),
                (SignatureField::NewMember, new_member_identifier.clone()),
            ]),
        });

        self
    }

    pub fn revoke_association(
        mut self,
        recovery_address_identifier: MemberIdentifier,
        revoked_member: MemberIdentifier,
    ) -> Self {
        self.actions.push(PendingIdentityAction {
            pending_signatures: HashMap::from([(
                SignatureField::RecoveryAddress,
                recovery_address_identifier.clone(),
            )]),
            unsigned_action: UnsignedAction::RevokeAssociation(UnsignedRevokeAssociation {
                revoked_member,
            }),
        });

        self
    }

    pub fn change_recovery_address(
        mut self,
        recovery_address_identifier: MemberIdentifier,
        new_recovery_address: String,
    ) -> Self {
        self.actions.push(PendingIdentityAction {
            pending_signatures: HashMap::from([(
                SignatureField::RecoveryAddress,
                recovery_address_identifier.clone(),
            )]),
            unsigned_action: UnsignedAction::ChangeRecoveryAddress(UnsignedChangeRecoveryAddress {
                new_recovery_address,
            }),
        });

        self
    }

    pub fn build(self) -> SignatureRequest {
        let unsigned_actions: Vec<UnsignedAction> = self
            .actions
            .iter()
            .map(|pending_action| pending_action.unsigned_action.clone())
            .collect();

        let signature_text = get_signature_text(
            unsigned_actions,
            self.inbox_id.clone(),
            self.client_timestamp_ns,
        );

        SignatureRequest::new(
            self.actions,
            signature_text,
            self.inbox_id,
            self.client_timestamp_ns,
        )
    }
}

#[derive(Debug, Error)]
pub enum SignatureRequestError {
    #[error("Unknown signer")]
    UnknownSigner,
    #[error("Required signature was not provided")]
    MissingSigner,
    #[error("Signature error {0}")]
    Signature(#[from] SignatureError),
}

/// A signature request is meant to be sent over the FFI barrier (wrapped in a mutex) to platform SDKs.
/// `xmtp_mls` can add any InstallationKey signatures first, so that the platform SDK does not need to worry about those.
/// The platform SDK can then fill in any missing signatures and convert it to an IdentityUpdate that is ready to be published
/// to the network
#[derive(Clone, Debug)]
pub struct SignatureRequest {
    pending_actions: Vec<PendingIdentityAction>,
    signature_text: String,
    signatures: HashMap<MemberIdentifier, UnverifiedSignature>,
    client_timestamp_ns: u64,
    inbox_id: String,
}

impl SignatureRequest {
    pub fn new(
        pending_actions: Vec<PendingIdentityAction>,
        signature_text: String,
        inbox_id: String,
        client_timestamp_ns: u64,
    ) -> Self {
        Self {
            inbox_id,
            pending_actions,
            signature_text,
            signatures: HashMap::new(),
            client_timestamp_ns,
        }
    }

    pub fn missing_signatures(&self) -> Vec<MemberIdentifier> {
        let signers: HashSet<MemberIdentifier> = self
            .pending_actions
            .iter()
            .flat_map(|pending_action| {
                pending_action
                    .pending_signatures
                    .values()
                    .cloned()
                    .collect::<Vec<MemberIdentifier>>()
            })
            .collect();

        let signatures: HashSet<MemberIdentifier> = self.signatures.keys().cloned().collect();

        signers.difference(&signatures).cloned().collect()
    }

    pub fn missing_address_signatures(&self) -> Vec<MemberIdentifier> {
        self.missing_signatures()
            .iter()
            .filter(|member| member.kind() == MemberKind::Address)
            .cloned()
            .collect()
    }

    pub async fn add_signature(
        &mut self,
        signature: UnverifiedSignature,
        scw_verifier: &dyn SmartContractSignatureVerifier,
    ) -> Result<(), SignatureRequestError> {
        let verified_sig = signature
            .to_verified(self.signature_text.clone(), scw_verifier)
            .await?;
        let signer_identity = &verified_sig.signer;

        let missing_signatures = self.missing_signatures();
        tracing::info!("Provided Signer: {}", signer_identity);
        tracing::info!("Missing Signatures: {:?}", missing_signatures);

        // Make sure the signer is someone actually in the request
        if !missing_signatures.contains(signer_identity) {
            return Err(SignatureRequestError::UnknownSigner);
        }

        self.signatures.insert(verified_sig.signer, signature);

        Ok(())
    }

    pub fn is_ready(&self) -> bool {
        self.missing_signatures().is_empty()
    }

    pub fn signature_text(&self) -> String {
        self.signature_text.clone()
    }

    pub fn build_identity_update(self) -> Result<UnverifiedIdentityUpdate, SignatureRequestError> {
        if !self.is_ready() {
            return Err(SignatureRequestError::MissingSigner);
        }

        let actions = self
            .pending_actions
            .clone()
            .into_iter()
            .map(|pending_action| build_action(pending_action, &self.signatures))
            .collect::<Result<Vec<UnverifiedAction>, SignatureRequestError>>()?;

        Ok(UnverifiedIdentityUpdate::new(
            self.inbox_id,
            self.client_timestamp_ns,
            actions,
        ))
    }

    pub fn inbox_id(&self) -> String {
        self.inbox_id.clone()
    }
}

fn build_action(
    pending_action: PendingIdentityAction,
    signatures: &HashMap<MemberIdentifier, UnverifiedSignature>,
) -> Result<UnverifiedAction, SignatureRequestError> {
    match pending_action.unsigned_action {
        UnsignedAction::CreateInbox(unsigned_action) => {
            let signer_identity = pending_action
                .pending_signatures
                .get(&SignatureField::InitialAddress)
                .ok_or(SignatureRequestError::MissingSigner)?;
            let initial_address_signature = signatures
                .get(signer_identity)
                .cloned()
                .ok_or(SignatureRequestError::MissingSigner)?;

            Ok(UnverifiedAction::CreateInbox(UnverifiedCreateInbox {
                unsigned_action,
                initial_address_signature,
            }))
        }
        UnsignedAction::AddAssociation(unsigned_action) => {
            let existing_member_signer_identity = pending_action
                .pending_signatures
                .get(&SignatureField::ExistingMember)
                .ok_or(SignatureRequestError::MissingSigner)?;
            let new_member_signer_identity = pending_action
                .pending_signatures
                .get(&SignatureField::NewMember)
                .ok_or(SignatureRequestError::MissingSigner)?;

            let existing_member_signature = signatures
                .get(existing_member_signer_identity)
                .cloned()
                .ok_or(SignatureRequestError::MissingSigner)?;

            let new_member_signature = signatures
                .get(new_member_signer_identity)
                .cloned()
                .ok_or(SignatureRequestError::MissingSigner)?;

            Ok(UnverifiedAction::AddAssociation(UnverifiedAddAssociation {
                unsigned_action,
                existing_member_signature,
                new_member_signature,
            }))
        }
        UnsignedAction::RevokeAssociation(unsigned_action) => {
            let signer_identity = pending_action
                .pending_signatures
                .get(&SignatureField::RecoveryAddress)
                .ok_or(SignatureRequestError::MissingSigner)?;
            let recovery_address_signature = signatures
                .get(signer_identity)
                .cloned()
                .ok_or(SignatureRequestError::MissingSigner)?;

            Ok(UnverifiedAction::RevokeAssociation(
                UnverifiedRevokeAssociation {
                    recovery_address_signature,
                    unsigned_action,
                },
            ))
        }
        UnsignedAction::ChangeRecoveryAddress(unsigned_action) => {
            let signer_identity = pending_action
                .pending_signatures
                .get(&SignatureField::RecoveryAddress)
                .ok_or(SignatureRequestError::MissingSigner)?;

            let recovery_address_signature = signatures
                .get(signer_identity)
                .cloned()
                .ok_or(SignatureRequestError::MissingSigner)?;

            Ok(UnverifiedAction::ChangeRecoveryAddress(
                UnverifiedChangeRecoveryAddress {
                    recovery_address_signature,
                    unsigned_action,
                },
            ))
        }
    }
}

fn get_signature_text(
    actions: Vec<UnsignedAction>,
    inbox_id: String,
    client_timestamp_ns: u64,
) -> String {
    let identity_update = UnsignedIdentityUpdate {
        client_timestamp_ns,
        actions,
        inbox_id,
    };

    identity_update.signature_text()
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);
    use ethers::signers::{LocalWallet, Signer};

    use crate::{
        associations::{
            get_state,
            hashes::generate_inbox_id,
            test_utils::{
                add_installation_key_signature, add_wallet_signature,
                MockSmartContractSignatureVerifier,
            },
            unverified::UnverifiedRecoverableEcdsaSignature,
            IdentityUpdate,
        },
        InboxOwner,
    };
    use ed25519_dalek::SigningKey as Ed25519SigningKey;

    use super::*;

    async fn convert_to_verified(identity_update: &UnverifiedIdentityUpdate) -> IdentityUpdate {
        let scw_verifier = MockSmartContractSignatureVerifier::new(false);
        identity_update
            .to_verified(&scw_verifier)
            .await
            .expect("should be valid")
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn create_inbox() {
        let wallet = LocalWallet::new(&mut rand::thread_rng());
        let account_address = wallet.get_address();
        let nonce = 0;
        let inbox_id = generate_inbox_id(&account_address, &nonce);

        let mut signature_request = SignatureRequestBuilder::new(inbox_id)
            .create_inbox(account_address.into(), nonce)
            .build();

        add_wallet_signature(&mut signature_request, &wallet).await;

        let identity_update = signature_request
            .build_identity_update()
            .expect("should be valid");

        get_state(vec![convert_to_verified(&identity_update).await]).expect("should be valid");
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn create_and_add_identity() {
        let wallet = LocalWallet::new(&mut rand::thread_rng());
        let installation_key = Ed25519SigningKey::generate(&mut rand::thread_rng());
        let account_address = wallet.get_address();
        let installation_key_id = installation_key.verifying_key().as_bytes().to_vec();
        let nonce = 0;
        let inbox_id = generate_inbox_id(&account_address, &nonce);
        let existing_member_identifier: MemberIdentifier = account_address.into();
        let new_member_identifier: MemberIdentifier = installation_key_id.into();

        let mut signature_request = SignatureRequestBuilder::new(inbox_id)
            .create_inbox(existing_member_identifier.clone(), nonce)
            .add_association(new_member_identifier, existing_member_identifier)
            .build();

        add_wallet_signature(&mut signature_request, &wallet).await;
        add_installation_key_signature(&mut signature_request, &installation_key).await;

        let identity_update = signature_request
            .build_identity_update()
            .expect("should be valid");

        let state =
            get_state(vec![convert_to_verified(&identity_update).await]).expect("should be valid");
        assert_eq!(state.members().len(), 2);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn create_and_revoke() {
        let wallet = LocalWallet::new(&mut rand::thread_rng());
        let account_address = wallet.get_address();
        let nonce = 0;
        let inbox_id = generate_inbox_id(&account_address, &nonce);
        let existing_member_identifier: MemberIdentifier = account_address.clone().into();

        let mut signature_request = SignatureRequestBuilder::new(inbox_id)
            .create_inbox(existing_member_identifier.clone(), nonce)
            .revoke_association(existing_member_identifier.clone(), account_address.into())
            .build();

        add_wallet_signature(&mut signature_request, &wallet).await;

        let identity_update = signature_request
            .build_identity_update()
            .expect("should be valid");

        let state =
            get_state(vec![convert_to_verified(&identity_update).await]).expect("should be valid");

        assert_eq!(state.members().len(), 0);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn attempt_adding_unknown_signer() {
        let account_address = "account_address".to_string();
        let nonce = 0;
        let inbox_id = generate_inbox_id(&account_address, &nonce);
        let mut signature_request = SignatureRequestBuilder::new(inbox_id)
            .create_inbox(account_address.into(), nonce)
            .build();

        let rand_wallet = LocalWallet::new(&mut rand::thread_rng());

        let signature_text = signature_request.signature_text();
        let sig = rand_wallet
            .sign_message(signature_text)
            .await
            .unwrap()
            .to_vec();
        let unverified_sig =
            UnverifiedSignature::RecoverableEcdsa(UnverifiedRecoverableEcdsaSignature::new(sig));
        let scw_verifier = MockSmartContractSignatureVerifier::new(false);

        let attempt_to_add_random_member = signature_request
            .add_signature(unverified_sig, &scw_verifier)
            .await;

        assert!(matches!(
            attempt_to_add_random_member,
            Err(SignatureRequestError::UnknownSigner)
        ));
    }
}
