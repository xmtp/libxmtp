//! Heap-canary proofs that parsing group metadata and the AppData helpers
//! wipe every copy of the commit-log signer they make, and that deriving,
//! sealing, opening, and signing with group keys wipe every copy of the group
//! root and its derived keys. Native only: it installs the heap-canary global
//! allocator. No backend needed.
// The heap-canary harness is Unix-only (`libc::memmem`).
#![cfg(unix)]

use std::collections::HashMap;

use openmls::{
    extensions::{AppDataDictionary, AppDataDictionaryExtension, Extension, Extensions},
    group::GroupContext,
    messages::proposals::AppDataUpdateOperation,
};
use prost::Message;
use xmtp_cryptography::secret::heap_canary::{Canary, scan};
use xmtp_mls_common::{
    app_data::{
        component_id::ComponentId,
        component_registry::ComponentRegistry,
        component_source::{
            apply_app_data_update_payload, expand_app_data_update_to_changes, read_component_bytes,
            read_from_app_data_dict_from_extensions,
        },
        components::metadata_attributes::CommitLogSignerComponent,
        creation::encode_metadata_attribute_value,
        typed::Component,
    },
    group_keys::{
        GroupKeys, GroupRoot, open_commit_log_entry, seal_commit_log_entry, sign_group_message,
    },
    group_mutable_metadata::{
        GroupMutableMetadata, MetadataField, merge_components_into_mutable_metadata,
        merge_dict_into_mutable_metadata,
    },
};
use xmtp_proto::xmtp::{
    backend::v1::GroupMessage,
    mls::message_contents::{GroupMutableMetadataV1 as GroupMutableMetadataProto, Inboxes},
};

xmtp_cryptography::install_heap_canary!();

const SIGNER: [u8; 32] = [
    0xc4, 0x1d, 0x8e, 0x53, 0xf0, 0x27, 0x9a, 0x6b, 0x15, 0xe8, 0x3c, 0xa1, 0x74, 0x0f, 0xd6, 0x49,
    0xb2, 0x5e, 0x81, 0x37, 0xca, 0x60, 0x1b, 0xf4, 0x98, 0x2d, 0x73, 0xae, 0x06, 0xe1, 0x5a, 0x9c,
];

/// The signer a rotation installs.
const NEXT_SIGNER: [u8; 32] = [
    0x5b, 0xe2, 0x07, 0x9f, 0x31, 0xc8, 0x6a, 0xd4, 0x80, 0x1e, 0xb7, 0x43, 0xf9, 0x2c, 0x65, 0x0d,
    0xa8, 0x14, 0x7e, 0xc3, 0x39, 0xf6, 0x52, 0x8b, 0x0e, 0xd1, 0x67, 0x2a, 0x94, 0x4f, 0xbc, 0x13,
];

/// Legacy mutable-metadata extension bytes that hold `SIGNER` as hex.
fn legacy_metadata_bytes() -> Vec<u8> {
    let attributes = HashMap::from([
        (
            MetadataField::GroupName.as_str().to_owned(),
            "zeroize".to_owned(),
        ),
        (
            MetadataField::CommitLogSigner.as_str().to_owned(),
            hex::encode(SIGNER),
        ),
    ]);
    GroupMutableMetadataProto {
        attributes,
        admin_list: Some(Inboxes { inbox_ids: vec![] }),
        super_admin_list: Some(Inboxes { inbox_ids: vec![] }),
    }
    .encode_to_vec()
}

/// An AppData dictionary whose `COMMIT_LOG_SIGNER` component holds `SIGNER`.
fn app_data_extensions() -> Extensions<GroupContext> {
    let mut dictionary = AppDataDictionary::new();
    let _ = dictionary.insert(ComponentId::COMMIT_LOG_SIGNER.as_u16(), SIGNER.to_vec());
    Extensions::from_vec(vec![Extension::AppDataDictionary(
        AppDataDictionaryExtension::new(dictionary),
    )])
    .unwrap()
}

/// Decoding the legacy metadata and the AppData component, reading the
/// signer, and cloning the metadata free no unwiped copy of the signer, as
/// raw bytes or as hex. The inputs are built before arming and dropped after
/// disarming, so only copies the code under test makes are seen.
#[xmtp_common::test(unwrap_try = true)]
fn parsing_metadata_wipes_the_commit_log_signer() {
    let legacy = legacy_metadata_bytes();
    let component = SIGNER.to_vec();
    let extensions = app_data_extensions();

    let ((), hits) = scan(&[Canary::new(&SIGNER)], || {
        let metadata = GroupMutableMetadata::try_from(&legacy).unwrap();
        let signer = metadata.commit_log_signer().unwrap();
        assert!(signer.as_slice() == SIGNER);
        let copy = metadata.clone();
        drop((signer, copy, metadata));

        let decoded = CommitLogSignerComponent::decode_value(&component).unwrap();
        assert!(decoded.as_slice() == SIGNER);
        drop(decoded);

        let mut merged = GroupMutableMetadata::new(HashMap::new(), vec![], vec![]);
        merge_dict_into_mutable_metadata(&mut merged, &extensions).unwrap();
        let signer = merged.commit_log_signer().unwrap();
        assert!(signer.as_slice() == SIGNER);
        drop((signer, merged));
    });
    drop((legacy, component, extensions));

    assert_eq!(hits, [0], "freed heap blocks held the commit-log signer");
}

