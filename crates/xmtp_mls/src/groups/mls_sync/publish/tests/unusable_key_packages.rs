//! A sender records unusable packages as failed installations and adds the
//! rest of the inbox. A backend double serves the selected packages.

use super::faults::{FaultyApi, faulty_group};
use super::*;
use crate::groups::send_message_opts::SendMessageOpts;
use crate::utils::{TestXmtpMlsContext, key_package_with_extensions};
use openmls::prelude::{Ciphersuite, Extension, UnknownExtension};
use xmtp_configuration::{
    WELCOME_POINTEE_ENCRYPTION_AEAD_TYPES_EXTENSION_ID, WELCOME_WRAPPER_ENCRYPTION_EXTENSION_ID,
    WELCOME_WRAPPER_XWING_KEY_BYTES,
};
use xmtp_cryptography::configuration::CIPHERSUITE;
use xmtp_id::key_package::WrapperAlgorithm;
use xmtp_mls_validation::commit::extract_group_membership;
use xmtp_proto::xmtp::mls::message_contents::WelcomeWrapperEncryption;

/// A pointee AEAD extension that does not decode.
fn undecodable_pointee_aead() -> Extension {
    Extension::Unknown(
        WELCOME_POINTEE_ENCRYPTION_AEAD_TYPES_EXTENSION_ID,
        UnknownExtension(vec![0xff]),
    )
}

/// An X-Wing wrapper key one byte short. It decodes, so a sender that does not
/// check its length adds the installation and then fails every Welcome of the
/// commit.
fn short_xwing_wrapper_key() -> Extension {
    let wrapper = WelcomeWrapperEncryption {
        pub_key: vec![9; WELCOME_WRAPPER_XWING_KEY_BYTES - 1],
        algorithm: WrapperAlgorithm::XWingMLKEM768Draft6.into(),
    };
    Extension::Unknown(
        WELCOME_WRAPPER_ENCRYPTION_EXTENSION_ID,
        UnknownExtension(wrapper.encode_to_vec()),
    )
}

/// Alix adds Bo and Caro while the backend serves Bo's second installation a
/// package built by `package`. The add succeeds, that installation is
/// recorded as failed, the one membership intent finishes with its Welcomes
/// sent, and Bo's first installation and Caro join.
// verifies: JOIN-090, JOIN-093, JOIN-012
#[rstest::rstest]
#[case::undecodable_pointee_aead(|bo2, _| key_package_with_extensions(
    &bo2, CIPHERSUITE, vec![undecodable_pointee_aead()]
))]
#[case::short_xwing_wrapper_key(|bo2, _| key_package_with_extensions(
    &bo2, CIPHERSUITE, vec![short_xwing_wrapper_key()]
))]
#[case::other_ciphersuite(|bo2, _| key_package_with_extensions(
    &bo2, Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519, vec![]
))]
#[case::other_installation_leaf(|_, bo| key_package_with_extensions(&bo, CIPHERSUITE, vec![]))]
#[xmtp_common::test(unwrap_try = true)]
async fn unusable_package_fails_only_its_installation(
    #[case] package: fn(TestXmtpMlsContext, TestXmtpMlsContext) -> Vec<u8>,
) {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(bo2, from: bo, disable_workers);
    tester!(caro, disable_workers);
    let unusable = bo2.context.installation_id().to_vec();
    let api = FaultyApi::new(&alix, |_| false, []);
    api.served_key_packages.lock().insert(
        unusable.clone(),
        package(bo2.context.clone(), bo.context.clone()),
    );
    let group_id = alix.create_group(None, None).unwrap().group_id;
    let group = faulty_group(&alix, &group_id, api).await;

    group
        .add_members(&[bo.inbox_id(), caro.inbox_id()])
        .await
        .unwrap();

    let membership = group
        .load_mls_group_with_lock_async(async |mls_group| {
            Ok::<_, GroupError>(extract_group_membership(mls_group.extensions())?)
        })
        .await
        .unwrap();
    assert_eq!(membership.failed_installations, vec![unusable]);
    let membership_intent_states: Vec<IntentState> = group
        .context
        .db()
        .find_group_intents(
            group_id,
            None,
            Some(vec![IntentKind::UpdateGroupMembership]),
        )
        .unwrap()
        .into_iter()
        .map(|intent| intent.state)
        .collect();
    assert_eq!(membership_intent_states, vec![IntentState::Processed]);
    for joiner in [&bo, &caro] {
        joiner.sync_welcomes().await.unwrap();
        assert!(joiner.group(&group_id).is_ok());
    }
}

/// Refreshing an existing inbox skips its new unusable installation without
/// preventing the next application message.
// verifies: JOIN-093, JOIN-012
#[xmtp_common::test(unwrap_try = true)]
async fn refresh_skips_unusable_package_and_send_succeeds() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group_id = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?
        .group_id;
    tester!(bo2, from: bo, disable_workers);
    let unusable = bo2.context.installation_id().to_vec();
    let api = FaultyApi::new(&alix, |_| false, []);
    api.served_key_packages.lock().insert(
        unusable.clone(),
        key_package_with_extensions(
            &bo2.context,
            Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519,
            vec![],
        ),
    );
    let group = faulty_group(&alix, &group_id, api).await;

    group
        .update_installations()
        .await
        .expect("refresh must skip the unusable installation");
    let membership = group
        .load_mls_group_with_lock_async(async |mls_group| {
            Ok::<_, GroupError>(extract_group_membership(mls_group.extensions())?)
        })
        .await?;
    assert_eq!(membership.failed_installations, vec![unusable]);
    group
        .send_message(b"hello", SendMessageOpts::default())
        .await
        .expect("sending must succeed after the refresh");
}

