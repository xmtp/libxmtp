//! Heap-canary proof that push notification messages, device-sync HMAC
//! updates and the messages that carry the commit-log signer wipe their
//! secrets on drop, in every copy libxmtp makes: the original, each retry
//! clone, and a value a failed decode drops. prost's own encode and decode
//! buffers are out of reach and not checked.
// The heap-canary harness is Unix-only (`libc::memmem`).
#![cfg(unix)]

use prost::Message;
use xmtp_cryptography::secret::heap_canary::{Canary, scan};
use xmtp_proto::backend_v1::{
    HttpDelivery, RegisterRequest, Subscription, UnregisterRequest, UpdateSubscriptionsRequest,
    register_request::Delivery,
};
use xmtp_proto::xmtp::device_sync::content::{
    DeviceSyncContent, HmacKeyUpdate, PreferenceUpdate, PreferenceUpdates,
    device_sync_content::Content, preference_update::Update,
};
use xmtp_proto::xmtp::mls::database::{
    SendMessageData, UpdateMetadataData,
    send_message_data::{V1 as SendMessageV1, Version as SendMessageVersion},
    update_metadata_data::{V1 as UpdateMetadataV1, Version as UpdateMetadataVersion},
};
use xmtp_proto::xmtp::mls::message_contents::{
    GroupMutableMetadataV1, PlaintextEnvelope,
    content_types::{MultiRemoteAttachment, RemoteAttachmentInfo},
    plaintext_envelope::{Content as EnvelopeContent, V1 as EnvelopeV1},
};

xmtp_cryptography::install_heap_canary!();

const RECIPIENT_SECRET: [u8; 32] = [
    0x5b, 0xe2, 0x19, 0x8c, 0x47, 0xd3, 0x0a, 0xf6, 0x91, 0x2e, 0xb8, 0x63, 0x1d, 0xc4, 0x7f, 0x30,
    0xa5, 0x58, 0xec, 0x03, 0x96, 0x4b, 0xd7, 0x22, 0x6e, 0xf9, 0x14, 0x8d, 0x31, 0xba, 0x45, 0xc0,
];

const SIGNING_KEY: [u8; 32] = [
    0xc8, 0x37, 0x6a, 0xf1, 0x0e, 0x95, 0x4c, 0xb3, 0x28, 0xdf, 0x72, 0x09, 0xe6, 0x5d, 0xa0, 0x1b,
    0x84, 0xf3, 0x3e, 0xc9, 0x52, 0x07, 0xbd, 0x66, 0x19, 0xe8, 0x9f, 0x24, 0x7b, 0xd0, 0x43, 0xae,
];

const HMAC_KEY: [u8; 42] = [
    0x2f, 0xa4, 0x71, 0xd8, 0x0b, 0xc6, 0x59, 0xe3, 0x34, 0x8f, 0x12, 0xbd, 0x60, 0xf5, 0x27, 0x9a,
    0xc1, 0x4e, 0x03, 0xb7, 0x6c, 0xd9, 0x15, 0x88, 0xfa, 0x3d, 0xa2, 0x57, 0xe0, 0x0c, 0x93, 0x46,
    0xbb, 0x78, 0x21, 0xdc, 0x65, 0xf0, 0x1a, 0xa7, 0x5e, 0xc3,
];

const SIGNER: [u8; 32] = [
    0x71, 0xa8, 0x3d, 0xe6, 0x0b, 0x94, 0x52, 0xcf, 0x18, 0xb3, 0x6e, 0x05, 0xda, 0x47, 0x9c, 0x21,
    0xf8, 0x63, 0x2a, 0xbd, 0x84, 0x1f, 0xc6, 0x59, 0x0e, 0xa1, 0x7b, 0x34, 0xe9, 0x56, 0x0d, 0x92,
];

/// A field header whose length runs past the end of the input, so a decode
/// fails after the fields before it.
const TRUNCATED_FIELD: [u8; 2] = [0x0a, 0x7f];

