use crate::builder::ClientBuilder;
use crate::client::Client;
use crate::context::XmtpSharedContext;
use crate::identity::IdentityStrategy;
use alloy::{dyn_abi::SolType, primitives::U256, providers::Provider, signers::Signer};
use std::time::Duration;
use xmtp_cryptography::utils::generate_local_wallet;
use xmtp_db::Fetch;
use xmtp_db::encrypted_store::identity::StoredIdentity;
use xmtp_db::group_message::MsgQueryArgs;
use xmtp_id::associations::test_utils::{MockSmartContractSignatureVerifier, WalletTestExt};
use xmtp_id::associations::{
    AccountId, Identifier, unverified::NewUnverifiedSmartContractWalletSignature,
};
use xmtp_id::utils::test::{SignatureWithNonce, SmartWalletContext, docker_smart_wallet};

#[rstest::rstest]
#[timeout(Duration::from_secs(60))]
#[tokio::test]
async fn test_remote_is_valid_signature(#[future] docker_smart_wallet: SmartWalletContext) {
    let SmartWalletContext {
        factory,
        owner0: wallet,
        sw,
        sw_address,
        ..
    } = docker_smart_wallet.await;
    let provider = factory.provider();
    let chain_id = provider.get_chain_id().await.unwrap();
    let account_address = format!("{sw_address}");
    let ident = Identifier::eth(&account_address).unwrap();
    let account_id = AccountId::new_evm(chain_id, account_address.clone());

    let identity_strategy = IdentityStrategy::new(
        ident.inbox_id(0).unwrap(),
        Identifier::eth(account_address).unwrap(),
        0,
        None,
    );

    let xmtp_client = Client::builder(identity_strategy)
        .temp_store()
        .await
        .local()
        .default_mls_store()
        .unwrap()
        .with_remote_verifier()
        .unwrap()
        .build()
        .await
        .unwrap();

    let mut signature_request = xmtp_client.context.signature_request().unwrap();
    let signature_text = signature_request.signature_text();
    let hash_to_sign = alloy::primitives::eip191_hash_message(signature_text);
    let rsh = sw.replaySafeHash(hash_to_sign).call().await.unwrap();
    let signed_hash = wallet.sign_hash(&rsh).await.unwrap().as_bytes().to_vec();
    let signature_bytes = SignatureWithNonce::abi_encode(&(U256::from(0), signed_hash));

    signature_request
        .add_new_unverified_smart_contract_signature(
            NewUnverifiedSmartContractWalletSignature::new(
                signature_bytes.to_vec(),
                account_id,
                None,
            ),
            &xmtp_client.scw_verifier(),
        )
        .await
        .unwrap();

    xmtp_client
        .register_identity(signature_request)
        .await
        .unwrap();
}

#[rstest::rstest]
#[timeout(Duration::from_secs(60))]
#[tokio::test]
async fn test_detect_scw_vs_eoa_creation(#[future] docker_smart_wallet: SmartWalletContext) {
    let SmartWalletContext {
        factory,
        owner0: wallet,
        sw,
        sw_address,
        ..
    } = docker_smart_wallet.await;

    let provider = factory.provider();
    let chain_id = provider.get_chain_id().await.unwrap();

    // Create an EOA client first
    let eoa_wallet = generate_local_wallet();
    let eoa_client = ClientBuilder::new_test_client(&eoa_wallet).await;
    let eoa_inbox_id = eoa_client.inbox_id().to_string();

    // Create an SCW client
    let account_address = format!("{sw_address}");
    let ident = Identifier::eth(&account_address).unwrap();
    let account_id = AccountId::new_evm(chain_id, account_address.clone());

    let identity_strategy = IdentityStrategy::new(
        ident.inbox_id(0).unwrap(),
        Identifier::eth(account_address).unwrap(),
        0,
        None,
    );

    let scw_client = Client::builder(identity_strategy)
        .temp_store()
        .await
        .local()
        .default_mls_store()
        .unwrap()
        .with_remote_verifier()
        .unwrap()
        .build()
        .await
        .unwrap();

    let mut signature_request = scw_client.context.signature_request().unwrap();
    let signature_text = signature_request.signature_text();
    let hash_to_sign = alloy::primitives::eip191_hash_message(signature_text);
    let rsh = sw.replaySafeHash(hash_to_sign).call().await.unwrap();
    let signed_hash = wallet.sign_hash(&rsh).await.unwrap().as_bytes().to_vec();
    let signature_bytes = SignatureWithNonce::abi_encode(&(U256::from(0), signed_hash));

    signature_request
        .add_new_unverified_smart_contract_signature(
            NewUnverifiedSmartContractWalletSignature::new(
                signature_bytes.to_vec(),
                account_id,
                None,
            ),
            &scw_client.scw_verifier(),
        )
        .await
        .unwrap();

    scw_client
        .register_identity(signature_request)
        .await
        .unwrap();

    let scw_inbox_id = scw_client.inbox_id().to_string();

    // Test the new API - check EOA client signature kind
    let eoa_kind = eoa_client
        .inbox_creation_signature_kind(eoa_inbox_id.as_str(), false)
        .await
        .unwrap();
    assert_eq!(
        eoa_kind,
        Some(xmtp_id::associations::SignatureKind::Erc191),
        "EOA client should return Erc191 signature kind"
    );

    // Test the new API - check SCW client signature kind
    let scw_kind = scw_client
        .inbox_creation_signature_kind(scw_inbox_id.as_str(), false)
        .await
        .unwrap();
    assert_eq!(
        scw_kind,
        Some(xmtp_id::associations::SignatureKind::Erc1271),
        "SCW client should return Erc1271 signature kind"
    );

    // Cross-check: EOA client checking SCW inbox
    // Since we're using false for refresh_from_network and the EOA client doesn't have
    // the SCW inbox's identity updates in its local DB, we should get an error
    let cross_check_result = eoa_client
        .inbox_creation_signature_kind(scw_inbox_id.as_str(), false)
        .await;

    // Should fail because EOA client doesn't have SCW inbox's identity updates locally
    assert!(
        cross_check_result.is_err(),
        "Should error when identity updates not found locally"
    );

    // Now try with refresh_from_network=true, which should succeed
    let cross_check = eoa_client
        .inbox_creation_signature_kind(scw_inbox_id.as_str(), true)
        .await
        .unwrap();
    assert_eq!(
        cross_check,
        Some(xmtp_id::associations::SignatureKind::Erc1271),
        "EOA client should correctly identify SCW inbox when fetching from network"
    );
}

