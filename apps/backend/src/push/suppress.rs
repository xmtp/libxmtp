//! Constant-time sender exclusion, using the envelope's stored server time.

use hmac::{Hmac, Mac};
use prost::Message;
use sha2::Sha256;
use subtle::ConstantTimeEq;

pub(crate) fn key(server_ns: i64, base: Option<i64>, keys: &[Option<Vec<u8>>; 3]) -> Option<&[u8]> {
    let index =
        xmtp_push_types::hmac_epoch(server_ns / xmtp_common::NS_IN_SEC).checked_sub(base?)?;
    keys.get(usize::try_from(index).ok()?)?.as_deref()
}

/// Decode only when a subscription has a key for this envelope's epoch.
/// Malformed or unavailable payloads never suppress a delivery.
pub(crate) fn matches(payload: &[u8], key: &[u8], expected: &[u8]) -> bool {
    let Ok(envelope) = crate::api::ClientEnvelope::decode(payload) else {
        return false;
    };
    let Some(crate::api::client_envelope::Payload::GroupMessage(group)) = envelope.payload else {
        return false;
    };
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(key) else {
        return false;
    };
    mac.update(&group.data);
    bool::from(mac.finalize().into_bytes().as_slice().ct_eq(expected))
}
