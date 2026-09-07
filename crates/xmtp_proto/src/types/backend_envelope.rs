use prost::Message;
use xmtp_common::sha256_array;

use crate::xmtp::backend::v1::ClientEnvelope;

/// Canonical outer encoding and its hash. Inner payload bytes are unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalEnvelope {
    pub bytes: Vec<u8>,
    pub hash: [u8; 32],
}

/// Encode the envelope once for storage, retries, and hash matching.
/// This does not validate the payload or calculate a client MLS message ID.
pub fn canonical_envelope(envelope: &ClientEnvelope) -> CanonicalEnvelope {
    let bytes = envelope.encode_to_vec();
    let hash = sha256_array(&bytes);
    CanonicalEnvelope { bytes, hash }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xmtp::{
        backend::v1::{
            CommitLogEntry, GroupMessage, KeyPackage, WelcomeMessage,
            client_envelope::Payload,
            welcome_message::{V1, Version},
        },
        identity::associations::{IdentityUpdate, RecoverableEd25519Signature},
    };

    /// P1-VAL-05, API-020/023/025, SEC-016. Same vector on native and wasm.
    #[xmtp_common::test(unwrap_try = true)]
    fn all_payloads_have_stable_canonical_bytes_and_distinct_outer_hashes() {
        // Reordered inner fields and an unknown outer field are noncanonical.
        let input = hex::decode("0a0b1801120201020a0300ff42800100")?;
        let group_envelope = ClientEnvelope::decode(input.as_slice())?;
        assert_eq!(
            group_envelope.payload,
            Some(Payload::GroupMessage(GroupMessage {
                data: vec![0, 255, 66],
                sender_hmac: vec![1, 2],
                should_push: true,
            }))
        );

        let cases = [
            (
                group_envelope.clone(),
                "0a0b0a0300ff42120201021801",
                "5fd2916dcdd5fe79bb09ab21d545d5637307ace6e5352d4c2d287dbdd9338888",
            ),
            (
                ClientEnvelope {
                    payload: Some(Payload::WelcomeMessage(WelcomeMessage {
                        version: Some(Version::V1(V1 {
                            installation_key: vec![0x10, 0x11],
                            data: vec![0x12],
                            hpke_public_key: vec![0x13, 0x14],
                            wrapper_algorithm: 1,
                            welcome_metadata: vec![0x15],
                        })),
                    })),
                },
                "12120a100a0210111201121a02131420012a0115",
                "9a47fe38765eeebb1f272663381503c987fabbf9bb99b023fc88eaaffda60d5f",
            ),
            (
                ClientEnvelope {
                    payload: Some(Payload::KeyPackage(KeyPackage {
                        key_package_tls_serialized: vec![0x20, 0x21, 0x22],
                    })),
                },
                "1a050a03202122",
                "3f46232f37d76bcc2e80d3c54c9f9d5a7273d78ef8f91336249ef79746fd06dc",
            ),
            (
                ClientEnvelope {
                    payload: Some(Payload::IdentityUpdate(IdentityUpdate {
                        client_timestamp_ns: 7,
                        inbox_id: "ab".into(),
                        ..Default::default()
                    })),
                },
                "220610071a026162",
                "0aab40455ce3dacc36006ef421e2746016960606e24d125946e7a73dc9cf6f9b",
            ),
            (
                ClientEnvelope {
                    payload: Some(Payload::CommitLogEntry(CommitLogEntry {
                        serialized_commit_log_entry: vec![0x30, 0x31],
                        signature: Some(RecoverableEd25519Signature {
                            bytes: vec![0x32],
                            public_key: vec![0x33, 0x34],
                        }),
                    })),
                },
                "2a0d0a02303112070a013212023334",
                "108bc3029f48dda98d9d43238154830ae0ee71e7d2e083264a2b07a3d736bee2",
            ),
        ];

        for (envelope, expected_bytes, expected_hash) in cases {
            let canonical = canonical_envelope(&envelope);
            assert_eq!(hex::encode(&canonical.bytes), expected_bytes);
            assert_eq!(hex::encode(canonical.hash), expected_hash);
            let decoded = ClientEnvelope::decode(canonical.bytes.as_slice())?;
            assert_eq!(decoded, envelope);
            assert_eq!(canonical_envelope(&decoded), canonical);
        }

        let Some(Payload::GroupMessage(group)) = group_envelope.payload else {
            unreachable!("first golden vector is a group message")
        };
        assert_eq!(
            hex::encode(sha256_array(&group.data)),
            "f803bec586282caafe409609aae90eb09f6d4cddb6e04431ddf76d22e7dcacd6"
        );
        assert_ne!(
            canonical_envelope(&ClientEnvelope {
                payload: Some(Payload::GroupMessage(group.clone())),
            })
            .hash,
            sha256_array(&group.data)
        );
    }
}