fn register() -> RegisterRequest {
    RegisterRequest {
        recipient_id: vec![1; 32],
        recipient_secret: RECIPIENT_SECRET.to_vec(),
        delivery: Some(Delivery::Http(HttpDelivery {
            url: "https://example.test/hook".into(),
            signing_key: SIGNING_KEY.to_vec(),
        })),
    }
}

fn update() -> UpdateSubscriptionsRequest {
    UpdateSubscriptionsRequest {
        recipient_id: vec![1; 32],
        recipient_secret: RECIPIENT_SECRET.to_vec(),
        adds: vec![Subscription {
            topic: vec![2; 33],
            hmac_epoch_base: 0,
            hmac_keys: vec![HMAC_KEY.to_vec()],
            include_commits: false,
        }],
        removes: vec![],
    }
}

fn unregister() -> UnregisterRequest {
    UnregisterRequest {
        recipient_id: vec![1; 32],
        recipient_secret: RECIPIENT_SECRET.to_vec(),
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn dropped_notification_requests_leave_no_secret_in_freed_heap() {
    let canaries = [
        Canary::new(&RECIPIENT_SECRET),
        Canary::new(&SIGNING_KEY),
        Canary::new(&HMAC_KEY),
    ];
    let ((), hits) = scan(&canaries, || {
        let register = register();
        drop((register.clone(), register.clone(), register));

        let update = update();
        drop((update.clone(), update.clone(), update));

        let unregister = unregister();
        drop((unregister.clone(), unregister.clone(), unregister));
    });

    assert_eq!(hits, [0, 0, 0], "freed heap blocks held a push secret");
}

/// A sync message whose HMAC key decodes before malformed trailing bytes
/// fails to decode, and the partly decoded value prost drops wipes the key.
/// The input is a `Bytes` buffer, so prost slices it instead of copying the
/// key through a buffer of its own.
#[xmtp_common::test(unwrap_try = true)]
fn a_failed_sync_decode_wipes_the_hmac_key() {
    let mut encoded = DeviceSyncContent {
        content: Some(Content::PreferenceUpdates(PreferenceUpdates {
            updates: vec![PreferenceUpdate {
                update: Some(Update::Hmac(HmacKeyUpdate {
                    key: HMAC_KEY.to_vec(),
                    cycled_at_ns: 1,
                })),
            }],
        })),
    }
    .encode_to_vec();
    // A field header with a length that runs past the end of the input.
    encoded.extend_from_slice(&[0x0a, 0x7f]);
    let input = bytes::Bytes::from(encoded);

    let ((), hits) = scan(&[Canary::new(&HMAC_KEY)], || {
        assert!(DeviceSyncContent::decode(input.clone()).is_err());
    });
    drop(input);

    assert_eq!(hits, [0], "freed heap blocks held the HMAC key");
}

/// Metadata and a metadata-update intent whose signer decodes before
/// malformed trailing bytes fail to decode, and the partly decoded values
/// prost drops wipe the signer. The inputs are `Bytes`, so prost slices them
/// instead of copying through buffers of its own.
#[xmtp_common::test(unwrap_try = true)]
fn a_failed_metadata_decode_wipes_the_commit_log_signer() {
    let mut metadata = GroupMutableMetadataV1 {
        attributes: [("_commit_log_signer".to_owned(), hex::encode(SIGNER))].into(),
        admin_list: None,
        super_admin_list: None,
    }
    .encode_to_vec();
    metadata.extend_from_slice(&TRUNCATED_FIELD);
    let metadata = bytes::Bytes::from(metadata);
    let mut intent = UpdateMetadataData {
        version: Some(UpdateMetadataVersion::V1(UpdateMetadataV1 {
            field_name: "_commit_log_signer".to_owned(),
            field_value: hex::encode(SIGNER),
            expected_field_value: Some(hex::encode(SIGNER)),
        })),
    }
    .encode_to_vec();
    intent.extend_from_slice(&TRUNCATED_FIELD);
    let intent = bytes::Bytes::from(intent);

    let ((), hits) = scan(&[Canary::new(&SIGNER)], || {
        assert!(GroupMutableMetadataV1::decode(metadata.clone()).is_err());
        assert!(UpdateMetadataData::decode(intent.clone()).is_err());
    });
    drop((metadata, intent));

    assert_eq!(hits, [0], "freed heap blocks held the commit-log signer");
}

/// A stored send intent and its plaintext envelope whose payload decodes
/// before malformed trailing bytes fail to decode, and the partly decoded
/// values prost drops wipe the payload, which can be an HMAC root key.
#[xmtp_common::test(unwrap_try = true)]
fn a_failed_send_intent_decode_wipes_the_payload() {
    let mut envelope = PlaintextEnvelope {
        content: Some(EnvelopeContent::V1(EnvelopeV1 {
            content: HMAC_KEY.to_vec(),
            idempotency_key: "key".to_owned(),
        })),
    }
    .encode_to_vec();
    envelope.extend_from_slice(&TRUNCATED_FIELD);
    let envelope = bytes::Bytes::from(envelope);
    let mut intent = SendMessageData {
        version: Some(SendMessageVersion::V1(SendMessageV1 {
            payload_bytes: HMAC_KEY.to_vec(),
        })),
    }
    .encode_to_vec();
    intent.extend_from_slice(&TRUNCATED_FIELD);
    let intent = bytes::Bytes::from(intent);

    let ((), hits) = scan(&[Canary::new(&HMAC_KEY)], || {
        assert!(PlaintextEnvelope::decode(envelope.clone()).is_err());
        assert!(SendMessageData::decode(intent.clone()).is_err());
    });
    drop((envelope, intent));

    assert_eq!(hits, [0], "freed heap blocks held the HMAC key");
}

fn remote_attachment() -> RemoteAttachmentInfo {
    let mut attachment = RemoteAttachmentInfo::default();
    attachment.secret = RECIPIENT_SECRET.to_vec();
    attachment.salt = SIGNING_KEY.to_vec();
    // The canary needs 16 bytes; normal 12-byte nonces are reviewed in Drop.
    attachment.nonce = HMAC_KEY[..16].to_vec();
    attachment
}

#[xmtp_common::test(unwrap_try = true)]
fn remote_attachment_owners_wipe_originals_and_clones() {
    let ((), hits) = scan(&attachment_canaries(), || {
        let original = remote_attachment();
        let multi = MultiRemoteAttachment {
            attachments: vec![original.clone(), original.clone()],
        };
        drop((original.clone(), original, multi.clone(), multi));
    });
    assert_eq!(hits, [0, 0, 0], "attachment fields were not wiped");
}

#[xmtp_common::test(unwrap_try = true)]
fn failed_remote_attachment_decode_wipes_populated_fields() {
    let mut single = remote_attachment().encode_to_vec();
    single.extend_from_slice(&TRUNCATED_FIELD);
    let single = bytes::Bytes::from(single);
    let mut multi = MultiRemoteAttachment {
        attachments: vec![remote_attachment()],
    }
    .encode_to_vec();
    multi.extend_from_slice(&TRUNCATED_FIELD);
    let multi = bytes::Bytes::from(multi);
    let ((), hits) = scan(&attachment_canaries(), || {
        assert!(RemoteAttachmentInfo::decode(single.clone()).is_err());
        assert!(MultiRemoteAttachment::decode(multi.clone()).is_err());
    });
    drop((single, multi));
    assert_eq!(hits, [0, 0, 0], "partial decode fields were not wiped");
}

#[xmtp_common::test(unwrap_try = true)]
fn remote_attachment_debug_redacts_secret_fields() {
    let value = remote_attachment();
    let debug = format!("{value:?} {:?}", value.clone());
    for bytes in [&value.secret, &value.salt, &value.nonce] {
        assert!(
            !debug.contains(&format!("{bytes:?}")),
            "attachment Debug shows decimal secret bytes"
        );
        assert!(
            !debug.contains(&hex::encode(bytes)),
            "attachment Debug shows hex secret bytes"
        );
    }
}

fn attachment_canaries() -> [Canary; 3] {
    [
        Canary::new(&RECIPIENT_SECRET),
        Canary::new(&SIGNING_KEY),
        Canary::new(&HMAC_KEY[..16]),
    ]
}
