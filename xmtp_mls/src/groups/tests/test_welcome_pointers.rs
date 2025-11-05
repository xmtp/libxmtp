use crate::context::XmtpSharedContext;
use crate::groups::mls_ext::{
    WelcomePointersExtension, WrapperAlgorithm, unwrap_welcome_symmetric, wrap_welcome,
    wrap_welcome_symmetric,
};
use crate::groups::welcome_pointer::resolve_welcome_pointer;
use crate::identity::ENABLE_WELCOME_POINTERS;
use crate::tester;
use crate::utils::test::TestMlsGroup;
use futures::StreamExt;
use prost::Message;
use std::time::Duration;
use xmtp_db::group::QueryGroup;
use xmtp_db::tasks::QueryTasks;
use xmtp_proto::mls_v1::WelcomeMetadata;
use xmtp_proto::types::{DecryptedWelcomePointer, WelcomeMessage, WelcomeMessageType};
use xmtp_proto::xmtp::mls::message_contents::welcome_pointer::WelcomeV1Pointer;
use xmtp_proto::xmtp::mls::message_contents::{
    WelcomePointeeEncryptionAeadType, WelcomePointer as WelcomePointerProto,
    WelcomePointerWrapperAlgorithm,
};

#[rstest::rstest]
#[xmtp_common::test(unwrap_try = true)]
#[timeout(Duration::from_secs(40))]
async fn test_welcome_pointer_round_trip_with_welcome_pointers() {
    test_welcome_pointer_round_trip(
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
    )
    .await;
}

#[rstest::rstest]
#[xmtp_common::test(unwrap_try = true)]
#[timeout(Duration::from_secs(40))]

async fn test_welcome_pointer_round_trip_without_welcome_pointers() {
    test_welcome_pointer_round_trip(
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
    )
    .await;
}

#[rstest::rstest]
#[xmtp_common::test(unwrap_try = true)]
#[timeout(Duration::from_secs(40))]

async fn test_welcome_pointer_round_trip_with_random_mix_of_welcome_pointers() {
    let random_vec = xmtp_common::rand_vec::<1024>();
    let mut random_vec = random_vec
        .into_iter()
        .map(|b| b & 1 == 1)
        .collect::<Vec<_>>();
    // set the first n to true to ensure that welcome pointers are used when updating installations.
    // it's required to not have random test failures (where the first 9 calls don't have 2 or more set to true)
    random_vec
        .iter_mut()
        .take(xmtp_configuration::INSTALLATION_THRESHOLD_FOR_WELCOME_POINTER_SENDING)
        .for_each(|b| *b = true);
    let mut gen_count = random_vec.len();
    let mut assert_count = random_vec.len();
    test_welcome_pointer_round_trip(
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
    )
    .await;
}