#[rstest::rstest]
#[tokio::test]
async fn test_two_smart_contract_wallets_group_messaging(
    #[future] docker_smart_wallet: SmartWalletContext,
) {
    let SmartWalletContext {
        factory,
        owner0: wallet1,
        sw,
        sw_address: sw_address1,
        ..
    } = docker_smart_wallet.await;

    let provider = factory.provider();
    let chain_id = provider.get_chain_id().await.unwrap();

    // Create first client with smart contract wallet
    let account_address1 = format!("{sw_address1}");
    let ident1 = Identifier::eth(&account_address1).unwrap();
    let account_id1 = AccountId::new_evm(chain_id, account_address1.clone());

    let identity_strategy1 = IdentityStrategy::new(
        ident1.inbox_id(0).unwrap(),
        Identifier::eth(account_address1).unwrap(),
        0,
        None,
    );

    let client1 = Client::builder(identity_strategy1)
        .temp_store()
        .await
        .local()
        .default_mls_store()
        .unwrap()
        .with_remote_verifier()
        .unwrap()
        .build()
        .await
        .unwrap();

    // Register identity for first client
    let mut signature_request1 = client1.context.signature_request().unwrap();
    let signature_text1 = signature_request1.signature_text();
    let hash_to_sign1 = alloy::primitives::eip191_hash_message(signature_text1);
    let rsh1 = sw.replaySafeHash(hash_to_sign1).call().await.unwrap();
    let signed_hash1 = wallet1.sign_hash(&rsh1).await.unwrap().as_bytes().to_vec();
    let signature_bytes1 = SignatureWithNonce::abi_encode(&(U256::from(0), signed_hash1));

    signature_request1
        .add_new_unverified_smart_contract_signature(
            NewUnverifiedSmartContractWalletSignature::new(
                signature_bytes1.to_vec(),
                account_id1,
                None,
            ),
            &client1.scw_verifier(),
        )
        .await
        .unwrap();

    client1.register_identity(signature_request1).await.unwrap();

    // Create a group with client1
    let group1 = client1.create_group(None, None).unwrap();
    println!("Created group with ID: {:?}", hex::encode(&group1.group_id));

    group1.sync().await.unwrap();

    // Exchange messages
    let message1 = b"Hello from smart wallet 1!";
    let message2 = b"Hello from smart wallet 2!";

    // Client1 sends messages
    group1
        .send_message(message1, Default::default())
        .await
        .unwrap();
    group1
        .send_message(message2, Default::default())
        .await
        .unwrap();

    // Sync and verify messages
    group1.sync().await.unwrap();
    let messages = group1.find_messages(&MsgQueryArgs::default()).unwrap();
    assert!(messages.len() >= 2, "Should have at least 2 messages");
    assert_eq!(
        messages.last().unwrap().decrypted_message_bytes,
        message2,
        "Last message should be message2"
    );
}

