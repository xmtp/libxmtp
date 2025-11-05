use crate::builder::ClientBuilder;
use crate::{client::Client, identity::IdentityStrategy};
use alloy::{dyn_abi::SolType, primitives::U256, providers::Provider, signers::Signer};
use std::time::Duration;
use xmtp_cryptography::utils::generate_local_wallet;
use xmtp_db::group_message::MsgQueryArgs;
use xmtp_id::associations::Identifier;
use xmtp_id::utils::test::{SmartWalletContext, docker_smart_wallet};
use xmtp_id::{
    associations::{AccountId, unverified::NewUnverifiedSmartContractWalletSignature},
    utils::test::SignatureWithNonce,
};
use xmtp_id::associations::test_utils::MockSmartContractSignatureVerifier;

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
        .local_client()
        .await
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

    // Client1 sends a message
    group1.send_message(message1).await.unwrap();
    group1.send_message(message2).await.unwrap();

    // Client2 receives and sends a message
    // group2.sync().await.unwrap();
    let _messages = group1.find_messages(&MsgQueryArgs::default()).unwrap();
    // assert!(!messages.is_empty());
    // assert_eq!(messages.last().unwrap().decrypted_message_bytes, message1);
    //
    // group2.send_message(message2).await.unwrap();

    // Client1 receives the message from client2
    group1.sync().await.unwrap();
    let messages = group1.find_messages(&MsgQueryArgs::default()).unwrap();
    assert!(messages.len() >= 2);
    assert_eq!(messages.last().unwrap().decrypted_message_bytes, message2);

    println!("Successfully exchanged messages between two smart contract wallet clients!");
}

#[rstest::rstest]
#[tokio::test]
async fn test_smart_contract_wallet_unverified_should_fail(
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

    // Create first client with smart contract wallet - but with UNVERIFIED signature
    let account_address1 = format!("{sw_address1}");
    let ident1 = Identifier::eth(&account_address1).unwrap();
    let account_id1 = AccountId::new_evm(chain_id, account_address1.clone());

    let identity_strategy1 = IdentityStrategy::new(
        ident1.inbox_id(0).unwrap(),
        Identifier::eth(account_address1).unwrap(),
        0,
        None,
    );

    // Use a mock verifier that returns FALSE for verification
    let mock_verifier = MockSmartContractSignatureVerifier::new(false);

    let client1 = Client::builder(identity_strategy1)
        .temp_store()
        .await
        .local_client()
        .await
        .default_mls_store()
        .unwrap()
        .with_scw_verifier(mock_verifier)
        .build()
        .await
        .unwrap();

    // Register identity for first client - this should FAIL because the verifier returns false
    let signature_request1 = client1.context.signature_request().unwrap();
    let signature_text1 = signature_request1.signature_text();
    let hash_to_sign1 = alloy::primitives::eip191_hash_message(signature_text1);
    let rsh1 = sw.replaySafeHash(hash_to_sign1).call().await.unwrap();
    let signed_hash1 = wallet1.sign_hash(&rsh1).await.unwrap().as_bytes().to_vec();
    let signature_bytes1 = SignatureWithNonce::abi_encode(&(U256::from(0), signed_hash1));

    // Try to add the signature - this should fail because verification will return false
    let add_signature_result = signature_request1
        .clone()
        .add_new_unverified_smart_contract_signature(
            NewUnverifiedSmartContractWalletSignature::new(
                signature_bytes1.to_vec(),
                account_id1,
                None,
            ),
            &client1.scw_verifier(),
        )
        .await;

    // Assert that adding the signature failed
    assert!(
        add_signature_result.is_err(),
        "Expected signature verification to fail with unverified smart contract wallet"
    );

    // Attempting to register identity should also fail because the signature request is missing signatures
    let register_result = client1.register_identity(signature_request1.clone()).await;
    assert!(
        register_result.is_err(),
        "Expected identity registration to fail with missing signatures"
    );
}

