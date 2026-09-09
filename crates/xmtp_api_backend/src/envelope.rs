//! Decode backend envelopes once for queries and streams.
use xmtp_proto::{
    ConversionError, backend_v1 as wire,
    types::{self, Topic, TopicKind},
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
