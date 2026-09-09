use crate::db::Projection;
use std::collections::HashSet;
use tonic::Status;
use xmtp_id::associations::{Identifier, MemberIdentifier};
use xmtp_mls_validation::AssociationValidation;
use xmtp_proto::xmtp::identity::associations::{
    IdentifierKind, IdentityUpdate, identity_action, signature,
};

/// Parse and normalize one lookup key.
///
/// Lookup normalization makes equivalent Ethereum and passkey identifiers map
/// to one projection key. Signed identity fields must never pass through this
/// function because changing their bytes would invalidate their signatures.
pub(crate) fn lookup_key(value: &str, kind: i32) -> Result<(String, i16), Status> {
    let kind = IdentifierKind::try_from(kind)
        .map_err(|_| Status::invalid_argument("unknown identifier kind"))?;
    let identifier = Identifier::from_proto(value, kind, None)
        .map_err(|_| Status::invalid_argument("malformed identifier"))?;
    Ok(identifier_key(&identifier))
}

/// Convert an owned identifier to the database projection key.
///
/// Ethereum addresses use lowercase ASCII; passkey keys use lowercase hex.
fn identifier_key(identifier: &Identifier) -> (String, i16) {
    match identifier {
        Identifier::Ethereum(address) => (
            address.0.to_ascii_lowercase(),
            IdentifierKind::Ethereum as i16,
        ),
        Identifier::Passkey(passkey) => (hex::encode(&passkey.key), IdentifierKind::Passkey as i16),
    }
}

/// Convert a member identifier to the normalized projection key when supported.
///
/// Invalid or unsupported member forms are omitted. Signed identity data is
/// never normalized here; this helper is only for the derived lookup table.
fn member_key(member: &MemberIdentifier) -> Option<(String, i16)> {
    let identifier: Option<Identifier> = member.clone().into();
    identifier.as_ref().map(identifier_key)
}

/// Derive projection changes from a validated identity transition.
///
/// The result contains only identifiers that changed active status. It compares
/// normalized keys, while also recovering unchanged aliases from the prior state
/// so a projection update cannot remove an association that remains active.
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

/// Count ERC-6492 signatures in an identity update.
///
/// The count covers every signature-bearing action and is used for the request
/// limit before chain verification begins.
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

/// Add backend telemetry at the verifier boundary, including identity validation.
pub(crate) struct ObservedVerifier<'a>(
    pub &'a xmtp_id::scw_verifier::CachedSmartContractSignatureVerifier,
);

#[xmtp_common::async_trait]
impl xmtp_id::scw_verifier::SmartContractSignatureVerifier for ObservedVerifier<'_> {
    async fn is_valid_signature(
        &self,
        account_id: xmtp_id::associations::AccountId,
        hash: [u8; 32],
        signature: alloy_primitives::Bytes,
        block_number: Option<u64>,
    ) -> Result<xmtp_id::scw_verifier::ValidationResponse, xmtp_id::scw_verifier::VerifierError>
    {
        self.verify(account_id, hash, signature, block_number).await
    }
}
impl ObservedVerifier<'_> {
    /// Preserve the verifier result and its retryability while recording its outcome.
    #[xmtp_common::span(prefix = "scw")]
    async fn verify(
        &self,
        account_id: xmtp_id::associations::AccountId,
        hash: [u8; 32],
        signature: alloy_primitives::Bytes,
        block_number: Option<u64>,
    ) -> Result<xmtp_id::scw_verifier::ValidationResponse, xmtp_id::scw_verifier::VerifierError>
    {
        use crate::telemetry::{self, VerificationResult};
        use xmtp_common::RetryableError;
        use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
        let result = self
            .0
            .is_valid_signature(account_id, hash, signature, block_number)
            .await;
        telemetry::scw_verified(match &result {
            Ok(response) if response.is_valid => VerificationResult::Valid,
            Ok(_) => VerificationResult::Invalid,
            Err(error) if error.is_retryable() => VerificationResult::Error,
            Err(_) => VerificationResult::Invalid,
        });
        result
    }
}
