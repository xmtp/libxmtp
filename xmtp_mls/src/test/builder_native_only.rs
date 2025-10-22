use crate::builder::ClientBuilder;
use crate::{client::Client, identity::IdentityStrategy};
use alloy::{dyn_abi::SolType, primitives::U256, providers::Provider, signers::Signer};
use std::time::Duration;
use xmtp_cryptography::utils::generate_local_wallet;
use xmtp_id::associations::Identifier;
use xmtp_id::utils::test::{SmartWalletContext, docker_smart_wallet};
use xmtp_id::{
    associations::{AccountId, unverified::NewUnverifiedSmartContractWalletSignature},
    utils::test::SignatureWithNonce,
};

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
        .local_client()
        .await
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
