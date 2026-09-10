//! Durable exact outgoing envelopes and fenced publish receipts.

use super::rejection::PreparedRejection;
use super::welcomes::PreparedWelcomes;
use super::*;
use prost::Message;
use serde::{Deserialize, Serialize};
use xmtp_proto::backend_v1::EnvelopeMeta;

#[derive(Debug, thiserror::Error, xmtp_common::ErrorCode)]
#[error_code(internal)]
pub enum OutgoingPreparationError {
    /// Published intent bytes are missing. Recovery must stop.
    #[error("published intent {0} has no prepared attempt")]
    MissingPreparedAttempt(i32),
    /// The stored attempt has invalid bytes or inconsistent identity.
    #[error("invalid prepared outgoing attempt")]
    InvalidPreparedAttempt,
    /// The group contains staged state outside the durable attempt.
    #[error("MLS group has an unexpected pending commit")]
    UnexpectedPendingCommit,
    /// Another writer changed the preparation base. Load it again.
    #[error("outgoing preparation state changed")]
    StateChanged,
}

impl RetryableError for OutgoingPreparationError {
    fn is_retryable(&self) -> bool {
        matches!(self, Self::StateChanged)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// The committed MLS state used to prepare an outgoing attempt.
pub(crate) struct PreparedBase {
    /// Epoch before the prepared commit. Preparation does not merge it locally.
    pub epoch: u64,
    /// Authenticator that distinguishes divergent states at the same epoch.
    pub authenticator: Vec<u8>,
    /// Ordered proposal references used to reject a changed preparation base.
    proposals: Vec<Vec<u8>>,
}

impl PreparedBase {
    pub(crate) fn capture(group: &OpenMlsGroup) -> Result<Self, GroupError> {
        Ok(Self {
            epoch: group.epoch().as_u64(),
            authenticator: group.epoch_authenticator().as_slice().to_vec(),
            proposals: group
                .pending_proposals()
                .map(|proposal| xmtp_db::db_serialize(proposal.proposal_reference_ref()))
                .collect::<Result<_, _>>()?,
        })
    }

    /// Check the epoch identity without rejecting proposals admitted since preparation.
    pub(crate) fn matches_epoch(&self, group: &OpenMlsGroup) -> bool {
        self.epoch == group.epoch().as_u64()
            && self.authenticator == group.epoch_authenticator().as_slice()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// A sender's proposal, kept outside the accepted proposal store until receipt.
pub(crate) struct PreparedProposal {
    /// Hash of the inner MLS bytes, not the backend's authoritative envelope hash.
    pub payload_hash: Vec<u8>,
    /// Serialized proposal needed when the sender receives its own ciphertext.
    pub proposal: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Durable publication state. Immutable attempt fields fence every late reply.
pub(crate) struct PreparedAttempt {
    /// Local storage format version.
    pub version: u8,
    /// Exact committed state against which this attempt was prepared.
    pub base: PreparedBase,
    /// Inner MLS hash of the last payload, used to match the ordered intent result.
    pub payload_hash: Vec<u8>,
    /// Exact encoded ClientEnvelopes. Retries do not regenerate MLS or HMAC bytes.
    pub envelopes: Vec<Vec<u8>>,
    /// Standalone proposals in their original wire order.
    pub proposals: Vec<PreparedProposal>,
    /// Authoritative backend metadata. Absence means acceptance is still ambiguous.
    pub receipts: Option<Vec<Vec<u8>>>,
    /// Required follow-up publication after the commit reaches ordered processing.
    pub welcomes: Option<PreparedWelcomes>,
    /// Terminal processing cause for this exact attempt. Later topic work cannot replace it.
    pub rejection: Option<PreparedRejection>,
}

/// Prior bincode layout. New fields cannot be supplied by serde defaults at EOF.
#[derive(Deserialize)]
struct PreparedAttemptV1 {
    version: u8,
    base: PreparedBase,
    payload_hash: Vec<u8>,
    envelopes: Vec<Vec<u8>>,
    proposals: Vec<PreparedProposal>,
    receipts: Option<Vec<Vec<u8>>>,
    welcomes: Option<PreparedWelcomes>,
}

impl PreparedAttempt {
    /// Decode local state and verify its inner payload identity before reuse.
    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, GroupError> {
        let attempt: Self = match bytes.first() {
            Some(1) => {
                let prior: PreparedAttemptV1 = xmtp_db::db_deserialize(bytes)?;
                if prior.version != 1 {
                    return Err(OutgoingPreparationError::InvalidPreparedAttempt.into());
                }
                Self {
                    version: 2,
                    base: prior.base,
                    payload_hash: prior.payload_hash,
                    envelopes: prior.envelopes,
                    proposals: prior.proposals,
                    receipts: prior.receipts,
                    welcomes: prior.welcomes,
                    rejection: None,
                }
            }
            Some(2) => xmtp_db::db_deserialize(bytes)?,
            _ => return Err(OutgoingPreparationError::InvalidPreparedAttempt.into()),
        };
        if attempt.version != 2 || attempt.payload_hash.len() != 32 || attempt.envelopes.is_empty()
        {
            return Err(OutgoingPreparationError::InvalidPreparedAttempt.into());
        }
        let envelopes = attempt.decode_envelopes()?;
        let Some(Payload::GroupMessage(message)) = envelopes
            .last()
            .and_then(|envelope| envelope.payload.as_ref())
        else {
            return Err(OutgoingPreparationError::InvalidPreparedAttempt.into());
        };
        if sha256(&message.data).as_slice() != attempt.payload_hash {
            return Err(OutgoingPreparationError::InvalidPreparedAttempt.into());
        }
        Ok(attempt)
    }

    /// Reject a saved attempt whose payload or base epoch does not match its intent.
    pub(crate) fn validate_intent(&self, intent: &StoredGroupIntent) -> Result<(), GroupError> {
        if intent.payload_hash.as_deref() != Some(self.payload_hash.as_slice())
            || intent.published_in_epoch != Some(self.base.epoch as i64)
        {
            return Err(OutgoingPreparationError::InvalidPreparedAttempt.into());
        }
        Ok(())
    }

    fn decode_envelopes(&self) -> Result<Vec<ClientEnvelope>, GroupError> {
        self.envelopes
            .iter()
            .map(|bytes| ClientEnvelope::decode(bytes.as_slice()).map_err(Into::into))
            .collect()
    }

    /// Reconstruct a request from saved envelopes without advancing a sender ratchet.
    pub(super) fn publish_unit(&self) -> Result<PublishUnit, GroupError> {
        Ok(PublishUnit::new(self.decode_envelopes()?)?)
    }

    /// Recover a prepared own proposal when its exact payload reaches ordered receipt.
    pub(crate) fn proposal_for_payload(
        &self,
        payload_hash: &[u8],
    ) -> Result<Option<openmls::group::QueuedProposal>, GroupError> {
        self.proposals
            .iter()
            .find(|proposal| proposal.payload_hash == payload_hash)
            .map(|proposal| xmtp_db::db_deserialize(&proposal.proposal).map_err(Into::into))
            .transpose()
    }

    /// Compare immutable identity only. Receipts, Welcome progress, and rejection can change.
    pub(super) fn same_attempt(&self, other: &Self) -> bool {
        self.version == other.version
            && self.base == other.base
            && self.payload_hash == other.payload_hash
            && self.envelopes == other.envelopes
            && self.proposals == other.proposals
    }
}

impl<Context: XmtpSharedContext> MlsGroup<Context> {
    /// Read the accepted publish target. An ambiguous attempt has no target.
    pub(crate) fn published_intent_target(
        &self,
        intent_id: i32,
    ) -> Result<Option<Cursor>, GroupError> {
        crate::state_tx::state_write(self.context.mls_storage(), |tx| {
            let storage = tx.storage();
            let db = storage.db();
            let Some(intent) = Fetch::<StoredGroupIntent>::fetch(&db, &intent_id)? else {
                return Ok(Continue(None));
            };
            if intent.group_id.as_slice() != self.group_id.as_ref() {
                return Err(OutgoingPreparationError::InvalidPreparedAttempt.into());
            }
            if matches!(
                intent.state,
                IntentState::Committed | IntentState::Processed
            ) && let Some(sequence_id) =
                intent.sequence_id.filter(|sequence_id| *sequence_id > 0)
            {
                return Ok(Continue(Some(Cursor(sequence_id as u64))));
            }
            if intent.state != IntentState::Published {
                return Ok(Continue(None));
            }
            let bytes = db
                .prepared_envelopes(intent.id)?
                .ok_or(OutgoingPreparationError::MissingPreparedAttempt(intent.id))?;
            let attempt = PreparedAttempt::decode(&bytes)?;
            attempt.validate_intent(&intent)?;
            let Some(receipts) = &attempt.receipts else {
                return Ok(Continue(None));
            };
            if receipts.len() != attempt.envelopes.len() {
                return Err(OutgoingPreparationError::InvalidPreparedAttempt.into());
            }
            let receipt = receipts
                .last()
                .ok_or(OutgoingPreparationError::InvalidPreparedAttempt)?;
            let meta = EnvelopeMeta::decode(receipt.as_slice())?;
            let (topic, cursor, _) = xmtp_api_backend::envelope::metadata(
                &meta,
                xmtp_proto::types::TopicKind::GroupMessagesV1,
            )?;
            if topic != xmtp_proto::types::Topic::new_group_message(self.group_id) {
                return Err(OutgoingPreparationError::InvalidPreparedAttempt.into());
            }
            Ok::<_, GroupError>(Continue(Some(cursor)))
        })
        .map(TransactionOutcome::into_continued)
    }

    /// Attach authoritative receipts only to the same still-published attempt.
    /// Deleted, resolved, and replaced attempts cannot receive a late reply.
    pub(super) fn record_publish_receipts(
        &self,
        sent_intent: &StoredGroupIntent,
        sent_attempt: &PreparedAttempt,
        receipts: Vec<EnvelopeMeta>,
    ) -> Result<(), GroupError> {
        if receipts.len() != sent_attempt.envelopes.len() {
            return Err(xmtp_api::ApiError::InvalidResponse("publish metadata count").into());
        }
        crate::state_tx::state_write(self.context.mls_storage(), |tx| {
            let storage = tx.storage();
            let db = storage.db();
            let Some(current) = Fetch::<StoredGroupIntent>::fetch(&db, &sent_intent.id)? else {
                return Ok(Continue(()));
            };
            if current.state != IntentState::Published {
                return Ok(Continue(()));
            }
            let Some(encoded) = db.prepared_envelopes(current.id)? else {
                return Ok(Continue(()));
            };
            let mut attempt = PreparedAttempt::decode(&encoded)?;
            if !attempt.same_attempt(sent_attempt) {
                return Ok(Continue(()));
            }
            attempt.validate_intent(&current)?;
            let receipt_bytes = receipts
                .iter()
                .map(Message::encode_to_vec)
                .collect::<Vec<_>>();
            if let Some(existing) = &attempt.receipts {
                if existing != &receipt_bytes {
                    return Err(xmtp_api::ApiError::InvalidResponse(
                        "conflicting publish metadata",
                    )
                    .into());
                }
                return Ok(Continue(()));
            }
            attempt.receipts = Some(receipt_bytes);
            let replacement = xmtp_db::db_serialize(&attempt)?;
            if !db.compare_and_set_prepared_envelopes(
                current.id,
                Some(&encoded),
                Some(&replacement),
            )? {
                return Ok(Continue(()));
            }
            if current.kind == IntentKind::SendMessage {
                let meta = &receipts[0];
                let message_id = calculate_message_id_for_intent(&current)?
                    .ok_or(GroupError::UninitializedResult)?;
                let hash = xmtp_api_backend::envelope::message_hash(meta)?;
                let expiry_ns = i64::try_from(meta.expiry_ns).map_err(|_| {
                    xmtp_proto::ConversionError::Unspecified("expiry_ns exceeds i64")
                })?;
                use xmtp_db::ConnectionExt;
                use xmtp_db::diesel::prelude::*;
                use xmtp_db::schema::group_messages::dsl;
                db.raw_query(|conn| {
                    xmtp_db::diesel::update(dsl::group_messages)
                        .filter(dsl::group_id.eq(current.group_id.as_slice()))
                        .filter(dsl::id.eq(message_id))
                        .set((dsl::envelope_hash.eq(hash), dsl::expiry_ns.eq(expiry_ns)))
                        .execute(conn)
                })?;
            }
            Ok::<_, GroupError>(Continue(()))
        })?;
        Ok(())
    }
}
