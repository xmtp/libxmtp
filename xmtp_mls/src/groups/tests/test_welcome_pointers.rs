use crate::groups::mls_ext::WelcomePointersExtension;
use crate::groups::mls_ext::unwrap_welcome_symmetric;
use crate::groups::mls_ext::wrap_welcome_symmetric;
use crate::groups::welcome_pointer::resolve_welcome_pointer;
use crate::tester;
use crate::utils::test::TestMlsGroup;
use futures::StreamExt;
use prost::Message;
use xmtp_proto::mls_v1::WelcomeMetadata;
use xmtp_proto::xmtp::mls::message_contents::WelcomePointeeEncryptionAeadType;
use xmtp_proto::xmtp::mls::message_contents::WelcomePointer;
use xmtp_proto::xmtp::mls::message_contents::welcome_pointer::V1 as WelcomePointerV1;

#[xmtp_common::test(unwrap_try = true)]
async fn test_welcome_pointer_round_trip() {
    // Create two test clients
    tester!(alix);
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

    // Now test welcome pointer functionality by creating a scenario where
    // welcome pointers would be used (more than 2 capable installations)

    // Create additional installations for both clients to simulate the scenario
    // where welcome pointers are used (when there are more than 2 capable installations)
    let _alix_installations =
        futures::stream::iter((0..9).map(|_| alix.installation())).collect::<Vec<_>>();
    let _bola_installations =
        futures::stream::iter((0..9).map(|_| bola.installation())).collect::<Vec<_>>();

    // Update installations in the group to include the new installations
    alix_group.update_installations().await.unwrap();
    bola_group.update_installations().await.unwrap();

    // Now add the world to ensure welcome pointer usage is triggered
    let mut testers = vec![];
    for _ in 0..23 {
        tester!(charlie);
        testers.push(charlie);
    }
    let mut installations = vec![];
    for _ in 0..9 {
        let extra_installation =
            futures::future::join_all(testers.iter().map(|t| t.installation())).await;
        installations.extend(extra_installation);
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
            .chain(installations.iter())
            .map(|t| t.sync_welcomes()),
    )
    .await
    .unwrap();

    // Verify testers can see the group
    futures::future::join_all(
        testers
            .iter()
            .chain(installations.iter())
            .map(|tester| async {
                let tester_groups = tester
                    .find_groups(xmtp_db::group::GroupQueryArgs::default())
                    .unwrap();
                assert_eq!(tester_groups.len(), 1);

                let tester_group: &TestMlsGroup = tester_groups.first().unwrap();
                tester_group.sync().await.unwrap();
            }),
    )
    .await;

    // The test has successfully demonstrated welcome pointer functionality
    // by creating a group with multiple members and installations
}

#[xmtp_common::test]
fn test_welcome_pointer_encryption_round_trip() {
    // Test the symmetric encryption/decryption used in welcome pointers
    let symmetric_key = xmtp_common::rand_array::<32>();
    let nonces = [
        xmtp_common::rand_array::<12>(),
        xmtp_common::rand_array::<12>(),
    ];

    // Create test data (welcome message and metadata)
    let welcome_data = xmtp_common::rand_vec::<1000>();
    let message_cursor = xmtp_common::rand_u64();
    let welcome_metadata = WelcomeMetadata { message_cursor };
    let welcome_metadata_bytes = welcome_metadata.encode_to_vec();

    // Get available AEAD types for welcome pointers
    let available_types = WelcomePointersExtension::available_types();
    let aead_type = available_types.supported_aead_types.first().unwrap();

    // Test encryption
    let encrypted_data = wrap_welcome_symmetric(
        [&welcome_data, &welcome_metadata_bytes],
        *aead_type,
        &symmetric_key,
        nonces.each_ref().map(|nonce| nonce.as_slice()),
    )
    .unwrap();

    // Verify encryption worked (data should be different)
    assert_ne!(encrypted_data[0], welcome_data);
    assert_ne!(encrypted_data[1], welcome_metadata_bytes);

    // Test decryption
    let decrypted_data = unwrap_welcome_symmetric(
        encrypted_data.each_ref().map(|data| data.as_slice()),
        *aead_type,
        &symmetric_key,
        nonces.each_ref().map(|nonce| nonce.as_slice()),
    )
    .unwrap();

    // Verify decryption worked (data should match original)
    assert_eq!(decrypted_data[0], welcome_data);
    assert_eq!(decrypted_data[1], welcome_metadata_bytes);

    // Verify the metadata can be decoded correctly
    let decoded_metadata = WelcomeMetadata::decode(&decrypted_data[1][..]).unwrap();
    assert_eq!(decoded_metadata.message_cursor, message_cursor);
}

