use std::collections::{HashMap, HashSet};

use super::{
    association_log::{
        Action, AddAssociation, ChangeRecoveryAddress, CreateInbox, RevokeAssociation,
    },
    member::Member,
    signature::{
        AccountId, InstallationKeySignature, LegacyDelegatedSignature, RecoverableEcdsaSignature,
        SmartContractWalletSignature, ValidatedLegacySignedPublicKey,
    },
    state::{AssociationState, AssociationStateDiff},
    unsigned_actions::{
        SignatureTextCreator, UnsignedAction, UnsignedAddAssociation,
        UnsignedChangeRecoveryAddress, UnsignedCreateInbox, UnsignedIdentityUpdate,
        UnsignedRevokeAssociation,
    },
    unverified::{
        UnverifiedAction, UnverifiedAddAssociation, UnverifiedChangeRecoveryAddress,
        UnverifiedCreateInbox, UnverifiedErc6492Signature, UnverifiedIdentityUpdate,
        UnverifiedInstallationKeySignature, UnverifiedLegacyDelegatedSignature,
        UnverifiedRecoverableEcdsaSignature, UnverifiedRevokeAssociation, UnverifiedSignature,
    },
    IdentityUpdate, MemberIdentifier, Signature, SignatureError,
};
use prost::{DecodeError, Message};
use regex::Regex;
use thiserror::Error;
use xmtp_cryptography::signature::{sanitize_evm_addresses, RecoverableSignature};
use xmtp_proto::xmtp::{
    identity::associations::{
        identity_action::Kind as IdentityActionKindProto,
        member_identifier::Kind as MemberIdentifierKindProto,
        signature::Signature as SignatureKindProto, AddAssociation as AddAssociationProto,
        AssociationState as AssociationStateProto,
        AssociationStateDiff as AssociationStateDiffProto,
        ChangeRecoveryAddress as ChangeRecoveryAddressProto, CreateInbox as CreateInboxProto,
        IdentityAction as IdentityActionProto, IdentityUpdate as IdentityUpdateProto,
        LegacyDelegatedSignature as LegacyDelegatedSignatureProto, Member as MemberProto,
        MemberIdentifier as MemberIdentifierProto, MemberMap as MemberMapProto,
        RecoverableEcdsaSignature as RecoverableEcdsaSignatureProto,
        RecoverableEd25519Signature as RecoverableEd25519SignatureProto,
        RevokeAssociation as RevokeAssociationProto, Signature as SignatureWrapperProto,
        SmartContractWalletSignature as SmartContractWalletSignatureProto,
    },
    message_contents::{
        signature::{Union, WalletEcdsaCompact},
        unsigned_public_key, Signature as SignedPublicKeySignatureProto,
        SignedPublicKey as LegacySignedPublicKeyProto,
        UnsignedPublicKey as LegacyUnsignedPublicKeyProto,
    },
};

