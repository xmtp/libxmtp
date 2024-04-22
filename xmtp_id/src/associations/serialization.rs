use super::{
    association_log::{
        Action, AddAssociation, ChangeRecoveryAddress, CreateInbox, RevokeAssociation,
    },
    signature::{
        Erc1271Signature, InstallationKeySignature, LegacyDelegatedSignature,
        RecoverableEcdsaSignature,
    },
    unsigned_actions::{
        SignatureTextCreator, UnsignedAction, UnsignedAddAssociation,
        UnsignedChangeRecoveryAddress, UnsignedCreateInbox, UnsignedIdentityUpdate,
        UnsignedRevokeAssociation,
    },
    IdentityUpdate, MemberIdentifier, Signature,
};
use prost::DecodeError;
use thiserror::Error;
use xmtp_proto::xmtp::identity::associations::{
    identity_action::Kind as IdentityActionKindProto,
    member_identifier::Kind as MemberIdentifierKindProto,
    signature::Signature as SignatureKindProto, AddAssociation as AddAssociationProto,
    ChangeRecoveryAddress as ChangeRecoveryAddressProto, CreateInbox as CreateInboxProto,
    IdentityAction as IdentityActionProto, IdentityUpdate as IdentityUpdateProto,
    MemberIdentifier as MemberIdentifierProto, RevokeAssociation as RevokeAssociationProto,
    Signature as SignatureWrapperProto,
};

