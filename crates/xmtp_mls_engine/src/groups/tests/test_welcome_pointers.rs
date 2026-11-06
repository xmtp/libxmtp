use crate::groups::mls_ext::WelcomePointersExtension;
use crate::groups::test_support::welcome_pointers::{
    publish_commit_without_welcome, publish_pointer, random_pointer, sealed_pointee,
};
use crate::groups::test_support::{
    EnginePeer, EngineSuiteOps, EngineTestGroup, EngineTestPeer, EngineTestPeerOps,
};
use crate::groups::welcome_pointer::resolve_welcome_pointer;
use prost::Message;
use std::time::Duration;
use xmtp_configuration::WELCOME_HPKE_LABEL;
use xmtp_db::group::QueryGroup;
use xmtp_db::incoming_envelope::QueryIncomingEnvelope;
use xmtp_id::key_package::WrapperAlgorithm;
use xmtp_mls_core::context::CoreContext;
use xmtp_mls_group_keys::mls_ext::payload_encryption::{
    unwrap_payload_symmetric, wrap_payload_hpke, wrap_payload_symmetric,
};
use xmtp_mls_identity::identity_lifecycle::{ENABLE_WELCOME_POINTERS, WelcomePointersHook};
use xmtp_proto::types::{DecryptedWelcomePointer, WelcomeMessage, WelcomeMessageType};
use xmtp_proto::xmtp::mls::message_contents::WelcomeMetadata;
use xmtp_proto::xmtp::mls::message_contents::welcome_pointer::WelcomeV1Pointer;
use xmtp_proto::xmtp::mls::message_contents::{
    WelcomePointeeEncryptionAeadType, WelcomePointer as WelcomePointerProto,
    WelcomeWrapperAlgorithm,
};

#[xmtp_common::timeout(Duration::from_secs(40))]
#[rstest::rstest]
#[xmtp_common::test(unwrap_try = true)]

async fn test_welcome_pointer_round_trip_with_welcome_pointers() {
    Box::pin(test_welcome_pointer_round_trip(
        || true,
        async |welcomes| {
            let [
                WelcomeMessage {
                    variant: WelcomeMessageType::WelcomePointer(_),
                    ..
                },
            ] = welcomes
            else {
                return Err("expected single welcome pointer".to_string());
            };
            Ok(())
        },
    ))
    .await;
}

#[xmtp_common::timeout(Duration::from_secs(80))]
#[rstest::rstest]
#[xmtp_common::test(unwrap_try = true)]

async fn test_welcome_pointer_round_trip_without_welcome_pointers() {
    Box::pin(test_welcome_pointer_round_trip(
        || false,
        async |welcomes| {
            let [
                WelcomeMessage {
                    variant: WelcomeMessageType::V1(_),
                    ..
                },
            ] = welcomes
            else {
                return Err("expected single welcome message".to_string());
            };
            Ok(())
        },
    ))
    .await;
}

// verifies: JOIN-091
#[xmtp_common::timeout(Duration::from_secs(40))]
#[rstest::rstest]
#[xmtp_common::test(unwrap_try = true)]
async fn test_welcome_pointer_round_trip_with_random_mix_of_welcome_pointers() {
    let random_vec = xmtp_common::rand_vec::<1024>();
    let mut random_vec = random_vec
        .into_iter()
        .map(|b| b & 1 == 1)
        .collect::<Vec<_>>();
    // The sender uses pointers only when more than the threshold of the added
    // installations can open one. Force threshold + 1 of the first 9 calls on,
    // so the update of Bola's installations always uses pointers.
    random_vec
        .iter_mut()
        .take(xmtp_configuration::INSTALLATION_THRESHOLD_FOR_WELCOME_POINTER_SENDING + 1)
        .for_each(|b| *b = true);
    let mut gen_count = random_vec.len();
    let mut assert_count = random_vec.len();
    Box::pin(test_welcome_pointer_round_trip(
        || {
            gen_count += 1;
            if gen_count >= random_vec.len() {
                gen_count = 0;
            }
            random_vec[gen_count]
        },
        async |welcomes| {
            assert_count += 1;
            if assert_count >= random_vec.len() {
                assert_count = 0;
            }
            let is_welcome_pointer = random_vec[assert_count];
            if is_welcome_pointer {
                let [
                    WelcomeMessage {
                        variant: WelcomeMessageType::WelcomePointer(_),
                        ..
                    },
                ] = welcomes
                else {
                    return Err("expected single welcome pointer".to_string());
                };
                Ok(())
            } else {
                let [
                    WelcomeMessage {
                        variant: WelcomeMessageType::V1(_),
                        ..
                    },
                ] = welcomes
                else {
                    return Err("expected single welcome message".to_string());
                };
                Ok(())
            }
        },
    ))
    .await;
}