/// The AppData helpers that read, apply and expand a `COMMIT_LOG_SIGNER`
/// rotation free no unwiped copy of the old or the new signer, and neither
/// does merging the post-rotation value into metadata, as commit validation
/// does. The inputs are built before arming and dropped after disarming.
#[xmtp_common::test(unwrap_try = true)]
fn app_data_helpers_wipe_the_commit_log_signer() {
    let extensions = app_data_extensions();
    let rotation = NEXT_SIGNER.to_vec();
    let operation = AppDataUpdateOperation::Update(rotation.clone().into());
    let registry = ComponentRegistry::new();
    let id = ComponentId::COMMIT_LOG_SIGNER;

    let ((), hits) = scan(&[Canary::new(&SIGNER), Canary::new(&NEXT_SIGNER)], || {
        let committed = read_from_app_data_dict_from_extensions(id, &extensions).unwrap();
        assert!(committed == SIGNER);
        assert!(read_component_bytes(id, &extensions).unwrap() == Some(&SIGNER[..]));

        let changes =
            expand_app_data_update_to_changes(id, &operation, Some(committed), &registry).unwrap();
        assert!(changes[0].value.as_deref() == Some(&NEXT_SIGNER[..]));
        drop(changes);

        let post =
            apply_app_data_update_payload(id, &rotation, Some(committed), &registry).unwrap();
        assert!(*post == NEXT_SIGNER);
        let post_commit = HashMap::from([(id, post)]);
        let mut metadata = GroupMutableMetadata::new(HashMap::new(), vec![], vec![]);
        merge_components_into_mutable_metadata(&mut metadata, |id| {
            post_commit.get(&id).map(|bytes| bytes.as_slice())
        })
        .unwrap();
        assert!(metadata.commit_log_signer().unwrap().as_slice() == NEXT_SIGNER);
        drop((post_commit, metadata));
    });
    drop((extensions, rotation, operation));

    assert_eq!(
        hits,
        [0, 0],
        "freed heap blocks held the old or the new commit-log signer"
    );
}

/// `Debug` of metadata that holds the signer does not show it.
#[xmtp_common::test(unwrap_try = true)]
fn metadata_debug_redacts_the_commit_log_signer() {
    let metadata = GroupMutableMetadata::try_from(&legacy_metadata_bytes())?;
    let debug = format!("{metadata:?}");
    assert!(debug.contains("zeroize"), "{debug}");
    for form in [hex::encode(SIGNER), hex::encode_upper(SIGNER)] {
        assert!(
            !debug.contains(&form),
            "GroupMutableMetadata Debug shows the signer: {debug}"
        );
    }
}

/// `Debug` of an expanded signer change shows its lengths, not the signer.
#[xmtp_common::test(unwrap_try = true)]
fn expanded_change_debug_redacts_the_commit_log_signer() {
    let operation = AppDataUpdateOperation::Update(NEXT_SIGNER.to_vec().into());
    let changes = expand_app_data_update_to_changes(
        ComponentId::COMMIT_LOG_SIGNER,
        &operation,
        Some(&SIGNER[..]),
        &ComponentRegistry::new(),
    )?;
    let debug = format!("{changes:?}");
    // A derived `Debug` prints the bytes as a decimal list.
    let leading = format!("{:?}", &NEXT_SIGNER[..4]);
    let leading = leading.trim_start_matches('[').trim_end_matches(']');
    assert!(
        !debug.contains(leading),
        "ExpandedComponentChange Debug shows the signer: {debug}"
    );
    assert!(debug.contains("value_len: Some(32)"), "{debug}");
}

/// Metadata that holds the signer but omits a required inbox list fails to
/// parse, and the failed parse frees no unwiped copy of the signer.
#[xmtp_common::test(unwrap_try = true)]
fn malformed_metadata_wipes_the_commit_log_signer() {
    let mut proto = GroupMutableMetadataProto::decode(legacy_metadata_bytes().as_slice())?;
    proto.admin_list = None;
    let malformed = proto.encode_to_vec();
    drop(proto);

    let ((), hits) = scan(&[Canary::new(&SIGNER)], || {
        assert!(GroupMutableMetadata::try_from(&malformed).is_err());
    });
    drop(malformed);

    assert_eq!(hits, [0], "freed heap blocks held the commit-log signer");
}

