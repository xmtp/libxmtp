use super::{
    ident,
    member::{Member, PublicIdentifier},
    signature::{AccountId, ValidatedLegacySignedPublicKey},
    state::{AssociationState, AssociationStateDiff},
    unsigned_actions::{
        UnsignedAddAssociation, UnsignedChangeRecoveryAddress, UnsignedCreateInbox,
        UnsignedRevokeAssociation,
    },
    unverified::{
        UnverifiedAction, UnverifiedAddAssociation, UnverifiedChangeRecoveryAddress,
        UnverifiedCreateInbox, UnverifiedIdentityUpdate, UnverifiedInstallationKeySignature,
        UnverifiedLegacyDelegatedSignature, UnverifiedRecoverableEcdsaSignature,
        UnverifiedRevokeAssociation, UnverifiedSignature, UnverifiedSmartContractWalletSignature,
    },
    verified_signature::VerifiedSignature,
    MemberIdentifier, SignatureError,
};
use crate::scw_verifier::ValidationResponse;
use prost::{DecodeError, Message};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use thiserror::Error;
use xmtp_cryptography::signature::{sanitize_evm_addresses, IdentifierValidationError};
use xmtp_proto::xmtp::{
    identity::{
        api::v1::verify_smart_contract_wallet_signatures_response::ValidationResponse as SmartContractWalletValidationResponseProto,
        associations::{
            identity_action::Kind as IdentityActionKindProto,
            member_identifier::Kind as MemberIdentifierKindProto,
            signature::Signature as SignatureKindProto, AddAssociation as AddAssociationProto,
            AssociationState as AssociationStateProto,
            AssociationStateDiff as AssociationStateDiffProto,
            ChangeRecoveryAddress as ChangeRecoveryAddressProto, CreateInbox as CreateInboxProto,
            IdentifierKind, IdentityAction as IdentityActionProto,
            IdentityUpdate as IdentityUpdateProto,
            LegacyDelegatedSignature as LegacyDelegatedSignatureProto, Member as MemberProto,
            MemberIdentifier as MemberIdentifierProto, MemberMap as MemberMapProto,
            RecoverableEcdsaSignature as RecoverableEcdsaSignatureProto,
            RecoverableEd25519Signature as RecoverableEd25519SignatureProto,
            RevokeAssociation as RevokeAssociationProto, Signature as SignatureWrapperProto,
            SmartContractWalletSignature as SmartContractWalletSignatureProto,
        },
    },
    message_contents::{
        signature::{Union, WalletEcdsaCompact},
        unsigned_public_key, Signature as SignedPublicKeySignatureProto,
        SignedPublicKey as LegacySignedPublicKeyProto, SignedPublicKey as SignedPublicKeyProto,
        UnsignedPublicKey as LegacyUnsignedPublicKeyProto,
    },
};
use xmtp_proto::ConversionError;

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
    #[error("Invalid passkey")]
    InvalidPasskey,
    #[error("Invalid hash (needs to be 32 bytes)")]
    InvalidHash,
    #[error("A required field is unspecified: {0}")]
    Unspecified(&'static str),
    #[error("Error creating public key from proto bytes")]
    Ed25519(#[from] ed25519_dalek::ed25519::Error),
    #[error("Unable to deserialize")]
    Bincode,
    #[error(transparent)]
    AddressValidation(#[from] IdentifierValidationError),
}

impl TryFrom<IdentityUpdateProto> for UnverifiedIdentityUpdate {
    type Error = ConversionError;

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
                None => Err(ConversionError::Missing {
                    item: "action",
                    r#type: std::any::type_name::<IdentityActionKindProto>(),
                }),
            })
            .collect::<Result<Vec<IdentityActionKindProto>, ConversionError>>()?;

        let processed_actions: Vec<UnverifiedAction> = all_actions
            .into_iter()
            .map(UnverifiedAction::try_from)
            .collect::<Result<Vec<UnverifiedAction>, ConversionError>>()?;

        Ok(UnverifiedIdentityUpdate::new(
            inbox_id,
            client_timestamp_ns,
            processed_actions,
        ))
    }
}

impl TryFrom<IdentityActionKindProto> for UnverifiedAction {
    type Error = ConversionError;

