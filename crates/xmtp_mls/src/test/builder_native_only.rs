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
#[xmtp_common::timeout(Duration::from_secs(60))]
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
#[xmtp_common::timeout(Duration::from_secs(60))]
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
    println!("Created group with ID: {:?}", hex::encode(group1.group_id));

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
    let identity_strategy = IdentityStrategy::new(ident.inbox_id(0).unwrap(), ident.clone(), 0);

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
        .find_or_create_dm_by_identity(target_wallet.identifier(), None)
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

// An offline build makes no request. A stored log whose state needs a smart
// contract wallet verdict, with no cached state, fails the identifier check
// instead of asking the backend's verifier.
#[rstest::rstest]
#[xmtp_common::timeout(Duration::from_secs(60))]
#[tokio::test]
async fn offline_build_asks_no_verifier(#[future] docker_smart_wallet: SmartWalletContext) {
    use crate::{builder::ClientBuilderError, identity::IdentityError};
    use xmtp_db::{ConnectionExt, XmtpTestDb, diesel::RunQueryDsl};
    let SmartWalletContext {
        factory,
        owner0: wallet,
        sw,
        sw_address,
        ..
    } = docker_smart_wallet.await;
    let chain_id = factory.provider().get_chain_id().await.unwrap();
    let account_address = format!("{sw_address}");
    let ident = Identifier::eth(&account_address).unwrap();
    let account_id = AccountId::new_evm(chain_id, account_address);
    let inbox_id = ident.inbox_id(0).unwrap();
    let path = xmtp_common::tmp_path();

    let client = Client::builder(IdentityStrategy::new(inbox_id.clone(), ident.clone(), 0))
        .store(xmtp_db::TestDb::create_persistent_store(Some(path.clone())).await)
        .local()
        .default_mls_store()
        .unwrap()
        .with_remote_verifier()
        .unwrap()
        .with_disable_workers(true)
        .build()
        .await
        .unwrap();
    let mut signature_request = client.context.signature_request().unwrap();
    let hash_to_sign = alloy::primitives::eip191_hash_message(signature_request.signature_text());
    let rsh = sw.replaySafeHash(hash_to_sign).call().await.unwrap();
    let signed_hash = wallet.sign_hash(&rsh).await.unwrap().as_bytes().to_vec();
    let signature_bytes = SignatureWithNonce::abi_encode(&(U256::from(0), signed_hash));
    signature_request
        .add_new_unverified_smart_contract_signature(
            NewUnverifiedSmartContractWalletSignature::new(signature_bytes, account_id, None),
            &client.scw_verifier(),
        )
        .await
        .unwrap();
    client.register_identity(signature_request).await.unwrap();
    // Only the log stays: no derived state is cached.
    client
        .context
        .db()
        .raw_query(|c| {
            xmtp_db::diesel::delete(xmtp_db::schema::association_state::table).execute(c)
        })
        .unwrap();
    drop(client);

    // Any request the offline build sent would reach no backend.
    let mut api = xmtp_api_backend::MessageBackendBuilder::new();
    api.host("http://127.0.0.1:1");
    let offline = Client::builder(IdentityStrategy::new(inbox_id.clone(), ident, 0))
        .api_client_with_streams(api.build().unwrap())
        .with_remote_verifier()
        .unwrap()
        .store(xmtp_db::TestDb::create_persistent_store(Some(path)).await)
        .default_mls_store()
        .unwrap()
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await;
    assert!(
        matches!(
            &offline,
            Err(ClientBuilderError::Identity(IdentityError::IdentifierNotInInbox { inbox_id: found }))
                if *found == inbox_id
        ),
        "build: {:?}",
        offline.err()
    );
}