// This test works great as long as the calls to enable_extension are done in the same order as the calls to assertion.
async fn test_welcome_pointer_round_trip(
    mut enable_extension: impl FnMut() -> bool,
    mut assertion: impl AsyncFnMut(&[WelcomeMessage]) -> Result<(), String>,
) {
    // Create two test clients
    let alix = EngineTestPeer::new().await;
    // Have to skip managing the extension for bola, because this installations is always added to the group with a V1 welcome message.
    let bola = EngineTestPeer::new().await;

    // Create a group with alix as the creator
    let alix_group = alix.create_group(None, None).unwrap();
    tracing::info!("Alix group id: {}", hex::encode(alix_group.group_id));
    alix_group.sync().await.unwrap();

    // Add bola to the group - this should trigger welcome message creation
    alix_group.add_members(&[bola.inbox_id()]).await.unwrap();

    // Sync the group to ensure the welcome is sent
    alix_group.sync().await.unwrap();

    // Bola should receive the welcome message
    bola.sync_welcomes().await.unwrap();

    // Verify bola can see the group
    let bola_groups = bola
        .find_groups(xmtp_db::group::GroupQueryArgs::default())
        .unwrap();
    assert_eq!(bola_groups.len(), 1);

    let bola_group: &EngineTestGroup = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();

    // Create additional installations for both clients to simulate the scenario
    // where welcome pointers are used (when there are more than 2 capable installations)
    let mut extra_installations = Vec::new();
    for _ in 0..9 {
        let hook = WelcomePointersHook::new(enable_extension());
        ENABLE_WELCOME_POINTERS
            .scope(hook.clone(), async {
                extra_installations.push(bola.new_installation().await);
            })
            .await;
        assert!(hook.reads() > 0);
    }

    // Now test welcome pointer functionality by creating a scenario where
    // welcome pointers would be used (more than 2 capable installations)

    // Update installations in the group to include the new installations
    alix_group.update_installations().await.unwrap();
    bola_group.update_installations().await.unwrap();

    for bola_installation in extra_installations.drain(..) {
        // use alix's context here to avoid caching issues for welcome topics
        let welcomes = alix_group
            .context
            .api()
            .query_welcome_messages(&bola_installation.identity().installation_id())
            .await
            .unwrap();
        assertion(&welcomes).await.unwrap();
    }

    // Now add the world to ensure welcome pointer usage is triggered
    let mut testers = vec![];
    // 5 testers with 10 installations each will invite 50 installations to the group.
    // This will trigger welcome pointer usage.
    for _ in 0..5 {
        let hook = WelcomePointersHook::new(enable_extension());
        ENABLE_WELCOME_POINTERS
            .scope(hook.clone(), async {
                let charlie = EngineTestPeer::new().await;
                testers.push(charlie);
            })
            .await;
        assert!(hook.reads() > 0);
    }
    // 10 is max installations per inbox. The `testers` vec contains one for each tester
    for _ in 0..9 {
        for tester in testers.iter() {
            let hook = WelcomePointersHook::new(enable_extension());
            ENABLE_WELCOME_POINTERS
                .scope(hook.clone(), async {
                    extra_installations.push(tester.new_installation().await);
                })
                .await;
            assert!(hook.reads() > 0);
        }
    }
    alix_group
        .add_members(testers.iter().map(|i| i.inbox_id()).collect::<Vec<_>>())
        .await
        .unwrap();

    // Sync to send the welcome
    alix_group.sync().await.unwrap();

    // Testers should receive the welcome
    futures::future::try_join_all(
        testers
            .iter()
            .chain(extra_installations.iter())
            .map(|t| t.sync_welcomes()),
    )
    .await
    .unwrap();

    // Verify testers can see the group
    let welcomes = futures::future::join_all(testers.into_iter().chain(extra_installations).map(
        |tester| async {
            tester.sync_welcomes().await.unwrap();
            let tester_groups = tester
                .find_groups(xmtp_db::group::GroupQueryArgs::default())
                .unwrap();
            assert_eq!(tester_groups.len(), 1);
            let installation_id = tester.identity().installation_id();
            drop(tester);
            // use alix's context here to avoid caching issues for welcome topics
            alix_group
                .context
                .api()
                .query_welcome_messages(&installation_id)
                .await
                .unwrap()
        },
    ))
    .await;

    for welcome in welcomes {
        assertion(&welcome).await.unwrap();
    }

    // The test has successfully demonstrated welcome pointer functionality
    // by creating a group with multiple members and installations
}