    fn try_from(action: IdentityActionKindProto) -> Result<Self, Self::Error> {
        Ok(match action {
            IdentityActionKindProto::Add(add_action) => {
                UnverifiedAction::AddAssociation(UnverifiedAddAssociation {
                    new_member_signature: add_action.new_member_signature.try_into()?,
                    existing_member_signature: add_action.existing_member_signature.try_into()?,
                    unsigned_action: UnsignedAddAssociation {
                        new_member_identifier: add_action
                            .new_member_identifier
                            .ok_or(ConversionError::Missing {
                                item: "member_identifier",
                                r#type: std::any::type_name::<MemberIdentifierProto>(),
                            })?
                            .try_into()?,
                    },
                })
            }
            IdentityActionKindProto::CreateInbox(action_proto) => {
                let account_identifier = PublicIdentifier::from_proto(
                    &action_proto.initial_identifier,
                    action_proto.initial_identifier_kind(),
                )?;

                UnverifiedAction::CreateInbox(UnverifiedCreateInbox {
                    initial_address_signature: action_proto
                        .initial_identifier_signature
                        .try_into()?,
                    unsigned_action: UnsignedCreateInbox {
                        nonce: action_proto.nonce,
                        account_identifier,
                    },
                })
            }
            IdentityActionKindProto::ChangeRecoveryAddress(action_proto) => {
                let new_recovery_identifier = PublicIdentifier::from_proto(
                    &action_proto.new_recovery_identifier,
                    action_proto.new_recovery_identifier_kind(),
                )?;
                UnverifiedAction::ChangeRecoveryAddress(UnverifiedChangeRecoveryAddress {
                    recovery_identifier_signature: action_proto
                        .existing_recovery_identifier_signature
                        .try_into()?,
                    unsigned_action: UnsignedChangeRecoveryAddress {
                        new_recovery_identifier,
                    },
                })
            }
            IdentityActionKindProto::Revoke(action_proto) => {
                UnverifiedAction::RevokeAssociation(UnverifiedRevokeAssociation {
                    recovery_identifier_signature: action_proto
                        .recovery_identifier_signature
                        .try_into()?,
                    unsigned_action: UnsignedRevokeAssociation {
                        revoked_member: action_proto
                            .member_to_revoke
                            .ok_or(ConversionError::Missing {
                                item: "member_to_revoke",
                                r#type: std::any::type_name::<MemberIdentifierProto>(),
                            })?
                            .try_into()?,
                    },
                })
            }
        })
    }
}

impl TryFrom<SignatureWrapperProto> for UnverifiedSignature {
    type Error = ConversionError;

    fn try_from(proto: SignatureWrapperProto) -> Result<Self, Self::Error> {
        let signature = unwrap_proto_signature(proto)?;
        let unverified_sig = match signature {
            SignatureKindProto::Erc191(sig) => UnverifiedSignature::RecoverableEcdsa(
                UnverifiedRecoverableEcdsaSignature::new(sig.bytes),
            ),
            SignatureKindProto::DelegatedErc191(sig) => {
                UnverifiedSignature::LegacyDelegated(UnverifiedLegacyDelegatedSignature::new(
                    UnverifiedRecoverableEcdsaSignature::new(
                        sig.signature
                            .ok_or(ConversionError::Missing {
                                item: "signature",
                                r#type: std::any::type_name::<RecoverableEcdsaSignatureProto>(),
                            })?
                            .bytes,
                    ),
                    sig.delegated_key.ok_or(ConversionError::Missing {
                        item: "delegated_key",
                        r#type: std::any::type_name::<SignedPublicKeyProto>(),
                    })?,
                ))
            }
            SignatureKindProto::InstallationKey(sig) => {
                UnverifiedSignature::InstallationKey(UnverifiedInstallationKeySignature::new(
                    sig.bytes,
                    sig.public_key.as_slice().try_into()?,
                ))
            }
            SignatureKindProto::Erc6492(sig) => UnverifiedSignature::SmartContractWallet(
                UnverifiedSmartContractWalletSignature::new(
                    sig.signature,
                    sig.account_id.try_into()?,
                    sig.block_number,
                ),
            ),
        };

        Ok(unverified_sig)
    }
}

impl TryFrom<Option<SignatureWrapperProto>> for UnverifiedSignature {
    type Error = ConversionError;

    fn try_from(value: Option<SignatureWrapperProto>) -> Result<Self, Self::Error> {
        value
            .ok_or_else(|| ConversionError::Missing {
                item: "signature",
                r#type: std::any::type_name::<SignatureWrapperProto>(),
            })?
            .try_into()
    }
}

