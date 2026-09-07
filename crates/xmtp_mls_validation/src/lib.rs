//! Shared payload admission. Storage and transport remain with their callers.

use openmls::prelude::{ContentType, KeyPackageIn, MlsMessageIn, ProtocolMessage};
use openmls_rust_crypto::RustCrypto;
use tls_codec::Deserialize;
use xmtp_common::RetryableError;
use xmtp_id::{
    associations::{
        self, AssociationError, AssociationState, AssociationStateDiff, DeserializationError,
        SignatureError, try_map_vec, unverified::UnverifiedIdentityUpdate, verify_updates,
    },
    key_package::{KeyPackageVerificationError, VerifiedKeyPackageV2},
    scw_verifier::SmartContractSignatureVerifier,
};
use xmtp_mls_common::commit_log::decode_commit_log;
use xmtp_proto::{
    ConversionError,
    types::{CanonicalEnvelope, Topic, TopicKind, canonical_envelope},
    xmtp::{
        backend::v1::{
            ClientEnvelope, client_envelope::Payload, publish_error::Reason,
            welcome_message::Version,
        },
        identity::associations::IdentityUpdate,
    },
};

#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils;

#[cfg(test)]
mod tests;

#[derive(Debug, thiserror::Error, xmtp_common::ErrorCode)]
#[error_code(internal)]
pub enum ValidationError {
    /// The payload selection is absent. Not retryable.
    #[error("envelope payload is absent")]
    MissingPayload,
    /// The welcome version is absent. Not retryable.
    #[error("welcome version is absent")]
    MissingWelcomeVersion,
    /// Protobuf framing is malformed. Not retryable.
    #[error(transparent)]
    Protobuf(#[from] prost::DecodeError),
    /// MLS framing is malformed. Not retryable.
    #[error(transparent)]
    Tls(#[from] tls_codec::Error),
    /// The MLS body is not a protocol message. Not retryable.
    #[error(transparent)]
    Protocol(#[from] openmls::framing::errors::ProtocolMessageError),
    /// The identifier or topic has an invalid shape. Not retryable.
    #[error(transparent)]
    #[error_code(inherit)]
    Conversion(#[from] ConversionError),
    /// The inbox identifier is not hexadecimal. Not retryable.
    #[error(transparent)]
    Inbox(#[from] hex::FromHexError),
    /// The key package fails existing validation. Not retryable.
    #[error(transparent)]
    #[error_code(inherit)]
    KeyPackage(#[from] KeyPackageVerificationError),
    /// Identity fields cannot be decoded. Not retryable.
    #[error(transparent)]
    #[error_code(inherit)]
    IdentityEncoding(#[from] DeserializationError),
    /// Identity state transition failed. Nested verifier failures can be retryable.
    #[error(transparent)]
    #[error_code(inherit)]
    Association(#[from] AssociationError),
    /// Signature verification failed. Provider and I/O failures can be retryable.
    #[error(transparent)]
    #[error_code(inherit)]
    Signature(#[from] SignatureError),
}

impl RetryableError for ValidationError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Signature(error) | Self::Association(AssociationError::Signature(error)) => {
                error.is_retryable()
            }
            _ => false,
        }
    }
}

impl ValidationError {
    /// The transport supplies the input index and handles retryable failures.
    pub fn reason(&self) -> Reason {
        match self {
            Self::KeyPackage(_) => Reason::InvalidKeyPackage,
            Self::Signature(_) | Self::Association(AssociationError::Signature(_)) => {
                Reason::InvalidSignature
            }
            Self::IdentityEncoding(_) | Self::Association(_) => Reason::InvalidIdentityUpdate,
            _ => Reason::MalformedPayload,
        }
    }
}

/// Parsed routing metadata. This is not proof of valid signatures or membership.
pub struct ParsedEnvelope {
    pub envelope: ClientEnvelope,
    pub topic: Topic,
    pub is_commit_or_proposal: bool,
    pub canonical: CanonicalEnvelope,
}

/// Preserve the legacy MLS parser's acceptance of trailing bytes.
pub fn parse_group_message(data: &[u8]) -> Result<ProtocolMessage, ValidationError> {
    Ok(MlsMessageIn::tls_deserialize(&mut &data[..])?.try_into_protocol_message()?)
}

/// Retention classification only. This does not authenticate the sender.
pub fn is_commit_or_proposal(message: &ProtocolMessage) -> bool {
    matches!(
        message.content_type(),
        ContentType::Commit | ContentType::Proposal
    )
}

fn checked_topic(kind: TopicKind, identifier: impl AsRef<[u8]>) -> Result<Topic, ValidationError> {
    let topic = kind.create(identifier);
    Ok(Topic::parse(&topic)?)
}

/// Derive routing and retry bytes before cryptographic or identity validation.
pub fn parse_envelope(envelope: ClientEnvelope) -> Result<ParsedEnvelope, ValidationError> {
    let payload = envelope
        .payload
        .as_ref()
        .ok_or(ValidationError::MissingPayload)?;
    let mut is_commit_or_proposal = false;
    let topic = match payload {
        Payload::GroupMessage(group) => {
            let message = parse_group_message(&group.data)?;
            is_commit_or_proposal = self::is_commit_or_proposal(&message);
            checked_topic(TopicKind::GroupMessagesV1, message.group_id().as_slice())?
        }
        Payload::WelcomeMessage(welcome) => {
            let key = match welcome
                .version
                .as_ref()
                .ok_or(ValidationError::MissingWelcomeVersion)?
            {
                Version::V1(welcome) => &welcome.installation_key,
                Version::WelcomePointer(pointer) => &pointer.installation_key,
            };
            checked_topic(TopicKind::WelcomeMessagesV1, key)?
        }
        Payload::KeyPackage(package) => {
            let package = KeyPackageIn::tls_deserialize_exact(&package.key_package_tls_serialized)?;
            checked_topic(
                TopicKind::KeyPackagesV1,
                package.unverified_credential().signature_key.as_slice(),
            )?
        }
        Payload::IdentityUpdate(update) => {
            checked_topic(TopicKind::IdentityUpdatesV1, hex::decode(&update.inbox_id)?)?
        }
        Payload::CommitLogEntry(entry) => {
            let decoded = decode_commit_log(&entry.serialized_commit_log_entry)?;
            checked_topic(TopicKind::CommitLogEntriesV1, &decoded.group_id)?
        }
    };
    let canonical = canonical_envelope(&envelope);
    Ok(ParsedEnvelope {
        envelope,
        topic,
        is_commit_or_proposal,
        canonical,
    })
}

/// Apply only the existing key-package admission checks.
pub fn verify_key_package(
    data: &[u8],
) -> Result<VerifiedKeyPackageV2, KeyPackageVerificationError> {
    VerifiedKeyPackageV2::from_bytes(&RustCrypto::default(), data)
}

pub struct AssociationValidation {
    pub state: AssociationState,
    pub diff: AssociationStateDiff,
}

/// The caller supplies one complete history snapshot. No storage is read here.
pub async fn validate_identity_updates(
    old_updates: Vec<IdentityUpdate>,
    new_updates: Vec<IdentityUpdate>,
    verifier: impl SmartContractSignatureVerifier,
) -> Result<AssociationValidation, ValidationError> {
    let old: Vec<UnverifiedIdentityUpdate> = try_map_vec(old_updates)?;
    let new: Vec<UnverifiedIdentityUpdate> = try_map_vec(new_updates)?;
    let old = verify_updates(old, &verifier).await?;
    let new = verify_updates(new, &verifier).await?;
    if old.is_empty() {
        let state = associations::get_state(new)?;
        let diff = state.as_diff();
        return Ok(AssociationValidation { state, diff });
    }
    let old_state = associations::get_state(old)?;
    let mut state = old_state.clone();
    for update in new {
        state = associations::apply_update(state, update)?;
    }
    let diff = old_state.diff(&state);
    Ok(AssociationValidation { state, diff })
}

/// Validate survivors after duplicate lookup. Other kinds need structural parsing only.
pub async fn validate_envelope(
    parsed: &ParsedEnvelope,
    history: &[IdentityUpdate],
    verifier: impl SmartContractSignatureVerifier,
) -> Result<Option<AssociationValidation>, ValidationError> {
    match parsed
        .envelope
        .payload
        .as_ref()
        .ok_or(ValidationError::MissingPayload)?
    {
        Payload::KeyPackage(package) => {
            verify_key_package(&package.key_package_tls_serialized)?;
            Ok(None)
        }
        Payload::IdentityUpdate(update) => Ok(Some(
            validate_identity_updates(history.to_vec(), vec![update.clone()], verifier).await?,
        )),
        _ => Ok(None),
    }
}
