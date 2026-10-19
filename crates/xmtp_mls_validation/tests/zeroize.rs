//! Heap-canary proof that validating a commit's `AppDataUpdate` proposals
//! wipes every copy of the commit-log signer it makes. Native only: it
//! installs the heap-canary global allocator. No backend needed.
// The heap-canary harness is Unix-only (`libc::memmem`).
#![cfg(unix)]

use openmls::messages::proposals::AppDataUpdateOperation;
use xmtp_cryptography::secret::heap_canary::{Canary, scan};
use xmtp_mls_common::app_data::{
    component_id::ComponentId,
    component_permissions::component_permissions,
    component_registry::{ComponentRegistry, new_component_metadata},
    validation::ActorAuthority,
};
use xmtp_mls_validation::commit::{AppDataUpdateInCommit, validate_app_data_update_sequence};
use xmtp_proto::xmtp::mls::message_contents::{
    ComponentType, MetadataPolicy,
    metadata_policy::{Kind, MetadataBasePolicy},
};

xmtp_cryptography::install_heap_canary!();

/// The committed signer.
const SIGNER: [u8; 32] = [
    0x3e, 0x91, 0xd4, 0x07, 0x6c, 0xa2, 0x58, 0xf1, 0x1b, 0xc7, 0x84, 0x2d, 0xe9, 0x50, 0x36, 0xab,
    0x72, 0x0f, 0xc5, 0x98, 0x4a, 0xe3, 0x16, 0x6d, 0xb0, 0x29, 0xfe, 0x83, 0x5c, 0x17, 0xa6, 0x4d,
];

/// The signer the first proposal installs.
const SECOND_SIGNER: [u8; 32] = [
    0xa9, 0x24, 0x6f, 0xd2, 0x13, 0x88, 0xc0, 0x5e, 0xf7, 0x3a, 0x91, 0x0c, 0x65, 0xbe, 0x42, 0xd8,
    0x1f, 0x7c, 0xe4, 0x53, 0x9a, 0x06, 0xcd, 0x38, 0x81, 0xf2, 0x2b, 0x97, 0x4e, 0xb5, 0x60, 0x1d,
];

/// The signer the second proposal installs.
const THIRD_SIGNER: [u8; 32] = [
    0x67, 0xd0, 0x1c, 0xa5, 0x3f, 0x82, 0xeb, 0x49, 0xb6, 0x0d, 0x74, 0xf8, 0x25, 0xc1, 0x9e, 0x5a,
    0xe0, 0x33, 0x8f, 0x14, 0xbd, 0x6a, 0xc9, 0x02, 0x57, 0xfa, 0x18, 0xa3, 0x7e, 0xd5, 0x40, 0x9b,
];

/// A registry that lets any member write the signer.
fn registry() -> ComponentRegistry {
    let allow = MetadataPolicy {
        kind: Some(Kind::Base(MetadataBasePolicy::Allow as i32)),
    };
    let mut registry = ComponentRegistry::new();
    registry
        .set(
            ComponentId::COMMIT_LOG_SIGNER,
            new_component_metadata(
                component_permissions()
                    .insert(allow.clone())
                    .update(allow.clone())
                    .delete(allow)
                    .call(),
                ComponentType::Bytes,
            ),
        )
        .unwrap();
    registry
}

/// Validating two signer rotations in one commit frees no unwiped copy of the
/// committed signer, the intermediate one or the final one: not the expanded
/// changes, the computed post-states, the replaced state or the returned map.
/// The proposals and the registry are built before arming and dropped after
/// disarming.
#[xmtp_common::test(unwrap_try = true)]
fn validating_signer_rotations_wipes_every_signer() {
    let registry = registry();
    let rotations = [
        AppDataUpdateOperation::Update(SECOND_SIGNER.to_vec().into()),
        AppDataUpdateOperation::Update(THIRD_SIGNER.to_vec().into()),
    ];
    let actor = ActorAuthority {
        is_admin: false,
        is_super_admin: false,
    };

    let canaries = [SIGNER, SECOND_SIGNER, THIRD_SIGNER].map(|signer| Canary::new(&signer));
    let ((), hits) = scan(&canaries, || {
        let post_states = validate_app_data_update_sequence(
            rotations.iter().map(|operation| AppDataUpdateInCommit {
                component_id: ComponentId::COMMIT_LOG_SIGNER,
                operation,
                actor,
                proposer_inbox_id: "inbox_alice",
            }),
            |_| Some(&SIGNER[..]),
            &registry,
            None,
            None,
        )
        .unwrap();
        let post = post_states[&ComponentId::COMMIT_LOG_SIGNER]
            .as_deref()
            .unwrap();
        assert!(*post == THIRD_SIGNER);
        drop(post_states);
    });
    drop((rotations, registry));

    assert_eq!(
        hits,
        [0, 0, 0],
        "freed heap blocks held a commit-log signer"
    );
}
