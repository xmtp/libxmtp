//! Constant-time sender exclusion, using the envelope's stored server time.

use hmac::{Hmac, Mac};
use prost::Message;
use sha2::Sha256;
use subtle::ConstantTimeEq;
use xmtp_cryptography::secret::Zeroizing;

pub(crate) fn key(
    server_ns: i64,
    base: Option<i64>,
    keys: &[Option<Zeroizing<Vec<u8>>>; 3],
) -> Option<&[u8]> {
    let index =
        xmtp_push_types::hmac_epoch(server_ns / xmtp_common::NS_IN_SEC).checked_sub(base?)?;
    keys.get(usize::try_from(index).ok()?)?
        .as_ref()
        .map(|key| key.as_slice())
}

/// HMAC input one envelope may spend on sender checks, across its pages.
pub(crate) const BUDGET_BYTES: usize = 16 * 1024 * 1024;

/// The `GroupMessage.data` sender checks run over. Malformed payloads have
/// none, so they never suppress a delivery.
pub(crate) fn data(payload: &[u8]) -> Option<Vec<u8>> {
    match crate::api::ClientEnvelope::decode(payload).ok()?.payload? {
        crate::api::client_envelope::Payload::GroupMessage(group) => Some(group.data),
        _ => None,
    }
}

pub(crate) fn matches(data: &[u8], key: &[u8], expected: &[u8]) -> bool {
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(key) else {
        return false;
    };
    mac.update(data);
    bool::from(mac.finalize().into_bytes().as_slice().ct_eq(expected))
}