#[derive(Error, Debug)]
pub enum DeserializationError {
    #[error(transparent)]
    SignatureError(#[from] crate::associations::SignatureError),
    #[error("Missing action")]
    MissingAction,
    #[error("Missing update")]
    MissingUpdate,
    #[error("Missing member identifier")]
    MissingMemberIdentifier,
    #[error("Missing signature")]
    Signature,
    #[error("Missing Member")]
    MissingMember,
    #[error("Decode error {0}")]
    Decode(#[from] DecodeError),
    #[error("Invalid account id")]
    InvalidAccountId,
}

impl TryFrom<IdentityUpdateProto> for UnverifiedIdentityUpdate {
    type Error = DeserializationError;

    fn try_from(proto: IdentityUpdateProto) -> Result<Self, Self::Error> {
        let IdentityUpdateProto {
            client_timestamp_ns,
            inbox_id,
            actions,
        } = proto;
        let all_actions = actions
            .into_iter()
            .map(|action| match action.kind {
                Some(action) => Ok(action),
                None => Err(DeserializationError::MissingAction),
            })
            .collect::<Result<Vec<IdentityActionKindProto>, DeserializationError>>()?;

        let processed_actions: Vec<UnverifiedAction> = all_actions
            .into_iter()
            .map(UnverifiedAction::try_from)
            .collect::<Result<Vec<UnverifiedAction>, DeserializationError>>()?;

        Ok(UnverifiedIdentityUpdate::new(
            inbox_id,
            client_timestamp_ns,
            processed_actions,
        ))
    }
}

impl TryFrom<IdentityActionKindProto> for UnverifiedAction {
    type Error = DeserializationError;

    fn try_from(action: IdentityActionKindProto) -> Result<Self, Self::Error> {
        Ok(match action {
            IdentityActionKindProto::Add(add_action) => {
                UnverifiedAction::AddAssociation(UnverifiedAddAssociation {
                    new_member_signature: add_action.new_member_signature.try_into()?,
                    existing_member_signature: add_action.existing_member_signature.try_into()?,
                    unsigned_action: UnsignedAddAssociation {
                        new_member_identifier: add_action
                            .new_member_identifier
                            .ok_or(DeserializationError::MissingMemberIdentifier)?
                            .try_into()?,
                    },
                })
            }
            IdentityActionKindProto::CreateInbox(action_proto) => {
                UnverifiedAction::CreateInbox(UnverifiedCreateInbox {
                    initial_address_signature: action_proto.initial_address_signature.try_into()?,
                    unsigned_action: UnsignedCreateInbox {
                        nonce: action_proto.nonce,
                        account_address: action_proto.initial_address,
                    },
                })
            }
            IdentityActionKindProto::ChangeRecoveryAddress(action_proto) => {
                UnverifiedAction::ChangeRecoveryAddress(UnverifiedChangeRecoveryAddress {
                    recovery_address_signature: action_proto
                        .existing_recovery_address_signature
                        .try_into()?,
                    unsigned_action: UnsignedChangeRecoveryAddress {
                        new_recovery_address: action_proto.new_recovery_address,
                    },
                })
            }
            IdentityActionKindProto::Revoke(action_proto) => {
                UnverifiedAction::RevokeAssociation(UnverifiedRevokeAssociation {
                    recovery_address_signature: action_proto
                        .recovery_address_signature
                        .try_into()?,
                    unsigned_action: UnsignedRevokeAssociation {
                        revoked_member: action_proto
                            .member_to_revoke
                            .ok_or(DeserializationError::MissingMember)?
                            .try_into()?,
                    },
                })
            }
        })
    }
}

impl TryFrom<SignatureWrapperProto> for UnverifiedSignature {
    type Error = DeserializationError;

    fn try_from(proto: SignatureWrapperProto) -> Result<Self, Self::Error> {
        let signature = unwrap_proto_signature(proto)?;
        let unverified_sig = match signature {
            SignatureKindProto::Erc191(sig) => UnverifiedSignature::RecoverableEcdsa(
                UnverifiedRecoverableEcdsaSignature::new(sig.bytes),
            ),
            SignatureKindProto::DelegatedErc191(sig) => {
                UnverifiedSignature::LegacyDelegated(UnverifiedLegacyDelegatedSignature::new(
                    UnverifiedRecoverableEcdsaSignature::new(
                        sig.signature.ok_or(DeserializationError::Signature)?.bytes,
                    ),
                    sig.delegated_key.ok_or(DeserializationError::Signature)?,
                ))
            }
            SignatureKindProto::InstallationKey(sig) => UnverifiedSignature::InstallationKey(
                UnverifiedInstallationKeySignature::new(sig.bytes, sig.public_key),
            ),
            SignatureKindProto::Erc6492(sig) => {
                UnverifiedSignature::Erc6492(UnverifiedErc6492Signature::new(
                    sig.signature,
                    sig.account_id.try_into()?,
                    sig.block_number,
                ))
            }
        };

        Ok(unverified_sig)
    }
}

impl TryFrom<Option<SignatureWrapperProto>> for UnverifiedSignature {
    type Error = DeserializationError;

    fn try_from(value: Option<SignatureWrapperProto>) -> Result<Self, Self::Error> {
        value
            .ok_or_else(|| DeserializationError::Signature)?
            .try_into()
    }
}

fn unwrap_proto_signature(
    value: SignatureWrapperProto,
) -> Result<SignatureKindProto, DeserializationError> {
    match value.signature {
        Some(inner) => Ok(inner),
        None => Err(DeserializationError::Signature),
    }
}

impl From<UnverifiedIdentityUpdate> for IdentityUpdateProto {
    fn from(value: UnverifiedIdentityUpdate) -> Self {
        Self {
            inbox_id: value.inbox_id,
            client_timestamp_ns: value.client_timestamp_ns,
            actions: map_vec(value.actions),
        }
    }
}

impl From<UnverifiedAction> for IdentityActionProto {
    fn from(value: UnverifiedAction) -> Self {
        let kind: IdentityActionKindProto = match value {
            UnverifiedAction::CreateInbox(action) => {
                IdentityActionKindProto::CreateInbox(CreateInboxProto {
                    nonce: action.unsigned_action.nonce,
                    initial_address: action.unsigned_action.account_address,
                    initial_address_signature: Some(action.initial_address_signature.into()),
                })
            }
            UnverifiedAction::AddAssociation(action) => {
                IdentityActionKindProto::Add(AddAssociationProto {
                    new_member_identifier: Some(
                        action.unsigned_action.new_member_identifier.into(),
                    ),
                    existing_member_signature: Some(action.existing_member_signature.into()),
                    new_member_signature: Some(action.new_member_signature.into()),
                })
            }
            UnverifiedAction::ChangeRecoveryAddress(action) => {
                IdentityActionKindProto::ChangeRecoveryAddress(ChangeRecoveryAddressProto {
                    new_recovery_address: action.unsigned_action.new_recovery_address,
                    existing_recovery_address_signature: Some(
                        action.recovery_address_signature.into(),
                    ),
                })
            }
            UnverifiedAction::RevokeAssociation(action) => {
                IdentityActionKindProto::Revoke(RevokeAssociationProto {
                    recovery_address_signature: Some(action.recovery_address_signature.into()),
                    member_to_revoke: Some(action.unsigned_action.revoked_member.into()),
                })
            }
        };

        IdentityActionProto { kind: Some(kind) }
    }
}

impl From<UnverifiedSignature> for SignatureWrapperProto {
    fn from(value: UnverifiedSignature) -> Self {
        let signature = match value {
            UnverifiedSignature::Erc6492(sig) => {
                SignatureKindProto::Erc6492(SmartContractWalletSignatureProto {
                    account_id: sig.account_id.into(),
                    block_number: sig.block_number,
                    signature: sig.signature_bytes,
                    // TOOD:nm Remove this field altogether
                    chain_rpc_url: "".to_string(),
                })
            }
            UnverifiedSignature::InstallationKey(sig) => {
                SignatureKindProto::InstallationKey(RecoverableEd25519SignatureProto {
                    bytes: sig.signature_bytes,
                    public_key: sig.verifying_key,
                })
            }
            UnverifiedSignature::LegacyDelegated(sig) => {
                SignatureKindProto::DelegatedErc191(LegacyDelegatedSignatureProto {
                    delegated_key: Some(sig.signed_public_key_proto),
                    signature: Some(RecoverableEcdsaSignatureProto {
                        bytes: sig.legacy_key_signature.signature_bytes,
                    }),
                })
            }
            UnverifiedSignature::RecoverableEcdsa(sig) => {
                SignatureKindProto::Erc191(RecoverableEcdsaSignatureProto {
                    bytes: sig.signature_bytes,
                })
            }
        };

        Self {
            signature: Some(signature),
        }
    }
}

// TODO:nm delete after usage removed
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

// TODO:nm delete
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
            Some(identifier) => Ok(identifier.into()),
            None => Err(DeserializationError::MissingMemberIdentifier),
        },
    }
}

