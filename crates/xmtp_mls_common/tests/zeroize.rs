//! Heap-canary proofs that parsing group metadata wipes every copy of the
//! commit-log signer it makes. Native only: it installs the heap-canary
//! global allocator. No backend needed.
#![cfg(not(target_arch = "wasm32"))]

use std::collections::HashMap;

use openmls::{
    extensions::{AppDataDictionary, AppDataDictionaryExtension, Extension, Extensions},
    group::GroupContext,
};
use prost::Message;
use xmtp_cryptography::secret::heap_canary::{Canary, scan};
use xmtp_mls_common::{
    app_data::{
        component_id::ComponentId, components::metadata_attributes::CommitLogSignerComponent,
        typed::Component,
    },
    group_mutable_metadata::{
        GroupMutableMetadata, MetadataField, merge_dict_into_mutable_metadata,
    },
};
use xmtp_proto::xmtp::mls::message_contents::{
    GroupMutableMetadataV1 as GroupMutableMetadataProto, Inboxes,
};

xmtp_cryptography::install_heap_canary!();

const SIGNER: [u8; 32] = [
    0xc4, 0x1d, 0x8e, 0x53, 0xf0, 0x27, 0x9a, 0x6b, 0x15, 0xe8, 0x3c, 0xa1, 0x74, 0x0f, 0xd6, 0x49,
    0xb2, 0x5e, 0x81, 0x37, 0xca, 0x60, 0x1b, 0xf4, 0x98, 0x2d, 0x73, 0xae, 0x06, 0xe1, 0x5a, 0x9c,
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
