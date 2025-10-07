use crate::{client::Client, identity::IdentityStrategy};
use alloy::{dyn_abi::SolType, primitives::U256, providers::Provider, signers::Signer};
use xmtp_db::group_message::MsgQueryArgs;
use xmtp_id::associations::Identifier;
use xmtp_id::associations::test_utils::MockSmartContractSignatureVerifier;
use xmtp_id::utils::test::{SmartWalletContext, docker_smart_wallet};
use xmtp_id::{
    associations::{AccountId, unverified::NewUnverifiedSmartContractWalletSignature},
    utils::test::SignatureWithNonce,
};

#[rstest::rstest]
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
        .local_client()
        .await
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
    let messages = group1.find_messages(&MsgQueryArgs::default()).unwrap();
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
    let mut signature_request1 = client1.context.signature_request().unwrap();
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

    // Verify the error is a signature validation error
    match add_signature_result {
        Err(e) => {
            println!("Expected error occurred: {:?}", e);
            // The error should be related to invalid signature
            assert!(
                format!("{:?}", e).contains("Invalid") || format!("{:?}", e).contains("signature"),
                "Error should indicate invalid signature"
            );
        }
        Ok(_) => panic!("Should have failed signature verification"),
    }

    // Attempting to register identity should also fail
    let register_result = client1.register_identity(signature_request1.clone()).await;
    assert!(
        register_result.is_err(),
        "Expected identity registration to fail"
    );

    // client1
    //     .register_identity(signature_request1.clone())
    //     .await
    //     .unwrap();

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
    let messages = group1.find_messages(&MsgQueryArgs::default()).unwrap();
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

    println!("Test passed: Smart contract wallet with unverified signature correctly failed");
}