/// A verifier whose every smart contract wallet check fails with `leaf`,
/// counting the checks in `calls`.
pub(crate) struct FailingVerifier {
    leaf: fn() -> xmtp_id::scw_verifier::VerifierError,
    calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl FailingVerifier {
    pub(crate) fn new(
        leaf: fn() -> xmtp_id::scw_verifier::VerifierError,
        calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    ) -> Self {
        Self { leaf, calls }
    }
}

#[xmtp_common::async_trait]
impl xmtp_id::scw_verifier::SmartContractSignatureVerifier for FailingVerifier {
    async fn is_valid_signature(
        &self,
        _account_id: AccountId,
        _hash: [u8; 32],
        _signature: alloy::primitives::Bytes,
        _block_number: Option<alloy::primitives::BlockNumber>,
    ) -> Result<xmtp_id::scw_verifier::ValidationResponse, xmtp_id::scw_verifier::VerifierError>
    {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Err((self.leaf)())
    }
}

/// A verifier that checks smart contract wallet signatures on the chain of a
/// `docker_smart_wallet` context, first running the hook set in `before`,
/// such as a disconnect of the client's store, and counting checks in `calls`.
pub(crate) struct HookedVerifier {
    chain: xmtp_id::scw_verifier::RpcSmartContractWalletVerifier,
    before: std::sync::Arc<std::sync::OnceLock<Box<dyn Fn() + Send + Sync>>>,
    calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl HookedVerifier {
    pub(crate) fn new(
        context: &SmartWalletContext,
        before: std::sync::Arc<std::sync::OnceLock<Box<dyn Fn() + Send + Sync>>>,
        calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    ) -> Self {
        let chain = xmtp_id::scw_verifier::RpcSmartContractWalletVerifier::new_from_provider(
            context.factory.provider().clone(),
        );
        Self {
            chain,
            before,
            calls,
        }
    }
}

#[xmtp_common::async_trait]
impl xmtp_id::scw_verifier::SmartContractSignatureVerifier for HookedVerifier {
    async fn is_valid_signature(
        &self,
        account_id: AccountId,
        hash: [u8; 32],
        signature: alloy::primitives::Bytes,
        block_number: Option<alloy::primitives::BlockNumber>,
    ) -> Result<xmtp_id::scw_verifier::ValidationResponse, xmtp_id::scw_verifier::VerifierError>
    {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if let Some(before) = self.before.get() {
            before();
        }
        self.chain
            .is_valid_signature(account_id, hash, signature, block_number)
            .await
    }
}

/// Register an inbox created by the smart contract wallet of `context` on the
/// local backend, returning its inbox id and the wallet's identifier.
pub(crate) async fn register_scw_inbox(context: SmartWalletContext) -> (String, Identifier) {
    let (client, ident) = register_scw_client(context, None::<FailingVerifier>).await;
    (client.inbox_id().to_string(), ident)
}

/// Register the smart contract wallet of `context` on the local backend and
/// return its client, on a persistent store with workers off, and the
/// wallet's identifier. The client checks signatures with `verifier`, or the
/// backend's without one.
#[allow(clippy::unwrap_used)]
pub(crate) async fn register_scw_client(
    context: SmartWalletContext,
    verifier: Option<impl xmtp_id::scw_verifier::SmartContractSignatureVerifier + 'static>,
) -> (crate::utils::FullXmtpClient, Identifier) {
    let SmartWalletContext {
        factory,
        owner0: wallet,
        sw,
        sw_address,
        ..
    } = context;
    let chain_id = factory.provider().get_chain_id().await.unwrap();
    let account_address = format!("{sw_address}");
    let ident = Identifier::eth(&account_address).unwrap();
    let account_id = AccountId::new_evm(chain_id, account_address.clone());
    let scw_client = Client::builder(IdentityStrategy::new(
        ident.inbox_id(0).unwrap(),
        ident.clone(),
        0,
    ))
    .temp_store()
    .await
    .local()
    .default_mls_store()
    .unwrap()
    .with_disable_workers(true);
    let scw_client = match verifier {
        Some(verifier) => scw_client.with_scw_verifier(verifier),
        None => scw_client.with_remote_verifier().unwrap(),
    }
    .build()
    .await
    .unwrap();
    let mut signature_request = scw_client.context.signature_request().unwrap();
    let hash_to_sign = alloy::primitives::eip191_hash_message(signature_request.signature_text());
    let rsh = sw.replaySafeHash(hash_to_sign).call().await.unwrap();
    let signed_hash = wallet.sign_hash(&rsh).await.unwrap().as_bytes().to_vec();
    signature_request
        .add_new_unverified_smart_contract_signature(
            NewUnverifiedSmartContractWalletSignature::new(
                SignatureWithNonce::abi_encode(&(U256::from(0), signed_hash)).to_vec(),
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
    (scw_client, ident)
}

/// Membership reads map an inbox with no identity log to `None`, a smart
/// contract wallet log they cannot verify offline to `Some(false)`, and a
/// verifier failure online to that failure with its own retryability. A
/// handle lookup that finds no inbox fails with `HandleError::NotFound`.
#[rstest::rstest]
#[xmtp_common::timeout(Duration::from_secs(60))]
#[tokio::test]
async fn membership_reads_map_missing_updates_and_scw_failures(
    #[future] docker_smart_wallet: SmartWalletContext,
) {
    use crate::client::ClientError;
    use crate::identity_updates::{
        identifier_membership, load_identity_updates, stored_identifier_membership,
    };
    use crate::inbox_handles::{HandleError, IdentityTarget, resolve_verified_with_verifier};
    use xmtp_common::RetryableError;
    use xmtp_id::associations::Handle;
    use xmtp_id::associations::SignatureError;
    use xmtp_id::scw_verifier::VerifierError;

    let (scw_inbox_id, ident) = register_scw_inbox(docker_smart_wallet.await).await;

    let viewer = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let db = viewer.context.db();
    load_identity_updates(viewer.context.api(), &db, &[scw_inbox_id.as_str()])
        .await
        .unwrap();

    let unknown = generate_local_wallet().identifier();
    let unknown_inbox = unknown.inbox_id(0).unwrap();
    assert_eq!(
        identifier_membership(&db, &unknown_inbox, &unknown, &viewer.scw_verifier())
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        stored_identifier_membership(&db, &scw_inbox_id, &ident)
            .await
            .unwrap(),
        Some(false)
    );

    let no_verifier = || VerifierError::NoVerifier("eip155:1".into());
    let malformed = || VerifierError::MalformedEipUrl;
    for (leaf, retryable) in [(no_verifier as fn() -> _, true), (malformed, false)] {
        let membership = identifier_membership(
            &db,
            &scw_inbox_id,
            &ident,
            &FailingVerifier::new(leaf, Default::default()),
        )
        .await
        .unwrap_err();
        let lookup = resolve_verified_with_verifier(
            viewer.context.api(),
            &db,
            &[IdentityTarget::Account(ident.clone())],
            &FailingVerifier::new(leaf, Default::default()),
        )
        .await
        .unwrap_err();
        for (route, error) in [("membership", membership), ("lookup", lookup)] {
            assert!(
                matches!(
                    error,
                    ClientError::SignatureValidation(SignatureError::VerifierError(_))
                ),
                "{route}: {error:?}"
            );
            assert_eq!(error.is_retryable(), retryable, "{route}: {error:?}");
        }
    }

    let unclaimed =
        Handle::parse(&format!("t{}", hex::encode(xmtp_common::rand_vec::<10>()))).unwrap();
    let error = viewer
        .resolve_targets(&[IdentityTarget::Handle(unclaimed)])
        .await
        .unwrap_err();
    assert!(
        matches!(error, ClientError::Handle(HandleError::NotFound)),
        "{error:?}"
    );
    assert!(!error.is_retryable());
}