fn unwrap_proto_signature(
    value: SignatureWrapperProto,
) -> Result<SignatureKindProto, ConversionError> {
    match value.signature {
        Some(inner) => Ok(inner),
        None => Err(ConversionError::Missing {
            item: "signature",
            r#type: std::any::type_name::<SignatureKindProto>(),
        }),
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
                let account_identifier = action.unsigned_action.account_identifier;
                let initial_identifier = format!("{account_identifier}");
                let initial_identifier_kind: IdentifierKind = account_identifier.into();
                IdentityActionKindProto::CreateInbox(CreateInboxProto {
                    nonce: action.unsigned_action.nonce,
                    initial_identifier,
                    initial_identifier_kind: initial_identifier_kind as i32,
                    initial_identifier_signature: Some(action.initial_address_signature.into()),
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
                let new_recovery_identifier = action.unsigned_action.new_recovery_identifier;
                let new_recovery_identifier_string = format!("{new_recovery_identifier}");
                let new_recovery_identifier_kind: IdentifierKind = new_recovery_identifier.into();
                IdentityActionKindProto::ChangeRecoveryAddress(ChangeRecoveryAddressProto {
                    new_recovery_identifier: new_recovery_identifier_string,
                    new_recovery_identifier_kind: new_recovery_identifier_kind as i32,
                    existing_recovery_identifier_signature: Some(
                        action.recovery_identifier_signature.into(),
                    ),
                })
            }
            UnverifiedAction::RevokeAssociation(action) => {
                IdentityActionKindProto::Revoke(RevokeAssociationProto {
                    recovery_identifier_signature: Some(
                        action.recovery_identifier_signature.into(),
                    ),
                    member_to_revoke: Some(action.unsigned_action.revoked_member.into()),
                })
            }
        };

        IdentityActionProto { kind: Some(kind) }
    }
}

impl From<&PublicIdentifier> for IdentifierKind {
    fn from(ident: &PublicIdentifier) -> Self {
        match ident {
            PublicIdentifier::Ethereum(_) => IdentifierKind::Ethereum,
            PublicIdentifier::Passkey(_) => IdentifierKind::Passkey,
        }
    }
}
impl From<PublicIdentifier> for IdentifierKind {
    fn from(ident: PublicIdentifier) -> Self {
        (&ident).into()
    }
}