/// Bo's key package advertises ChaCha20-Poly1305 pointees but only the
/// classical wrapper, a valid first-party combination. A pointer cannot be
/// wrapped for Bo, so Bo gets an inline Welcome, and Bo does not count toward
/// the pointer threshold: two post-quantum co-recipients stay below it, three
/// exceed it.
// verifies: JOIN-091
#[xmtp_common::timeout(Duration::from_secs(80))]
#[rstest::rstest]
#[case::below_threshold(2)]
#[case::above_threshold(3)]
#[xmtp_common::test]
async fn classical_pointee_recipient_gets_an_inline_welcome(#[case] pq_recipients: usize) {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    xmtp_mls_identity::identity_lifecycle::rotate_and_upload_key_package(
        bo.identity(),
        bo.context.api(),
        bo.context.mls_storage(),
        false,
    )
    .await
    .unwrap();
    let mut pq = Vec::new();
    for _ in 0..pq_recipients {
        let recipient = EngineTestPeer::new().await;
        pq.push(recipient);
    }

    let group = alix.create_group(None, None).unwrap();
    group
        .add_members(
            std::iter::once(bo.inbox_id())
                .chain(pq.iter().map(|recipient| recipient.inbox_id()))
                .collect::<Vec<_>>(),
        )
        .await
        .unwrap();
    group.sync().await.unwrap();

    let pointer_mode =
        pq_recipients > xmtp_configuration::INSTALLATION_THRESHOLD_FOR_WELCOME_POINTER_SENDING;
    for recipient in &pq {
        let welcomes = alix
            .context
            .api()
            .query_welcome_messages(recipient.context.installation_id())
            .await
            .unwrap();
        let [welcome] = welcomes.as_slice() else {
            panic!("expected one Welcome per recipient");
        };
        assert_eq!(
            matches!(welcome.variant, WelcomeMessageType::WelcomePointer(_)),
            pointer_mode
        );
    }
    let welcomes = alix
        .context
        .api()
        .query_welcome_messages(bo.context.installation_id())
        .await
        .unwrap();
    let [welcome] = welcomes.as_slice() else {
        panic!("expected one Welcome for Bo");
    };
    let Some(inline) = welcome.as_v1() else {
        panic!("Bo must get an inline Welcome");
    };
    assert_eq!(
        inline.wrapper_algorithm,
        WelcomeWrapperAlgorithm::Curve25519
    );

    for recipient in std::iter::once(&bo).chain(&pq) {
        recipient.sync_welcomes().await.unwrap();
        assert!(recipient.group(&group.group_id).is_ok());
    }
}