/// Test that invalid SCW signature prevents client from being stored in DB
#[rstest::rstest]
#[tokio::test]
async fn test_invalid_scw_prevents_db_storage(#[future] docker_smart_wallet: SmartWalletContext) {
    let SmartWalletContext {
        factory,
        owner0: wallet,
        sw,
        sw_address,
        ..
    } = docker_smart_wallet.await;

    let provider = factory.provider();
    let chain_id = provider.get_chain_id().await.unwrap();

    // Create client with smart contract wallet
    let account_address = format!("{sw_address}");
    let ident = Identifier::eth(&account_address).unwrap();
    let account_id = AccountId::new_evm(chain_id, account_address.clone());

    let identity_strategy = IdentityStrategy::new(
        ident.inbox_id(0).unwrap(),
        Identifier::eth(account_address).unwrap(),
        0,
        None,
    );

    // Use a mock verifier that returns FALSE for verification
    let mock_verifier = MockSmartContractSignatureVerifier::new(false);

    let client = Client::builder(identity_strategy)
        .temp_store()
        .await
        .local()
        .default_mls_store()
        .unwrap()
        .with_scw_verifier(mock_verifier)
        .build()
        .await
        .unwrap();

    // Create a valid signature but it will fail verification due to mock verifier
    let mut signature_request = client.context.signature_request().unwrap();
    let signature_text = signature_request.signature_text();
    let hash_to_sign = alloy::primitives::eip191_hash_message(signature_text);
    let rsh = sw.replaySafeHash(hash_to_sign).call().await.unwrap();
    let signed_hash = wallet.sign_hash(&rsh).await.unwrap().as_bytes().to_vec();
    let signature_bytes = SignatureWithNonce::abi_encode(&(U256::from(0), signed_hash));

    // Try to add the signature - this should fail because verification returns false
    let add_signature_result = signature_request
        .add_new_unverified_smart_contract_signature(
            NewUnverifiedSmartContractWalletSignature::new(
                signature_bytes.to_vec(),
                account_id,
                None,
            ),
            &client.scw_verifier(),
        )
        .await;

    // Assert that adding the signature failed
    assert!(
        add_signature_result.is_err(),
        "Expected signature verification to fail"
    );

    // Attempting to register identity should fail with missing signatures
    let register_result = client.register_identity(signature_request).await;
    assert!(
        register_result.is_err(),
        "Expected identity registration to fail with invalid signature"
    );

    // CRITICAL: Verify that the client was NOT stored in the database
    let stored_identity: Option<StoredIdentity> = client.context.db().fetch(&()).unwrap();
    assert!(
        stored_identity.is_none(),
        "Client should NOT be stored in DB after failed registration"
    );

    // Verify that is_ready() is still false
    assert!(
        !client.identity().is_ready(),
        "Identity should not be ready after failed registration"
    );
}

