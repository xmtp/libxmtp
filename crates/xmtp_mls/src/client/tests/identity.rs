use super::*;

#[xmtp_common::test]
fn test_client_error_signature_validation_retryability_propagates() {
    use xmtp_common::RetryableError;
    use xmtp_id::associations::signature::SignatureError;
    use xmtp_id::scw_verifier::VerifierError;

    // A retryable verifier error (transient RPC failure) must surface as
    // retryable at the ClientError layer so the welcome sync path does not
    // advance the cursor past welcomes involving SCW users. See xmtp/libxmtp#3394.
    let retryable = crate::client::ClientError::SignatureValidation(SignatureError::VerifierError(
        VerifierError::NoVerifier("eip155:1".to_string()),
    ));
    assert!(retryable.is_retryable());

    // A terminal verifier error (malformed input) must remain non-retryable
    // so we don't spin forever on bad data.
    let non_retryable = crate::client::ClientError::SignatureValidation(
        SignatureError::VerifierError(VerifierError::MalformedEipUrl),
    );
    assert!(!non_retryable.is_retryable());
}

#[xmtp_common::test]
async fn test_mls_error() {
    tester!(client);
    let result = client.context.api().upload_key_package(vec![1, 2, 3]).await;

    assert!(result.is_err());
    assert!(matches!(
        result,
        Err(xmtp_api::ApiError::InvalidEnvelope(_))
    ));
}

#[xmtp_common::test]
async fn test_register_installation() {
    tester!(client);
    tester!(client_2);
    // Make sure the installation is actually on the network
    let association_state = client_2
        .identity_updates()
        .get_latest_association_state(&client_2.context.db(), client.inbox_id())
        .await
        .unwrap();

    assert_eq!(association_state.installation_ids().len(), 1);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(
    not(target_arch = "wasm32"),
    tokio::test(flavor = "multi_thread", worker_threads = 1)
)]
async fn test_rotate_key_package() {
    tester!(client);

    let installation_public_key = client.installation_public_key().to_vec();
    // Get original KeyPackage.
    let mut kp1 = client
        .get_key_packages_for_installation_ids(vec![installation_public_key.clone()])
        .await
        .unwrap();
    assert_eq!(kp1.len(), 1);
    let binding = kp1.remove(&installation_public_key).unwrap().unwrap();
    let init1 = binding.inner.hpke_init_key();
    let fetched_identity: StoredIdentity = client.context.db().fetch(&()).unwrap().unwrap();
    assert!(fetched_identity.next_key_package_rotation_ns.is_some());
    // Rotate and fetch again.
    client.queue_key_rotation().unwrap();
    //check the rotation value has been set
    let fetched_identity: StoredIdentity = client.context.db().fetch(&()).unwrap().unwrap();
    assert!(fetched_identity.next_key_package_rotation_ns.is_some());

    xmtp_common::time::sleep(std::time::Duration::from_secs(11)).await;

    let mut kp2 = client
        .get_key_packages_for_installation_ids(vec![installation_public_key.clone()])
        .await
        .unwrap();
    assert_eq!(kp2.len(), 1);
    let binding = kp2.remove(&installation_public_key).unwrap().unwrap();
    let init2 = binding.inner.hpke_init_key();

    assert_ne!(init1, init2);
}

#[xmtp_common::test]
async fn test_find_inbox_id() {
    tester!(client);
    assert_eq!(
        client
            .find_inbox_id_from_identifier(&client.context.db(), client.identifier())
            .await
            .unwrap(),
        Some(client.inbox_id().to_string())
    );
}