#[test]
fn test_welcome_pointer_encryption_round_trip() {
    // Test the symmetric encryption/decryption used in welcome pointers
    let symmetric_key = xmtp_common::rand_array::<32>();
    let data_nonce = xmtp_common::rand_array::<12>();
    let metadata_nonce = xmtp_common::rand_array::<12>();

    // Create test data (welcome message and metadata)
    let welcome_data = xmtp_common::rand_vec::<1000>();
    let message_cursor = xmtp_common::rand_u64();
    let welcome_metadata = WelcomeMetadata { message_cursor };
    let welcome_metadata_bytes = welcome_metadata.encode_to_vec();

    // Get available AEAD types for welcome pointers
    let available_types = WelcomePointersExtension::available_types();
    let aead_type = available_types.supported_aead_types.first().unwrap();

    // Test encryption
    let encrypted_welcome_data =
        wrap_payload_symmetric(&welcome_data, *aead_type, &symmetric_key, &data_nonce).unwrap();
    let encrypted_welcome_metadata = wrap_payload_symmetric(
        &welcome_metadata_bytes,
        *aead_type,
        &symmetric_key,
        &metadata_nonce,
    )
    .unwrap();
    // Verify encryption worked (data should be different)
    assert_ne!(encrypted_welcome_data, welcome_data);
    assert_ne!(encrypted_welcome_metadata, welcome_metadata_bytes);

    // Test decryption
    let decrypted_welcome_data = unwrap_payload_symmetric(
        &encrypted_welcome_data,
        *aead_type,
        &symmetric_key,
        &data_nonce,
    )
    .unwrap();
    let decrypted_welcome_metadata = unwrap_payload_symmetric(
        &encrypted_welcome_metadata,
        *aead_type,
        &symmetric_key,
        &metadata_nonce,
    )
    .unwrap();

    // Verify decryption worked (data should match original)
    assert_eq!(decrypted_welcome_data, welcome_data);
    assert_eq!(decrypted_welcome_metadata, welcome_metadata_bytes);

    // Verify the metadata can be decoded correctly
    let decoded_metadata = WelcomeMetadata::decode(decrypted_welcome_metadata.as_slice()).unwrap();
    assert_eq!(decoded_metadata.message_cursor, message_cursor);
}

#[test]
fn test_welcome_pointer_proto_round_trip() {
    // Test the protobuf serialization/deserialization of welcome pointers
    let destination = xmtp_common::rand_array::<32>();
    let encryption_key = xmtp_common::rand_array::<32>();

    // Create a welcome pointer
    let welcome_pointer = WelcomePointerProto {
        version: Some(
            xmtp_proto::xmtp::mls::message_contents::welcome_pointer::Version::WelcomeV1Pointer(
                WelcomeV1Pointer {
                    destination: destination.to_vec(),
                    aead_type: WelcomePointeeEncryptionAeadType::Chacha20Poly1305.into(),
                    encryption_key: encryption_key.to_vec(),
                    data_nonce: xmtp_common::rand_vec::<12>(),
                    welcome_metadata_nonce: xmtp_common::rand_vec::<12>(),
                },
            ),
        ),
    };

    // Serialize the welcome pointer
    let serialized = welcome_pointer.encode_to_vec();
    assert!(!serialized.is_empty());

    // Deserialize the welcome pointer
    let deserialized = WelcomePointerProto::decode(&serialized[..]).unwrap();

    // Verify the round trip worked
    assert_eq!(welcome_pointer.version, deserialized.version);

    // Verify the V1 fields
    match (&welcome_pointer.version, &deserialized.version) {
        (
            Some(
                xmtp_proto::xmtp::mls::message_contents::welcome_pointer::Version::WelcomeV1Pointer(
                    original,
                ),
            ),
            Some(
                xmtp_proto::xmtp::mls::message_contents::welcome_pointer::Version::WelcomeV1Pointer(
                    deserialized_v1,
                ),
            ),
        ) => {
            assert_eq!(original.destination, deserialized_v1.destination);
            assert_eq!(original.aead_type, deserialized_v1.aead_type);
            assert_eq!(original.encryption_key, deserialized_v1.encryption_key);
            assert_eq!(original.data_nonce, deserialized_v1.data_nonce);
            assert_eq!(
                original.welcome_metadata_nonce,
                deserialized_v1.welcome_metadata_nonce
            );
        }
        _ => panic!("Expected V1 versions"),
    }
}

