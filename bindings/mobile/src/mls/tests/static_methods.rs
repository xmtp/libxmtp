//! Tests for static API methods that don't require a client instance

use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_static_revoke_installations() {
    let wallet = PrivateKeySigner::random();

    let ident = wallet.identifier();
    let ffi_ident: FfiIdentifier = ident.clone().into();
    let api_backend = connect_to_backend_test().await;

    let client_1 = new_test_client_with_wallet(wallet.clone()).await;
    let client_2 = new_test_client_with_wallet(wallet.clone()).await;
    let _client_3 = new_test_client_with_wallet(wallet.clone()).await;
    let _client_4 = new_test_client_with_wallet(wallet.clone()).await;
    let _client_5 = new_test_client_with_wallet(wallet.clone()).await;

    let inbox_id = client_1.inbox_id();

    let client_1_state = client_1.inbox_state(true).await.unwrap();
    let client_2_state = client_2.inbox_state(true).await.unwrap();
    assert_eq!(client_1_state.installations.len(), 5);
    assert_eq!(client_2_state.installations.len(), 5);

    let revoke_request = revoke_installations(
        api_backend.clone(),
        ffi_ident,
        &inbox_id,
        vec![client_2.installation_id()],
    )
    .unwrap();

    revoke_request.add_wallet_signature(&wallet).await;
    apply_signature_request(api_backend.clone(), revoke_request)
        .await
        .unwrap();

    let client_1_state_after = client_1.inbox_state(true).await.unwrap();
    let client_2_state_after = client_2.inbox_state(true).await.unwrap();

    assert_eq!(client_1_state_after.installations.len(), 4);
    assert_eq!(client_2_state_after.installations.len(), 4);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_static_revoke_fails_with_non_recovery_identity() {
    let wallet_a = PrivateKeySigner::random();
    let wallet_b = PrivateKeySigner::random();

    let client_a = new_test_client_with_wallet(wallet_a.clone()).await;
    let client_a2 = new_test_client_with_wallet(wallet_a.clone()).await;
    let inbox_id = client_a.inbox_id();

    let add_identity_request = client_a
        .add_identity(wallet_b.identifier().into())
        .await
        .unwrap();
    add_identity_request.add_wallet_signature(&wallet_b).await;
    client_a
        .apply_signature_request(add_identity_request)
        .await
        .unwrap();

    let client_a_state = client_a.inbox_state(true).await.unwrap();
    assert_eq!(client_a_state.installations.len(), 2);

    let ffi_ident: FfiIdentifier = wallet_b.identifier().into();
    let api_backend = connect_to_backend_test().await;

    let revoke_request = revoke_installations(
        api_backend.clone(),
        ffi_ident,
        &inbox_id,
        vec![client_a2.installation_id()],
    )
    .unwrap();

    revoke_request.add_wallet_signature(&wallet_b).await;
    let revoke_result = apply_signature_request(api_backend.clone(), revoke_request).await;

    assert!(
        revoke_result.is_err(),
        "Revocation should fail when using a non-recovery identity"
    );

    let client_a_state_after = client_a.inbox_state(true).await.unwrap();
    assert_eq!(client_a_state_after.installations.len(), 2);
}

#[tokio::test]
async fn test_can_get_inbox_state_statically() {
    let alix_wallet = PrivateKeySigner::random();
    let alix = new_test_client_no_panic(alix_wallet.clone(), None)
        .await
        .unwrap();
    let _alix2 = new_test_client_no_panic(alix_wallet.clone(), None)
        .await
        .unwrap();
    let _alix3 = new_test_client_no_panic(alix_wallet.clone(), None)
        .await
        .unwrap();

    let api_backend = connect_to_backend_test().await;

    let state = inbox_state_from_inbox_ids(api_backend, vec![alix.inbox_id()])
        .await
        .unwrap();
    assert_eq!(state[0].installations.len(), 3);
    assert_eq!(
        state[0].creation_signature_kind.clone().unwrap(),
        FfiSignatureKind::Erc191
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_get_newest_message_metadata() {
    let alix = new_test_client().await;
    let bo = new_test_client().await;

    // Create a group with alix and bo
    let group = alix
        .conversations()
        .create_group(
            vec![bo.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    // Send a message to the group
    group
        .send(b"Hello from alix".to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();

    let api_backend = connect_to_backend_test().await;

    // Get the latest message metadata for the group
    let metadata = get_newest_message_metadata(api_backend.clone(), vec![group.id()])
        .await
        .unwrap();

    assert_eq!(metadata.len(), 1, "Should have metadata for one group");
    let group_metadata = metadata.get(&group.id()).unwrap();
    assert!(
        group_metadata.created_ns > 0,
        "Message should have a valid timestamp"
    );

    // Send another message and verify the metadata updates
    group
        .send(b"Second message".to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();

    let updated_metadata = get_newest_message_metadata(api_backend.clone(), vec![group.id()])
        .await
        .unwrap();

    let updated_group_metadata = updated_metadata.get(&group.id()).unwrap();
    assert!(
        updated_group_metadata.created_ns >= group_metadata.created_ns,
        "Updated metadata should have same or later timestamp"
    );

    // Test with a group that has no messages (new empty group)
    let empty_group = alix
        .conversations()
        .create_group(vec![], FfiCreateGroupOptions::default())
        .await
        .unwrap();

    let empty_metadata =
        get_newest_message_metadata(api_backend.clone(), vec![empty_group.id()]).await;

    // Empty group may or may not have metadata depending on implementation
    // Just verify the call doesn't error
    assert!(empty_metadata.is_ok(), "Should not error for empty group");
}