impl From<MemberIdentifierKindProto> for MemberIdentifier {
    fn from(proto: MemberIdentifierKindProto) -> Self {
        match proto {
            MemberIdentifierKindProto::Address(address) => address.into(),
            MemberIdentifierKindProto::InstallationPublicKey(public_key) => public_key.into(),
        }
    }
}

// TODO:nm delete
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

// TODO:nm delete
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
        SignatureKindProto::Erc6492(signature) => Box::new(SmartContractWalletSignature::new(
            signature_text,
            signature.signature,
            signature.account_id.try_into()?,
            signature.chain_rpc_url,
            signature.block_number,
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

// TODO:nm delete
impl From<IdentityUpdate> for IdentityUpdateProto {
    fn from(update: IdentityUpdate) -> IdentityUpdateProto {
        let actions: Vec<IdentityActionProto> =
            update.actions.into_iter().map(Into::into).collect();

        IdentityUpdateProto {
            client_timestamp_ns: update.client_timestamp_ns,
            inbox_id: update.inbox_id,
            actions,
        }
    }
}

// TODO:nm delete
impl From<Action> for IdentityActionProto {
    fn from(action: Action) -> IdentityActionProto {
        match action {
            Action::AddAssociation(add_association) => IdentityActionProto {
                kind: Some(IdentityActionKindProto::Add(AddAssociationProto {
                    new_member_identifier: Some(add_association.new_member_identifier.into()),
                    new_member_signature: Some(add_association.new_member_signature.to_proto()),
                    existing_member_signature: Some(
                        add_association.existing_member_signature.to_proto(),
                    ),
                })),
            },
            Action::CreateInbox(create_inbox) => IdentityActionProto {
                kind: Some(IdentityActionKindProto::CreateInbox(CreateInboxProto {
                    nonce: create_inbox.nonce,
                    initial_address: create_inbox.account_address,
                    initial_address_signature: Some(
                        create_inbox.initial_address_signature.to_proto(),
                    ),
                })),
            },
            Action::RevokeAssociation(revoke_association) => IdentityActionProto {
                kind: Some(IdentityActionKindProto::Revoke(RevokeAssociationProto {
                    member_to_revoke: Some(revoke_association.revoked_member.into()),
                    recovery_address_signature: Some(
                        revoke_association.recovery_address_signature.to_proto(),
                    ),
                })),
            },
            Action::ChangeRecoveryAddress(change_recovery_address) => IdentityActionProto {
                kind: Some(IdentityActionKindProto::ChangeRecoveryAddress(
                    ChangeRecoveryAddressProto {
                        new_recovery_address: change_recovery_address.new_recovery_address,
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
}

impl From<Member> for MemberProto {
    fn from(member: Member) -> MemberProto {
        MemberProto {
            identifier: Some(member.identifier.into()),
            added_by_entity: member.added_by_entity.map(Into::into),
        }
    }
}

impl TryFrom<MemberProto> for Member {
    type Error = DeserializationError;

    fn try_from(proto: MemberProto) -> Result<Self, Self::Error> {
        Ok(Member {
            identifier: proto
                .identifier
                .ok_or(DeserializationError::MissingMemberIdentifier)?
                .try_into()?,
            added_by_entity: proto.added_by_entity.map(TryInto::try_into).transpose()?,
        })
    }
}

impl From<MemberIdentifier> for MemberIdentifierProto {
    fn from(member_identifier: MemberIdentifier) -> MemberIdentifierProto {
        match member_identifier {
            MemberIdentifier::Address(address) => MemberIdentifierProto {
                kind: Some(MemberIdentifierKindProto::Address(address)),
            },
            MemberIdentifier::Installation(public_key) => MemberIdentifierProto {
                kind: Some(MemberIdentifierKindProto::InstallationPublicKey(public_key)),
            },
        }
    }
}

impl TryFrom<MemberIdentifierProto> for MemberIdentifier {
    type Error = DeserializationError;

    fn try_from(proto: MemberIdentifierProto) -> Result<Self, Self::Error> {
        match proto.kind {
            Some(MemberIdentifierKindProto::Address(address)) => {
                Ok(MemberIdentifier::Address(address))
            }
            Some(MemberIdentifierKindProto::InstallationPublicKey(public_key)) => {
                Ok(MemberIdentifier::Installation(public_key))
            }
            None => Err(DeserializationError::MissingMemberIdentifier),
        }
    }
}

impl From<AssociationState> for AssociationStateProto {
    fn from(state: AssociationState) -> AssociationStateProto {
        let members = state
            .members
            .into_iter()
            .map(|(key, value)| MemberMapProto {
                key: Some(key.into()),
                value: Some(value.into()),
            })
            .collect();

        AssociationStateProto {
            inbox_id: state.inbox_id,
            members,
            recovery_address: state.recovery_address,
            seen_signatures: state.seen_signatures.into_iter().collect(),
        }
    }
}

impl TryFrom<AssociationStateProto> for AssociationState {
    type Error = DeserializationError;

    fn try_from(proto: AssociationStateProto) -> Result<Self, Self::Error> {
        let members = proto
            .members
            .into_iter()
            .map(|kv| {
                let key = kv
                    .key
                    .ok_or(DeserializationError::MissingMemberIdentifier)?
                    .try_into()?;
                let value = kv
                    .value
                    .ok_or(DeserializationError::MissingMember)?
                    .try_into()?;
                Ok((key, value))
            })
            .collect::<Result<HashMap<MemberIdentifier, Member>, DeserializationError>>()?;
        Ok(AssociationState {
            inbox_id: proto.inbox_id,
            members,
            recovery_address: proto.recovery_address,
            seen_signatures: HashSet::from_iter(proto.seen_signatures),
        })
    }
}

impl From<AssociationStateDiff> for AssociationStateDiffProto {
    fn from(diff: AssociationStateDiff) -> AssociationStateDiffProto {
        AssociationStateDiffProto {
            new_members: diff.new_members.into_iter().map(Into::into).collect(),
            removed_members: diff.removed_members.into_iter().map(Into::into).collect(),
        }
    }
}

/// Convert a vector of `A` into a vector of `B` using [`From`]
pub fn map_vec<A, B: From<A>>(other: Vec<A>) -> Vec<B> {
    other.into_iter().map(B::from).collect()
}

/// Convert a vector of `A` into a vector of `B` using [`TryFrom`]
/// Useful to convert vectors of structs into protos, like `Vec<IdentityUpdate>` to `Vec<IdentityUpdateProto>` or vice-versa.
pub fn try_map_vec<A, B: TryFrom<A>>(other: Vec<A>) -> Result<Vec<B>, <B as TryFrom<A>>::Error> {
    other.into_iter().map(B::try_from).collect()
}

// TODO:nm This doesn't really feel like serialization, maybe should move
impl TryFrom<LegacySignedPublicKeyProto> for ValidatedLegacySignedPublicKey {
    type Error = SignatureError;

    fn try_from(proto: LegacySignedPublicKeyProto) -> Result<Self, Self::Error> {
        let serialized_key_data = proto.key_bytes;
        let union = proto
            .signature
            .ok_or(SignatureError::Invalid)?
            .union
            .ok_or(SignatureError::Invalid)?;
        let wallet_signature = match union {
            Union::WalletEcdsaCompact(wallet_ecdsa_compact) => {
                let mut wallet_signature = wallet_ecdsa_compact.bytes.clone();
                wallet_signature.push(wallet_ecdsa_compact.recovery as u8); // TODO: normalize recovery ID if necessary
                if wallet_signature.len() != 65 {
                    return Err(SignatureError::Invalid);
                }
                wallet_signature
            }
            Union::EcdsaCompact(ecdsa_compact) => {
                let mut signature = ecdsa_compact.bytes.clone();
                signature.push(ecdsa_compact.recovery as u8); // TODO: normalize recovery ID if necessary
                if signature.len() != 65 {
                    return Err(SignatureError::Invalid);
                }
                signature
            }
        };
        let wallet_signature = RecoverableSignature::Eip191Signature(wallet_signature);
        let account_address =
            wallet_signature.recover_address(&Self::text(&serialized_key_data))?;
        let account_address = sanitize_evm_addresses(vec![account_address])?[0].clone();

        let legacy_unsigned_public_key_proto =
            LegacyUnsignedPublicKeyProto::decode(serialized_key_data.as_slice())
                .or(Err(SignatureError::Invalid))?;
        let public_key_bytes = match legacy_unsigned_public_key_proto
            .union
            .ok_or(SignatureError::Invalid)?
        {
            unsigned_public_key::Union::Secp256k1Uncompressed(secp256k1_uncompressed) => {
                secp256k1_uncompressed.bytes
            }
        };
        let created_ns = legacy_unsigned_public_key_proto.created_ns;

        Ok(Self {
            account_address,
            wallet_signature,
            serialized_key_data,
            public_key_bytes,
            created_ns,
        })
    }
}

impl From<ValidatedLegacySignedPublicKey> for LegacySignedPublicKeyProto {
    fn from(validated: ValidatedLegacySignedPublicKey) -> Self {
        let RecoverableSignature::Eip191Signature(signature) = validated.wallet_signature;
        Self {
            key_bytes: validated.serialized_key_data,
            signature: Some(SignedPublicKeySignatureProto {
                union: Some(Union::WalletEcdsaCompact(WalletEcdsaCompact {
                    bytes: signature[0..64].to_vec(),
                    recovery: signature[64] as u32,
                })),
            }),
        }
    }
}

impl TryFrom<String> for AccountId {
    type Error = DeserializationError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 3 {
            return Err(DeserializationError::InvalidAccountId);
        }
        let chain_id = format!("{}:{}", parts[0], parts[1]);
        let chain_id_regex = Regex::new(r"^[-a-z0-9]{3,8}:[-_a-zA-Z0-9]{1,32}$").unwrap();
        let account_address = parts[2];
        let account_address_regex = Regex::new(r"^[-.%a-zA-Z0-9]{1,128}$").unwrap();
        if !chain_id_regex.is_match(&chain_id) || !account_address_regex.is_match(account_address) {
            return Err(DeserializationError::InvalidAccountId);
        }

        Ok(AccountId {
            chain_id: chain_id.to_string(),
            account_address: account_address.to_string(),
        })
    }
}

impl TryFrom<&str> for AccountId {
    type Error = DeserializationError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        s.to_string().try_into()
    }
}

impl From<AccountId> for String {
    fn from(account_id: AccountId) -> Self {
        format!("{}:{}", account_id.chain_id, account_id.account_address)
    }
}

#[cfg(test)]
mod tests {
    use crate::associations::{
        hashes::generate_inbox_id,
        test_utils::{rand_string, rand_u64, rand_vec},
    };

    use super::*;

    #[test]
    fn test_round_trip_unverified() {
        let account_address = rand_string();
        let nonce = rand_u64();
        let inbox_id = generate_inbox_id(&account_address, &nonce);
        let client_timestamp_ns = rand_u64();
        let signature_bytes = rand_vec();

        let identity_update = UnverifiedIdentityUpdate::new(
            inbox_id,
            client_timestamp_ns,
            vec![UnverifiedAction::CreateInbox(UnverifiedCreateInbox {
                initial_address_signature: UnverifiedSignature::RecoverableEcdsa(
                    UnverifiedRecoverableEcdsaSignature::new(signature_bytes),
                ),
                unsigned_action: UnsignedCreateInbox {
                    nonce,
                    account_address,
                },
            })],
        );

        let serialized_update = IdentityUpdateProto::from(identity_update.clone());

        assert_eq!(
            serialized_update.client_timestamp_ns,
            identity_update.client_timestamp_ns
        );
        assert_eq!(serialized_update.actions.len(), 1);

        let deserialized_update: UnverifiedIdentityUpdate = serialized_update
            .clone()
            .try_into()
            .expect("deserialization error");

        let reserialized = IdentityUpdateProto::from(deserialized_update);

        assert_eq!(serialized_update, reserialized);
    }

    #[test]
    fn test_round_trip() {
        let account_address = rand_string();
        let nonce = rand_u64();
        let inbox_id = generate_inbox_id(&account_address, &nonce);

        let identity_update = IdentityUpdate::new(
            vec![Action::CreateInbox(CreateInbox {
                nonce,
                account_address,
                initial_address_signature: Box::new(RecoverableEcdsaSignature::new(
                    "foo".to_string(),
                    vec![1, 2, 3],
                )),
            })],
            inbox_id,
            rand_u64(),
        );

        let serialized_update = IdentityUpdateProto::from(identity_update.clone());

        assert_eq!(
            serialized_update.client_timestamp_ns,
            identity_update.client_timestamp_ns
        );
        assert_eq!(serialized_update.actions.len(), 1);

        let deserialized_update = from_identity_update_proto(serialized_update.clone())
            .expect("deserialization should succeed");

        let reserialized = IdentityUpdateProto::from(deserialized_update);

        assert_eq!(serialized_update, reserialized);
    }

    #[test]
    fn test_accound_id() {
        // valid evm chain
        let text = "eip155:1:0xab16a96D359eC26a11e2C2b3d8f8B8942d5Bfcdb".to_string();
        let account_id: AccountId = text.clone().try_into().unwrap();
        assert_eq!(account_id.chain_id, "eip155:1");
        assert_eq!(
            account_id.account_address,
            "0xab16a96D359eC26a11e2C2b3d8f8B8942d5Bfcdb"
        );
        assert!(account_id.is_evm_chain());
        let proto: String = account_id.into();
        assert_eq!(text, proto);

        // valid Bitcoin mainnet
        let text = "bip122:000000000019d6689c085ae165831e93:128Lkh3S7CkDTBZ8W7BbpsN3YYizJMp8p6";
        let account_id: AccountId = text.try_into().unwrap();
        assert_eq!(
            account_id.chain_id,
            "bip122:000000000019d6689c085ae165831e93"
        );
        assert_eq!(
            account_id.account_address,
            "128Lkh3S7CkDTBZ8W7BbpsN3YYizJMp8p6"
        );
        assert!(!account_id.is_evm_chain());
        let proto: String = account_id.into();
        assert_eq!(text, proto);

        // valid Cosmos Hub
        let text = "cosmos:cosmoshub-3:cosmos1t2uflqwqe0fsj0shcfkrvpukewcw40yjj6hdc0";
        let account_id: AccountId = text.try_into().unwrap();
        assert_eq!(account_id.chain_id, "cosmos:cosmoshub-3");
        assert_eq!(
            account_id.account_address,
            "cosmos1t2uflqwqe0fsj0shcfkrvpukewcw40yjj6hdc0"
        );
        assert!(!account_id.is_evm_chain());
        let proto: String = account_id.into();
        assert_eq!(text, proto);

        // valid Kusama network
        let text = "polkadot:b0a8d493285c2df73290dfb7e61f870f:5hmuyxw9xdgbpptgypokw4thfyoe3ryenebr381z9iaegmfy";
        let account_id: AccountId = text.try_into().unwrap();
        assert_eq!(
            account_id.chain_id,
            "polkadot:b0a8d493285c2df73290dfb7e61f870f"
        );
        assert_eq!(
            account_id.account_address,
            "5hmuyxw9xdgbpptgypokw4thfyoe3ryenebr381z9iaegmfy"
        );
        assert!(!account_id.is_evm_chain());
        let proto: String = account_id.into();
        assert_eq!(text, proto);

        // valid StarkNet Testnet
        let text =
            "starknet:SN_GOERLI:0x02dd1b492765c064eac4039e3841aa5f382773b598097a40073bd8b48170ab57";
        let account_id: AccountId = text.try_into().unwrap();
        assert_eq!(account_id.chain_id, "starknet:SN_GOERLI");
        assert_eq!(
            account_id.account_address,
            "0x02dd1b492765c064eac4039e3841aa5f382773b598097a40073bd8b48170ab57"
        );
        assert!(!account_id.is_evm_chain());
        let proto: String = account_id.into();
        assert_eq!(text, proto);

        // dummy max length (64+1+8+1+32 = 106 chars/bytes)
        let text = "chainstd:8c3444cf8970a9e41a706fab93e7a6c4:6d9b0b4b9994e8a6afbd3dc3ed983cd51c755afb27cd1dc7825ef59c134a39f7";
        let account_id: AccountId = text.try_into().unwrap();
        assert_eq!(
            account_id.chain_id,
            "chainstd:8c3444cf8970a9e41a706fab93e7a6c4"
        );
        assert_eq!(
            account_id.account_address,
            "6d9b0b4b9994e8a6afbd3dc3ed983cd51c755afb27cd1dc7825ef59c134a39f7"
        );
        assert!(!account_id.is_evm_chain());
        let proto: String = account_id.into();
        assert_eq!(text, proto);

        // Hedera address (with optional checksum suffix per [HIP-15][])
        let text = "hedera:mainnet:0.0.1234567890-zbhlt";
        let account_id: AccountId = text.try_into().unwrap();
        assert_eq!(account_id.chain_id, "hedera:mainnet");
        assert_eq!(account_id.account_address, "0.0.1234567890-zbhlt");
        assert!(!account_id.is_evm_chain());
        let proto: String = account_id.into();
        assert_eq!(text, proto);

        // invalid
        let text = "eip/155:1:0xab16a96D359eC26a11e2C2b3d8f8B8942d5Bfcd";
        let result: Result<AccountId, DeserializationError> = text.try_into();
        assert!(matches!(
            result,
            Err(DeserializationError::InvalidAccountId)
        ));
    }

    #[test]
    fn test_account_id_create() {
        let address = "0xab16a96D359eC26a11e2C2b3d8f8B8942d5Bfcdb".to_string();
        let chain_id = 12;
        let account_id = AccountId::new_evm(chain_id, address.clone());
        assert_eq!(account_id.account_address, address);
        assert_eq!(account_id.chain_id, "eip155:12");
    }
}
