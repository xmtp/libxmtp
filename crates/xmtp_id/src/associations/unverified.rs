#![allow(dead_code)]
use crate::scw_verifier::{ScwInput, SmartContractSignatureVerifier};

use super::{
    AccountId, Action, AddAssociation, ChangeRecoveryIdentity, ClaimHandle, CreateInbox,
    IdentityUpdate, RevokeAssociation, SignatureError,
    unsigned_actions::{
        SignatureTextCreator, UnsignedAction, UnsignedAddAssociation,
        UnsignedChangeRecoveryAddress, UnsignedClaimHandle, UnsignedCreateInbox,
        UnsignedIdentityUpdate, UnsignedRevokeAssociation,
    },
    verified_signature::VerifiedSignature,
    verify_updates::{self, Checks},
};

#[derive(Debug, Clone, PartialEq)]
pub struct UnverifiedIdentityUpdate {
    pub inbox_id: String,
    pub client_timestamp_ns: u64,
    pub actions: Vec<UnverifiedAction>,
}

impl UnverifiedIdentityUpdate {
    pub fn new(inbox_id: String, client_timestamp_ns: u64, actions: Vec<UnverifiedAction>) -> Self {
        Self {
            inbox_id,
            client_timestamp_ns,
            actions,
        }
    }

    pub(crate) fn signature_text(&self) -> String {
        let unsigned_actions = self
            .actions
            .iter()
            .map(|action| action.unsigned_action())
            .collect();
        let unsigned_identity_update = UnsignedIdentityUpdate::new(
            unsigned_actions,
            self.inbox_id.clone(),
            self.client_timestamp_ns,
        );

        unsigned_identity_update.signature_text()
    }