#[xmtp_common::test]
async fn test_key_package_rotation() {
    let alix_wallet = generate_local_wallet();
    let bo_wallet = generate_local_wallet();
    let alix = ClientBuilder::new_test_client(&alix_wallet).await;
    let bo = ClientBuilder::new_test_client(&bo_wallet).await;

    let alix_original_init_key = get_key_package_init_key(&alix, alix.installation_public_key())
        .await
        .unwrap();
    let bo_original_init_key = get_key_package_init_key(&bo, bo.installation_public_key())
        .await
        .unwrap();

    let alix_fetched_identity: StoredIdentity = alix.context.db().fetch(&()).unwrap().unwrap();
    assert!(alix_fetched_identity.next_key_package_rotation_ns.is_some());
    let bo_fetched_identity: StoredIdentity = bo.context.db().fetch(&()).unwrap().unwrap();
    assert!(bo_fetched_identity.next_key_package_rotation_ns.is_some());
    // Bo's original key should be deleted
    let bo_original_from_db = bo
        .db()
        .find_key_package_history_entry_by_hash_ref(bo_original_init_key.clone());
    assert!(bo_original_from_db.is_ok());

    alix.create_group_with_identifiers(&[bo_wallet.identifier()], None, None)
        .await
        .unwrap();
    let bo_keys_queued_for_rotation = bo.context.db().is_identity_needs_rotation().unwrap();
    assert!(!bo_keys_queued_for_rotation);

    bo.sync_welcomes().await.unwrap();

    //check the rotation value has been set and less than Queue rotation interval
    let bo_fetched_identity: StoredIdentity = bo.context.db().fetch(&()).unwrap().unwrap();
    assert!(bo_fetched_identity.next_key_package_rotation_ns.is_some());
    let updated_at = bo
        .context
        .db()
        .key_package_rotation_history()
        .into_iter()
        .map(|(_, updated_at)| updated_at)
        .next_back()
        .unwrap();
    assert!(bo_fetched_identity.next_key_package_rotation_ns.unwrap() - updated_at < 5 * NS_IN_SEC);

    //check original keys must not be marked to be deleted
    let bo_keys = bo
        .context
        .db()
        .find_key_package_history_entry_by_hash_ref(bo_original_init_key.clone());
    assert!(bo_keys.unwrap().delete_at_ns.is_none());
    //wait for worker to rotate the keypackage
    xmtp_common::time::sleep(std::time::Duration::from_secs(11)).await;
    //check the rotation queue must be cleared
    let bo_keys_queued_for_rotation = bo.context.db().is_identity_needs_rotation().unwrap();
    assert!(!bo_keys_queued_for_rotation);

    let bo_fetched_identity: StoredIdentity = bo.context.db().fetch(&()).unwrap().unwrap();
    assert!(bo_fetched_identity.next_key_package_rotation_ns.unwrap() > 0);

    let bo_new_key = get_key_package_init_key(&bo, bo.installation_public_key())
        .await
        .unwrap();
    // Bo's key should have changed
    assert_ne!(bo_original_init_key, bo_new_key);

    // Depending on timing, old key should already be deleted, or marked to be deleted
    let bo_keys = bo
        .context
        .db()
        .find_key_package_history_entry_by_hash_ref(bo_original_init_key.clone())
        .ok();
    if let Some(key) = bo_keys {
        assert!(key.delete_at_ns.is_some());
    }

    xmtp_common::time::sleep(std::time::Duration::from_secs(10)).await;
    let bo_keys = bo
        .context
        .db()
        .find_key_package_history_entry_by_hash_ref(bo_original_init_key.clone());
    assert!(bo_keys.is_err());

    bo.sync_welcomes().await.unwrap();
    let bo_new_key_2 = get_key_package_init_key(&bo, bo.installation_public_key())
        .await
        .unwrap();
    // Bo's key should not have changed syncing the second time.
    assert_eq!(bo_new_key, bo_new_key_2);

    let alix_keys_queued_for_rotation = alix.context.db().is_identity_needs_rotation().unwrap();
    assert!(!alix_keys_queued_for_rotation);

    alix.sync_welcomes().await.unwrap();
    let alix_key_2 = get_key_package_init_key(&alix, alix.installation_public_key())
        .await
        .unwrap();

    // Alix's key should not have changed at all
    assert_eq!(alix_original_init_key, alix_key_2);

    alix.create_group_with_identifiers(&[bo_wallet.identifier()], None, None)
        .await
        .unwrap();
    bo.sync_welcomes().await.unwrap();

    // Bo should have two groups now
    let bo_groups = bo.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bo_groups.len(), 2);

    // Bo's original key should be deleted
    let bo_original_after_delete = bo
        .db()
        .find_key_package_history_entry_by_hash_ref(bo_original_init_key);
    assert!(bo_original_after_delete.is_err());
}

#[xmtp_common::test]
async fn test_find_or_create_dm_by_inbox_id() {
    let user1 = generate_local_wallet();
    let user2 = generate_local_wallet();
    let client1 = ClientBuilder::new_test_client(&user1).await;
    let client2 = ClientBuilder::new_test_client(&user2).await;

    // First call should create a new DM
    let dm1 = client1
        .find_or_create_dm(client2.inbox_id().to_string(), None)
        .await
        .unwrap();

    // Verify DM was created with correct properties
    let metadata = dm1.metadata().await.unwrap();
    assert_eq!(
        metadata.dm_members.clone().unwrap().member_one_inbox_id,
        client1.inbox_id()
    );
    assert_eq!(
        metadata.dm_members.unwrap().member_two_inbox_id,
        client2.inbox_id()
    );

    // Second call should find the existing DM
    let dm2 = client1
        .find_or_create_dm(client2.inbox_id().to_string(), None)
        .await
        .unwrap();

    // Verify we got back the same DM
    assert_eq!(dm1.group_id, dm2.group_id);
    assert_eq!(dm1.created_at_ns, dm2.created_at_ns);

    // Verify the DM appears in conversations list
    let conversations = client1.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(conversations.len(), 1);
    assert_eq!(conversations[0].group_id, dm1.group_id);
}
