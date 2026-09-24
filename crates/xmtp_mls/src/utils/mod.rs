use std::sync::Arc;

use crate::groups::validated_commit::LibXMTPVersion;

#[cfg(feature = "bench")]
pub mod bench;
pub mod cleanup_duplicate_updates;
#[cfg(any(test, feature = "test-utils"))]
pub mod test;

#[cfg(any(test, feature = "test-utils"))]
pub use self::test::*;

pub mod hash {
    pub use xmtp_cryptography::hash::sha256_bytes as sha256;
}

pub mod time {
    const SECS_IN_30_DAYS: i64 = 60 * 60 * 24 * 30;

    /// Current hmac epoch. HMAC keys change every 30 days
    pub fn hmac_epoch() -> i64 {
        xmtp_common::time::now_secs() / SECS_IN_30_DAYS
    }
}

pub mod id {
    use xmtp_db::group_intent::IntentKind;

    use crate::groups::intents::{IntentError, SendMessageIntentData};
    use prost::Message;
    use xmtp_proto::xmtp::mls::message_contents::plaintext_envelope::Content;
    use xmtp_proto::xmtp::mls::message_contents::{PlaintextEnvelope, plaintext_envelope::V1};

    /// Relies on a client-created idempotency_key (which could be a timestamp)
    pub fn calculate_message_id(
        group_id: impl AsRef<[u8]>,
        decrypted_message_bytes: &[u8],
        idempotency_key: &str,
    ) -> Vec<u8> {
        let separator = b"\t";
        let mut id_vec = Vec::new();
        id_vec.extend_from_slice(group_id.as_ref());
        id_vec.extend_from_slice(separator);
        id_vec.extend_from_slice(idempotency_key.as_bytes());
        id_vec.extend_from_slice(separator);
        id_vec.extend_from_slice(decrypted_message_bytes);
        super::hash::sha256(&id_vec)
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

        let data = SendMessageIntentData::from_bytes(&intent.data)?;
        let envelope: PlaintextEnvelope = PlaintextEnvelope::decode(data.message.as_slice())?;

        // optimistic message should always have a plaintext envelope
        let PlaintextEnvelope {
            content:
                Some(Content::V1(V1 {
                    content: message,
                    idempotency_key: key,
                })),
        } = envelope
        else {
            return Ok(None);
        };

        Ok(Some(calculate_message_id(intent.group_id, &message, &key)))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct VersionInfo {
    pkg_version: Arc<str>,
    /// `pkg_version` parsed once at construction. This is the client's own
    /// build identity, not remote data, so it is always valid semver
    /// (see [`VersionInfo::new`]) — receive-path guards can compare against
    /// it without re-parsing the string on every message.
    pkg_semver: LibXMTPVersion,
}

impl Default for VersionInfo {
    fn default() -> Self {
        Self::new(env!("CARGO_PKG_VERSION"))
    }
}

impl VersionInfo {
    /// Build a `VersionInfo`, parsing and caching the semver form.
    ///
    /// `version` is the client's own package version — in production always
    /// the compile-time `CARGO_PKG_VERSION`. A non-semver value is a build
    /// or configuration bug, not runtime data, so it panics here rather than
    /// silently disabling the below-floor pause on every group later.
    fn new(version: &str) -> Self {
        let pkg_semver = LibXMTPVersion::parse(version)
            .unwrap_or_else(|_| panic!("client pkg_version {version:?} is not valid semver"));
        Self {
            pkg_version: version.into(),
            pkg_semver,
        }
    }

    pub fn pkg_version(&self) -> &str {
        &self.pkg_version
    }

    /// The client's own version, parsed once at construction. Prefer this
    /// over re-parsing [`pkg_version`](Self::pkg_version) on hot paths.
    pub fn pkg_semver(&self) -> &LibXMTPVersion {
        &self.pkg_semver
    }

    // Test only function to update the version of the client
    #[cfg(test)]
    pub fn test_update_version(&mut self, version: &str) {
        *self = Self::new(version);
    }
}
