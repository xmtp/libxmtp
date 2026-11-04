use xmtp_db::group_intent::IntentKind;

use crate::groups::intents::{IntentError, SendMessageIntentData};
use prost::Message;
use xmtp_proto::xmtp::mls::message_contents::PlaintextEnvelope;
use xmtp_proto::xmtp::mls::message_contents::plaintext_envelope::Content;

/// A message id is a SHA-256 digest: 32 bytes, 64 hex characters.
const MESSAGE_ID_HEX_LEN: usize = 64;

/// Decodes a message id that a reply, reaction, or deletion references.
/// Anything other than 64 hex characters, in either case, is not an id.
/// `hex::decode` strips a `0x` prefix, so the decoded length is checked too.
pub fn decode_message_reference(reference: &str) -> Option<Vec<u8>> {
    if reference.len() != MESSAGE_ID_HEX_LEN {
        return None;
    }
    hex::decode(reference)
        .ok()
        .filter(|id| id.len() == MESSAGE_ID_HEX_LEN / 2)
}

/// Relies on a client-created idempotency_key (which could be a timestamp)
// implements: SEND-002
pub fn calculate_message_id(
    group_id: impl AsRef<[u8]>,
    decrypted_message_bytes: &[u8],
    idempotency_key: &str,
) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    // Hash the parts in order instead of joining them, so the content,
    // which can hold a device-sync HMAC root key, is never copied.
    let separator = b"\t";
    Sha256::new()
        .chain_update(group_id.as_ref())
        .chain_update(separator)
        .chain_update(idempotency_key.as_bytes())
        .chain_update(separator)
        .chain_update(decrypted_message_bytes)
        .finalize()
        .to_vec()
}

/// Calculate the message id for this intent.
///
/// # Note
/// This functions deserializes and decodes a [`PlaintextEnvelope`] from encoded bytes.
/// It would be costly to call this method while pulling extra data from a
/// [`PlaintextEnvelope`] elsewhere. The caller should consider combining implementations.
///
/// # Returns
/// Returns [`Option::None`] if `StoredGroupIntent` is not [`IntentKind::SendMessage`] or if
/// an error occurs during decoding of intent data for [`IntentKind::SendMessage`].
pub fn calculate_message_id_for_intent(
    intent: &xmtp_db::group_intent::StoredGroupIntent,
) -> Result<Option<Vec<u8>>, IntentError> {
    if intent.kind != IntentKind::SendMessage {
        return Ok(None);
    }

    let data = SendMessageIntentData::from_bytes(intent.data.as_slice())?;
    let envelope: PlaintextEnvelope = PlaintextEnvelope::decode(data.message.as_slice())?;

    // optimistic message should always have a plaintext envelope. Its
    // content can hold a device-sync HMAC root key; the envelope wipes it
    // on drop.
    let Some(Content::V1(v1)) = &envelope.content else {
        return Ok(None);
    };
    Ok(Some(calculate_message_id(
        intent.group_id,
        &v1.content,
        &v1.idempotency_key,
    )))
}