/// A pointer whose key ChaCha20-Poly1305 cannot use, here 64 KiB, can never
/// open a pointee. It is rejected before its pointee is fetched, and no hold
/// keeps it or its key material.
// verifies: JOIN-094, JOIN-048
#[xmtp_common::timeout(Duration::from_secs(30))]
#[xmtp_common::test(unwrap_try = true)]
async fn malformed_welcome_pointer_is_rejected_before_retrieval() {
    use crate::groups::welcome_sync::{WelcomeHeadOutcome, WelcomeService};

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let pointer = WelcomeV1Pointer {
        encryption_key: vec![0x42; 64 * 1024],
        ..random_pointer()
    };
    let welcome = publish_pointer(&alix, &bo, &pointer).await;

    // Resolve through a backend that fails the test on any query.
    let mut api = xmtp_api_backend::MockBackendClient::new();
    api.expect_query_raw_envelopes().never();
    let offline: EngineTestPeer<_> = bo
        .with_api(std::sync::Arc::new(api), None, |options| {
            options.config_provider = Some(std::sync::Arc::new(
                xmtp_configuration::StaticConfigProvider::default(),
            ))
        })
        .await;
    let outcome = WelcomeService::new(offline.context.clone())
        .resolve_pending_welcome(welcome.cursor)
        .await?;
    assert!(matches!(
        outcome,
        WelcomeHeadOutcome::Progress {
            result: Err(crate::groups::GroupError::ConversionError(_)),
            ..
        }
    ));
    bo.assert_newest_welcome_rejected().await;
}

#[xmtp_common::timeout(Duration::from_secs(20))]
#[rstest::rstest]
#[xmtp_common::test(unwrap_try = true)]
async fn test_welcome_pointer_resolution_for_no_destination() {
    // This test would require mocking the API client to test the resolution logic
    // For now, we'll test the basic structure and error handling

    let alix = EngineTestPeer::new().await;

    // Test with valid structure but invalid destination (would fail at API level)
    let valid_pointer = DecryptedWelcomePointer {
        destination: xmtp_common::rand_array::<32>().into(),
        aead_type: WelcomePointeeEncryptionAeadType::Chacha20Poly1305,
        encryption_key: xmtp_common::rand_vec::<32>(),
        data_nonce: xmtp_common::rand_vec::<12>(),
        welcome_metadata_nonce: xmtp_common::rand_vec::<12>(),
    };

    // This should return None because the destination doesn't exist in the API
    let result = resolve_welcome_pointer(&valid_pointer, &alix.context).await;
    assert!(result.unwrap().is_none());
}

