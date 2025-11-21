use super::*;

#[tokio::test]
async fn get_inbox_id() {
    let client = new_test_client().await;
    let ident = &client.account_identifier;
    let real_inbox_id = client.inbox_id();

    let api = connect_to_backend_test().await;
    let from_network = get_inbox_id_for_identifier(api, ident.clone())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(real_inbox_id, from_network);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_can_add_wallet_to_inbox() {
    // Setup the initial first client
    let ffi_inbox_owner = FfiWalletInboxOwner::new();
    let ident = ffi_inbox_owner.identifier();
    let nonce = 1;
    let inbox_id = ident.inbox_id(nonce).unwrap();

    let path = tmp_path();
    let key = static_enc_key().to_vec();
    let client = create_client(
        connect_to_backend_test().await,
        connect_to_backend_test().await,
        Some(path.clone()),
        Some(key),
        &inbox_id,
        ffi_inbox_owner.identifier(),
        nonce,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();

    let signature_request = client.signature_request().unwrap().clone();
    register_client_with_wallet(&ffi_inbox_owner, &client).await;

    signature_request
        .add_wallet_signature(&ffi_inbox_owner.wallet)
        .await;

    let conn = client.inner_client.context.store().db();
    let state = client
        .inner_client
        .identity_updates()
        .get_latest_association_state(&conn, &inbox_id)
        .await
        .expect("could not get state");

    assert_eq!(state.members().len(), 2);

    // Now, add the second wallet to the client
    let wallet_to_add = generate_local_wallet();
    let new_account_address = wallet_to_add.identifier();
    println!("second address: {}", new_account_address);

    let signature_request = client
        .add_identity(new_account_address.into())
        .await
        .expect("could not add wallet");

    signature_request.add_wallet_signature(&wallet_to_add).await;

    client
        .apply_signature_request(signature_request)
        .await
        .unwrap();

    let updated_state = client
        .inner_client
        .identity_updates()
        .get_latest_association_state(&conn, &inbox_id)
        .await
        .expect("could not get state");

    assert_eq!(updated_state.members().len(), 3);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_associate_passkey() {
    let alex = new_test_client().await;
    let passkey = PasskeyUser::new().await;

    let sig_request = alex
        .add_identity(passkey.identifier().into())
        .await
        .unwrap();
    let challenge = sig_request.signature_text().await.unwrap();
    let UnverifiedSignature::Passkey(sig) = passkey.sign(&challenge).unwrap() else {
        unreachable!("Should always be a passkey.")
    };
    sig_request
        .add_passkey_signature(FfiPasskeySignature {
            public_key: sig.public_key,
            signature: sig.signature,
            authenticator_data: sig.authenticator_data,
            client_data_json: sig.client_data_json,
        })
        .await
        .unwrap();

    alex.apply_signature_request(sig_request).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_can_revoke_wallet() {
    // Setup the initial first client
    let ffi_inbox_owner = FfiWalletInboxOwner::new();
    let nonce = 1;
    let ident = ffi_inbox_owner.identifier();
    let inbox_id = ident.inbox_id(nonce).unwrap();
    let path = tmp_path();
    let key = static_enc_key().to_vec();

    let client = create_client(
        connect_to_backend_test().await,
        connect_to_backend_test().await,
        Some(path.clone()),
        Some(key),
        &inbox_id,
        ffi_inbox_owner.identifier(),
        nonce,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();

    let signature_request = client.signature_request().unwrap().clone();
    register_client_with_wallet(&ffi_inbox_owner, &client).await;

    signature_request
        .add_wallet_signature(&ffi_inbox_owner.wallet)
        .await;

    let conn = client.inner_client.context.store().db();
    let state = client
        .inner_client
        .identity_updates()
        .get_latest_association_state(&conn, &inbox_id)
        .await
        .expect("could not get state");

    assert_eq!(state.members().len(), 2);

    // Now, add the second wallet to the client

    let wallet_to_add = generate_local_wallet();
    let new_account_address = wallet_to_add.identifier();
    println!("second address: {}", new_account_address);

    let signature_request = client
        .add_identity(new_account_address.into())
        .await
        .expect("could not add wallet");

    signature_request.add_wallet_signature(&wallet_to_add).await;

    client
        .apply_signature_request(signature_request.clone())
        .await
        .unwrap();

    let updated_state = client
        .inner_client
        .identity_updates()
        .get_latest_association_state(&conn, &inbox_id)
        .await
        .expect("could not get state");

    assert_eq!(updated_state.members().len(), 3);

    // Now, revoke the second wallet
    let signature_request = client
        .revoke_identity(wallet_to_add.identifier().into())
        .await
        .expect("could not revoke wallet");

    signature_request
        .add_wallet_signature(&ffi_inbox_owner.wallet)
        .await;

    client
        .apply_signature_request(signature_request)
        .await
        .unwrap();

    let revoked_state = client
        .inner_client
        .identity_updates()
        .get_latest_association_state(&conn, &inbox_id)
        .await
        .expect("could not get state");

    assert_eq!(revoked_state.members().len(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_invalid_external_signature() {
    let inbox_owner = FfiWalletInboxOwner::new();
    let ident = inbox_owner.identifier();
    let nonce = 1;
    let inbox_id = ident.inbox_id(nonce).unwrap();
    let path = tmp_path();

    let client = create_client(
        connect_to_backend_test().await,
        connect_to_backend_test().await,
        Some(path.clone()),
        None, // encryption_key
        &inbox_id,
        inbox_owner.identifier(),
        nonce,
        None, // v2_signed_private_key_proto
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();

    let signature_request = client.signature_request().unwrap();
    assert!(client.register_identity(signature_request).await.is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_sign_and_verify() {
    let signature_text = "Hello there.";

    let client = new_test_client().await;
    let signature_bytes = client.sign_with_installation_key(signature_text).unwrap();

    // check if verification works
    let result =
        client.verify_signed_with_installation_key(signature_text, signature_bytes.clone());
    assert!(result.is_ok());

    // different text should result in an error.
    let result = client.verify_signed_with_installation_key("Hello here.", signature_bytes);
    assert!(result.is_err());

    // different bytes should result in an error
    let signature_bytes = vec![0; 64];
    let result = client.verify_signed_with_installation_key(signature_text, signature_bytes);
    assert!(result.is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_revoke_all_installations() {
    let wallet = PrivateKeySigner::random();
    let client_1 = new_test_client_with_wallet(wallet.clone()).await;
    let client_2 = new_test_client_with_wallet(wallet.clone()).await;

    let client_1_state = client_1.inbox_state(true).await.unwrap();
    let client_2_state = client_2.inbox_state(true).await.unwrap();
    assert_eq!(client_1_state.installations.len(), 2);
    assert_eq!(client_2_state.installations.len(), 2);

    let Some(signature_request) = client_1
        .revoke_all_other_installations_signature_request()
        .await
        .unwrap()
    else {
        panic!("No signature request found");
    };

    signature_request.add_wallet_signature(&wallet).await;
    client_1
        .apply_signature_request(signature_request)
        .await
        .unwrap();

    let client_1_state_after_revoke = client_1.inbox_state(true).await.unwrap();
    let client_2_state_after_revoke = client_2.inbox_state(true).await.unwrap();
    assert_eq!(client_1_state_after_revoke.installations.len(), 1);
    assert_eq!(client_2_state_after_revoke.installations.len(), 1);
    assert_eq!(
        client_1_state_after_revoke
            .installations
            .first()
            .unwrap()
            .id,
        client_1.installation_id()
    );
    assert_eq!(
        client_2_state_after_revoke
            .installations
            .first()
            .unwrap()
            .id,
        client_1.installation_id()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_revoke_all_installations_no_crash() {
    let wallet = PrivateKeySigner::random();
    let client_1 = new_test_client_with_wallet(wallet.clone()).await;

    let client_1_state = client_1.inbox_state(true).await.unwrap();
    assert_eq!(client_1_state.installations.len(), 1);

    // revoke all other installations should return None since we only have 1 installation
    let signature_request = client_1
        .revoke_all_other_installations_signature_request()
        .await
        .unwrap();
    assert!(signature_request.is_none());

    // Now we should have two installations
    let _client_2 = new_test_client_with_wallet(wallet.clone()).await;
    let client_1_state = client_1.inbox_state(true).await.unwrap();
    assert_eq!(client_1_state.installations.len(), 2);

    let signature_request = client_1
        .revoke_all_other_installations_signature_request()
        .await
        .unwrap();
    assert!(signature_request.is_some());

    let Some(signature_request) = signature_request else {
        panic!("No signature request found");
    };
    signature_request.add_wallet_signature(&wallet).await;
    let result = client_1.apply_signature_request(signature_request).await;

    // should not error
    assert!(result.is_ok());

    // Should still have 1 installation
    let client_1_state_after_revoke = client_1.inbox_state(true).await.unwrap();
    assert_eq!(client_1_state_after_revoke.installations.len(), 1);
    assert_eq!(
        client_1_state_after_revoke
            .installations
            .first()
            .unwrap()
            .id,
        client_1.installation_id()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_revoke_installations() {
    let wallet = PrivateKeySigner::random();
    let client_1 = new_test_client_with_wallet(wallet.clone()).await;
    let client_2 = new_test_client_with_wallet(wallet.clone()).await;

    let client_1_state = client_1.inbox_state(true).await.unwrap();
    let client_2_state = client_2.inbox_state(true).await.unwrap();
    assert_eq!(client_1_state.installations.len(), 2);
    assert_eq!(client_2_state.installations.len(), 2);

    let signature_request = client_1
        .revoke_installations(vec![client_2.installation_id()])
        .await
        .unwrap();
    signature_request.add_wallet_signature(&wallet).await;
    client_1
        .apply_signature_request(signature_request)
        .await
        .unwrap();

    let client_1_state_after_revoke = client_1.inbox_state(true).await.unwrap();
    let client_2_state_after_revoke = client_2.inbox_state(true).await.unwrap();
    assert_eq!(client_1_state_after_revoke.installations.len(), 1);
    assert_eq!(client_2_state_after_revoke.installations.len(), 1);
    assert_eq!(
        client_1_state_after_revoke
            .installations
            .first()
            .unwrap()
            .id,
        client_1.installation_id()
    );
    assert_eq!(
        client_2_state_after_revoke
            .installations
            .first()
            .unwrap()
            .id,
        client_1.installation_id()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_can_not_create_new_inbox_id_with_already_associated_wallet() {
    // Step 1: Generate wallet A
    let wallet_a = generate_local_wallet();
    let ident_a = wallet_a.identifier();

    // Step 2: Use wallet A to create a new client with a new inbox id derived from wallet A
    let wallet_a_inbox_id = ident_a.inbox_id(1).unwrap();
    let ffi_ident: FfiIdentifier = wallet_a.identifier().into();
    let client_a = create_client(
        connect_to_backend_test().await,
        connect_to_backend_test().await,
        Some(tmp_path()),
        Some(xmtp_db::EncryptedMessageStore::<()>::generate_enc_key().into()),
        &wallet_a_inbox_id,
        ffi_ident,
        1,
        None,
        Some(HISTORY_SYNC_URL.to_string()),
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();
    let ffi_inbox_owner = FfiWalletInboxOwner::with_wallet(wallet_a.clone());
    register_client_with_wallet(&ffi_inbox_owner, &client_a).await;

    // Step 3: Generate wallet B
    let wallet_b = generate_local_wallet();
    let wallet_b_ident = wallet_b.identifier();

    // Step 4: Associate wallet B to inbox A
    let add_wallet_signature_request = client_a
        .add_identity(wallet_b.identifier().into())
        .await
        .expect("could not add wallet");
    add_wallet_signature_request
        .add_wallet_signature(&wallet_b)
        .await;
    client_a
        .apply_signature_request(add_wallet_signature_request)
        .await
        .unwrap();

    // Verify that we can now use wallet B to create a new client that has inbox_id == client_a.inbox_id
    let nonce = 1;
    let inbox_id = client_a.inbox_id();

    let ffi_ident: FfiIdentifier = wallet_b.identifier().into();
    let client_b = create_client(
        connect_to_backend_test().await,
        connect_to_backend_test().await,
        Some(tmp_path()),
        Some(xmtp_db::EncryptedMessageStore::<()>::generate_enc_key().into()),
        &inbox_id,
        ffi_ident,
        nonce,
        None,
        Some(HISTORY_SYNC_URL.to_string()),
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();
    let ffi_inbox_owner = FfiWalletInboxOwner::with_wallet(wallet_b.clone());
    register_client_with_wallet(&ffi_inbox_owner, &client_b).await;

    assert!(client_b.inbox_id() == client_a.inbox_id());

    // Verify both clients can receive messages for inbox_id == client_a.inbox_id
    let bo = new_test_client().await;

    // Alix creates DM with Bo
    let bo_dm = bo
        .conversations()
        .find_or_create_dm(wallet_a.identifier().into(), FfiCreateDMOptions::default())
        .await
        .unwrap();

    bo_dm
        .send(
            "Hello in DM".as_bytes().to_vec(),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();

    // Verify that client_a and client_b received the dm message to wallet a address
    client_a
        .conversations()
        .sync_all_conversations(None)
        .await
        .unwrap();
    client_b
        .conversations()
        .sync_all_conversations(None)
        .await
        .unwrap();
    bo.conversations()
        .sync_all_conversations(None)
        .await
        .unwrap();

    let a_dm_messages = client_a
        .conversations()
        .list(FfiListConversationsOptions::default())
        .unwrap()[0]
        .conversation
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();
    let b_dm_messages = client_b
        .conversations()
        .list(FfiListConversationsOptions::default())
        .unwrap()[0]
        .conversation
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();
    let bo_dm_messages = bo
        .conversations()
        .list(FfiListConversationsOptions::default())
        .unwrap()[0]
        .conversation
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();
    assert_eq!(a_dm_messages[1].content, "Hello in DM".as_bytes());
    assert_eq!(b_dm_messages[1].content, "Hello in DM".as_bytes());
    assert_eq!(bo_dm_messages[1].content, "Hello in DM".as_bytes());

    let client_b_inbox_id = wallet_b_ident.inbox_id(nonce).unwrap();
    let ffi_ident: FfiIdentifier = wallet_b.identifier().into();
    let client_b_new_result = create_client(
        connect_to_backend_test().await,
        connect_to_backend_test().await,
        Some(tmp_path()),
        Some(xmtp_db::EncryptedMessageStore::<()>::generate_enc_key().into()),
        &client_b_inbox_id,
        ffi_ident,
        nonce,
        None,
        Some(HISTORY_SYNC_URL.to_string()),
        None,
        None,
        None,
        None,
    )
    .await;

    // Client creation for b now fails since wallet b is already associated with inbox a
    match client_b_new_result {
        Err(err) => {
            println!("Error returned: {:?}", err);
            assert_eq!(
                err.to_string(),
                "Client builder error: error creating new identity: Inbox ID mismatch".to_string()
            );
        }
        Ok(_) => panic!("Expected an error, but got Ok"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_wallet_b_cannot_create_new_client_for_inbox_b_after_association() {
    // Step 1: Wallet A creates a new client with inbox_id A
    let wallet_a = generate_local_wallet();
    let ident_a = wallet_a.identifier();
    let wallet_a_inbox_id = ident_a.inbox_id(1).unwrap();
    let ffi_ident: FfiIdentifier = wallet_a.identifier().into();
    let client_a = create_client(
        connect_to_backend_test().await,
        connect_to_backend_test().await,
        Some(tmp_path()),
        Some(xmtp_db::EncryptedMessageStore::<()>::generate_enc_key().into()),
        &wallet_a_inbox_id,
        ffi_ident,
        1,
        None,
        Some(HISTORY_SYNC_URL.to_string()),
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();
    let ffi_inbox_owner_a = FfiWalletInboxOwner::with_wallet(wallet_a.clone());
    register_client_with_wallet(&ffi_inbox_owner_a, &client_a).await;

    // Step 2: Wallet B creates a new client with inbox_id B
    let wallet_b = generate_local_wallet();
    let ident_b = wallet_b.identifier();
    let wallet_b_inbox_id = ident_b.inbox_id(1).unwrap();
    let ffi_ident: FfiIdentifier = wallet_b.identifier().into();
    let client_b1 = create_client(
        connect_to_backend_test().await,
        connect_to_backend_test().await,
        Some(tmp_path()),
        Some(xmtp_db::EncryptedMessageStore::<()>::generate_enc_key().into()),
        &wallet_b_inbox_id,
        ffi_ident,
        1,
        None,
        Some(HISTORY_SYNC_URL.to_string()),
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();
    let ffi_inbox_owner_b1 = FfiWalletInboxOwner::with_wallet(wallet_b.clone());
    register_client_with_wallet(&ffi_inbox_owner_b1, &client_b1).await;

    // Step 3: Wallet B creates a second client for inbox_id B
    let ffi_ident: FfiIdentifier = wallet_b.identifier().into();
    let _client_b2 = create_client(
        connect_to_backend_test().await,
        connect_to_backend_test().await,
        Some(tmp_path()),
        Some(xmtp_db::EncryptedMessageStore::<()>::generate_enc_key().into()),
        &wallet_b_inbox_id,
        ffi_ident,
        1,
        None,
        Some(HISTORY_SYNC_URL.to_string()),
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();

    // Step 4: Client A adds association to wallet B
    let add_wallet_signature_request = client_a
        .add_identity(wallet_b.identifier().into())
        .await
        .expect("could not add wallet");
    add_wallet_signature_request
        .add_wallet_signature(&wallet_b)
        .await;
    client_a
        .apply_signature_request(add_wallet_signature_request)
        .await
        .unwrap();

    // Step 5: Wallet B tries to create another new client for inbox_id B, but it fails
    let ffi_ident: FfiIdentifier = wallet_b.identifier().into();
    let client_b3 = create_client(
        connect_to_backend_test().await,
        connect_to_backend_test().await,
        Some(tmp_path()),
        Some(xmtp_db::EncryptedMessageStore::<()>::generate_enc_key().into()),
        &wallet_b_inbox_id,
        ffi_ident,
        1,
        None,
        Some(HISTORY_SYNC_URL.to_string()),
        None,
        None,
        None,
        None,
    )
    .await;

    // Client creation for b now fails since wallet b is already associated with inbox a
    match client_b3 {
        Err(err) => {
            println!("Error returned: {:?}", err);
            assert_eq!(
                err.to_string(),
                "Client builder error: error creating new identity: Inbox ID mismatch".to_string()
            );
        }
        Ok(_) => panic!("Expected an error, but got Ok"),
    }
}

#[tokio::test]
async fn test_cannot_create_more_than_max_installations() {
    // Create a base tester
    let alix_wallet = PrivateKeySigner::random();
    let bo = Tester::new().await;
    let alix = new_test_client_no_panic(alix_wallet.clone(), None)
        .await
        .unwrap();

    // Create (MAX_INSTALLATIONS_PER_INBOX - 1) additional installations (total MAX_INSTALLATIONS_PER_INBOX)
    let mut installations = vec![];
    for _ in 0..(MAX_INSTALLATIONS_PER_INBOX - 1) {
        let new_client_installation = new_test_client_no_panic(alix_wallet.clone(), None)
            .await
            .unwrap();
        installations.push(new_client_installation);
    }

    // Verify we have MAX_INSTALLATIONS_PER_INBOX installations
    let state = alix.inbox_state(true).await.unwrap();
    assert_eq!(state.installations.len(), MAX_INSTALLATIONS_PER_INBOX);

    // Attempt to create an additional installation, expect failure
    let alix_max_plus_one_result = new_test_client_no_panic(alix_wallet.clone(), None).await;
    assert!(
        alix_max_plus_one_result.is_err(),
        "Expected failure when creating MAX_INSTALLATIONS_PER_INBOX + 1 installation, but got Ok"
    );

    // Create a group with one of the valid installations
    let bo_group = bo
        .conversations()
        .create_group_with_inbox_ids(
            vec![installations[2].inbox_id()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    // Confirm group members list Alix's inbox with exactly 5 installations
    let members = bo_group.list_members().await.unwrap();
    let alix_member = members
        .iter()
        .find(|m| m.inbox_id == alix.inbox_id())
        .expect("Alix should be a group member");
    assert_eq!(
        alix_member.installation_ids.len(),
        MAX_INSTALLATIONS_PER_INBOX
    );

    // Revoke one of Alix's installations (e.g. installations[4])
    let signature_request = alix
        .revoke_installations(vec![installations[4].installation_id()])
        .await
        .unwrap();

    signature_request.add_wallet_signature(&alix_wallet).await;
    alix.apply_signature_request(signature_request)
        .await
        .unwrap();

    let state_after_revoke = alix.inbox_state(true).await.unwrap();
    assert_eq!(
        state_after_revoke.installations.len(),
        MAX_INSTALLATIONS_PER_INBOX - 1
    );

    // Now try building alix6 again – should succeed
    let _new_client_installation = new_test_client_no_panic(alix_wallet.clone(), None).await;
    let updated_state = alix.inbox_state(true).await.unwrap();
    assert_eq!(
        updated_state.installations.len(),
        MAX_INSTALLATIONS_PER_INBOX
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_sorts_members_by_created_at_using_ffi_identifiers() {
    let ffi_inbox_owner = FfiWalletInboxOwner::new();
    let ident = ffi_inbox_owner.identifier();
    let nonce = 1;
    let inbox_id = ident.inbox_id(nonce).unwrap();

    let path = tmp_path();
    let key = static_enc_key().to_vec();
    let client = create_client(
        connect_to_backend_test().await,
        connect_to_backend_test().await,
        Some(path.clone()),
        Some(key),
        &inbox_id,
        ffi_inbox_owner.identifier(),
        nonce,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();

    let signature_request = client.signature_request().unwrap().clone();
    register_client_with_wallet(&ffi_inbox_owner, &client).await;

    signature_request
        .add_wallet_signature(&ffi_inbox_owner.wallet)
        .await;

    let initial_state = client
        .get_latest_inbox_state(inbox_id.clone())
        .await
        .expect("Failed to fetch inbox state");

    assert_eq!(
        initial_state.account_identities.len(),
        1,
        "Should have 1 identity initially"
    );

    for _i in 0..5 {
        let wallet_to_add = generate_local_wallet();
        let new_account_address = wallet_to_add.identifier();

        let signature_request = client
            .add_identity(new_account_address.into())
            .await
            .expect("could not add wallet");

        signature_request.add_wallet_signature(&wallet_to_add).await;

        client
            .apply_signature_request(signature_request)
            .await
            .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    let updated_ffi_state = client
        .get_latest_inbox_state(inbox_id.clone())
        .await
        .expect("Failed to fetch updated inbox state");

    assert_eq!(
        updated_ffi_state.account_identities.len(),
        1 + 5,
        "Expected 1 initial identity + 5 added"
    );

    let association_state = client
        .inner_client
        .identity_updates()
        .get_latest_association_state(&client.inner_client.context.store().db(), &inbox_id)
        .await
        .expect("Failed to fetch association state");

    let expected_order: Vec<_> = association_state
        .members()
        .iter()
        .filter_map(|m| match &m.identifier {
            MemberIdentifier::Ethereum(addr) => Some(addr.to_string()),
            _ => None,
        })
        .collect();

    let ffi_identities: Vec<_> = updated_ffi_state
        .account_identities
        .iter()
        .map(|id| id.identifier.clone())
        .collect();

    assert_eq!(
        ffi_identities, expected_order,
        "FFI identifiers are not ordered by creation timestamp"
    );
}
