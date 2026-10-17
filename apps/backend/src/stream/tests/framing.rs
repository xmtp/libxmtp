//! The framing bound the configuration reserves around each envelope covers
//! the largest metadata and the nested messages a delivery encodes.

use crate::{
    api,
    config::{ENVELOPE_METADATA_AND_FRAMING_BYTES, GRPC_HEADER_BYTES, MAX_METADATA_BYTES},
};
use prost::Message;
use xmtp_configuration::BACKEND_DEFAULT_MAX_ENVELOPE_BYTES;

#[xmtp_common::test(unwrap_try = true)]
fn framing_bound_covers_maximum_metadata_and_nested_messages() {
    const STORED_TOPIC_BYTES: usize = 128;
    const HASH_BYTES: usize = 32;
    let meta = api::EnvelopeMeta {
        cursor: Some(api::Cursor {
            sequence_id: u64::MAX,
        }),
        server_ns: u64::MAX,
        message_hash: Some(api::MessageHash {
            hash: Some(api::message_hash::Hash::Sha256(vec![0; HASH_BYTES])),
        }),
        topic: Some(api::Topic {
            topic: vec![0; STORED_TOPIC_BYTES],
        }),
        expiry_ns: u64::MAX,
        is_commit_or_proposal: true,
    };
    assert_eq!(meta.encoded_len(), MAX_METADATA_BYTES);
    for size in [
        0,
        127,
        128,
        16_383,
        16_384,
        BACKEND_DEFAULT_MAX_ENVELOPE_BYTES,
    ] {
        let envelope = api::ClientEnvelope {
            payload: Some(api::client_envelope::Payload::GroupMessage(
                api::GroupMessage {
                    data: vec![0; size],
                    ..Default::default()
                },
            )),
        };
        let bound = envelope.encoded_len() + ENVELOPE_METADATA_AND_FRAMING_BYTES;
        let response = api::SubscribeResponse {
            response: Some(api::subscribe_response::Response::Messages(
                api::subscribe_response::Messages {
                    envelopes: vec![api::ServerEnvelope {
                        meta: Some(meta.clone()),
                        envelope: Some(envelope),
                    }],
                },
            )),
        };
        assert!(response.encoded_len() + GRPC_HEADER_BYTES <= bound);
    }
}