// verifies: JOIN-033
#[xmtp_common::test(unwrap_try = true)]
async fn test_welcome_pointer_resolution_to_another_welcome_pointer() {
    let alix = EngineTestPeer::new().await;

    let welcome_pointer = DecryptedWelcomePointer {
        destination: xmtp_common::rand_array::<32>().into(),
        aead_type: WelcomePointeeEncryptionAeadType::Chacha20Poly1305,
        encryption_key: xmtp_common::rand_vec::<32>(),
        data_nonce: xmtp_common::rand_vec::<12>(),
        welcome_metadata_nonce: xmtp_common::rand_vec::<12>(),
    };

    let welcome_message = xmtp_proto::backend_v1::WelcomeMessage {
        version: Some(
            xmtp_proto::backend_v1::welcome_message::Version::WelcomePointer(
                xmtp_proto::backend_v1::welcome_message::WelcomePointer {
                    installation_key: welcome_pointer.destination.to_vec(),
                    welcome_pointer: xmtp_common::rand_vec::<32>(),
                    hpke_public_key: xmtp_common::rand_vec::<32>(),
                    wrapper_algorithm: xmtp_proto::xmtp::mls::message_contents::WelcomePointerWrapperAlgorithm::XwingMlkem768Draft6.into(),
                },
            ),
        ),
    };

    alix.context
        .api_client
        .send_welcome_messages(&[welcome_message])
        .await
        .unwrap();

    // This should return an error because it's another welcome pointer.
    resolve_welcome_pointer(&welcome_pointer, &alix.context)
        .await
        .unwrap_err();
}

/// Anyone who learns a pointee's destination can publish to it, and the
/// backend accepts an undecodable row there, so only the lowest-sequence row
/// counts. After the real pointee the junk row changes nothing; before it, it
/// is the pointee, and the pointer is rejected at once.
// verifies: JOIN-032, JOIN-048
#[xmtp_common::timeout(Duration::from_secs(30))]
#[rstest::rstest]
#[case::later_invalid_pointee_does_not_block_the_first(false)]
#[case::malformed_first_pointee_is_rejected(true)]
#[xmtp_common::test]
async fn only_the_first_pointee_row_counts(#[case] junk_first: bool) {
    use crate::groups::GroupError;
    use crate::groups::welcome_sync::{WelcomeHeadOutcome, WelcomeService};
    use xmtp_proto::backend_v1::welcome_message::{V1, Version};

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let group = crate::groups::MlsGroup::create_and_insert(
        alix.context.clone(),
        xmtp_db::group::ConversationType::Group,
        crate::groups::group_permissions::PolicySet::default(),
        xmtp_mls_group_state::group::GroupMetadataOptions::default(),
        None,
    )
    .unwrap();
    group.sync().await.unwrap();
    let (welcome_message, join_anchor) =
        publish_commit_without_welcome(&group, bo.inbox_id()).await;

    let pointer = random_pointer();
    let pointee = sealed_pointee(&pointer, &welcome_message, join_anchor);
    let junk = xmtp_proto::backend_v1::WelcomeMessage {
        version: Some(Version::V1(V1 {
            installation_key: pointer.destination.clone(),
            data: vec![],
            hpke_public_key: vec![],
            wrapper_algorithm: i32::MAX,
            welcome_metadata: vec![],
        })),
    };
    let mut rows = [pointee, junk];
    if junk_first {
        rows.reverse();
    }
    for row in rows {
        alix.context
            .api()
            .send_welcome_messages(&[row])
            .await
            .unwrap();
    }
    let welcome = publish_pointer(&alix, &bo, &pointer).await;

    let outcome = WelcomeService::new(bo.context.clone())
        .resolve_pending_welcome(welcome.cursor)
        .await
        .unwrap();
    let db = bo.context.db();
    if junk_first {
        assert!(matches!(
            outcome,
            WelcomeHeadOutcome::Progress {
                result: Err(GroupError::ConversionError(_)),
                ..
            }
        ));
        bo.assert_newest_welcome_rejected().await;
        assert!(db.find_group(&group.group_id).unwrap().is_none());
    } else {
        assert!(matches!(
            outcome,
            WelcomeHeadOutcome::Progress {
                result: Ok(Some(_)),
                ..
            }
        ));
        assert!(
            db.pending_envelope(&bo.welcome_topic(), welcome.cursor)
                .unwrap()
                .is_none()
        );
        assert!(db.find_group(&group.group_id).unwrap().is_some());
    }
}