// This test works great as long as the calls to enable_extension are done in the same order as the calls to assertion.
async fn test_welcome_pointer_round_trip(
    mut enable_extension: impl FnMut() -> bool,
    mut assertion: impl AsyncFnMut(&[WelcomeMessage]) -> Result<(), String>,
) {
    // Create two test clients
    tester!(alix);
    // Have to skip managing the extension for bola, because this installations is always added to the group with a V1 welcome message.
    tester!(bola);

    // Create a group with alix as the creator
    let alix_group = alix.create_group(None, None).unwrap();
    tracing::info!("Alix group id: {}", hex::encode(&alix_group.group_id));
    alix_group.sync().await.unwrap();

    // Add bola to the group - this should trigger welcome message creation
    alix_group
        .add_members_by_inbox_id(&[bola.inbox_id()])
        .await
        .unwrap();

    // Sync the group to ensure the welcome is sent
    alix_group.sync().await.unwrap();

    // Bola should receive the welcome message
    bola.sync_welcomes().await.unwrap();

    // Verify bola can see the group
    let bola_groups = bola
        .find_groups(xmtp_db::group::GroupQueryArgs::default())
        .unwrap();
    assert_eq!(bola_groups.len(), 1);

    let bola_group: &TestMlsGroup = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();

    // Create additional installations for both clients to simulate the scenario
    // where welcome pointers are used (when there are more than 2 capable installations)
    let mut extra_installations = Vec::new();
    for _ in 0..9 {
        ENABLE_WELCOME_POINTERS
            .scope(enable_extension(), async {
                extra_installations.push(bola.new_installation().await);
            })
            .await;
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
        ENABLE_WELCOME_POINTERS
            .scope(enable_extension(), async {
                tester!(charlie);
                testers.push(charlie);
            })
            .await;
    }
    // 10 is max installations per inbox. The `testers` vec contains one for each tester
    for _ in 0..9 {
        for tester in testers.iter() {
            ENABLE_WELCOME_POINTERS
                .scope(enable_extension(), async {
                    extra_installations.push(tester.new_installation().await);
                })
                .await;
        }
    }
    alix_group
        .add_members_by_inbox_id(testers.iter().map(|i| i.inbox_id()).collect::<Vec<_>>())
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
    let welcomes = futures::future::join_all(
        testers
            .into_iter()
            .chain(extra_installations.into_iter())
            .map(|tester| async {
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
            }),
    )
    .await;

    for welcome in welcomes {
        assertion(&welcome).await.unwrap();
    }

    // The test has successfully demonstrated welcome pointer functionality
    // by creating a group with multiple members and installations
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
        wrap_welcome_symmetric(&welcome_data, *aead_type, &symmetric_key, &data_nonce).unwrap();
    let encrypted_welcome_metadata = wrap_welcome_symmetric(
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
    let decrypted_welcome_data = unwrap_welcome_symmetric(
        &encrypted_welcome_data,
        *aead_type,
        &symmetric_key,
        &data_nonce,
    )
    .unwrap();
    let decrypted_welcome_metadata = unwrap_welcome_symmetric(
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

#[rstest::rstest]
#[xmtp_common::test(unwrap_try = true)]
#[timeout(Duration::from_secs(20))]
async fn test_welcome_pointer_resolution_for_no_destination() {
    // This test would require mocking the API client to test the resolution logic
    // For now, we'll test the basic structure and error handling

    tester!(alix);

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

#[xmtp_common::test(unwrap_try = true)]
async fn test_welcome_pointer_resolution_to_another_welcome_pointer() {
    tester!(alix);

    let welcome_pointer = DecryptedWelcomePointer {
        destination: xmtp_common::rand_array::<32>().into(),
        aead_type: WelcomePointeeEncryptionAeadType::Chacha20Poly1305,
        encryption_key: xmtp_common::rand_vec::<32>(),
        data_nonce: xmtp_common::rand_vec::<12>(),
        welcome_metadata_nonce: xmtp_common::rand_vec::<12>(),
    };

    let welcome_message = xmtp_proto::xmtp::mls::api::v1::WelcomeMessageInput {
        version: Some(
            xmtp_proto::xmtp::mls::api::v1::welcome_message_input::Version::WelcomePointer(
                xmtp_proto::xmtp::mls::api::v1::welcome_message_input::WelcomePointer {
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

#[rstest::rstest]
#[xmtp_common::test(unwrap_try = true)]
#[timeout(Duration::from_secs(40))]
async fn test_welcome_pointer_task_retry_resolution() {
    tester!(alix);
    tester!(bo);

    // create new key package with post quantum key for bo
    let bo_key_package = bo
        .context
        .identity()
        .new_key_package(&bo.context.mls_provider(), true)
        .unwrap();

    let bo_hpke_public_key = bo_key_package.pq_pub_key.as_deref().unwrap();

    tracing::info!("Creating welcome pointer");
    let welcome_pointer_v1 = WelcomeV1Pointer {
        destination: xmtp_common::rand_vec::<32>(),
        aead_type: WelcomePointeeEncryptionAeadType::Chacha20Poly1305.into(),
        encryption_key: xmtp_common::rand_vec::<32>(),
        data_nonce: xmtp_common::rand_vec::<12>(),
        welcome_metadata_nonce: xmtp_common::rand_vec::<12>(),
    };

    let welcome_pointer = WelcomePointerProto {
        version: Some(
            xmtp_proto::xmtp::mls::message_contents::welcome_pointer::Version::WelcomeV1Pointer(
                welcome_pointer_v1.clone(),
            ),
        ),
    };

    let welcome_pointer_encrypted_bytes = wrap_welcome(
        &welcome_pointer.encode_to_vec(),
        &[],
        bo_hpke_public_key,
        WrapperAlgorithm::XWingMLKEM768Draft6,
    )
    .unwrap()
    .0;

    tracing::info!("Sending welcome pointer to nowhere to bo");
    alix.context.api()
        .send_welcome_messages(&[xmtp_proto::xmtp::mls::api::v1::WelcomeMessageInput {
            version: Some(xmtp_proto::xmtp::mls::api::v1::welcome_message_input::Version::WelcomePointer(xmtp_proto::xmtp::mls::api::v1::welcome_message_input::WelcomePointer {
                installation_key: bo.context.installation_id().to_vec(),
                welcome_pointer: welcome_pointer_encrypted_bytes.clone(),
                hpke_public_key: bo_hpke_public_key.to_vec(),
                wrapper_algorithm: xmtp_proto::xmtp::mls::message_contents::WelcomePointerWrapperAlgorithm::XwingMlkem768Draft6.into(),
            })),
        }])
        .await
        .unwrap();

    tracing::info!("Querying welcome messages for bo");
    // TODO: Use a call that gives control over caching. Using alix because the cache isn't interfering.
    let welcomes = alix
        .context
        .api()
        .query_welcome_messages(bo.context.installation_id())
        .await
        .unwrap();

    tracing::info!("Verifying welcome messages for bo");
    assert!(!welcomes.is_empty());
    assert_eq!(welcomes.len(), 1);
    let welcome_from_api = welcomes.into_iter().next().unwrap();

    let WelcomeMessageType::WelcomePointer(welcome_pointer_from_api) = &welcome_from_api.variant
    else {
        panic!("Welcome message is not a welcome pointer");
    };

    assert_eq!(
        welcome_pointer_from_api.installation_key,
        bo.context.installation_id()
    );
    assert_eq!(
        welcome_pointer_from_api.welcome_pointer,
        welcome_pointer_encrypted_bytes
    );
    assert_eq!(welcome_pointer_from_api.hpke_public_key, bo_hpke_public_key);
    assert_eq!(
        welcome_pointer_from_api.wrapper_algorithm,
        WelcomePointerWrapperAlgorithm::XwingMlkem768Draft6
    );

    tracing::info!("Syncing welcomes for bo");
    let welcomes = bo.sync_welcomes().await.unwrap();
    assert!(welcomes.is_empty());

    // Have to give time for the task to be received by the task worker
    xmtp_common::time::sleep(std::time::Duration::from_secs(1)).await;

    tracing::info!("Getting tasks for bo");
    let tasks = bo.context.db().get_tasks().unwrap();
    assert_eq!(tasks.len(), 1, "{tasks:#?}");
    let task = tasks.into_iter().next().unwrap();
    assert_eq!(
        task.data,
        xmtp_proto::xmtp::mls::database::Task {
            task: Some(
                xmtp_proto::xmtp::mls::database::task::Task::ProcessWelcomePointer(
                    welcome_pointer.clone()
                )
            )
        }
        .encode_to_vec()
    );
    assert_eq!(task.id, 1);
    assert_eq!(
        task.originating_message_sequence_id,
        welcome_from_api.sequence_id() as i64
    );
    assert_eq!(
        task.originating_message_originator_id,
        welcome_from_api.originator_id() as i32
    );
    assert_eq!(task.created_at_ns, welcome_from_api.timestamp());
    tracing::info!("Asserted tasks for bo are correct");

    let group = crate::groups::MlsGroup::create_and_insert(
        alix.context.clone(),
        xmtp_db::group::ConversationType::Group,
        crate::groups::group_permissions::PolicySet::default(),
        xmtp_mls_common::group::GroupMetadataOptions::default(),
        None,
    )
    .unwrap();

    // Have to sync the group otherwise bo won't find it and it won't get created
    group.sync().await.unwrap();

    tracing::info!("Creating welcome for group");
    // Now we send a welcome from this group to bo. To get the delay we want,
    // we reach into some internals.
    let intent = group
        .get_membership_update_intent(&[bo.inbox_id()], &[])
        .await?;
    let signer = &group.context.identity().installation_keys;
    let context = &group.context;
    let send_welcome_action = group
        .load_mls_group_with_lock_async(|mut openmls_group| async move {
            let publish_intent_data =
                crate::groups::mls_sync::update_group_membership::apply_update_group_membership_intent(&context, &mut openmls_group, intent, signer)
                    .await?
                    .unwrap();
            let post_commit_action = crate::groups::intents::PostCommitAction::from_bytes(
                publish_intent_data.post_commit_data().unwrap().as_slice(),
            )?;
            let crate::groups::intents::PostCommitAction::SendWelcomes(action) = post_commit_action;
            let staged_commit = publish_intent_data.staged_commit().unwrap();
            openmls_group.merge_staged_commit(
                &xmtp_db::XmtpOpenMlsProviderRef::new(context.mls_storage()),
                crate::groups::mls_sync::decode_staged_commit(staged_commit.as_slice())?,
            )?;

            Ok::<_, crate::groups::GroupError>(action)
        })
        .await?;
    let data = wrap_welcome_symmetric(
        &send_welcome_action.welcome_message,
        WelcomePointersExtension::preferred_type(),
        &welcome_pointer_v1.encryption_key,
        &welcome_pointer_v1.data_nonce,
    )
    .unwrap();
    let welcome_metadata = wrap_welcome_symmetric(
        WelcomeMetadata { message_cursor: 0 }
            .encode_to_vec()
            .as_slice(),
        WelcomePointersExtension::preferred_type(),
        &welcome_pointer_v1.encryption_key,
        &welcome_pointer_v1.welcome_metadata_nonce,
    )
    .unwrap();

    let welcome_data = xmtp_proto::xmtp::mls::api::v1::WelcomeMessageInput {
        version: Some(
            xmtp_proto::xmtp::mls::api::v1::welcome_message_input::Version::V1(
                xmtp_proto::xmtp::mls::api::v1::welcome_message_input::V1 {
                    installation_key:welcome_pointer_v1.destination.clone(),
                    data,
                    hpke_public_key:bo_hpke_public_key.to_vec(),
                    wrapper_algorithm: xmtp_proto::xmtp::mls::message_contents::WelcomePointerWrapperAlgorithm::XwingMlkem768Draft6.into(),
                    welcome_metadata,
                }
            )
        ),
    };

    let mut events = bo.context.local_events().subscribe();
    let conversations = bo.stream_conversations(None, true).await.unwrap();
    tokio::pin!(conversations);

    // Let the task try once and fail
    // This is tied to the delay in the task retry logic, so changing this can cause some lines to not be tested.
    xmtp_common::time::sleep(std::time::Duration::from_secs(5)).await;

    tracing::info!("Sending welcome to where welcome pointer resolves to");
    alix.context
        .api()
        .send_welcome_messages(&[welcome_data])
        .await
        .unwrap();

    // TODO subscribe to all messages and then assert that group is received.

    tracing::info!("Receiving event for new group");
    let event = xmtp_common::time::timeout(std::time::Duration::from_secs(10), events.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        event,
        crate::subscriptions::LocalEvents::NewGroup(group.group_id.clone())
    );

    tracing::info!("Finding group for bo");
    let bo_group = bo
        .find_groups(xmtp_db::group::GroupQueryArgs::default())
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    assert_eq!(bo_group.group_id, group.group_id);

    let stored_group = bo
        .context
        .db()
        .find_group_by_sequence_id(welcome_from_api.cursor)
        .unwrap()
        .unwrap();
    assert_eq!(stored_group.id, bo_group.group_id);

    tracing::info!("Verifying message is received in conversation stream");
    let conversation_group =
        xmtp_common::time::timeout(std::time::Duration::from_secs(10), conversations.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    assert_eq!(conversation_group.group_id, bo_group.group_id);
}
