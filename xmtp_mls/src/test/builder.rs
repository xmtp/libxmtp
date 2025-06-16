#[cfg(target_arch = "wasm32")]
wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);
use std::sync::atomic::AtomicBool;

use crate::builder::ClientBuilderError;
use crate::identity::Identity;
use crate::identity::IdentityError;
use crate::utils::test::TestClient;
use crate::XmtpApi;
use xmtp_api::test_utils::*;
use xmtp_api::ApiClientWrapper;
use xmtp_common::{rand_vec, tmp_path, ExponentialBackoff, Retry};
use xmtp_db::events::Events;
use xmtp_db::XmtpDb;
use xmtp_db::XmtpTestDb;
use xmtp_db::{identity::StoredIdentity, Store};

use openmls::credentials::{Credential, CredentialType};
use prost::Message;
use xmtp_common::rand_u64;
use xmtp_cryptography::utils::{generate_local_wallet, rng};
use xmtp_cryptography::XmtpInstallationCredential;
use xmtp_id::associations::test_utils::{MockSmartContractSignatureVerifier, WalletTestExt};
use xmtp_id::associations::unverified::UnverifiedSignature;
use xmtp_id::associations::{Identifier, ValidatedLegacySignedPublicKey};
use xmtp_proto::api_client::ApiBuilder;
use xmtp_proto::api_client::XmtpTestClient;
use xmtp_proto::xmtp::identity::api::v1::{
    get_inbox_ids_response::Response as GetInboxIdsResponseItem, GetInboxIdsResponse,
};
use xmtp_proto::xmtp::identity::associations::IdentifierKind;
use xmtp_proto::xmtp::message_contents::signature::WalletEcdsaCompact;
use xmtp_proto::xmtp::message_contents::signed_private_key::{Secp256k1, Union};
use xmtp_proto::xmtp::message_contents::unsigned_public_key::{self, Secp256k1Uncompressed};
use xmtp_proto::xmtp::message_contents::{
    signature, Signature, SignedPrivateKey, SignedPublicKey, UnsignedPublicKey,
};

use crate::{builder::ClientBuilder, identity::IdentityStrategy};
use crate::{Client, InboxOwner};

async fn register_client<C: XmtpApi, Db: XmtpDb>(client: &Client<C, Db>, owner: &impl InboxOwner) {
    let mut signature_request = client.context.signature_request().unwrap();
    let signature_text = signature_request.signature_text();
    let scw_verifier = MockSmartContractSignatureVerifier::new(true);
    signature_request
        .add_signature(owner.sign(&signature_text).unwrap(), &scw_verifier)
        .await
        .unwrap();

    client.register_identity(signature_request).await.unwrap();
}

fn retry() -> Retry<ExponentialBackoff> {
    Retry::default()
}

/// Generate a random legacy key proto bytes and corresponding account address.
async fn generate_random_legacy_key() -> (Vec<u8>, String) {
    let wallet = generate_local_wallet();
    let ident = wallet.get_identifier().unwrap();
    let address = format!("{ident}");
    let created_ns = rand_u64();
    let secret_key = alloy::signers::k256::ecdsa::SigningKey::random(&mut rng());
    let public_key = alloy::signers::k256::ecdsa::VerifyingKey::from(&secret_key);
    let public_key_bytes = public_key.to_sec1_bytes().to_vec();
    let mut public_key_buf = vec![];
    UnsignedPublicKey {
        created_ns,
        union: Some(unsigned_public_key::Union::Secp256k1Uncompressed(
            Secp256k1Uncompressed {
                bytes: public_key_bytes.clone(),
            },
        )),
    }
    .encode(&mut public_key_buf)
    .unwrap();
    let message = ValidatedLegacySignedPublicKey::text(&public_key_buf);
    let signed_public_key = match wallet.sign(&message).unwrap() {
        UnverifiedSignature::RecoverableEcdsa(sig) => sig.signature_bytes().to_vec(),
        _ => unreachable!("Wallets only provide ecdsa signatures."),
    };
    let (bytes, recovery_id) = signed_public_key.as_slice().split_at(64);
    let recovery_id = recovery_id[0];
    let signed_private_key: SignedPrivateKey = SignedPrivateKey {
        created_ns,
        public_key: Some(SignedPublicKey {
            key_bytes: public_key_buf,
            signature: Some(Signature {
                union: Some(signature::Union::WalletEcdsaCompact(WalletEcdsaCompact {
                    bytes: bytes.to_vec(),
                    recovery: recovery_id.into(),
                })),
            }),
        }),
        union: Some(Union::Secp256k1(Secp256k1 {
            bytes: secret_key.to_bytes().to_vec(),
        })),
    };
    let mut buf = vec![];
    signed_private_key.encode(&mut buf).unwrap();
    (buf, address.to_lowercase())
}

