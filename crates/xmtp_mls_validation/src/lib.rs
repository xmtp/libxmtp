//! Shared payload admission. Storage and transport remain with their callers.

use openmls::prelude::{ContentType, KeyPackageIn, MlsMessageIn, ProtocolMessage};
use openmls_rust_crypto::RustCrypto;
use tls_codec::Deserialize;
use xmtp_common::RetryableError;
use xmtp_id::{
    associations::{
        self, AccountId, AssociationError, AssociationState, AssociationStateDiff,
        DeserializationError, SignatureError, try_map_vec, unverified::UnverifiedIdentityUpdate,
        verify_updates,
    },
    key_package::{KeyPackageVerificationError, VerifiedKeyPackageV2},
    scw_verifier::{ChainBlocks, SmartContractSignatureVerifier},
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
        identity::associations::{
            IdentityUpdate, SmartContractWalletSignature, identity_action, signature,
        },
    },
};

pub mod commit;
pub mod group_membership;
pub mod group_permissions;
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
    /// An ERC-6492 signature names a block after the chain head or more than
    /// [`MAX_BLOCK_AGE_SECS`] before it. Not retryable.
    #[error("signature block {0} is not fresh")]
    StaleBlock(u64),
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
    /// Classify this error for the backend publish response.
    ///
    /// The transport supplies the input index. Retryability is kept separate so
    /// a provider failure can become `UNAVAILABLE` instead of a bad-payload
    /// reason.
    pub fn reason(&self) -> Reason {
        match self {
            Self::KeyPackage(_) => Reason::InvalidKeyPackage,
            Self::Signature(_)
            | Self::Association(AssociationError::Signature(_))
            | Self::StaleBlock(_) => Reason::InvalidSignature,
            Self::IdentityEncoding(_) | Self::Association(_) => Reason::InvalidIdentityUpdate,
            _ => Reason::MalformedPayload,
        }
    }
}

/// Parsed routing metadata and canonical storage bytes.
///
/// Parsing derives the topic and retention flag, but does not prove signatures,
/// group membership, or key-package validity. The outer bytes and hash are
/// stable for retries; payload byte fields remain unchanged.
pub struct ParsedEnvelope {
    /// The decoded client envelope, including its original payload bytes.
    pub envelope: ClientEnvelope,
    /// The topic derived from the payload, never supplied by the client.
    pub topic: Topic,
    /// Whether an MLS group message carries a commit or proposal content type.
    pub is_commit_or_proposal: bool,
    /// Canonical outer protobuf bytes and their SHA-256 hash.
    pub canonical: CanonicalEnvelope,
}

impl ParsedEnvelope {
    /// Classify a parsed envelope for push storage without changing canonical bytes.
    /// Commits and proposals remain eligible even when the sender disables pushes.
    pub fn push_fields(&self) -> (bool, Option<Vec<u8>>) {
        match self.envelope.payload.as_ref() {
            Some(Payload::GroupMessage(group)) => (
                self.is_commit_or_proposal || group.should_push,
                (group.sender_hmac.len() == 32).then(|| group.sender_hmac.clone()),
            ),
            Some(Payload::WelcomeMessage(_)) => (true, None),
            _ => (false, None),
        }
    }
}

/// Parse an MLS group message and preserve accepted trailing bytes.
///
/// The parser consumes the first TLS-encoded message. It returns framing or
/// protocol errors, but does not authenticate the sender or inspect membership.
pub fn parse_group_message(data: &[u8]) -> Result<ProtocolMessage, ValidationError> {
    Ok(MlsMessageIn::tls_deserialize(&mut &data[..])?.try_into_protocol_message()?)
}

/// Classify an MLS message for retention purposes.
///
/// Commit and proposal messages are retained without an expiry. This predicate
/// does not authenticate the sender or validate group state.
pub fn is_commit_or_proposal(message: &ProtocolMessage) -> bool {
    matches!(
        message.content_type(),
        ContentType::Commit | ContentType::Proposal
    )
}

/// Build and re-parse a topic so malformed identifiers fail before storage.
fn checked_topic(kind: TopicKind, identifier: impl AsRef<[u8]>) -> Result<Topic, ValidationError> {
    let topic = kind.create(identifier);
    Ok(Topic::parse(&topic)?)
}