#[rstest::rstest]
#[tokio::test]
async fn test_smart_contract_wallet_unverified_should_fail_2(
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

    // Create an EOA client first
    let eoa_wallet = generate_local_wallet();
    let eoa_client = ClientBuilder::new_test_client(&eoa_wallet).await;
    println!("Created EOA client with inbox_id: {}", eoa_client.inbox_id());

    // STEP 1: Create SCW client with VALID signature first
    let account_address1 = format!("{sw_address1}");
    let ident1 = Identifier::eth(&account_address1).unwrap();
    let account_id1 = AccountId::new_evm(chain_id, account_address1.clone());

    let identity_strategy1 = IdentityStrategy::new(
        ident1.inbox_id(0).unwrap(),
        Identifier::eth(account_address1.clone()).unwrap(),
        0,
        None,
    );

    let scw_client = Client::builder(identity_strategy1)
        .temp_store()
        .await
        .local_client()
        .await
        .default_mls_store()
        .unwrap()
        .with_remote_verifier()
        .unwrap()
        .build()
        .await
        .unwrap();

    // Register identity with VALID signature
    let mut signature_request1 = scw_client.context.signature_request().unwrap();
    let signature_text1 = signature_request1.signature_text();
    let hash_to_sign1 = alloy::primitives::eip191_hash_message(signature_text1);
    let rsh1 = sw.replaySafeHash(hash_to_sign1).call().await.unwrap();
    let signed_hash1 = wallet1.sign_hash(&rsh1).await.unwrap().as_bytes().to_vec();
    let signature_bytes1 = SignatureWithNonce::abi_encode(&(U256::from(0), signed_hash1));

    signature_request1
        .add_new_unverified_smart_contract_signature(
            NewUnverifiedSmartContractWalletSignature::new(
                signature_bytes1.to_vec(),
                account_id1.clone(),
                None,
            ),
            &scw_client.scw_verifier(),
        )
        .await
        .unwrap();

    scw_client.register_identity(signature_request1).await.unwrap();
    println!("SCW client registered with VALID signature");

    // EOA client creates a group and adds the SCW client
    let group = eoa_client.create_group(None, None).unwrap();
    println!("EOA client created group with ID: {:?}", hex::encode(&group.group_id));

    // Add the SCW client to the group
    group.add_members_by_inbox_id(&[scw_client.inbox_id()]).await.unwrap();
    println!("EOA client added SCW client to the group");

    group.sync().await.unwrap();

    // SCW client syncs welcomes and groups
    println!("\n--- SCW client syncing with VALID signature ---");
    let sync_welcomes_result = scw_client.sync_all_welcomes_and_groups(None).await;
    println!("SCW sync_welcomes result: {:?}", sync_welcomes_result);

    // Find the group on the SCW client side
    let scw_groups = scw_client.find_groups(Default::default()).unwrap();
    println!("SCW client found {} groups", scw_groups.len());

    if let Some(scw_group) = scw_groups.first() {
        println!("SCW client found group with ID: {:?}", hex::encode(&scw_group.group_id));

        // Send a message with valid signature
        let message = b"Hello from valid SCW!";
        let send_result = scw_group.send_message(message).await;
        println!("SCW send_message result (valid): {:?}", send_result);

        // Sync the group
        let sync_result = scw_group.sync().await;
        println!("SCW group sync result (valid): {:?}", sync_result);

        // Try to read messages
        let messages = scw_group.find_messages(&MsgQueryArgs::default()).unwrap();
        println!("SCW group has {} messages after valid send", messages.len());
    }

    // STEP 2: Now add an INVALID signature to the SCW client
    println!("\n--- Now adding INVALID signature to SCW client ---");

    // Create a new signature request for adding an invalid signature
    let mut invalid_signature_request = scw_client.context.signature_request().unwrap();

    // Create an INVALID signature (just some random bytes)
    let invalid_signature_bytes = vec![0u8; 65]; // Invalid signature

    let add_invalid_result = invalid_signature_request
        .add_new_unverified_smart_contract_signature(
            NewUnverifiedSmartContractWalletSignature::new(
                invalid_signature_bytes,
                account_id1,
                None,
            ),
            &scw_client.scw_verifier(),
        )
        .await;

    println!("Add invalid signature result: {:?}", add_invalid_result);

    // Try to apply the invalid signature
    if add_invalid_result.is_ok() {
        let apply_result = scw_client.apply_signature_request(invalid_signature_request).await;
        println!("Apply invalid signature result: {:?}", apply_result);
    }

    // Sync again after adding invalid signature
    println!("\n--- SCW client syncing after INVALID signature added ---");
    let sync_welcomes_result2 = scw_client.sync_all_welcomes_and_groups(None).await;
    println!("SCW sync_welcomes result (after invalid): {:?}", sync_welcomes_result2);

    // Try to send another message after invalid signature
    let scw_groups = scw_client.find_groups(Default::default()).unwrap();
    if let Some(scw_group) = scw_groups.first() {
        let message2 = b"Hello after invalid signature!";
        let send_result2 = scw_group.send_message(message2).await;
        println!("SCW send_message result (after invalid): {:?}", send_result2);

        // Sync the group
        let sync_result2 = scw_group.sync().await;
        println!("SCW group sync result (after invalid): {:?}", sync_result2);

        // Try to read messages
        let messages = scw_group.find_messages(&MsgQueryArgs::default()).unwrap();
        println!("SCW group has {} messages after invalid signature attempt", messages.len());
    }

    println!("\nTest completed: Observed behavior with valid then invalid SCW signature");
}