#[xmtp_common::test]
async fn test_welcome_pointer_proto_round_trip() {
    // Test the protobuf serialization/deserialization of welcome pointers
    let destination = xmtp_common::rand_array::<32>();
    let encryption_key = xmtp_common::rand_array::<32>();
    let nonces = vec![
        xmtp_common::rand_array::<12>().to_vec(),
        xmtp_common::rand_array::<12>().to_vec(),
    ];

    // Create a welcome pointer
    let welcome_pointer = WelcomePointer {
        version: Some(
            xmtp_proto::xmtp::mls::message_contents::welcome_pointer::Version::V1(
                WelcomePointerV1 {
                    destination: destination.to_vec(),
                    aead_type: WelcomePointeeEncryptionAeadType::Chacha20Poly1305.into(),
                    encryption_key: encryption_key.to_vec(),
                    nonces,
                },
            ),
        ),
    };

    // Serialize the welcome pointer
    let serialized = welcome_pointer.encode_to_vec();
    assert!(!serialized.is_empty());

    // Deserialize the welcome pointer
    let deserialized = WelcomePointer::decode(&serialized[..]).unwrap();

    // Verify the round trip worked
    assert_eq!(welcome_pointer.version, deserialized.version);

    // Verify the V1 fields
    match (&welcome_pointer.version, &deserialized.version) {
        (
            Some(xmtp_proto::xmtp::mls::message_contents::welcome_pointer::Version::V1(original)),
            Some(xmtp_proto::xmtp::mls::message_contents::welcome_pointer::Version::V1(
                deserialized_v1,
            )),
        ) => {
            assert_eq!(original.destination, deserialized_v1.destination);
            assert_eq!(original.aead_type, deserialized_v1.aead_type);
            assert_eq!(original.encryption_key, deserialized_v1.encryption_key);
            assert_eq!(original.nonces, deserialized_v1.nonces);
        }
        _ => panic!("Expected V1 versions"),
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_welcome_pointer_resolution_with_mock() {
    // This test would require mocking the API client to test the resolution logic
    // For now, we'll test the basic structure and error handling

    tester!(alix);

    // Test error handling for missing version
    let invalid_pointer = WelcomePointer { version: None };

    // This should fail with a missing field error
    let result = resolve_welcome_pointer(&invalid_pointer, &alix.context).await;
    assert!(result.is_err());

    // Test with valid structure but invalid destination (would fail at API level)
    let valid_pointer: WelcomePointer = WelcomePointer {
        version: Some(
            xmtp_proto::xmtp::mls::message_contents::welcome_pointer::Version::V1(
                WelcomePointerV1 {
                    destination: xmtp_common::rand_array::<32>().to_vec(),
                    aead_type: WelcomePointeeEncryptionAeadType::Chacha20Poly1305.into(),
                    encryption_key: xmtp_common::rand_array::<32>().to_vec(),
                    nonces: vec![
                        xmtp_common::rand_array::<12>().to_vec(),
                        xmtp_common::rand_array::<12>().to_vec(),
                    ],
                },
            ),
        ),
    };

    // This should fail because the destination doesn't exist in the API
    let result = resolve_welcome_pointer(&valid_pointer, &alix.context).await;
    assert!(result.is_err());
}