/// A backend serves Bo's package under Bo's second installation's topic. Bo's
/// second installation has no key package and Bo's own package is unchanged.
// verifies: API-245
#[xmtp_common::test(unwrap_try = true)]
async fn served_package_of_another_installation_is_absent() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(bo2, from: bo, disable_workers);
    let api = FaultyApi::new(&alix, |_| false, []);
    api.served_key_packages.lock().insert(
        bo2.context.installation_id().to_vec(),
        key_package_with_extensions(&bo.context, CIPHERSUITE, vec![]),
    );
    let group_id = alix.create_group(None, None)?.group_id;
    let group = faulty_group(&alix, &group_id, api).await;

    let packages = crate::mls_store::MlsStore::new(group.context.clone())
        .get_key_packages_for_installation_ids(vec![
            bo2.context.installation_id().to_vec(),
            bo.context.installation_id().to_vec(),
        ])
        .await?;

    assert!(!packages.contains_key(bo2.context.installation_id().as_slice()));
    let bo_package = packages[bo.context.installation_id().as_slice()].as_ref()?;
    assert_eq!(
        bo_package.installation_public_key,
        bo.context.installation_id().to_vec()
    );
}

/// Alix rotates three times, each behind a fresh client. A is refused with
/// UNAVAILABLE, B is stored and its response lost, and C is refused with
/// INVALID_ARGUMENT. The read before B settles A, the read before C finds B
/// served and records it, and C is discarded at once. A lagged read settles
/// nothing.
// verifies: JOIN-074, JOIN-020
#[rstest::rstest]
#[case::served(false)]
#[case::lagged(true)]
#[xmtp_common::test]
async fn rotation_settles_unconfirmed_packages_and_discards_refused_ones(#[case] lag: bool) {
    use super::faults::Fault;
    use openmls_traits::storage::StorageProvider;
    use xmtp_id::key_package::VerifiedKeyPackageV2;
    tester!(alix, disable_workers);
    let group_id = alix.create_group(None, None).unwrap().group_id;
    let key_package = |payload: &Payload| matches!(payload, Payload::KeyPackage(_));
    let unavailable = || std::iter::repeat_n(Fault::Refuse(tonic::Code::Unavailable), 10);
    let rotations = [
        unavailable().collect::<Vec<_>>(),
        std::iter::once(Fault::StoreThenUnavailable)
            .chain(unavailable())
            .collect(),
        vec![Fault::Refuse(tonic::Code::InvalidArgument)],
    ];
    let mut hash_refs = Vec::new();
    let mut lost = Vec::new();
    for faults in rotations {
        let api = FaultyApi::new(&alix, key_package, faults);
        api.lag_newest
            .store(lag, std::sync::atomic::Ordering::SeqCst);
        let context = faulty_group(&alix, &group_id, api.clone()).await.context;
        let rotated = context
            .identity()
            .rotate_and_upload_key_package(context.api(), context.mls_storage(), true)
            .await;
        assert!(rotated.is_err());
        let Some(Payload::KeyPackage(package)) = &api.requests.lock()[0].envelopes[0].payload
        else {
            panic!("an intercepted key package");
        };
        let crypto = xmtp_db::XmtpOpenMlsProvider::<()>::new_crypto();
        let verified =
            VerifiedKeyPackageV2::from_bytes(&crypto, &package.key_package_tls_serialized).unwrap();
        let provider = xmtp_db::XmtpOpenMlsProviderRef::new(context.mls_storage());
        hash_refs.push(
            crate::identity::serialize_key_package_hash_ref(&verified.inner, &provider).unwrap(),
        );
        lost.extend(
            api.lost
                .lock()
                .iter()
                .map(|meta| meta.cursor.unwrap().sequence_id),
        );
    }
    let db = alix.context.db();
    let bundle = |hash_ref: &Vec<u8>| -> Option<openmls::prelude::KeyPackageBundle> {
        let hash_ref = crate::identity::deserialize_key_package_hash_ref(hash_ref).unwrap();
        alix.context.mls_storage().key_package(&hash_ref).unwrap()
    };
    let [a, b, c] = &hash_refs[..] else {
        panic!("three rotations")
    };
    assert!(
        db.find_key_package_history_entry_by_hash_ref(c.clone())
            .is_err()
    );
    assert!(bundle(c).is_none());
    let wrapper_key = |hash_ref| alix.context.mls_storage().wrapper_private_key(hash_ref);
    assert!(wrapper_key(c).unwrap().is_none());
    assert!(wrapper_key(b).unwrap().is_some());
    let a = db
        .find_key_package_history_entry_by_hash_ref(a.clone())
        .unwrap();
    let b_row = db
        .find_key_package_history_entry_by_hash_ref(b.clone())
        .unwrap();
    assert!(bundle(b).is_some());
    assert!(b_row.delete_at_ns.is_none());
    if lag {
        assert_eq!(a.published_sequence_id, None);
        assert!(a.delete_at_ns.is_none());
        assert_eq!(b_row.published_sequence_id, None);
    } else {
        assert!(a.delete_at_ns.is_some());
        assert_eq!(b_row.published_sequence_id, Some(lost[0] as i64));
    }
}