/// Decode one envelope, derive its topic, and compute canonical retry bytes.
///
/// This is the routing phase. It performs the payload-specific decoding needed
/// to find a topic, but leaves key-package and identity admission to
/// [`validate_envelope`]. A malformed envelope returns before any verifier call.
// implements: TOPIC-001, TOPIC-002, API-230
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

/// Apply the existing key-package admission checks to serialized bytes.
///
/// This function returns the verified package for callers that need it, but the
/// backend uses it only to reject invalid packages. It does not alter the input.
pub fn verify_key_package(
    data: &[u8],
) -> Result<VerifiedKeyPackageV2, KeyPackageVerificationError> {
    VerifiedKeyPackageV2::from_bytes(&RustCrypto::default(), data)
}

pub struct AssociationValidation {
    /// State after applying all supplied updates.
    pub state: AssociationState,
    /// Active-member changes between the old and resulting states.
    pub diff: AssociationStateDiff,
}

/// Validate a new identity suffix against one complete history snapshot.
///
/// `old_updates` must be the ordered state read from storage, and
/// `new_updates` must be the proposed suffix. This function performs signature
/// verification and pure state transitions only; it does not read storage. A
/// verifier error is returned unchanged so callers can preserve retryability.
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

/// The oldest a new update's ERC-6492 block may be, relative to the chain head.
pub const MAX_BLOCK_AGE_SECS: u64 = 1800;

/// Every ERC-6492 signature an identity update carries, in action order.
pub fn erc6492_signatures(
    update: &IdentityUpdate,
) -> impl Iterator<Item = &SmartContractWalletSignature> {
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
        .filter_map(|value| match &value.signature {
            Some(signature::Signature::Erc6492(signature)) => Some(signature),
            _ => None,
        })
}

/// Reject a new update whose ERC-6492 signature names a block after its
/// chain's head, or one more than [`MAX_BLOCK_AGE_SECS`] before it.
///
/// Each signature is judged on its own chain, after the account id's form
/// check so a malformed one is rejected before any chain call. The head is
/// read first, so a block after it is rejected without a second call. Chain
/// failures stay retryable; they are never a verdict on the signature. A
/// block stamped after the head, as reads straddling a reorg can report, is
/// rejected rather than read as fresh.
/// Callers run this before signature verification: a verifier asked about a
/// block the chain has not produced fails retryably instead of rejecting.
// implements: IDENT-062
pub async fn check_freshness(
    update: &IdentityUpdate,
    chain: &dyn ChainBlocks,
) -> Result<(), ValidationError> {
    for signature in erc6492_signatures(update) {
        let account = AccountId::try_from(signature.account_id.as_str())?;
        account.get_chain_id_u64().map_err(SignatureError::from)?;
        let number = signature.block_number;
        let head = chain
            .head(account.get_chain_id())
            .await
            .map_err(SignatureError::from)?;
        if number > head.number {
            return Err(ValidationError::StaleBlock(number));
        }
        let timestamp = chain
            .timestamp(account.get_chain_id(), number)
            .await
            .map_err(SignatureError::from)?;
        if head
            .timestamp
            .checked_sub(timestamp)
            .is_none_or(|age| age > MAX_BLOCK_AGE_SECS)
        {
            return Err(ValidationError::StaleBlock(number));
        }
    }
    Ok(())
}

/// Validate one parsed envelope after duplicate lookup.
///
/// Key packages receive their existing package checks. Identity updates are
/// folded against the caller's snapshot and return a projection diff; only the
/// new update, never the stored history, must first pass [`check_freshness`]. Other
/// kinds need no cryptographic admission beyond the parsing phase.
pub async fn validate_envelope(
    parsed: &ParsedEnvelope,
    history: &[IdentityUpdate],
    verifier: impl SmartContractSignatureVerifier,
    chain: &dyn ChainBlocks,
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
        Payload::IdentityUpdate(update) => {
            check_freshness(update, chain).await?;
            Ok(Some(
                validate_identity_updates(history.to_vec(), vec![update.clone()], verifier).await?,
            ))
        }
        _ => Ok(None),
    }
}
