//! Decode backend envelopes once for queries and streams.
use prost::Message;
use std::collections::HashMap;
use xmtp_proto::{
    ConversionError, backend_v1 as wire,
    types::{self, IncomingBatchLimits, OrderedEnvelopeBatch, Topic, TopicCursor, TopicKind},
};

/// A backend envelope cannot be decoded. These errors are not retryable.
#[derive(Debug, thiserror::Error)]
pub enum EnvelopeError {
    /// A required field is absent or invalid. Not retryable.
    #[error(transparent)]
    Conversion(#[from] ConversionError),
    /// A payload cannot be parsed. Not retryable.
    #[error(transparent)]
    Validation(#[from] xmtp_mls_validation::ValidationError),
    /// The complete delivery cannot fit the receive buffer. No cursor advances.
    #[error("incoming delivery exceeds its row or byte limit")]
    Capacity,
}
impl xmtp_common::RetryableError for EnvelopeError {
    fn is_retryable(&self) -> bool {
        false
    }
}
impl From<EnvelopeError> for xmtp_proto::api::ApiClientError {
    fn from(error: EnvelopeError) -> Self {
        Self::other(error)
    }
}

fn invalid(item: &'static str) -> ConversionError {
    ConversionError::InvalidValue {
        item,
        expected: "valid backend envelope",
        got: "missing or invalid field".into(),
    }
}

/// Validate a complete ordered read without parsing or changing MLS bytes.
/// Preserve each topic's input cursor as `after`; sequence gaps are valid.
/// Advance only these in-memory cursors on success, never durable receipt `F`.
pub fn ordered_batches(
    cursors: &mut TopicCursor,
    envelopes: Vec<wire::ServerEnvelope>,
    limits: IncomingBatchLimits,
) -> Result<Vec<OrderedEnvelopeBatch>, EnvelopeError> {
    let bytes = envelopes.iter().try_fold(0usize, |bytes, envelope| {
        bytes.checked_add(envelope.encoded_len())
    });
    if envelopes.len() > limits.max_rows || bytes.is_none_or(|bytes| bytes > limits.max_bytes) {
        return Err(EnvelopeError::Capacity);
    }
    let mut next = cursors.clone();
    let mut batches: Vec<OrderedEnvelopeBatch> = Vec::new();
    let mut indices = HashMap::new();
    for envelope in envelopes {
        let meta = envelope.meta.as_ref().ok_or_else(|| invalid("metadata"))?;
        let topic = Topic::parse(&meta.topic.as_ref().ok_or_else(|| invalid("topic"))?.topic)?;
        let (_, sequence, _) = metadata(meta, topic.kind())?;
        let cursor = next
            .get_mut(&topic)
            .ok_or_else(|| invalid("unrequested topic"))?;
        if sequence <= *cursor {
            return Err(invalid("ordered cursor").into());
        }
        let index = *indices.entry(topic.clone()).or_insert_with(|| {
            let index = batches.len();
            batches.push(OrderedEnvelopeBatch {
                topic,
                after: *cursor,
                envelopes: Vec::new(),
            });
            index
        });
        batches[index].envelopes.push(envelope);
        *cursor = sequence;
    }
    *cursors = next;
    Ok(batches)
}

/// Check every fixed target against the exact registered topic set.
/// Require one target per topic. A target may be below the requested start.
pub fn registration_targets(
    starts: &TopicCursor,
    targets: Vec<wire::CatchupTarget>,
) -> Result<TopicCursor, EnvelopeError> {
    if targets.len() != starts.len() {
        return Err(invalid("registration target count").into());
    }
    let mut output = TopicCursor::new();
    for target in targets {
        let topic = Topic::parse(&target.topic.ok_or_else(|| invalid("target topic"))?.topic)?;
        if !starts.contains_key(&topic)
            || target.through_sequence_id > i64::MAX as u64
            || output
                .insert(topic, types::Cursor(target.through_sequence_id))
                .is_some()
        {
            return Err(invalid("registration target").into());
        }
    }
    Ok(output)
}

/// Check the topic, cursor, hash, and server timestamp.
pub fn metadata(
    meta: &wire::EnvelopeMeta,
    kind: TopicKind,
) -> Result<(Topic, types::Cursor, chrono::DateTime<chrono::Utc>), ConversionError> {
    let topic = Topic::parse(&meta.topic.as_ref().ok_or_else(|| invalid("topic"))?.topic)?;
    if topic.kind() != kind {
        return Err(invalid("topic kind"));
    }
    let seq = meta
        .cursor
        .as_ref()
        .ok_or_else(|| invalid("cursor"))?
        .sequence_id;
    if seq == 0 || seq > i64::MAX as u64 {
        return Err(invalid("sequence id"));
    }
    let timestamp = i64::try_from(meta.server_ns).map_err(|_| invalid("server timestamp"))?;
    message_hash(meta)?;
    Ok((
        topic,
        types::Cursor(seq),
        chrono::DateTime::from_timestamp_nanos(timestamp),
    ))
}

/// Read the SHA-256 envelope hash from server metadata.
pub fn message_hash(meta: &wire::EnvelopeMeta) -> Result<Vec<u8>, ConversionError> {
    match meta.message_hash.as_ref().and_then(|h| h.hash.as_ref()) {
        Some(wire::message_hash::Hash::Sha256(bytes)) if bytes.len() == 32 => Ok(bytes.clone()),
        _ => Err(invalid("message hash")),
    }
}

fn parts(
    envelope: wire::ServerEnvelope,
    kind: TopicKind,
) -> Result<(wire::EnvelopeMeta, wire::client_envelope::Payload), EnvelopeError> {
    let meta = envelope.meta.ok_or_else(|| invalid("metadata"))?;
    metadata(&meta, kind)?;
    let payload = envelope
        .envelope
        .and_then(|e| e.payload)
        .ok_or_else(|| invalid("payload"))?;
    Ok((meta, payload))
}

/// Decode an MLS message and keep its payload hash and server metadata.
pub fn decode_group_message(
    envelope: wire::ServerEnvelope,
) -> Result<types::GroupMessage, EnvelopeError> {
    let (meta, payload) = parts(envelope, TopicKind::GroupMessagesV1)?;
    let (topic, cursor, created_ns) = metadata(&meta, TopicKind::GroupMessagesV1)?;
    let wire::client_envelope::Payload::GroupMessage(group) = payload else {
        return Err(invalid("group payload").into());
    };
    let message = xmtp_mls_validation::parse_group_message(&group.data)?;
    if message.group_id().as_slice() != topic.identifier() {
        return Err(invalid("group id").into());
    }
    Ok(types::GroupMessage {
        cursor,
        created_ns,
        group_id: topic.identifier().try_into()?,
        message,
        payload_hash: xmtp_common::sha256_array(&group.data).to_vec(),
        sender_hmac: group.sender_hmac,
        should_push: group.should_push,
        envelope_hash: Some(message_hash(&meta)?),
        expiry_ns: Some(meta.expiry_ns),
    })
}

/// Decode the newest group metadata without loading its payload.
pub fn decode_group_message_metadata(
    meta: wire::EnvelopeMeta,
) -> Result<types::GroupMessageMetadata, EnvelopeError> {
    let (topic, cursor, created_ns) = metadata(&meta, TopicKind::GroupMessagesV1)?;
    Ok(types::GroupMessageMetadata {
        cursor,
        created_ns,
        group_id: topic.identifier().try_into()?,
        envelope_hash: Some(message_hash(&meta)?),
        expiry_ns: Some(meta.expiry_ns),
    })
}

/// Decode an inline welcome or encrypted welcome pointer.
pub fn decode_welcome_message(
    envelope: wire::ServerEnvelope,
) -> Result<types::WelcomeMessage, EnvelopeError> {
    let (meta, payload) = parts(envelope, TopicKind::WelcomeMessagesV1)?;
    let (topic, cursor, created_ns) = metadata(&meta, TopicKind::WelcomeMessagesV1)?;
    let wire::client_envelope::Payload::WelcomeMessage(welcome) = payload else {
        return Err(invalid("welcome payload").into());
    };
    let variant = match welcome.version.ok_or_else(|| invalid("welcome version"))? {
        wire::welcome_message::Version::V1(v) => {
            if v.installation_key != topic.identifier() {
                return Err(invalid("installation id").into());
            }
            types::WelcomeMessageV1 {
                installation_key: v.installation_key.try_into()?,
                hpke_public_key: v.hpke_public_key,
                wrapper_algorithm: v
                    .wrapper_algorithm
                    .try_into()
                    .map_err(ConversionError::from)?,
                data: v.data,
                welcome_metadata: v.welcome_metadata,
            }
            .into()
        }
        wire::welcome_message::Version::WelcomePointer(v) => {
            if v.installation_key != topic.identifier() {
                return Err(invalid("installation id").into());
            }
            types::WelcomePointer {
                installation_key: v.installation_key.try_into()?,
                hpke_public_key: v.hpke_public_key,
                wrapper_algorithm: v
                    .wrapper_algorithm
                    .try_into()
                    .map_err(ConversionError::from)?,
                welcome_pointer: v.welcome_pointer,
            }
            .into()
        }
    };
    Ok(types::WelcomeMessage {
        cursor,
        created_ns,
        variant,
    })
}

/// Decode a key package returned by a full newest query.
pub fn decode_key_package(
    envelope: wire::ServerEnvelope,
) -> Result<wire::KeyPackage, EnvelopeError> {
    let (_, payload) = parts(envelope, TopicKind::KeyPackagesV1)?;
    match payload {
        wire::client_envelope::Payload::KeyPackage(key) => Ok(key),
        _ => Err(invalid("key package payload").into()),
    }
}

/// Decode a signed commit-log record without verifying its signature.
pub fn decode_commit_log_entry(
    envelope: wire::ServerEnvelope,
) -> Result<types::CommitLogEntry, EnvelopeError> {
    let (meta, payload) = parts(envelope, TopicKind::CommitLogEntriesV1)?;
    let wire::client_envelope::Payload::CommitLogEntry(payload) = payload else {
        return Err(invalid("commit log payload").into());
    };
    let entry =
        xmtp_mls_common::commit_log::decode_commit_log(&payload.serialized_commit_log_entry)
            .map_err(ConversionError::from)?;
    let (topic, _, _) = metadata(&meta, TopicKind::CommitLogEntriesV1)?;
    if entry.group_id != topic.identifier() {
        return Err(invalid("commit log group id").into());
    }
    Ok(types::CommitLogEntry {
        meta,
        entry,
        payload,
    })
}

/// Decode an identity update and check its inbox topic.
pub fn decode_identity_update(
    envelope: wire::ServerEnvelope,
) -> Result<types::IdentityUpdateLog, EnvelopeError> {
    let (meta, payload) = parts(envelope, TopicKind::IdentityUpdatesV1)?;
    let wire::client_envelope::Payload::IdentityUpdate(update) = payload else {
        return Err(invalid("identity payload").into());
    };
    let (topic, _, _) = metadata(&meta, TopicKind::IdentityUpdatesV1)?;
    if hex::decode(&update.inbox_id).map_err(|_| invalid("inbox id"))? != topic.identifier() {
        return Err(invalid("inbox id").into());
    }
    Ok(types::IdentityUpdateLog { meta, update })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope(topic: &Topic, sequence: u64) -> wire::ServerEnvelope {
        wire::ServerEnvelope {
            meta: Some(wire::EnvelopeMeta {
                topic: Some(wire::Topic {
                    topic: topic.cloned_vec(),
                }),
                cursor: Some(wire::Cursor {
                    sequence_id: sequence,
                }),
                message_hash: Some(wire::MessageHash {
                    hash: Some(wire::message_hash::Hash::Sha256(vec![7; 32])),
                }),
                ..Default::default()
            }),
            // Receipt deliberately does not parse a malformed MLS payload.
            envelope: Some(wire::ClientEnvelope {
                payload: Some(wire::client_envelope::Payload::GroupMessage(
                    wire::GroupMessage {
                        data: vec![255],
                        ..Default::default()
                    },
                )),
            }),
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn ordered_batches_keep_sparse_positions_and_authoritative_bytes() {
        let a = Topic::new_group_message([1; 16]);
        let b = Topic::new_group_message([2; 16]);
        let mut cursors = [(a.clone(), types::Cursor(2)), (b.clone(), types::Cursor(0))].into();
        let original = envelope(&a, 8);
        let batches = ordered_batches(
            &mut cursors,
            vec![original.clone(), envelope(&b, 11), envelope(&a, 20)],
            IncomingBatchLimits {
                max_rows: 3,
                max_bytes: 4096,
            },
        )?;
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].after, types::Cursor(2));
        assert_eq!(batches[0].envelopes[0], original);
        assert_eq!(batches[1].after, types::Cursor(0));
        assert_eq!(cursors[&a], types::Cursor(20));
        assert_eq!(cursors[&b], types::Cursor(11));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn failed_frame_does_not_advance_any_topic() {
        let a = Topic::new_group_message([1; 16]);
        let b = Topic::new_group_message([2; 16]);
        let starts: TopicCursor =
            [(a.clone(), types::Cursor(2)), (b.clone(), types::Cursor(0))].into();
        for envelopes in [
            vec![envelope(&a, 8), envelope(&b, 0)],
            vec![envelope(&a, 8), envelope(&a, 7)],
        ] {
            let mut cursors = starts.clone();
            assert!(
                ordered_batches(
                    &mut cursors,
                    envelopes,
                    IncomingBatchLimits {
                        max_rows: 2,
                        max_bytes: 4096
                    }
                )
                .is_err()
            );
            assert_eq!(cursors, starts);
        }
        let mut cursors = starts.clone();
        assert!(matches!(
            ordered_batches(
                &mut cursors,
                vec![envelope(&a, 8)],
                IncomingBatchLimits {
                    max_rows: 1,
                    max_bytes: 1
                }
            ),
            Err(EnvelopeError::Capacity)
        ));
        assert_eq!(cursors, starts);
    }
}
