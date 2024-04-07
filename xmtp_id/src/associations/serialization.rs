use super::{
    unsigned_actions::{
        SignatureTextCreator, UnsignedAction, UnsignedAddAssociation,
        UnsignedChangeRecoveryAddress, UnsignedCreateInbox, UnsignedIdentityUpdate,
        UnsignedRevokeAssociation,
    },
    IdentityUpdate, MemberIdentifier,
};
use thiserror::Error;
use xmtp_proto::xmtp::identity::associations::{
    identity_action::Kind as IdentityActionKind,
    member_identifier::Kind as MemberIdentifierKindProto, IdentityAction as IdentityActionProto,
    IdentityUpdate as IdentityUpdateProto, MemberIdentifier as MemberIdentifierProto,
};

#[derive(Error, Debug)]
pub enum SerializationError {
    #[error("Invalid action")]
    InvalidAction,
    #[error("Missing action")]
    MissingAction,
    #[error("Missing member identifier")]
    MissingMemberIdentifier,
}

pub fn from_identity_update_proto(
    proto: IdentityUpdateProto,
) -> Result<IdentityUpdate, SerializationError> {
    let client_timestamp_ns = proto.client_timestamp_ns;
    let all_actions: Vec<IdentityActionKind> = proto
        .actions
        .into_iter()
        .map(|action| match action.kind {
            Some(action) => Ok(action),
            None => Err(SerializationError::MissingAction),
        })
        .collect()?;
}

fn get_signature_text(
    actions: &Vec<IdentityActionKind>,
    client_timestamp_ns: u64,
) -> Result<String, SerializationError> {
    let unsigned_actions: Vec<UnsignedAction> = actions
        .iter()
        .map(|action| match action {
            IdentityActionKind::Add(add_action) => {
                Ok(UnsignedAction::AddAssociation(UnsignedAddAssociation {
                    inbox_id: add_action.inbox_id,
                    new_member_identifier: from_member_identifier_proto_option(
                        add_action.new_member_identifier,
                    )?,
                }))
            }
            IdentityActionKind::CreateInbox(create_inbox_action) => {
                Ok(UnsignedAction::CreateInbox(UnsignedCreateInbox {
                    nonce: create_inbox_action.nonce as u64,
                    account_address: create_inbox_action.initial_address,
                }))
            }
            IdentityActionKind::ChangeRecoveryAddress(change_recovery_address_action) => Ok(
                UnsignedAction::ChangeRecoveryAddress(UnsignedChangeRecoveryAddress {
                    inbox_id: change_recovery_address_action.inbox_id,
                    new_recovery_address: change_recovery_address_action.new_recovery_address,
                }),
            ),
            IdentityActionKind::Revoke(revoke_action) => Ok(UnsignedAction::RevokeAssociation(
                UnsignedRevokeAssociation {
                    inbox_id: revoke_action.inbox_id,
                    revoked_member: from_member_identifier_proto_option(
                        revoke_action.member_to_revoke,
                    )?,
                },
            )),
        })
        .collect::<Result<Vec<UnsignedAction>, SerializationError>>()?;

    let unsigned_update = UnsignedIdentityUpdate::new(client_timestamp_ns, unsigned_actions);

    Ok(unsigned_update.signature_text())
}

fn from_member_identifier_proto_option(
    proto: Option<MemberIdentifierProto>,
) -> Result<MemberIdentifier, SerializationError> {
    match proto {
        None => return Err(SerializationError::MissingMemberIdentifier),
        Some(identifier_proto) => match identifier_proto.kind {
            Some(identifier) => Ok(from_member_identifier_kind_proto(identifier)),
            None => Err(SerializationError::MissingMemberIdentifier),
        },
    }
}

fn from_member_identifier_kind_proto(proto: MemberIdentifierKindProto) -> MemberIdentifier {
    match proto {
        MemberIdentifierKindProto::Address(address) => address.into(),
        MemberIdentifierKindProto::InstallationPublicKey(public_key) => public_key.into(),
    }
}