/// Signer hex that turns invalid in its last digit fails to decode, and the
/// failed decode frees no unwiped copy of the key bytes decoded before it.
/// const-hex decodes in 16-byte chunks and stops at the chunk with the bad
/// digit, so the first chunk is the part that can leak.
#[xmtp_common::test(unwrap_try = true)]
fn malformed_signer_hex_wipes_the_decoded_prefix() {
    let mut malformed = hex::encode(SIGNER);
    malformed.pop();
    malformed.push('z');

    let ((), hits) = scan(&[Canary::new(&SIGNER[..16])], || {
        assert!(
            encode_metadata_attribute_value(ComponentId::COMMIT_LOG_SIGNER, &malformed).is_err()
        );
    });
    drop(malformed);

    assert_eq!(hits, [0], "freed heap blocks held the commit-log signer");
}

/// The group-key vectors, made by an independent implementation.
const GROUP_KEY_VECTORS: &str = include_str!("../src/group_keys/fixtures/vectors.txt");

/// The group root of the vectors and the four keys derived from it.
const GROUP_KEY_SECRETS: [&str; 5] = [
    "root",
    "group_signing_seed",
    "commit_log_signing_seed",
    "commit_log_encryption_key",
    "commit_log_nonce_key",
];

fn group_key_vector(name: &str) -> Vec<u8> {
    GROUP_KEY_VECTORS
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once(" = ")?;
            (key == name).then(|| hex::decode(value).unwrap())
        })
        .unwrap()
}

/// The vector root and derived keys, read before a scan is armed.
fn group_key_secrets() -> Vec<Vec<u8>> {
    GROUP_KEY_SECRETS.map(group_key_vector).to_vec()
}

fn vector_group_keys() -> Box<GroupKeys> {
    Box::new(GroupKeys::derive(
        &GroupRoot::from_bytes(&group_key_vector("root")).unwrap(),
    ))
}

/// Reading a root and deriving its keys into heap storage, then dropping
/// both, frees no unwiped copy of the root or of any derived key.
#[xmtp_common::test(unwrap_try = true)]
fn deriving_group_keys_wipes_the_root_and_every_key() {
    let secrets = group_key_secrets();
    let canaries: Vec<Canary> = secrets.iter().map(|secret| Canary::new(secret)).collect();
    let root = group_key_vector("root");
    let group_id = group_key_vector("group_id");

    let ((), hits) = scan(&canaries, || {
        let group_root = Box::new(GroupRoot::from_bytes(&root).unwrap());
        let keys = Box::new(GroupKeys::derive(&group_root));
        assert!(keys.group_id()[..] == group_id[..]);
        drop((keys, group_root));
    });
    drop((root, group_id, canaries, secrets));

    assert_eq!(hits, [0; 5], "freed heap blocks held the root or a key");
}

/// Sealing and opening a commit-log entry free no unwiped copy of the root
/// or of any derived key.
#[xmtp_common::test(unwrap_try = true)]
fn sealing_and_opening_wipe_every_group_key() {
    let secrets = group_key_secrets();
    let canaries: Vec<Canary> = secrets.iter().map(|secret| Canary::new(secret)).collect();
    let keys = vector_group_keys();
    let plaintext = group_key_vector("plaintext");

    let ((), hits) = scan(&canaries, || {
        let entry = seal_commit_log_entry(&keys, &plaintext).unwrap();
        let opened = open_commit_log_entry(&keys, &entry).unwrap();
        assert!(opened == plaintext);
        drop((entry, opened));
    });
    drop((keys, plaintext, canaries, secrets));

    assert_eq!(hits, [0; 5], "freed heap blocks held the root or a key");
}

/// Signing a group message frees no unwiped copy of the root or of any
/// derived key.
#[xmtp_common::test(unwrap_try = true)]
fn signing_a_group_message_wipes_every_group_key() {
    let secrets = group_key_secrets();
    let canaries: Vec<Canary> = secrets.iter().map(|secret| Canary::new(secret)).collect();
    let keys = vector_group_keys();
    let mut message = GroupMessage {
        data: group_key_vector("group_message_data"),
        sender_hmac: group_key_vector("group_message_sender_hmac"),
        should_push: true,
        signature: Vec::with_capacity(64),
    };

    let ((), hits) = scan(&canaries, || {
        sign_group_message(&keys, &mut message).unwrap();
    });
    assert!(message.signature == group_key_vector("group_message_signature"));
    drop((keys, message, canaries, secrets));

    assert_eq!(hits, [0; 5], "freed heap blocks held the root or a key");
}
