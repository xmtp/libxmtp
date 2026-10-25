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
    /// A saved attempt cannot be sent again, because its publish returned
    /// `OUT_OF_RANGE` or it no longer fits in one request, and a recovery read
    /// proved that the backend stored none of it.
    #[error("the backend stored none of a saved attempt that cannot be sent again")]
    Unstored,
    /// The group holds no 32-byte root that derives its id, so it cannot sign
    /// a group message. A Restored placeholder or a legacy group. Not retryable.
    #[error("the group cannot sign group messages: {0}")]
    GroupKeys(xmtp_mls_common::app_data::group_root::GroupRootError),
    /// Signing a group message failed its self-check, and nothing was saved.
    /// Not retryable.
    #[error("signing a group message failed: {0}")]
    SignGroupMessage(xmtp_mls_common::group_keys::SealError),
}

impl RetryableError for OutgoingPreparationError {
    fn is_retryable(&self) -> bool {
        matches!(self, Self::StateChanged)
    }
}

// Every byte field of the stored attempt types is a CBOR byte string
// (`xmtp_db::byte_string` and its list forms). rc4 is the first release that
// stores attempts as CBOR, so no reader of the integer-array form exists.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// The committed MLS state used to prepare an outgoing attempt.
pub(crate) struct PreparedBase {
    /// Epoch before the prepared commit. Preparation does not merge it locally.
    pub epoch: u64,
    /// Authenticator that distinguishes divergent states at the same epoch.
    #[serde(with = "xmtp_db::byte_string")]
    pub authenticator: Vec<u8>,
    /// Ordered proposal references used to reject a changed preparation base.
    #[serde(with = "xmtp_db::byte_strings")]
    pub(super) proposals: Vec<Vec<u8>>,
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
    #[serde(with = "xmtp_db::byte_string")]
    pub payload_hash: Vec<u8>,
    /// Serialized proposal needed when the sender receives its own ciphertext.
    #[serde(with = "xmtp_db::byte_string")]
    pub proposal: Vec<u8>,
}

/// Current local storage format of [`PreparedAttempt`].
pub(crate) const PREPARED_ATTEMPT_VERSION: u8 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Saved bytes whose publish returned `OUT_OF_RANGE`. The backend may have
/// stored them, and the same request cannot be sent again, so only a `Query`
/// of their topics can settle them.
pub(crate) enum Unsettled {
    /// The group attempt's envelopes.
    Attempt,
    /// The required follow-up Welcome batch.
    Welcomes,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Durable publication state. Immutable attempt fields fence every late reply.
pub(crate) struct PreparedAttempt {
    /// Local storage format version.
    pub version: u8,
    /// Exact committed state against which this attempt was prepared.
    pub base: PreparedBase,
    /// Original desired membership target, when this attempt is one split part.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_intent: Option<ID>,
    /// Inner MLS hash of the last payload, used to match the ordered intent result.
    #[serde(with = "xmtp_db::byte_string")]
    pub payload_hash: Vec<u8>,
    /// Exact encoded ClientEnvelopes. Retries do not regenerate MLS or HMAC bytes.
    #[serde(with = "xmtp_db::byte_strings")]
    pub envelopes: Vec<Vec<u8>>,
    /// Standalone proposals in their original wire order.
    pub proposals: Vec<PreparedProposal>,
    /// Authoritative backend metadata. Absence means acceptance is still ambiguous.
    #[serde(default, with = "xmtp_db::optional_byte_strings")]
    pub receipts: Option<Vec<Vec<u8>>>,
    /// Required follow-up publication after the commit reaches ordered processing.
    pub welcomes: Option<PreparedWelcomes>,
    /// Terminal processing cause for this exact attempt. Later topic work cannot replace it.
    pub rejection: Option<PreparedRejection>,
    /// Bytes that must be settled by a recovery read before anything else is sent.
    pub unsettled: Option<Unsettled>,
}

impl PreparedAttempt {
    /// Largest encoded request envelope, including required Welcome delivery.
    #[cfg(test)]
    pub(in crate::groups) fn max_envelope_size(&self) -> usize {
        self.envelopes
            .iter()
            .chain(
                self.welcomes
                    .iter()
                    .flat_map(|welcomes| &welcomes.envelopes),
            )
            .map(Vec::len)
            .max()
            .unwrap_or_default()
    }