#[derive(Error, Debug)]
pub enum DeserializationError {
    #[error("Missing action")]
    MissingAction,
    #[error("Missing update")]
    MissingUpdate,
    #[error("Missing member identifier")]
    MissingMemberIdentifier,
    #[error("Missing signature")]
    Signature,
    #[error("Decode error {0}")]
    Decode(#[from] DecodeError),
}

pub fn from_identity_update_proto(
    proto: IdentityUpdateProto,
) -> Result<IdentityUpdate, DeserializationError> {
    let client_timestamp_ns = proto.client_timestamp_ns;
    let inbox_id = proto.inbox_id;
    let all_actions = proto
        .actions
        .into_iter()
        .map(|action| match action.kind {
            Some(action) => Ok(action),
            None => Err(DeserializationError::MissingAction),
        })
        .collect::<Result<Vec<IdentityActionKindProto>, DeserializationError>>()?;

    let signature_text = get_signature_text(&all_actions, inbox_id.clone(), client_timestamp_ns)?;

    let processed_actions: Vec<Action> = all_actions
        .into_iter()
        .map(|action| match action {
            IdentityActionKindProto::Add(add_action) => {
                Ok(Action::AddAssociation(AddAssociation {
                    new_member_signature: from_signature_proto_option(
                        add_action.new_member_signature,
                        signature_text.clone(),
                    )?,
                    existing_member_signature: from_signature_proto_option(
                        add_action.existing_member_signature,
                        signature_text.clone(),
                    )?,
                    new_member_identifier: from_member_identifier_proto_option(
                        add_action.new_member_identifier,
                    )?,
                }))
            }
            IdentityActionKindProto::CreateInbox(create_inbox_action) => {
                Ok(Action::CreateInbox(CreateInbox {
                    nonce: create_inbox_action.nonce,
                    account_address: create_inbox_action.initial_address,
                    initial_address_signature: from_signature_proto_option(
                        create_inbox_action.initial_address_signature,
                        signature_text.clone(),
                    )?,
                }))
            }
            IdentityActionKindProto::ChangeRecoveryAddress(change_recovery_address_action) => {
                Ok(Action::ChangeRecoveryAddress(ChangeRecoveryAddress {
                    new_recovery_address: change_recovery_address_action.new_recovery_address,
                    recovery_address_signature: from_signature_proto_option(
                        change_recovery_address_action.existing_recovery_address_signature,
                        signature_text.clone(),
                    )?,
                }))
            }
            IdentityActionKindProto::Revoke(revoke_action) => {
                Ok(Action::RevokeAssociation(RevokeAssociation {
                    revoked_member: from_member_identifier_proto_option(
                        revoke_action.member_to_revoke,
                    )?,
                    recovery_address_signature: from_signature_proto_option(
                        revoke_action.recovery_address_signature,
                        signature_text.clone(),
                    )?,
                }))
            }
        })
        .collect::<Result<Vec<Action>, DeserializationError>>()?;

    Ok(IdentityUpdate::new(
        processed_actions,
        inbox_id,
        client_timestamp_ns,
    ))
}

fn get_signature_text(
    actions: &[IdentityActionKindProto],
    inbox_id: String,
    client_timestamp_ns: u64,
) -> Result<String, DeserializationError> {
    let unsigned_actions: Vec<UnsignedAction> = actions
        .iter()
        .map(|action| match action {
            IdentityActionKindProto::Add(add_action) => {
                Ok(UnsignedAction::AddAssociation(UnsignedAddAssociation {
                    new_member_identifier: from_member_identifier_proto_option(
                        add_action.new_member_identifier.clone(),
                    )?,
                }))
            }
            IdentityActionKindProto::CreateInbox(create_inbox_action) => {
                Ok(UnsignedAction::CreateInbox(UnsignedCreateInbox {
                    nonce: create_inbox_action.nonce,
                    account_address: create_inbox_action.initial_address.clone(),
                }))
            }
            IdentityActionKindProto::ChangeRecoveryAddress(change_recovery_address_action) => Ok(
                UnsignedAction::ChangeRecoveryAddress(UnsignedChangeRecoveryAddress {
                    new_recovery_address: change_recovery_address_action
                        .new_recovery_address
                        .clone(),
                }),
            ),
            IdentityActionKindProto::Revoke(revoke_action) => Ok(
                UnsignedAction::RevokeAssociation(UnsignedRevokeAssociation {
                    revoked_member: from_member_identifier_proto_option(
                        revoke_action.member_to_revoke.clone(),
                    )?,
                }),
            ),
        })
        .collect::<Result<Vec<UnsignedAction>, DeserializationError>>()?;

    let unsigned_update =
        UnsignedIdentityUpdate::new(unsigned_actions, inbox_id, client_timestamp_ns);

    Ok(unsigned_update.signature_text())
}

fn from_member_identifier_proto_option(
    proto: Option<MemberIdentifierProto>,
) -> Result<MemberIdentifier, DeserializationError> {
    match proto {
        None => Err(DeserializationError::MissingMemberIdentifier),
        Some(identifier_proto) => match identifier_proto.kind {
            Some(identifier) => Ok(from_member_identifier_kind_proto(identifier)),
            None => Err(DeserializationError::MissingMemberIdentifier),
        },
    }
}

fn from_member_identifier_kind_proto(proto: MemberIdentifierKindProto) -> MemberIdentifier {
    match proto {
        MemberIdentifierKindProto::Address(address) => address.into(),
        MemberIdentifierKindProto::InstallationPublicKey(public_key) => public_key.into(),
    }
}

fn from_signature_proto_option(
    proto: Option<SignatureWrapperProto>,
    signature_text: String,
) -> Result<Box<dyn Signature>, DeserializationError> {
    match proto {
        None => Err(DeserializationError::Signature),
        Some(signature_proto) => match signature_proto.signature {
            Some(signature) => Ok(from_signature_kind_proto(signature, signature_text)?),
            None => Err(DeserializationError::Signature),
        },
    }
}

fn from_signature_kind_proto(
    proto: SignatureKindProto,
    signature_text: String,
) -> Result<Box<dyn Signature>, DeserializationError> {
    Ok(match proto {
        SignatureKindProto::InstallationKey(installation_key_signature) => {
            Box::new(InstallationKeySignature::new(
                signature_text,
                installation_key_signature.bytes,
                installation_key_signature.public_key,
            ))
        }
        SignatureKindProto::Erc191(erc191_signature) => Box::new(RecoverableEcdsaSignature::new(
            signature_text,
            erc191_signature.bytes,
        )),
        SignatureKindProto::Erc1271(erc1271_signature) => Box::new(Erc1271Signature::new(
            signature_text,
            erc1271_signature.signature,
            erc1271_signature.contract_address,
            erc1271_signature.block_number,
        )),
        SignatureKindProto::DelegatedErc191(delegated_erc191_signature) => {
            let signature_value = delegated_erc191_signature
                .signature
                .ok_or(DeserializationError::Signature)?;
            let recoverable_ecdsa_signature =
                RecoverableEcdsaSignature::new(signature_text, signature_value.bytes);

            Box::new(LegacyDelegatedSignature::new(
                recoverable_ecdsa_signature,
                delegated_erc191_signature
                    .delegated_key
                    .ok_or(DeserializationError::Signature)?,
            ))
        }
    })
}

pub fn to_identity_update_proto(identity_update: &IdentityUpdate) -> IdentityUpdateProto {
    let actions: Vec<IdentityActionProto> = identity_update
        .actions
        .iter()
        .map(to_identity_action_proto)
        .collect();

    IdentityUpdateProto {
        client_timestamp_ns: identity_update.client_timestamp_ns,
        inbox_id: identity_update.inbox_id.clone(),
        actions,
    }
}

fn to_identity_action_proto(action: &Action) -> IdentityActionProto {
    match action {
        Action::AddAssociation(add_association) => IdentityActionProto {
            kind: Some(IdentityActionKindProto::Add(AddAssociationProto {
                new_member_identifier: Some(to_member_identifier_proto(
                    add_association.new_member_identifier.clone(),
                )),
                new_member_signature: Some(add_association.new_member_signature.to_proto()),
                existing_member_signature: Some(
                    add_association.existing_member_signature.to_proto(),
                ),
            })),
        },
        Action::CreateInbox(create_inbox) => IdentityActionProto {
            kind: Some(IdentityActionKindProto::CreateInbox(CreateInboxProto {
                nonce: create_inbox.nonce,
                initial_address: create_inbox.account_address.clone(),
                initial_address_signature: Some(create_inbox.initial_address_signature.to_proto()),
            })),
        },
        Action::RevokeAssociation(revoke_association) => IdentityActionProto {
            kind: Some(IdentityActionKindProto::Revoke(RevokeAssociationProto {
                member_to_revoke: Some(to_member_identifier_proto(
                    revoke_association.revoked_member.clone(),
                )),
                recovery_address_signature: Some(
                    revoke_association.recovery_address_signature.to_proto(),
                ),
            })),
        },
        Action::ChangeRecoveryAddress(change_recovery_address) => IdentityActionProto {
            kind: Some(IdentityActionKindProto::ChangeRecoveryAddress(
                ChangeRecoveryAddressProto {
                    new_recovery_address: change_recovery_address.new_recovery_address.clone(),
                    existing_recovery_address_signature: Some(
                        change_recovery_address
                            .recovery_address_signature
                            .to_proto(),
                    ),
                },
            )),
        },
    }
}

fn to_member_identifier_proto(member_identifier: MemberIdentifier) -> MemberIdentifierProto {
    match member_identifier {
        MemberIdentifier::Address(address) => MemberIdentifierProto {
            kind: Some(MemberIdentifierKindProto::Address(address)),
        },
        MemberIdentifier::Installation(public_key) => MemberIdentifierProto {
            kind: Some(MemberIdentifierKindProto::InstallationPublicKey(public_key)),
        },
    }
}

#[cfg(test)]
mod tests {
    use crate::associations::{
        hashes::generate_inbox_id,
        test_utils::{rand_string, rand_u64},
    };

    use super::*;

    #[test]
    fn test_round_trip() {
        let account_address = rand_string();
        let nonce = rand_u64();
        let inbox_id = generate_inbox_id(&account_address, &nonce);

        let identity_update = IdentityUpdate::new(
            vec![Action::CreateInbox(CreateInbox {
                nonce: nonce,
                account_address: account_address,
                initial_address_signature: Box::new(RecoverableEcdsaSignature::new(
                    "foo".to_string(),
                    vec![1, 2, 3],
                )),
            })],
            inbox_id,
            rand_u64(),
        );

        let serialized_update = to_identity_update_proto(&identity_update);

        assert_eq!(
            serialized_update.client_timestamp_ns,
            identity_update.client_timestamp_ns
        );
        assert_eq!(serialized_update.actions.len(), 1);

        let deserialized_update = from_identity_update_proto(serialized_update.clone())
            .expect("deserialization should succeed");

        let reserialized = to_identity_update_proto(&deserialized_update);

        assert_eq!(serialized_update, reserialized);
    }
}