#[xmtp_common::test]
async fn builder_test() {
    let wallet = generate_local_wallet();
    let client = ClientBuilder::new_test_client(&wallet).await;
    assert!(!client.installation_public_key().is_empty());
}

// Test client creation using various identity strategies that creates new inboxes
#[xmtp_common::test]
async fn test_client_creation() {
    struct IdentityStrategyTestCase {
        strategy: IdentityStrategy,
        err: Option<String>,
    }

    let identity_strategies_test_cases = vec![
        // legacy cases
        IdentityStrategyTestCase {
            strategy: {
                let (legacy_key, legacy_account_address) = generate_random_legacy_key().await;
                let legacy_ident = Identifier::eth(&legacy_account_address).unwrap();
                IdentityStrategy::new(
                    legacy_ident.inbox_id(1).unwrap(),
                    Identifier::eth(legacy_account_address.clone()).unwrap(),
                    1,
                    Some(legacy_key),
                )
            },
            err: Some("Nonce must be 0 if legacy key is provided".to_string()),
        },
        IdentityStrategyTestCase {
            strategy: {
                let (legacy_key, legacy_account_address) = generate_random_legacy_key().await;
                let legacy_ident = Identifier::eth(&legacy_account_address).unwrap();
                IdentityStrategy::new(
                    legacy_ident.inbox_id(1).unwrap(),
                    Identifier::eth(legacy_account_address.clone()).unwrap(),
                    0,
                    Some(legacy_key),
                )
            },
            err: Some("Inbox ID doesn't match nonce & address".to_string()),
        },
        IdentityStrategyTestCase {
            strategy: {
                let (legacy_key, legacy_account_address) = generate_random_legacy_key().await;
                let legacy_ident = Identifier::eth(&legacy_account_address).unwrap();
                IdentityStrategy::new(
                    legacy_ident.inbox_id(0).unwrap(),
                    Identifier::eth(legacy_account_address.clone()).unwrap(),
                    0,
                    Some(legacy_key),
                )
            },
            err: None,
        },
        // non-legacy cases
        IdentityStrategyTestCase {
            strategy: {
                let ident = generate_local_wallet().get_identifier().unwrap();
                IdentityStrategy::new(ident.inbox_id(1).unwrap(), ident, 0, None)
            },
            err: Some("Inbox ID doesn't match nonce & address".to_string()),
        },
        IdentityStrategyTestCase {
            strategy: {
                let nonce = 1;
                let account_ident = generate_local_wallet().get_identifier().unwrap();
                IdentityStrategy::new(
                    account_ident.inbox_id(nonce).unwrap(),
                    Identifier::eth(account_ident.clone()).unwrap(),
                    nonce,
                    None,
                )
            },
            err: None,
        },
        IdentityStrategyTestCase {
            strategy: {
                let nonce = 0;
                let account_ident = generate_local_wallet().get_identifier().unwrap();
                IdentityStrategy::new(
                    account_ident.inbox_id(nonce).unwrap(),
                    account_ident,
                    nonce,
                    None,
                )
            },
            err: None,
        },
    ];

    for test_case in identity_strategies_test_cases {
        let result = Client::builder(test_case.strategy)
            .temp_store()
            .await
            .api_client(
                <TestClient as XmtpTestClient>::create_local()
                    .build()
                    .await
                    .unwrap(),
            )
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
            .build()
            .await;

        if let Some(err_string) = test_case.err {
            assert!(result.is_err());
            assert!(matches!(
                result,
                Err(ClientBuilderError::Identity(IdentityError::NewIdentity(err))) if err == err_string
            ));
        } else {
            if let Err(ref e) = result {
                println!("{e}");
            }
            assert!(result.is_ok());
        }
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_turn_local_telemetry_off() {
    let (legacy_key, legacy_account_address) = generate_random_legacy_key().await;
    let legacy_ident = Identifier::eth(&legacy_account_address).unwrap();
    let inbox_id = legacy_ident.inbox_id(0).unwrap();

    let identity_strategy = IdentityStrategy::new(
        inbox_id.clone(),
        legacy_ident.clone(),
        0,
        Some(legacy_key.clone()),
    );
    let store = xmtp_db::TestDb::create_persistent_store(None).await;
    let client = Client::builder(identity_strategy.clone())
        .store(store)
        .api_client(
            <TestClient as XmtpTestClient>::create_local()
                .build()
                .await
                .unwrap(),
        )
        .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
        .with_disable_events(Some(true))
        .build()
        .await?;

    let provider = client.mls_provider();
    let events = Events::all_events(provider.db())?;

    // No events should be logged if telemetry is turned off.
    assert!(events.is_empty());
}

// First, create a client1 using legacy key and then test following cases:
// - create client2 from same db with [IdentityStrategy::CachedOnly]
// - create client3 from same db with [IdentityStrategy::CreateIfNotFound]
// - create client4 with different db.
#[xmtp_common::test]
async fn test_2nd_time_client_creation() {
    let (legacy_key, legacy_account_address) = generate_random_legacy_key().await;
    let legacy_ident = Identifier::eth(&legacy_account_address).unwrap();
    let inbox_id = legacy_ident.inbox_id(0).unwrap();

    let identity_strategy = IdentityStrategy::new(
        inbox_id.clone(),
        legacy_ident.clone(),
        0,
        Some(legacy_key.clone()),
    );
    let store = xmtp_db::TestDb::create_persistent_store(None).await;

    let client1 = Client::builder(identity_strategy.clone())
        .store(store.clone())
        .api_client(
            <TestClient as XmtpTestClient>::create_local()
                .build()
                .await
                .unwrap(),
        )
        .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
        .build()
        .await
        .unwrap();
    assert!(client1.context.signature_request().is_none());

    let client2 = Client::builder(IdentityStrategy::CachedOnly)
        .store(store.clone())
        .api_client(
            <TestClient as XmtpTestClient>::create_local()
                .build()
                .await
                .unwrap(),
        )
        .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
        .build()
        .await
        .unwrap();
    assert!(client2.context.signature_request().is_none());
    assert!(client1.inbox_id() == client2.inbox_id());
    assert!(client1.installation_public_key() == client2.installation_public_key());

    let client3 = Client::builder(IdentityStrategy::new(
        inbox_id.clone(),
        legacy_ident.clone(),
        0,
        None,
    ))
    .store(store.clone())
    .api_client(
        <TestClient as XmtpTestClient>::create_local()
            .build()
            .await
            .unwrap(),
    )
    .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
    .build()
    .await
    .unwrap();
    assert!(client3.context.signature_request().is_none());
    assert!(client1.inbox_id() == client3.inbox_id());
    assert!(client1.installation_public_key() == client3.installation_public_key());

    let client4 = Client::builder(identity_strategy)
        .temp_store()
        .await
        .api_client(
            <TestClient as XmtpTestClient>::create_local()
                .build()
                .await
                .unwrap(),
        )
        .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
        .build()
        .await
        .unwrap();
    assert!(client4.context.signature_request().is_some());
    assert!(client1.inbox_id() == client4.inbox_id());
    assert!(client1.installation_public_key() != client4.installation_public_key());
}

// Should return error if inbox associated with given account_address doesn't match the provided one.
#[xmtp_common::test]
async fn api_identity_mismatch() {
    let mut mock_api = MockApiClient::new();
    let scw_verifier = MockSmartContractSignatureVerifier::new(true);

    let store = xmtp_db::TestDb::create_persistent_store(None).await;
    let nonce = 0;
    let ident = generate_local_wallet().identifier();
    let inbox_id = ident.inbox_id(nonce).unwrap();

    let inbox_id_cloned = inbox_id.clone();
    mock_api.expect_get_inbox_ids().returning({
        let ident = ident.clone();
        move |_| {
            let kind: IdentifierKind = (&ident).into();
            Ok(GetInboxIdsResponse {
                responses: vec![GetInboxIdsResponseItem {
                    identifier: format!("{ident}"),
                    identifier_kind: kind as i32,
                    inbox_id: Some(inbox_id_cloned.clone()),
                }],
            })
        }
    });

    let wrapper = ApiClientWrapper::new(mock_api, retry());

    let identity = IdentityStrategy::new("other_inbox_id".to_string(), ident, nonce, None);
    assert!(matches!(
        identity
            .initialize_identity(&wrapper, &store.mls_provider(), &scw_verifier)
            .await
            .unwrap_err(),
        IdentityError::NewIdentity(msg) if msg == "Inbox ID mismatch"
    ));
}

// Use the account_address associated inbox
#[xmtp_common::test]
async fn api_identity_happy_path() {
    let mut mock_api = MockApiClient::new();
    let tmpdb = tmp_path();
    let scw_verifier = MockSmartContractSignatureVerifier::new(true);

    let store = xmtp_db::TestDb::create_persistent_store(Some(tmpdb.clone())).await;
    let nonce = 0;
    let ident = generate_local_wallet().identifier();
    let inbox_id = ident.inbox_id(nonce).unwrap();

    let inbox_id_cloned = inbox_id.clone();
    mock_api.expect_get_inbox_ids().returning({
        let ident = ident.clone();
        move |_| {
            let kind: IdentifierKind = (&ident).into();
            Ok(GetInboxIdsResponse {
                responses: vec![GetInboxIdsResponseItem {
                    identifier: format!("{ident}"),
                    identifier_kind: kind as i32,
                    inbox_id: Some(inbox_id_cloned.clone()),
                }],
            })
        }
    });

    let wrapper = ApiClientWrapper::new(mock_api, retry());

    let identity = IdentityStrategy::new(inbox_id.clone(), ident, nonce, None);
    assert!(dbg!(
        identity
            .initialize_identity(&wrapper, &store.mls_provider(), &scw_verifier)
            .await
    )
    .is_ok());
}

// Use a stored identity as long as the inbox_id matches the one provided.
#[xmtp_common::test]
async fn stored_identity_happy_path() {
    let mock_api = MockApiClient::new();
    let tmpdb = tmp_path();
    let scw_verifier = MockSmartContractSignatureVerifier::new(true);

    let store = xmtp_db::TestDb::create_persistent_store(Some(tmpdb.clone())).await;

    let nonce = 0;
    let ident = generate_local_wallet().identifier();
    let inbox_id = ident.inbox_id(nonce).unwrap();

    let stored: StoredIdentity = (&Identity {
        inbox_id: inbox_id.clone(),
        installation_keys: XmtpInstallationCredential::new(),
        credential: Credential::new(CredentialType::Basic, rand_vec::<24>()),
        signature_request: None,
        is_ready: AtomicBool::new(true),
    })
        .try_into()
        .unwrap();

    stored.store(&store.conn()).unwrap();
    let wrapper = ApiClientWrapper::new(mock_api, retry());
    let identity = IdentityStrategy::new(inbox_id.clone(), ident, nonce, None);
    assert!(identity
        .initialize_identity(&wrapper, &store.mls_provider(), &scw_verifier)
        .await
        .is_ok());
}

#[xmtp_common::test]
async fn stored_identity_mismatch() {
    let mock_api = MockApiClient::new();
    let scw_verifier = MockSmartContractSignatureVerifier::new(true);

    let nonce = 0;
    let ident = generate_local_wallet().identifier();
    let stored_inbox_id = ident.inbox_id(nonce).unwrap();

    let tmpdb = tmp_path();
    let store = xmtp_db::TestDb::create_persistent_store(Some(tmpdb.clone())).await;

    let stored: StoredIdentity = (&Identity {
        inbox_id: stored_inbox_id.clone(),
        installation_keys: Default::default(),
        credential: Credential::new(CredentialType::Basic, rand_vec::<24>()),
        signature_request: None,
        is_ready: AtomicBool::new(true),
    })
        .try_into()
        .unwrap();

    stored.store(&store.conn()).unwrap();

    let wrapper = ApiClientWrapper::new(mock_api, retry());

    let inbox_id = "inbox_id".to_string();
    let identity = IdentityStrategy::new(inbox_id.clone(), ident, nonce, None);
    let err = identity
        .initialize_identity(&wrapper, &store.mls_provider(), &scw_verifier)
        .await
        .unwrap_err();

    assert!(
        matches!(err, IdentityError::InboxIdMismatch { id, stored } if id == inbox_id && stored == stored_inbox_id)
    );
}

#[xmtp_common::test]
async fn identity_persistence_test() {
    let tmpdb = tmp_path();
    let wallet = &generate_local_wallet();

    // Generate a new Wallet + Store
    let store_a = xmtp_db::TestDb::create_persistent_store(Some(tmpdb.clone())).await;

    let nonce = 1;
    let ident = wallet.identifier();
    let inbox_id = ident.inbox_id(nonce).unwrap();

    let client_a = Client::builder(IdentityStrategy::new(
        inbox_id.clone(),
        wallet.identifier(),
        nonce,
        None,
    ))
    .api_client(
        <TestClient as XmtpTestClient>::create_local()
            .build()
            .await
            .unwrap(),
    )
    .store(store_a)
    .with_scw_verifier(MockSmartContractSignatureVerifier::new(true));
    let client_a = client_a.build().await.unwrap();

    register_client(&client_a, wallet).await;
    assert!(client_a.identity().is_ready());

    let keybytes_a = client_a.installation_public_key().to_vec();
    drop(client_a);

    // Reload the existing store and wallet
    let store_b = xmtp_db::TestDb::create_persistent_store(Some(tmpdb.clone())).await;

    let client_b = Client::builder(IdentityStrategy::new(
        inbox_id,
        wallet.identifier(),
        nonce,
        None,
    ))
    .api_client(
        <TestClient as XmtpTestClient>::create_local()
            .build()
            .await
            .unwrap(),
    )
    .store(store_b)
    .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
    .build()
    .await
    .unwrap();
    let keybytes_b = client_b.installation_public_key().to_vec();
    drop(client_b);

    // Ensure the persistence was used to store the generated keys
    assert_eq!(keybytes_a, keybytes_b);

    // Create a new wallet and store
    // TODO: Need to return error if the found identity doesn't match the provided arguments
    // let store_c =
    //     EncryptedMessageStore::new_unencrypted(StorageOption::Persistent(tmpdb.clone()))
    //         .unwrap();

    // ClientBuilder::new(IdentityStrategy::new(
    //     generate_local_wallet().get_address(),
    //     None,
    // ))
    // .api_client(<TestClient as XmtpTestClient>::create_local().build().await)
    // .store(store_c)
    // .build()
    // .await
    // .expect_err("Testing expected mismatch error");

    // Use cached only strategy
    let store_d = xmtp_db::TestDb::create_persistent_store(Some(tmpdb.clone())).await;
    let client_d = Client::builder(IdentityStrategy::CachedOnly)
        .api_client(
            <TestClient as XmtpTestClient>::create_local()
                .build()
                .await
                .unwrap(),
        )
        .store(store_d)
        .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
        .build()
        .await
        .unwrap();
    assert_eq!(client_d.installation_public_key().to_vec(), keybytes_a);
}
