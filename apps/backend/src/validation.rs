use std::collections::HashSet;
use tonic::Status;
use xmtp_id::associations::{Identifier, MemberIdentifier};
use xmtp_mls_validation::AssociationValidation;
use xmtp_proto::xmtp::identity::associations::{
    IdentifierKind, IdentityUpdate, identity_action, signature,
};

/// Normalize only lookup keys. Signed fields must never pass through this function.
pub(crate) fn lookup_key(value: &str, kind: i32) -> Result<(String, i16), Status> {
    let kind = IdentifierKind::try_from(kind)
        .map_err(|_| Status::invalid_argument("unknown identifier kind"))?;
    let identifier = Identifier::from_proto(value, kind, None)
        .map_err(|_| Status::invalid_argument("malformed identifier"))?;
    Ok(identifier_key(&identifier))
}

fn identifier_key(identifier: &Identifier) -> (String, i16) {
    match identifier {
        Identifier::Ethereum(address) => (
            address.0.to_ascii_lowercase(),
            IdentifierKind::Ethereum as i16,
        ),
        Identifier::Passkey(passkey) => (hex::encode(&passkey.key), IdentifierKind::Passkey as i16),
    }
}

fn member_key(member: &MemberIdentifier) -> Option<(String, i16)> {
    let identifier: Option<Identifier> = member.clone().into();
    identifier.as_ref().map(identifier_key)
}

pub(crate) struct Projection {
    pub added: HashSet<(String, i16)>,
    pub removed: HashSet<(String, i16)>,
}

pub(crate) fn projection(validation: &AssociationValidation) -> Projection {
    let active: HashSet<_> = validation
        .state
        .identifiers()
        .iter()
        .map(identifier_key)
        .collect();
    let changed: HashSet<_> = validation
        .diff
        .new_members
        .iter()
        .chain(&validation.diff.removed_members)
        .filter_map(member_key)
        .collect();
    // Recover prior normalized membership to preserve unchanged aliases.
    let new_raw: HashSet<_> = validation.diff.new_members.iter().cloned().collect();
    let prior: HashSet<_> = validation
        .state
        .members()
        .iter()
        .filter(|member| !new_raw.contains(&member.identifier))
        .filter_map(|member| member_key(&member.identifier))
        .chain(
            validation
                .diff
                .removed_members
                .iter()
                .filter_map(member_key),
        )
        .collect();
    Projection {
        added: changed
            .iter()
            .filter(|key| active.contains(*key) && !prior.contains(*key))
            .cloned()
            .collect(),
        removed: changed
            .into_iter()
            .filter(|key| !active.contains(key) && prior.contains(key))
            .collect(),
    }
}

pub(crate) fn scw_count(update: &IdentityUpdate) -> usize {
    update
        .actions
        .iter()
        .flat_map(|action| match &action.kind {
            Some(identity_action::Kind::CreateInbox(value)) => {
                [value.initial_identifier_signature.as_ref(), None]
            }
            Some(identity_action::Kind::Add(value)) => [
                value.existing_member_signature.as_ref(),
                value.new_member_signature.as_ref(),
            ],
            Some(identity_action::Kind::Revoke(value)) => {
                [value.recovery_identifier_signature.as_ref(), None]
            }
            Some(identity_action::Kind::ChangeRecoveryAddress(value)) => {
                [value.existing_recovery_identifier_signature.as_ref(), None]
            }
            None => [None, None],
        })
        .flatten()
        .filter(|value| matches!(value.signature, Some(signature::Signature::Erc6492(_))))
        .count()
}