impl From<UnverifiedSignature> for SignatureWrapperProto {
    fn from(value: UnverifiedSignature) -> Self {
        let signature = match value {
            UnverifiedSignature::SmartContractWallet(sig) => {
                SignatureKindProto::Erc6492(SmartContractWalletSignatureProto {
                    account_id: sig.account_id.into(),
                    block_number: sig.block_number,
                    signature: sig.signature_bytes,
                })
            }
            UnverifiedSignature::InstallationKey(UnverifiedInstallationKeySignature {
                signature_bytes,
                verifying_key,
            }) => SignatureKindProto::InstallationKey(RecoverableEd25519SignatureProto {
                bytes: signature_bytes,
                public_key: verifying_key.as_bytes().to_vec(),
            }),
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

impl TryFrom<Vec<u8>> for UnverifiedIdentityUpdate {
    type Error = ConversionError;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        let update_proto: IdentityUpdateProto = IdentityUpdateProto::decode(value.as_slice())?;
        UnverifiedIdentityUpdate::try_from(update_proto)
    }
}

impl From<UnverifiedIdentityUpdate> for Vec<u8> {
    fn from(value: UnverifiedIdentityUpdate) -> Self {
        let proto: IdentityUpdateProto = value.into();
        proto.encode_to_vec()
    }
}

impl From<SmartContractWalletValidationResponseProto> for ValidationResponse {
    fn from(value: SmartContractWalletValidationResponseProto) -> Self {
        Self {
            is_valid: value.is_valid,
            block_number: value.block_number,
            error: value.error,
        }
    }
}

impl From<MemberIdentifierKindProto> for MemberIdentifier {
    fn from(proto: MemberIdentifierKindProto) -> Self {
        match proto {
            MemberIdentifierKindProto::EthereumAddress(address) => {
                Self::Ethereum(ident::Ethereum(address))
            }
            MemberIdentifierKindProto::InstallationPublicKey(public_key) => {
                Self::Installation(ident::Installation(public_key))
            }
            MemberIdentifierKindProto::PasskeyPublicKey(key) => Self::Passkey(ident::Passkey(key)),
        }
    }
}

impl From<Member> for MemberProto {
    fn from(member: Member) -> MemberProto {
        MemberProto {
            identifier: Some(member.identifier.into()),
            added_by_entity: member.added_by_entity.map(Into::into),
            client_timestamp_ns: member.client_timestamp_ns,
            added_on_chain_id: member.added_on_chain_id,
        }
    }
}

impl TryFrom<MemberProto> for Member {
    type Error = ConversionError;

    fn try_from(proto: MemberProto) -> Result<Self, Self::Error> {
        Ok(Member {
            identifier: proto
                .identifier
                .ok_or(ConversionError::Missing {
                    item: "member_identifier",
                    r#type: std::any::type_name::<MemberIdentifierProto>(),
                })?
                .try_into()?,
            added_by_entity: proto.added_by_entity.map(TryInto::try_into).transpose()?,
            client_timestamp_ns: proto.client_timestamp_ns,
            added_on_chain_id: proto.added_on_chain_id,
        })
    }
}

impl From<MemberIdentifier> for MemberIdentifierProto {
    fn from(member_identifier: MemberIdentifier) -> MemberIdentifierProto {
        match member_identifier {
            MemberIdentifier::Ethereum(ident::Ethereum(address)) => MemberIdentifierProto {
                kind: Some(MemberIdentifierKindProto::EthereumAddress(address)),
            },
            MemberIdentifier::Installation(ident::Installation(public_key)) => {
                MemberIdentifierProto {
                    kind: Some(MemberIdentifierKindProto::InstallationPublicKey(public_key)),
                }
            }
            MemberIdentifier::Passkey(ident::Passkey(key)) => MemberIdentifierProto {
                kind: Some(MemberIdentifierKindProto::PasskeyPublicKey(key.to_vec())),
            },
        }
    }
}

impl TryFrom<MemberIdentifierProto> for MemberIdentifier {
    type Error = ConversionError;

    fn try_from(proto: MemberIdentifierProto) -> Result<Self, Self::Error> {
        match proto.kind {
            Some(MemberIdentifierKindProto::EthereumAddress(address)) => {
                Ok(MemberIdentifier::Ethereum(ident::Ethereum(address)))
            }
            Some(MemberIdentifierKindProto::InstallationPublicKey(public_key)) => Ok(
                MemberIdentifier::Installation(ident::Installation(public_key)),
            ),
            Some(MemberIdentifierKindProto::PasskeyPublicKey(key)) => {
                Ok(MemberIdentifier::Passkey(ident::Passkey(key)))
            }
            None => Err(ConversionError::Missing {
                item: "member_identifier",
                r#type: std::any::type_name::<MemberIdentifierKindProto>(),
            }),
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

        let kind: IdentifierKind = (&state.recovery_identifier).into();

        AssociationStateProto {
            inbox_id: state.inbox_id,
            members,
            recovery_identifier: state.recovery_identifier.to_string(),
            recovery_identifier_kind: kind as i32,
            seen_signatures: state.seen_signatures.into_iter().collect(),
        }
    }
}

impl TryFrom<AssociationStateProto> for AssociationState {
    type Error = ConversionError;

    fn try_from(proto: AssociationStateProto) -> Result<Self, Self::Error> {
        let recovery_identifier = PublicIdentifier::from_proto(
            &proto.recovery_identifier,
            proto.recovery_identifier_kind(),
        )?;

        let members = proto
            .members
            .into_iter()
            .map(|kv| {
                let key = kv
                    .key
                    .ok_or(ConversionError::Missing {
                        item: "member_identifier",
                        r#type: std::any::type_name::<MemberIdentifierProto>(),
                    })?
                    .try_into()?;
                let value = kv
                    .value
                    .ok_or(ConversionError::Missing {
                        item: "member",
                        r#type: std::any::type_name::<MemberProto>(),
                    })?
                    .try_into()?;
                Ok((key, value))
            })
            .collect::<Result<HashMap<MemberIdentifier, Member>, ConversionError>>()?;

        Ok(AssociationState {
            inbox_id: proto.inbox_id,
            members,
            recovery_identifier,
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
        let verified_wallet_signature = VerifiedSignature::from_recoverable_ecdsa(
            Self::text(&serialized_key_data),
            &wallet_signature,
        )?;

        let account_address = verified_wallet_signature.signer.to_string();
        let account_address = sanitize_evm_addresses(&[account_address])?[0].clone();

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
            wallet_signature: verified_wallet_signature,
            serialized_key_data,
            public_key_bytes,
            created_ns,
        })
    }
}

impl From<ValidatedLegacySignedPublicKey> for LegacySignedPublicKeyProto {
    fn from(validated: ValidatedLegacySignedPublicKey) -> Self {
        let signature = validated.wallet_signature.raw_bytes;
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
    type Error = ConversionError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 3 {
            return Err(ConversionError::InvalidLength {
                item: "account_id",
                expected: 3,
                got: parts.len(),
            });
        }
        let chain_id = format!("{}:{}", parts[0], parts[1]);
        let chain_id_regex = Regex::new(r"^[-a-z0-9]{3,8}:[-_a-zA-Z0-9]{1,32}$")
            .expect("Static regex should always compile");
        let account_address = parts[2];
        let account_address_regex =
            Regex::new(r"^[-.%a-zA-Z0-9]{1,128}$").expect("static regex should always compile");
        if !chain_id_regex.is_match(&chain_id) || !account_address_regex.is_match(account_address) {
            return Err(ConversionError::InvalidValue {
                item: "eth account_id",
                expected: "well-formed chain_id & address",
                got: "chain_id/address did not pass validation",
            });
        }

        Ok(AccountId {
            chain_id: chain_id.to_string(),
            account_address: account_address.to_string(),
        })
    }
}

impl TryFrom<&str> for AccountId {
    type Error = ConversionError;

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
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use xmtp_common::{rand_u64, rand_vec};

    use super::*;

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_round_trip_unverified() {
        let account_identifier = PublicIdentifier::rand_ethereum();
        let nonce = rand_u64();
        let inbox_id = account_identifier.inbox_id(nonce).unwrap();
        let client_timestamp_ns = rand_u64();
        let signature_bytes = rand_vec::<32>();

        let identity_update = UnverifiedIdentityUpdate::new(
            inbox_id,
            client_timestamp_ns,
            vec![
                UnverifiedAction::CreateInbox(UnverifiedCreateInbox {
                    initial_address_signature: UnverifiedSignature::RecoverableEcdsa(
                        UnverifiedRecoverableEcdsaSignature::new(signature_bytes),
                    ),
                    unsigned_action: UnsignedCreateInbox {
                        nonce,
                        account_identifier,
                    },
                }),
                UnverifiedAction::AddAssociation(UnverifiedAddAssociation {
                    new_member_signature: UnverifiedSignature::new_recoverable_ecdsa(vec![1, 2, 3]),
                    existing_member_signature: UnverifiedSignature::new_recoverable_ecdsa(vec![
                        4, 5, 6,
                    ]),
                    unsigned_action: UnsignedAddAssociation {
                        new_member_identifier: MemberIdentifier::rand_ethereum(),
                    },
                }),
                UnverifiedAction::ChangeRecoveryAddress(UnverifiedChangeRecoveryAddress {
                    recovery_identifier_signature: UnverifiedSignature::new_recoverable_ecdsa(
                        vec![7, 8, 9],
                    ),
                    unsigned_action: UnsignedChangeRecoveryAddress {
                        new_recovery_identifier: PublicIdentifier::rand_ethereum(),
                    },
                }),
                UnverifiedAction::RevokeAssociation(UnverifiedRevokeAssociation {
                    recovery_identifier_signature: UnverifiedSignature::new_recoverable_ecdsa(
                        vec![10, 11, 12],
                    ),
                    unsigned_action: UnsignedRevokeAssociation {
                        revoked_member: MemberIdentifier::rand_ethereum(),
                    },
                }),
            ],
        );

        let serialized_update = IdentityUpdateProto::from(identity_update.clone());

        assert_eq!(
            serialized_update.client_timestamp_ns,
            identity_update.client_timestamp_ns
        );
        assert_eq!(serialized_update.actions.len(), 4);

        let deserialized_update: UnverifiedIdentityUpdate = serialized_update
            .clone()
            .try_into()
            .expect("deserialization error");

        assert_eq!(deserialized_update, identity_update);

        let reserialized = IdentityUpdateProto::from(deserialized_update);

        assert_eq!(serialized_update, reserialized);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_account_id() {
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
        let result: Result<AccountId, ConversionError> = text.try_into();
        tracing::info!("{:?}", result);
        assert!(matches!(
            result,
            Err(ConversionError::InvalidValue {
                item: "eth account_id",
                expected: "well-formed chain_id & address",
                got: "chain_id/address did not pass validation"
            })
        ));
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_account_id_create() {
        let address = "0xab16a96D359eC26a11e2C2b3d8f8B8942d5Bfcdb".to_string();
        let chain_id = 12;
        let account_id = AccountId::new_evm(chain_id, address.clone());
        assert_eq!(account_id.account_address, address);
        assert_eq!(account_id.chain_id, "eip155:12");
    }
}