    /// Verify every signature of the update as
    /// [`verify_updates`](super::verify_updates()) verifies a list: every check
    /// that needs no chain first, then one
    /// [`SmartContractSignatureVerifier::verify_many`] call for the distinct
    /// ERC-6492 inputs.
    // implements: IDENT-020
    pub async fn to_verified(
        &self,
        scw_verifier: impl SmartContractSignatureVerifier,
    ) -> Result<IdentityUpdate, SignatureError> {
        let mut verified =
            verify_updates::verify_all(std::slice::from_ref(self), scw_verifier).await?;
        Ok(verified.remove(0))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnverifiedAction {
    CreateInbox(UnverifiedCreateInbox),
    AddAssociation(UnverifiedAddAssociation),
    RevokeAssociation(UnverifiedRevokeAssociation),
    ChangeRecoveryAddress(UnverifiedChangeRecoveryAddress),
    ClaimHandle(UnverifiedClaimHandle),
}

impl UnverifiedAction {
    fn unsigned_action(&self) -> UnsignedAction {
        match self {
            UnverifiedAction::CreateInbox(action) => {
                UnsignedAction::CreateInbox(action.unsigned_action.clone())
            }
            UnverifiedAction::AddAssociation(action) => {
                UnsignedAction::AddAssociation(action.unsigned_action.clone())
            }
            UnverifiedAction::RevokeAssociation(action) => {
                UnsignedAction::RevokeAssociation(action.unsigned_action.clone())
            }
            UnverifiedAction::ChangeRecoveryAddress(action) => {
                UnsignedAction::ChangeRecoveryAddress(action.unsigned_action.clone())
            }
            UnverifiedAction::ClaimHandle(action) => {
                UnsignedAction::ClaimHandle(action.unsigned_action.clone())
            }
        }
    }

    /// The action's signatures in field order. [`Self::verified`] takes their
    /// verified forms back in this order.
    pub(crate) fn signatures(&self) -> Vec<&UnverifiedSignature> {
        match self {
            UnverifiedAction::CreateInbox(action) => vec![&action.initial_identifier_signature],
            UnverifiedAction::AddAssociation(action) => vec![
                &action.existing_member_signature,
                &action.new_member_signature,
            ],
            UnverifiedAction::RevokeAssociation(action) => {
                vec![&action.recovery_identifier_signature]
            }
            UnverifiedAction::ChangeRecoveryAddress(action) => {
                vec![&action.recovery_identifier_signature]
            }
            UnverifiedAction::ClaimHandle(action) => vec![&action.signature],
        }
    }

    /// The action with each signature replaced by its verified form, which
    /// `next` returns in the order of [`Self::signatures`].
    pub(crate) fn verified(
        &self,
        mut next: impl FnMut() -> Result<VerifiedSignature, SignatureError>,
    ) -> Result<Action, SignatureError> {
        Ok(match self {
            UnverifiedAction::CreateInbox(action) => Action::CreateInbox(CreateInbox {
                nonce: action.unsigned_action.nonce,
                account_identifier: action.unsigned_action.account_identifier.clone(),
                initial_identifier_signature: next()?,
            }),
            UnverifiedAction::AddAssociation(action) => {
                let existing_member_signature = next()?;
                let new_member_signature = next()?;
                Action::AddAssociation(AddAssociation {
                    new_member_signature,
                    new_member_identifier: action.unsigned_action.new_member_identifier.clone(),
                    existing_member_signature,
                })
            }
            UnverifiedAction::RevokeAssociation(action) => {
                Action::RevokeAssociation(RevokeAssociation {
                    recovery_identifier_signature: next()?,
                    revoked_member: action.unsigned_action.revoked_member.clone(),
                })
            }
            UnverifiedAction::ChangeRecoveryAddress(action) => {
                Action::ChangeRecoveryIdentity(ChangeRecoveryIdentity {
                    recovery_identifier_signature: next()?,
                    new_recovery_identifier: action.unsigned_action.new_recovery_identifier.clone(),
                })
            }
            UnverifiedAction::ClaimHandle(action) => Action::ClaimHandle(ClaimHandle {
                handle: action.unsigned_action.handle.clone(),
                signature: next()?,
            }),
        })
    }

    /// Verify the action's signatures over `signature_text` as
    /// [`UnverifiedIdentityUpdate::to_verified`] verifies an update's.
    pub async fn to_verified<Text: AsRef<str>>(
        &self,
        signature_text: Text,
        scw_verifier: impl SmartContractSignatureVerifier,
    ) -> Result<Action, SignatureError> {
        let signature_text = signature_text.as_ref();
        let mut checks = Checks::default();
        checks.add(signature_text, self)?;
        let verdicts = verify_updates::verdicts(scw_verifier, checks.inputs()).await?;
        checks.bind(&verdicts)?.action(signature_text, self)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnverifiedCreateInbox {
    pub(crate) unsigned_action: UnsignedCreateInbox,
    pub(crate) initial_identifier_signature: UnverifiedSignature,
}

impl UnverifiedCreateInbox {
    pub fn new(
        unsigned_action: UnsignedCreateInbox,
        initial_identifier_signature: UnverifiedSignature,
    ) -> Self {
        Self {
            unsigned_action,
            initial_identifier_signature,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnverifiedAddAssociation {
    pub(crate) unsigned_action: UnsignedAddAssociation,
    pub(crate) new_member_signature: UnverifiedSignature,
    pub(crate) existing_member_signature: UnverifiedSignature,
}

impl UnverifiedAddAssociation {
    pub fn new(
        unsigned_action: UnsignedAddAssociation,
        new_member_signature: UnverifiedSignature,
        existing_member_signature: UnverifiedSignature,
    ) -> Self {
        Self {
            unsigned_action,
            new_member_signature,
            existing_member_signature,
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct UnverifiedRevokeAssociation {
    pub(crate) recovery_identifier_signature: UnverifiedSignature,
    pub(crate) unsigned_action: UnsignedRevokeAssociation,
}

impl UnverifiedRevokeAssociation {
    pub fn new(
        unsigned_action: UnsignedRevokeAssociation,
        recovery_identifier_signature: UnverifiedSignature,
    ) -> Self {
        Self {
            unsigned_action,
            recovery_identifier_signature,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnverifiedChangeRecoveryAddress {
    pub(crate) recovery_identifier_signature: UnverifiedSignature,
    pub(crate) unsigned_action: UnsignedChangeRecoveryAddress,
}

impl UnverifiedChangeRecoveryAddress {
    pub fn new(
        unsigned_action: UnsignedChangeRecoveryAddress,
        recovery_identifier_signature: UnverifiedSignature,
    ) -> Self {
        Self {
            unsigned_action,
            recovery_identifier_signature,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnverifiedClaimHandle {
    pub(crate) unsigned_action: UnsignedClaimHandle,
    pub(crate) signature: UnverifiedSignature,
}

impl UnverifiedClaimHandle {
    pub fn new(unsigned_action: UnsignedClaimHandle, signature: UnverifiedSignature) -> Self {
        Self {
            unsigned_action,
            signature,
        }
    }
}

/// A signature checked as far as it can be checked without a chain.
pub(crate) enum Checked {
    /// A signature whose verification needs no chain, verified.
    Verified(VerifiedSignature),
    /// An ERC-6492 signature, whose verdict needs a chain.
    Scw(ScwInput),
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnverifiedSignature {
    InstallationKey(UnverifiedInstallationKeySignature),
    RecoverableEcdsa(UnverifiedRecoverableEcdsaSignature),
    SmartContractWallet(UnverifiedSmartContractWalletSignature),
    Passkey(UnverifiedPasskeySignature),
}

impl UnverifiedSignature {
    /// Verify the signature over `signature_text`. An ERC-6492 signature's
    /// input is checked before the verifier is called.
    // implements: IDENT-030
    pub async fn to_verified<Text: AsRef<str>>(
        &self,
        signature_text: Text,
        scw_verifier: impl SmartContractSignatureVerifier,
    ) -> Result<VerifiedSignature, SignatureError> {
        let signature_text = signature_text.as_ref();
        let mut checks = Checks::default();
        checks.add_signature(signature_text, self)?;
        let verdicts = verify_updates::verdicts(scw_verifier, checks.inputs()).await?;
        checks.bind(&verdicts)?.signature(signature_text)
    }

    /// Check the signature over `signature_text` as far as it can be checked
    /// without a chain: a signature of any other kind is verified, and an
    /// ERC-6492 signature's input passes every check before chain access.
    /// Does no I/O; a failure is permanent.
    pub(crate) fn check(&self, signature_text: &str) -> Result<Checked, SignatureError> {
        Ok(match self {
            UnverifiedSignature::InstallationKey(sig) => {
                Checked::Verified(VerifiedSignature::from_installation_key(
                    signature_text,
                    &sig.signature_bytes,
                    *sig.verifying_key(),
                )?)
            }
            UnverifiedSignature::RecoverableEcdsa(sig) => Checked::Verified(
                VerifiedSignature::from_recoverable_ecdsa(signature_text, &sig.signature_bytes)?,
            ),
            UnverifiedSignature::SmartContractWallet(sig) => Checked::Scw(ScwInput::checked(
                sig.account_id.clone(),
                alloy::primitives::eip191_hash_message(signature_text).into(),
                sig.signature_bytes.clone().into(),
                Some(sig.block_number),
            )?),
            UnverifiedSignature::Passkey(sig) => {
                Checked::Verified(VerifiedSignature::from_passkey(
                    signature_text,
                    &sig.public_key,
                    &sig.signature,
                    &sig.authenticator_data,
                    &sig.client_data_json,
                )?)
            }
        })
    }

    pub fn new_recoverable_ecdsa(signature: Vec<u8>) -> Self {
        Self::RecoverableEcdsa(UnverifiedRecoverableEcdsaSignature::new(signature))
    }

    pub fn new_installation_key(
        signature: Vec<u8>,
        verifying_key: ed25519_dalek::VerifyingKey,
    ) -> Self {
        Self::InstallationKey(UnverifiedInstallationKeySignature::new(
            signature,
            verifying_key,
        ))
    }

    pub fn new_passkey(
        public_key: Vec<u8>,
        signature: Vec<u8>,
        authenticator_data: Vec<u8>,
        client_data_json: Vec<u8>,
    ) -> Self {
        Self::Passkey(UnverifiedPasskeySignature {
            client_data_json,
            authenticator_data,
            signature,
            public_key,
        })
    }

    pub fn new_smart_contract_wallet(
        signature: Vec<u8>,
        account_id: AccountId,
        block_number: u64,
    ) -> Self {
        Self::SmartContractWallet(UnverifiedSmartContractWalletSignature::new(
            signature,
            account_id,
            block_number,
        ))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnverifiedInstallationKeySignature {
    pub(crate) signature_bytes: Vec<u8>,
    /// The Signature Verifying Key
    // boxing avoids large enum variants if an enum contains multiple signature
    pub(crate) verifying_key: Box<ed25519_dalek::VerifyingKey>,
}

impl UnverifiedInstallationKeySignature {
    pub fn new(signature_bytes: Vec<u8>, verifying_key: ed25519_dalek::VerifyingKey) -> Self {
        Self {
            signature_bytes,
            verifying_key: Box::new(verifying_key),
        }
    }

    pub fn verifying_key(&self) -> &ed25519_dalek::VerifyingKey {
        self.verifying_key.as_ref()
    }

    pub fn verifying_key_bytes(&self) -> Vec<u8> {
        self.verifying_key.as_ref().as_ref().to_vec()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnverifiedPasskeySignature {
    // This json string contains the challenge we sent out
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
    pub authenticator_data: Vec<u8>,
    pub client_data_json: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnverifiedRecoverableEcdsaSignature {
    pub(crate) signature_bytes: Vec<u8>,
}

impl UnverifiedRecoverableEcdsaSignature {
    pub fn new(signature_bytes: Vec<u8>) -> Self {
        Self { signature_bytes }
    }
    pub fn signature_bytes(&self) -> &[u8] {
        &self.signature_bytes
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnverifiedSmartContractWalletSignature {
    pub(crate) signature_bytes: Vec<u8>,
    pub(crate) account_id: AccountId,
    pub(crate) block_number: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewUnverifiedSmartContractWalletSignature {
    pub(crate) signature_bytes: Vec<u8>,
    pub(crate) account_id: AccountId,
    pub(crate) block_number: Option<u64>,
}

impl NewUnverifiedSmartContractWalletSignature {
    pub fn new(signature_bytes: Vec<u8>, account_id: AccountId, block_number: Option<u64>) -> Self {
        Self {
            account_id,
            block_number,
            signature_bytes,
        }
    }
}

impl UnverifiedSmartContractWalletSignature {
    pub fn new(signature_bytes: Vec<u8>, account_id: AccountId, block_number: u64) -> Self {
        Self {
            signature_bytes,
            account_id,
            block_number,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::associations::{
        MemberIdentifier, get_state,
        member::Identifier,
        test_utils::{MockSmartContractSignatureVerifier, WalletTestExt},
        unsigned_actions::{UnsignedCreateInbox, UnsignedRevokeAssociation},
    };
    use alloy::signers::{Signer, local::PrivateKeySigner};
    use xmtp_configuration::MAX_IDENTITY_UPDATE_ACTIONS;
    use xmtp_proto::{
        ConversionError, xmtp::identity::associations::IdentityUpdate as IdentityUpdateProto,
    };

    use super::{
        UnverifiedAction, UnverifiedCreateInbox, UnverifiedIdentityUpdate,
        UnverifiedRecoverableEcdsaSignature, UnverifiedRevokeAssociation, UnverifiedSignature,
    };

    /// An update of `actions` genuinely signed actions: `wallet` creates its
    /// inbox and revokes an absent member over and over.
    async fn signed_revocations(
        wallet: &PrivateKeySigner,
        actions: usize,
    ) -> Result<IdentityUpdateProto, Box<dyn std::error::Error>> {
        let owner = wallet.identifier();
        let absent_member = MemberIdentifier::rand_installation();
        let placeholder = UnverifiedSignature::new_recoverable_ecdsa(vec![0; 65]);
        let create = UnverifiedAction::CreateInbox(UnverifiedCreateInbox::new(
            UnsignedCreateInbox {
                nonce: 0,
                account_identifier: owner.clone(),
            },
            placeholder.clone(),
        ));
        let revoke = UnverifiedAction::RevokeAssociation(UnverifiedRevokeAssociation::new(
            UnsignedRevokeAssociation {
                revoked_member: absent_member,
            },
            placeholder,
        ));
        let mut update = UnverifiedIdentityUpdate::new(
            owner.inbox_id(0)?,
            1,
            std::iter::once(create)
                .chain(std::iter::repeat_n(revoke, actions - 1))
                .collect(),
        );
        let signature = UnverifiedSignature::new_recoverable_ecdsa(
            wallet
                .sign_message(update.signature_text().as_bytes())
                .await?
                .as_bytes()
                .to_vec(),
        );
        for action in &mut update.actions {
            match action {
                UnverifiedAction::CreateInbox(action) => {
                    action.initial_identifier_signature = signature.clone()
                }
                UnverifiedAction::RevokeAssociation(action) => {
                    action.recovery_identifier_signature = signature.clone()
                }
                _ => unreachable!("the update only creates and revokes"),
            }
        }
        Ok(update.into())
    }

    /// Every validator replays every action of every stored update, so an
    /// update with more actions than the limit fails when it is decoded,
    /// before any of its signatures is verified, even when each one is
    /// genuine. An update at the limit decodes, verifies and applies.
    // verifies: IDENT-094
    #[xmtp_common::test(unwrap_try = true)]
    async fn identity_update_over_action_limit_fails_decode() {
        let wallet = PrivateKeySigner::random();
        for actions in [MAX_IDENTITY_UPDATE_ACTIONS + 1, 513] {
            let result =
                UnverifiedIdentityUpdate::try_from(signed_revocations(&wallet, actions).await?);
            assert!(
                matches!(
                    result,
                    Err(ConversionError::InvalidLength {
                        item: "identity update actions",
                        expected: MAX_IDENTITY_UPDATE_ACTIONS,
                        got,
                    }) if got == actions
                ),
                "{actions}: {result:?}"
            );
        }

        let at_limit = UnverifiedIdentityUpdate::try_from(
            signed_revocations(&wallet, MAX_IDENTITY_UPDATE_ACTIONS).await?,
        )?;
        let verified = at_limit
            .to_verified(MockSmartContractSignatureVerifier::new(false))
            .await?;
        assert_eq!(verified.actions.len(), MAX_IDENTITY_UPDATE_ACTIONS);
        assert_eq!(get_state([verified])?.inbox_id(), at_limit.inbox_id);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn create_identity_update() {
        let account_identifier = Identifier::rand_ethereum();
        let nonce = 1;
        let update = UnverifiedIdentityUpdate {
            inbox_id: account_identifier.inbox_id(nonce).unwrap(),
            client_timestamp_ns: 10,
            actions: vec![UnverifiedAction::CreateInbox(UnverifiedCreateInbox {
                unsigned_action: UnsignedCreateInbox {
                    account_identifier: account_identifier.clone(),
                    nonce,
                },
                initial_identifier_signature: UnverifiedSignature::RecoverableEcdsa(
                    UnverifiedRecoverableEcdsaSignature {
                        signature_bytes: vec![1, 2, 3],
                    },
                ),
            })],
        };
        assert!(
            update
                .signature_text()
                .contains(format!("(Owner: {})", account_identifier).as_str()),
            "could not find account address in signature text: {}",
            update.signature_text()
        );
    }
}