/// Test recovery: invalid SCW signature, then valid SCW signature should work
#[rstest::rstest]
#[tokio::test]
async fn test_invalid_scw_then_valid_scw_recovery(
    #[future] docker_smart_wallet: SmartWalletContext,
) {
    let SmartWalletContext {
        factory,
        owner0: wallet,
        sw,
        sw_address,
        ..
    } = docker_smart_wallet.await;

    let provider = factory.provider();
    let chain_id = provider.get_chain_id().await.unwrap();

    // Create client with smart contract wallet
    let account_address = format!("{sw_address}");
    let ident = Identifier::eth(&account_address).unwrap();
    let account_id = AccountId::new_evm(chain_id, account_address.clone());

    let identity_strategy = IdentityStrategy::new(
        ident.inbox_id(0).unwrap(),
        Identifier::eth(&account_address).unwrap(),
        0,
        None,
    );

    // STEP 1: Start with a mock verifier that returns FALSE
    let mock_verifier = MockSmartContractSignatureVerifier::new(false);

    let client = Client::builder(identity_strategy)
        .temp_store()
        .await
        .local()
        .default_mls_store()
        .unwrap()
        .with_scw_verifier(mock_verifier)
        .build()
        .await
        .unwrap();

    // Create a signature request with INVALID verifier
    let mut invalid_signature_request = client.context.signature_request().unwrap();
    let signature_text = invalid_signature_request.signature_text();
    let hash_to_sign = alloy::primitives::eip191_hash_message(signature_text);
    let rsh = sw.replaySafeHash(hash_to_sign).call().await.unwrap();
    let signed_hash = wallet.sign_hash(&rsh).await.unwrap().as_bytes().to_vec();
    let signature_bytes = SignatureWithNonce::abi_encode(&(U256::from(0), signed_hash));

    // Try to add the signature - should fail due to mock verifier returning false
    let add_invalid_result = invalid_signature_request
        .add_new_unverified_smart_contract_signature(
            NewUnverifiedSmartContractWalletSignature::new(
                signature_bytes.clone(),
                account_id.clone(),
                None,
            ),
            &client.scw_verifier(),
        )
        .await;

    assert!(
        add_invalid_result.is_err(),
        "Expected invalid signature to be rejected"
    );

    // STEP 2: Now rebuild client with VALID verifier (remote verifier)
    let identity_strategy2 = IdentityStrategy::new(
        ident.inbox_id(0).unwrap(),
        Identifier::eth(account_address.clone()).unwrap(),
        0,
        None,
    );

    let client2 = Client::builder(identity_strategy2)
        .temp_store()
        .await
        .local()
        .default_mls_store()
        .unwrap()
        .with_remote_verifier()
        .unwrap()
        .build()
        .await
        .unwrap();

    // Create a new signature request with VALID verifier
    let mut valid_signature_request = client2.context.signature_request().unwrap();

    // Generate a NEW signature for client2's signature_text
    let signature_text2 = valid_signature_request.signature_text();
    let hash_to_sign2 = alloy::primitives::eip191_hash_message(signature_text2);
    let rsh2 = sw.replaySafeHash(hash_to_sign2).call().await.unwrap();
    let signed_hash2 = wallet.sign_hash(&rsh2).await.unwrap().as_bytes().to_vec();
    let signature_bytes2 = SignatureWithNonce::abi_encode(&(U256::from(0), signed_hash2));

    // Add the VALID signature
    valid_signature_request
        .add_new_unverified_smart_contract_signature(
            NewUnverifiedSmartContractWalletSignature::new(
                signature_bytes2.to_vec(),
                account_id.clone(),
                None,
            ),
            &client2.scw_verifier(),
        )
        .await
        .unwrap();

    // Register identity - should succeed now
    let register_result = client2.register_identity(valid_signature_request).await;
    assert!(
        register_result.is_ok(),
        "Expected valid signature to allow registration: {:?}",
        register_result
    );

    // Verify that the client IS stored in the database
    let stored_identity: Option<StoredIdentity> = client2.context.db().fetch(&()).unwrap();
    assert!(
        stored_identity.is_some(),
        "Client SHOULD be stored in DB after successful registration"
    );

    // Verify that is_ready() is now true
    assert!(
        client2.identity().is_ready(),
        "Identity should be ready after successful registration"
    );

    // STEP 3: Verify client can perform operations
    let group_result = client2.create_group(None, None);
    assert!(
        group_result.is_ok(),
        "Client should be able to create groups after successful registration"
    );
}

/// Test that operations fail when identity is not ready
#[xmtp_common::test]
async fn test_operations_fail_when_not_ready() {
    // Create client with regular wallet but don't register identity
    let wallet = generate_local_wallet();
    let ident = wallet.identifier();
    let identity_strategy =
        IdentityStrategy::new(ident.inbox_id(0).unwrap(), ident.clone(), 0, None);

    let client = Client::builder(identity_strategy)
        .temp_store()
        .await
        .local()
        .default_mls_store()
        .unwrap()
        .with_remote_verifier()
        .unwrap()
        .build()
        .await
        .unwrap();

    // Verify identity is not ready
    assert!(
        !client.identity().is_ready(),
        "Identity should not be ready before registration"
    );

    // Try to create a group - should fail with UninitializedIdentity
    let create_group_result = client.create_group(None, None);
    assert!(
        create_group_result.is_err(),
        "create_group should fail when identity is not ready"
    );

    let err = create_group_result.unwrap_err();
    assert!(
        matches!(
            err,
            crate::client::ClientError::Identity(
                crate::identity::IdentityError::UninitializedIdentity
            )
        ),
        "Expected UninitializedIdentity error, got: {:?}",
        err
    );

    // Try to sync welcomes - should fail with UninitializedIdentity
    let sync_welcomes_result = client.sync_welcomes().await;
    assert!(
        sync_welcomes_result.is_err(),
        "sync_welcomes should fail when identity is not ready"
    );

    // Try to create a DM - should fail with UninitializedIdentity
    let target_wallet = generate_local_wallet();
    let find_or_create_dm_result = client
        .find_or_create_dm(target_wallet.identifier(), None)
        .await;
    assert!(
        find_or_create_dm_result.is_err(),
        "find_or_create_dm should fail when identity is not ready"
    );

    let err = find_or_create_dm_result.unwrap_err();
    assert!(
        matches!(
            err,
            crate::client::ClientError::Identity(
                crate::identity::IdentityError::UninitializedIdentity
            )
        ),
        "Expected UninitializedIdentity error, got: {:?}",
        err
    );
}