    /// Decode local state and verify its inner payload identity before reuse.
    ///
    /// The column is CBOR, which stores field names. A field added later as
    /// an `Option` reads older rows as `None`, so it needs no new version.
    /// Bump [`PREPARED_ATTEMPT_VERSION`] only for a change older code cannot
    /// read; a row with another version is invalid.
    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, GroupError> {
        let attempt: Self = xmtp_db::db_deserialize(bytes)
            .map_err(|_| OutgoingPreparationError::InvalidPreparedAttempt)?;
        if attempt.version != PREPARED_ATTEMPT_VERSION
            || attempt.payload_hash.len() != 32
            || attempt.envelopes.is_empty()
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
    ///
    /// Measured against the deployment's shapes, like every other
    /// publish, so a lowered envelope limit is caught before the network call.
    pub(super) fn publish_unit(
        &self,
        limits: &xmtp_configuration::LimitsConfiguration,
    ) -> Result<PublishUnit, GroupError> {
        Ok(PublishUnit::new_within(self.decode_envelopes()?, limits)?)
    }

    /// The saved envelopes as a unit for a recovery read only. Unlike
    /// [`Self::publish_unit`], it is not measured, so an attempt that no
    /// longer fits in one request can still be settled.
    pub(super) fn settlement_unit(&self) -> Result<PublishUnit, GroupError> {
        Ok(PublishUnit::for_settlement(self.decode_envelopes()?)?)
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

    /// Compare immutable identity only. Receipts, Welcome progress, rejection,
    /// and settlement can change.
    pub(super) fn same_attempt(&self, other: &Self) -> bool {
        self.version == other.version
            && self.base == other.base
            && self.parent_intent == other.parent_intent
            && self.payload_hash == other.payload_hash
            && self.envelopes == other.envelopes
            && self.proposals == other.proposals
    }
}

impl<Context: XmtpSharedContext> MlsGroup<Context> {
    /// Read only attempts whose durable marker names this membership target.
    /// Callers already holding a writer pass its connection so rejection
    /// lookup and parent state observe the same committed child outcome.
    pub(super) fn membership_child_attempts(
        db: &impl DbQuery,
        group_id: GroupId,
        parent_id: ID,
        states: &[IntentState],
    ) -> Result<Vec<(StoredGroupIntent, PreparedAttempt)>, GroupError> {
        if !Fetch::<StoredGroupIntent>::fetch(db, &parent_id)?.is_some_and(|intent| {
            intent.group_id == group_id && intent.kind == IntentKind::UpdateGroupMembership
        }) {
            return Ok(Vec::new());
        }
        let mut children = Vec::new();
        for intent in db.find_group_intents(
            group_id,
            Some(states.to_vec()),
            Some(vec![IntentKind::UpdateGroupMembership]),
        )? {
            if intent.id <= parent_id {
                continue;
            }
            let Some(bytes) = db.prepared_envelopes(intent.id)? else {
                continue;
            };
            let attempt = PreparedAttempt::decode(&bytes)?;
            if attempt.parent_intent == Some(parent_id) {
                attempt.validate_intent(&intent)?;
                children.push((intent, attempt));
            }
        }
        Ok(children)
    }

    /// A split target is not complete until every child finishes its Welcomes.
    /// Once the original target is processed, it cannot create more children.
    pub(in crate::groups::mls_sync) fn pending_membership_children(
        &self,
        parent_id: ID,
    ) -> Result<Vec<ID>, GroupError> {
        Ok(Self::membership_child_attempts(
            &self.context.db(),
            self.group_id,
            parent_id,
            &[IntentState::Published, IntentState::Committed],
        )?
        .into_iter()
        .map(|(intent, _)| intent.id)
        .collect())
    }

    /// A saved child receipt is publication of its original membership request,
    /// including a committed part that still owes Welcomes.
    // implements: SEND-015
    pub(in crate::groups::mls_sync) fn published_request_target(
        &self,
        intent_id: ID,
    ) -> Result<Option<Cursor>, GroupError> {
        self.request_target(intent_id, &[IntentState::Published, IntentState::Committed])
    }

    /// Only an unapplied child receipt can drive the request's next read.
    /// An older applied part must not hide a newer ambiguous publication.
    pub(in crate::groups::mls_sync) fn request_read_target(
        &self,
        intent_id: ID,
    ) -> Result<Option<Cursor>, GroupError> {
        self.request_target(intent_id, &[IntentState::Published])
    }

    fn request_target(
        &self,
        intent_id: ID,
        child_states: &[IntentState],
    ) -> Result<Option<Cursor>, GroupError> {
        if let Some(target) = self.published_intent_target(intent_id)? {
            return Ok(Some(target));
        }
        let mut target = None;
        for (child, _) in Self::membership_child_attempts(
            &self.context.db(),
            self.group_id,
            intent_id,
            child_states,
        )? {
            target = target.max(self.published_intent_target(child.id)?);
        }
        Ok(target)
    }

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

    /// Record that `batch` of `sent` returned `OUT_OF_RANGE`, so that no later
    /// round, including one after a restart, sends it again before a recovery
    /// read settles it. Changes nothing once that batch has receipts or the
    /// attempt was replaced.
    pub(super) fn mark_unsettled(
        &self,
        id: i32,
        sent: &PreparedAttempt,
        batch: Unsettled,
    ) -> Result<(), GroupError> {
        crate::state_tx::state_write(self.context.mls_storage(), |tx| {
            let storage = tx.storage();
            let db = storage.db();
            let Some(intent) = Fetch::<StoredGroupIntent>::fetch(&db, &id)? else {
                return Ok(Continue(()));
            };
            let Some(encoded) = db.prepared_envelopes(id)? else {
                return Ok(Continue(()));
            };
            let mut current = PreparedAttempt::decode(&encoded)?;
            let open = match (batch, &current.welcomes, &sent.welcomes) {
                (Unsettled::Attempt, _, _) => {
                    intent.state == IntentState::Published && current.receipts.is_none()
                }
                (Unsettled::Welcomes, Some(saved), Some(sent)) => {
                    intent.state == IntentState::Committed
                        && saved.same_batch(sent)
                        && saved.receipts.is_none()
                }
                (Unsettled::Welcomes, _, _) => false,
            };
            if !open || !current.same_attempt(sent) {
                return Ok(Continue(()));
            }
            current.unsettled = Some(batch);
            let replacement = xmtp_db::db_serialize(&current)?;
            db.compare_and_set_prepared_envelopes(id, Some(&encoded), Some(&replacement))?;
            Ok::<_, GroupError>(Continue(()))
        })?;
        Ok(())
    }

    /// Attach authoritative receipts only to the same still-published attempt.
    /// Deleted, resolved, and replaced attempts cannot receive a late reply.
    // implements: SEND-008
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
            attempt.unsettled = None;
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
