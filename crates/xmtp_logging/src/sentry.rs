//! Sentry backend for the telemetry slot: client + SentryLayer construction,
//! FFI-boundary event promotion, and HKDF pseudonymization of identity values.

// The scrub helpers below have no non-test caller until the client wiring lands.
#![allow(dead_code)]

const HKDF_SALT: &[u8] = b"convos-metrics";

/// Normalized identity field keys and the HKDF `info` class each hashes under.
/// Values of these keys are pseudonymized before leaving the device for Sentry;
/// local layers (logcat/oslog/file) keep raw values.
pub(crate) const ID_KEYS: &[(&str, &[u8])] = &[
    ("inbox_id", b"inbox-stable-id"),
    ("sender_inbox_id", b"inbox-stable-id"),
    ("proposer_inbox_id", b"inbox-stable-id"),
    ("removed_inbox_id", b"inbox-stable-id"),
    ("claimed_inbox_id", b"inbox-stable-id"),
    ("group_id", b"group-stable-id"),
    ("dm_id", b"group-stable-id"),
    ("installation_id", b"installation-stable-id"),
    ("sender_installation_id", b"installation-stable-id"),
    ("actor_installation_id", b"installation-stable-id"),
    ("message_id", b"message-stable-id"),
    ("topic", b"topic-stable-id"),
];

pub(crate) fn stable_id(value: &str, info: &[u8]) -> String {
    use hkdf::Hkdf;
    use sha2::Sha256;
    let hk = Hkdf::<Sha256>::new(Some(HKDF_SALT), value.as_bytes());
    let mut okm = [0u8; 32];
    hk.expand(info, &mut okm)
        .expect("32 bytes is a valid HKDF-SHA256 length");
    hex::encode(okm)
}

pub(crate) fn scrub_value(key: &str, value: &str) -> Option<String> {
    ID_KEYS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, info)| stable_id(value, info))
}

/// Sentry backend configuration. `user_stable_id` is the HKDF stable id
/// computed by the host app (matches the Convos iOS PostHog person id).
#[derive(Debug, Clone)]
pub struct SentryConfig {
    pub dsn: String,
    pub environment: Option<String>,
    pub release: Option<String>,
    pub traces_sample_rate: f32,
    pub max_breadcrumbs: usize,
    pub user_stable_id: Option<String>,
    pub tags: Vec<(String, String)>,
}

impl Default for SentryConfig {
    fn default() -> Self {
        Self {
            dsn: String::new(),
            environment: None,
            release: None,
            traces_sample_rate: 0.0,
            max_breadcrumbs: 100,
            user_stable_id: None,
            tags: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Vectors computed with reference HKDF-SHA256 (extract+expand), matching
    // Convos iOS MetricsStableIdEncoder(salt: "convos-metrics", info: <class>).
    #[test]
    fn stable_id_matches_reference_vectors() {
        assert_eq!(
            stable_id(
                "a9be0a2ae5aca34ff1a4bd25277f7e56e3b32e4b418ba1b48f0d33d004cd4b9a",
                b"inbox-stable-id"
            ),
            "68a7e54af6e50bf438d86104d6ea9e25ffa0cfe80ceac0a5497db459da371074"
        );
        assert_eq!(
            stable_id("d2794b1478b6e0a06d3bd1a52a3aae1c", b"group-stable-id"),
            "97a6abdc3b86714e001232db751893461d16038c4dc03a24c1e9e648e6485675"
        );
        assert_eq!(
            stable_id("8ab721a3", b"installation-stable-id"),
            "d0687fd03b5e3aa19778ba0852972ff8d9c8d6ae5f542c5b2acc1d6a2dc0ce61"
        );
    }

    #[test]
    fn scrub_value_hashes_only_id_keys() {
        let hashed = scrub_value("inbox_id", "a9be").unwrap();
        assert_eq!(hashed.len(), 64);
        assert_ne!(hashed, "a9be");
        assert_eq!(scrub_value("sender_inbox_id", "a9be").unwrap(), hashed);
        assert!(scrub_value("cursor", "123").is_none());
        assert!(scrub_value("operation", "mls.sync").is_none());
    }
}
