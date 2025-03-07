#![allow(dead_code)]
use crate::scw_verifier::SmartContractSignatureVerifier;

use super::{
    unsigned_actions::{
        SignatureTextCreator, UnsignedAction, UnsignedAddAssociation,
        UnsignedChangeRecoveryAddress, UnsignedCreateInbox, UnsignedIdentityUpdate,
        UnsignedRevokeAssociation,
    },
    verified_signature::VerifiedSignature,
    AccountId, Action, AddAssociation, CreateInbox, IdentityUpdate, RevokeAssociation,
    SignatureError,
};
use futures::future::try_join_all;
use xmtp_proto::xmtp::message_contents::SignedPublicKey as LegacySignedPublicKeyProto;

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

    fn signature_text(&self) -> String {
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

    fn signatures(&self) -> Vec<UnverifiedSignature> {
        self.actions
            .iter()
            .flat_map(|action| action.signatures())
            .collect()
    }

    pub async fn to_verified(
        &self,
        scw_verifier: impl SmartContractSignatureVerifier,
    ) -> Result<IdentityUpdate, SignatureError> {
        let signature_text = self.signature_text();

        let actions: Vec<Action> = try_join_all(
            self.actions
                .iter()
                .map(|action| action.to_verified(&signature_text, &scw_verifier)),
        )
        .await?;

        Ok(IdentityUpdate::new(
            actions,
            self.inbox_id.clone(),
            self.client_timestamp_ns,
        ))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnverifiedAction {
    CreateInbox(UnverifiedCreateInbox),
    AddAssociation(UnverifiedAddAssociation),
    RevokeAssociation(UnverifiedRevokeAssociation),
    ChangeRecoveryAddress(UnverifiedChangeRecoveryAddress),
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
        }
    }

    fn signatures(&self) -> Vec<UnverifiedSignature> {
        match self {
            UnverifiedAction::CreateInbox(action) => {
                vec![action.initial_identifier_signature.clone()]
            }
            UnverifiedAction::AddAssociation(action) => vec![
                action.existing_member_signature.clone(),
                action.new_member_signature.clone(),
            ],
            UnverifiedAction::RevokeAssociation(action) => {
                vec![action.recovery_identifier_signature.clone()]
            }
            UnverifiedAction::ChangeRecoveryAddress(action) => {
                vec![action.recovery_identifier_signature.clone()]
            }
        }
    }

    pub async fn to_verified<Text: AsRef<str>>(
        &self,
        signature_text: Text,
        scw_verifier: impl SmartContractSignatureVerifier,
    ) -> Result<Action, SignatureError> {
        let action = match self {
            UnverifiedAction::CreateInbox(action) => Action::CreateInbox(CreateInbox {
                nonce: action.unsigned_action.nonce,
                account_identifier: action.unsigned_action.account_identifier.clone(),
                initial_identifier_signature: action
                    .initial_identifier_signature
                    .to_verified(signature_text.as_ref(), &scw_verifier)
                    .await?,
            }),
            UnverifiedAction::AddAssociation(action) => Action::AddAssociation(AddAssociation {
                new_member_signature: action
                    .new_member_signature
                    .to_verified(signature_text.as_ref(), &scw_verifier)
                    .await?,
                new_member_identifier: action.unsigned_action.new_member_identifier.clone(),
                existing_member_signature: action
                    .existing_member_signature
                    .to_verified(signature_text.as_ref(), &scw_verifier)
                    .await?,
            }),
            UnverifiedAction::RevokeAssociation(action) => {
                Action::RevokeAssociation(RevokeAssociation {
                    recovery_identifier_signature: action
                        .recovery_identifier_signature
                        .to_verified(signature_text.as_ref(), &scw_verifier)
                        .await?,
                    revoked_member: action.unsigned_action.revoked_member.clone(),
                })
            }
            UnverifiedAction::ChangeRecoveryAddress(action) => {
                Action::ChangeRecoveryIdentity(super::ChangeRecoveryIdentity {
                    recovery_identifier_signature: action
                        .recovery_identifier_signature
                        .to_verified(signature_text.as_ref(), &scw_verifier)
                        .await?,
                    new_recovery_identifier: action.unsigned_action.new_recovery_identifier.clone(),
                })
            }
        };

        Ok(action)
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
pub enum UnverifiedSignature {
    InstallationKey(UnverifiedInstallationKeySignature),
    RecoverableEcdsa(UnverifiedRecoverableEcdsaSignature),
    SmartContractWallet(UnverifiedSmartContractWalletSignature),
    LegacyDelegated(UnverifiedLegacyDelegatedSignature),
    Passkey(UnverifiedPasskeySignature),
}

impl UnverifiedSignature {
    pub async fn to_verified<Text: AsRef<str>>(
        &self,
        signature_text: Text,
        scw_verifier: impl SmartContractSignatureVerifier,
    ) -> Result<VerifiedSignature, SignatureError> {
        match self {
            UnverifiedSignature::InstallationKey(sig) => VerifiedSignature::from_installation_key(
                signature_text,
                &sig.signature_bytes,
                *sig.verifying_key(),
            ),
            UnverifiedSignature::RecoverableEcdsa(sig) => {
                VerifiedSignature::from_recoverable_ecdsa(signature_text, &sig.signature_bytes)
            }
            UnverifiedSignature::SmartContractWallet(sig) => {
                VerifiedSignature::from_smart_contract_wallet(
                    signature_text,
                    scw_verifier,
                    &sig.signature_bytes,
                    sig.account_id.clone(),
                    &mut Some(sig.block_number),
                )
                .await
            }
            UnverifiedSignature::LegacyDelegated(sig) => VerifiedSignature::from_legacy_delegated(
                signature_text,
                &sig.legacy_key_signature.signature_bytes,
                sig.signed_public_key_proto.clone(),
            ),
            UnverifiedSignature::Passkey(sig) => VerifiedSignature::from_passkey(
                &sig.public_key,
                &sig.signature,
                &sig.authenticator_data,
                &sig.client_data_json,
                sig.relying_party.clone(),
            ),
        }
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
        client_data_json: String,
    ) -> Self {
        Self::Passkey(UnverifiedPasskeySignature {
            client_data_json,
            authenticator_data,
            signature,
            public_key,
            relying_party: None,
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

    pub fn new_legacy_delegated(
        signature: Vec<u8>,
        signed_public_key_proto: LegacySignedPublicKeyProto,
    ) -> Self {
        Self::LegacyDelegated(UnverifiedLegacyDelegatedSignature::new(
            UnverifiedRecoverableEcdsaSignature::new(signature),
            signed_public_key_proto,
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
    pub(crate) public_key: Vec<u8>,
    pub(crate) signature: Vec<u8>,
    pub(crate) authenticator_data: Vec<u8>,
    pub(crate) client_data_json: String,
    pub(crate) relying_party: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnverifiedRecoverableEcdsaSignature {
    pub(crate) signature_bytes: Vec<u8>,
}

impl UnverifiedRecoverableEcdsaSignature {
    pub fn new(signature_bytes: Vec<u8>) -> Self {
        Self { signature_bytes }
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

#[derive(Debug, Clone, PartialEq)]
pub struct UnverifiedLegacyDelegatedSignature {
    pub(crate) legacy_key_signature: UnverifiedRecoverableEcdsaSignature,
    pub(crate) signed_public_key_proto: LegacySignedPublicKeyProto,
}

impl UnverifiedLegacyDelegatedSignature {
    pub fn new(
        legacy_key_signature: UnverifiedRecoverableEcdsaSignature,
        signed_public_key_proto: LegacySignedPublicKeyProto,
    ) -> Self {
        Self {
            legacy_key_signature,
            signed_public_key_proto,
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use crate::associations::{member::Identifier, unsigned_actions::UnsignedCreateInbox};

    use super::{
        UnverifiedAction, UnverifiedCreateInbox, UnverifiedIdentityUpdate,
        UnverifiedRecoverableEcdsaSignature, UnverifiedSignature,
    };

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
