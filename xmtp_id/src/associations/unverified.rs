#![allow(dead_code)]
use crate::scw_verifier::SmartContractSignatureVerifier;

use super::{
    unsigned_actions::{
        SignatureTextCreator, UnsignedAction, UnsignedAddAssociation,
        UnsignedChangeRecoveryAddress, UnsignedCreateInbox, UnsignedIdentityUpdate,
        UnsignedRevokeAssociation,
    },
    AccountId, SignatureError, VerifiedSignature,
};
use xmtp_proto::xmtp::message_contents::SignedPublicKey as LegacySignedPublicKeyProto;

#[derive(Debug, Clone)]
pub struct UnverifiedIdentityUpdate {
    inbox_id: String,
    client_timestamp_ns: u64,
    actions: Vec<UnverifiedAction>,
}

impl UnverifiedIdentityUpdate {
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
}

#[derive(Debug, Clone)]
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
            UnverifiedAction::CreateInbox(action) => vec![action.initial_address_signature.clone()],
            UnverifiedAction::AddAssociation(action) => vec![
                action.existing_member_signature.clone(),
                action.new_member_signature.clone(),
            ],
            UnverifiedAction::RevokeAssociation(action) => {
                vec![action.recovery_address_signature.clone()]
            }
            UnverifiedAction::ChangeRecoveryAddress(action) => {
                vec![action.recovery_address_signature.clone()]
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct UnverifiedCreateInbox {
    unsigned_action: UnsignedCreateInbox,
    initial_address_signature: UnverifiedSignature,
}

#[derive(Debug, Clone)]
pub struct UnverifiedAddAssociation {
    unsigned_action: UnsignedAddAssociation,
    new_member_signature: UnverifiedSignature,
    existing_member_signature: UnverifiedSignature,
}

#[derive(Debug, Clone)]
pub struct UnverifiedRevokeAssociation {
    recovery_address_signature: UnverifiedSignature,
    unsigned_action: UnsignedRevokeAssociation,
}

#[derive(Debug, Clone)]
pub struct UnverifiedChangeRecoveryAddress {
    recovery_address_signature: UnverifiedSignature,
    unsigned_action: UnsignedChangeRecoveryAddress,
}

#[derive(Debug, Clone)]
pub enum UnverifiedSignature {
    InstallationKey(UnverifiedInstallationKeySignature),
    RecoverableEcdsa(UnverifiedRecoverableEcdsaSignature),
    Erc6492(UnverifiedErc6492Signature),
    LegacyDelegated(UnverifiedLegacyDelegatedSignature),
}

impl UnverifiedSignature {
    async fn to_verified(
        &self,
        _signature_text: String,
        _scw_verifier: &dyn SmartContractSignatureVerifier,
    ) -> Result<VerifiedSignature, SignatureError> {
        todo!("not implemented")
    }
}

#[derive(Debug, Clone)]
pub struct UnverifiedInstallationKeySignature {
    signature_bytes: Vec<u8>,
    verifying_key: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct UnverifiedRecoverableEcdsaSignature {
    signature_bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct UnverifiedErc6492Signature {
    signature_bytes: Vec<u8>,
    account_id: AccountId,
    block_number: u64,
}

#[derive(Debug, Clone)]
pub struct UnverifiedLegacyDelegatedSignature {
    legacy_key_signature: UnverifiedRecoverableEcdsaSignature,
    signed_public_key_proto: LegacySignedPublicKeyProto,
}

#[cfg(test)]
mod tests {
    use crate::associations::{
        generate_inbox_id, test_utils::rand_string, unsigned_actions::UnsignedCreateInbox,
    };

    use super::{
        UnverifiedAction, UnverifiedCreateInbox, UnverifiedIdentityUpdate,
        UnverifiedRecoverableEcdsaSignature, UnverifiedSignature,
    };

    #[test]
    fn create_identity_update() {
        let account_address = rand_string();
        let nonce = 1;
        let update = UnverifiedIdentityUpdate {
            inbox_id: generate_inbox_id(account_address.as_str(), &nonce),
            client_timestamp_ns: 10,
            actions: vec![UnverifiedAction::CreateInbox(UnverifiedCreateInbox {
                unsigned_action: UnsignedCreateInbox {
                    account_address: account_address.to_string(),
                    nonce,
                },
                initial_address_signature: UnverifiedSignature::RecoverableEcdsa(
                    UnverifiedRecoverableEcdsaSignature {
                        signature_bytes: vec![1, 2, 3],
                    },
                ),
            })],
        };
        assert!(
            update
                .signature_text()
                .contains(format!("(Owner: {})", account_address).as_str()),
            "could not find account address in signature text: {}",
            update.signature_text()
        );
    }
}