/// One of the two public keys a post-quantum key package is reachable through.
#[derive(Clone, Copy)]
enum PackageKey {
    Init,
    PostQuantum,
}

/// A Welcome or welcome pointer is opened only under the algorithm and key its
/// key package advertised, and a pointee only in the symmetric-key form.
///
/// Every input is encrypted to the key matching its stated algorithm, so it
/// decrypts under code that picks the private key from the stated algorithm
/// alone; each rejection therefore comes from the advertisement check, not from
/// a decryption failure. A classical wrapper over a post-quantum package is the
/// downgrade a party able to break X25519 would forge. Pointer wrappers admit no
/// classical algorithm on the wire, so their decryptable mismatch is the
/// post-quantum wrapper addressed through the package's classical init key.
// verifies: JOIN-076, JOIN-056
#[rstest::rstest]
#[case::classical_package(false, WrapperAlgorithm::Curve25519, PackageKey::Init, None, None)]
#[case::post_quantum_package(
    true,
    WrapperAlgorithm::XWingMLKEM768Draft6,
    PackageKey::PostQuantum,
    None,
    None
)]
#[case::classical_wrapper_through_init_key_of_post_quantum_package(
    true,
    WrapperAlgorithm::Curve25519,
    PackageKey::Init,
    None,
    Some(ADVERTISEMENT)
)]
#[case::classical_wrapper_through_post_quantum_key(
    true,
    WrapperAlgorithm::Curve25519,
    PackageKey::PostQuantum,
    None,
    Some(ADVERTISEMENT)
)]
#[case::post_quantum_wrapper_through_init_key(
    true,
    WrapperAlgorithm::XWingMLKEM768Draft6,
    PackageKey::Init,
    None,
    Some(ADVERTISEMENT)
)]
#[case::pointer(
    true,
    WrapperAlgorithm::XWingMLKEM768Draft6,
    PackageKey::PostQuantum,
    Some(WelcomeWrapperAlgorithm::SymmetricKey),
    None
)]
#[case::pointer_through_init_key(
    true,
    WrapperAlgorithm::XWingMLKEM768Draft6,
    PackageKey::Init,
    Some(WelcomeWrapperAlgorithm::SymmetricKey),
    Some(ADVERTISEMENT)
)]
#[case::pointee_with_wrong_wrapper_field(
    true,
    WrapperAlgorithm::XWingMLKEM768Draft6,
    PackageKey::PostQuantum,
    Some(WelcomeWrapperAlgorithm::XwingMlkem768Draft6),
    Some("WelcomePointee.wrapper_algorithm")
)]
#[xmtp_common::test(unwrap_try = true)]
async fn welcome_wrapper_matches_advertisement(
    #[case] post_quantum_package: bool,
    #[case] stated: WrapperAlgorithm,
    #[case] addressed: PackageKey,
    #[case] pointee_wrapper: Option<WelcomeWrapperAlgorithm>,
    #[case] rejected_by: Option<&'static str>,
) {
    use crate::groups::welcome_sync::{
        WelcomeHeadOutcome, WelcomeService, pending_welcome_for_test,
    };
    use xmtp_proto::backend_v1::welcome_message::{V1, Version, WelcomePointer};

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let group = alix.create_group(None, None).unwrap();
    let (welcome_message, join_anchor) =
        publish_commit_without_welcome(&group, bo.inbox_id()).await;
    let package = xmtp_mls_identity::identity_lifecycle::new_key_package(
        bo.context.identity(),
        &bo.context.mls_provider(),
        post_quantum_package,
    )
    .unwrap();
    let key = |key| match key {
        PackageKey::Init => package.key_package.hpke_init_key().as_slice().to_vec(),
        PackageKey::PostQuantum => package.pq_pub_key.clone().unwrap(),
    };
    let encrypted_to = key(match stated {
        WrapperAlgorithm::Curve25519 => PackageKey::Init,
        WrapperAlgorithm::XWingMLKEM768Draft6 => PackageKey::PostQuantum,
    });
    let metadata = WelcomeMetadata {
        message_cursor: join_anchor,
    }
    .encode_to_vec();
    let send = async |version| {
        alix.context
            .api()
            .send_welcome_messages(&[xmtp_proto::backend_v1::WelcomeMessage {
                version: Some(version),
            }])
            .await
            .unwrap()
    };

    let message = match pointee_wrapper {
        None => {
            let (data, welcome_metadata) = wrap_payload_hpke(
                &welcome_message,
                &metadata,
                &encrypted_to,
                stated,
                WELCOME_HPKE_LABEL,
            )
            .unwrap();
            Version::V1(V1 {
                installation_key: bo.context.installation_id().to_vec(),
                data,
                hpke_public_key: key(addressed),
                wrapper_algorithm: stated.into(),
                welcome_metadata,
            })
        }
        Some(pointee_wrapper) => {
            let pointer = WelcomeV1Pointer {
                destination: xmtp_common::rand_vec::<32>(),
                aead_type: WelcomePointeeEncryptionAeadType::Chacha20Poly1305.into(),
                encryption_key: xmtp_common::rand_vec::<32>(),
                data_nonce: xmtp_common::rand_vec::<12>(),
                welcome_metadata_nonce: xmtp_common::rand_vec::<12>(),
            };
            let seal = |plaintext: &[u8], nonce: &[u8]| {
                wrap_payload_symmetric(
                    plaintext,
                    WelcomePointersExtension::preferred_type(),
                    &pointer.encryption_key,
                    nonce,
                )
                .unwrap()
            };
            send(Version::V1(V1 {
                installation_key: pointer.destination.clone(),
                data: seal(&welcome_message, &pointer.data_nonce),
                hpke_public_key: vec![],
                wrapper_algorithm: pointee_wrapper.into(),
                welcome_metadata: seal(&metadata, &pointer.welcome_metadata_nonce),
            }))
            .await;
            let pointer = WelcomePointerProto {
                version: Some(
                    xmtp_proto::xmtp::mls::message_contents::welcome_pointer::Version::WelcomeV1Pointer(
                        pointer,
                    ),
                ),
            };
            Version::WelcomePointer(WelcomePointer {
                installation_key: bo.context.installation_id().to_vec(),
                welcome_pointer: wrap_payload_hpke(
                    &pointer.encode_to_vec(),
                    &[],
                    &encrypted_to,
                    stated,
                    WELCOME_HPKE_LABEL,
                )
                .unwrap()
                .0,
                hpke_public_key: key(addressed),
                wrapper_algorithm: stated.into(),
            })
        }
    };
    send(message).await;

    let welcome = alix
        .context
        .api()
        .query_welcome_messages(bo.context.installation_id())
        .await
        .unwrap()
        .pop()
        .unwrap();
    pending_welcome_for_test(&bo.context, &welcome)
        .await
        .unwrap();
    let WelcomeHeadOutcome::Progress { result, .. } = WelcomeService::new(bo.context.clone())
        .resolve_pending_welcome(welcome.cursor)
        .await
        .unwrap()
    else {
        panic!("the Welcome neither joined nor was rejected");
    };
    match (result, rejected_by) {
        (Ok(Some(joined)), None) => assert_eq!(joined.group_id, group.group_id),
        (
            Err(crate::groups::GroupError::ConversionError(
                xmtp_proto::ConversionError::InvalidValue { item, .. },
            )),
            Some(check),
        ) => assert_eq!(item, check),
        (result, expected) => panic!(
            "expected rejection by {expected:?}, got {:?}",
            result.map(|joined| joined.map(|group| group.group_id))
        ),
    }
}

/// The rejection raised by the advertised-wrapper check.
const ADVERTISEMENT: &str = "WelcomeMessage.wrapper_algorithm";
