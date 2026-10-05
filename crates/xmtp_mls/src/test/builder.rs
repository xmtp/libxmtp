use std::sync::atomic::AtomicBool;

use crate::builder::ClientBuilderError;
use crate::context::XmtpSharedContext;
use crate::identity::Identity;
use crate::identity::IdentityError;
use crate::utils::DefaultTestClientCreator;
use xmtp_api::ApiClientWrapper;
use xmtp_api_backend::MockBackendClient;
use xmtp_common::{ExponentialBackoff, Retry, rand_vec, tmp_path};
use xmtp_db::XmtpTestDb;
use xmtp_db::sql_key_store::SqlKeyStore;
use xmtp_db::{Store, identity::StoredIdentity};

use openmls::credentials::{Credential, CredentialType};
use xmtp_cryptography::XmtpInstallationCredential;
use xmtp_cryptography::utils::generate_local_wallet;
use xmtp_id::associations::Identifier;
use xmtp_id::associations::test_utils::{MockSmartContractSignatureVerifier, WalletTestExt};
use xmtp_proto::api_client::ApiBuilder;
use xmtp_proto::api_client::XmtpTestClient;

use xmtp_proto::backend_v1::{
    GetInboxIdsResponse, get_inbox_ids_response::Response as GetInboxIdsResponseItem,
};

use xmtp_proto::xmtp::identity::associations::{
    CreateInbox as CreateInboxProto, IdentifierKind, IdentityAction, IdentityUpdate,
    RecoverableEcdsaSignature, Signature as ProtoSignature,
    identity_action::Kind as IdentityActionKindProto, signature::Signature as SignatureEnum,
};

use crate::{Client, InboxOwner};
use crate::{builder::ClientBuilder, identity::IdentityStrategy};

async fn register_client<C: XmtpSharedContext>(client: &Client<C>, owner: &impl InboxOwner) {
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
        IdentityStrategyTestCase {
            strategy: {
                let ident = generate_local_wallet().get_identifier().unwrap();
                IdentityStrategy::new(ident.inbox_id(1).unwrap(), ident, 0)
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
                )
            },
            err: None,
        },
        IdentityStrategyTestCase {
            strategy: {
                let nonce = 0;
                let account_ident = generate_local_wallet().get_identifier().unwrap();
                IdentityStrategy::new(account_ident.inbox_id(nonce).unwrap(), account_ident, nonce)
            },
            err: None,
        },
    ];

    for test_case in identity_strategies_test_cases {
        let result = Client::builder(test_case.strategy)
            .temp_store()
            .await
            .api_client(DefaultTestClientCreator::create().build().unwrap())
            .default_mls_store()
            .unwrap()
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

// First, create and register client1 with a wallet, then test the following cases:
// - create client2 from same db with [IdentityStrategy::CachedOnly]
// - create client3 from same db with [IdentityStrategy::CreateIfNotFound]
// - create client4 with different db.
#[xmtp_common::test]
async fn test_2nd_time_client_creation() {
    let wallet = generate_local_wallet();
    let ident = wallet.identifier();
    let inbox_id = ident.inbox_id(0).unwrap();

    let identity_strategy = IdentityStrategy::new(inbox_id.clone(), ident.clone(), 0);
    let store = xmtp_db::TestDb::create_persistent_store(None).await;

    let client1 = Client::builder(identity_strategy.clone())
        .store(store.clone())
        .api_client(DefaultTestClientCreator::create().build().unwrap())
        .default_mls_store()
        .unwrap()
        .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
        .build()
        .await
        .unwrap();
    register_client(&client1, &wallet).await;

    let client2 = Client::builder(IdentityStrategy::CachedOnly)
        .store(store.clone())
        .api_client(DefaultTestClientCreator::create().build().unwrap())
        .default_mls_store()
        .unwrap()
        .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
        .build()
        .await
        .unwrap();
    assert!(client2.context.signature_request().is_none());
    assert!(client1.inbox_id() == client2.inbox_id());
    assert!(client1.installation_public_key() == client2.installation_public_key());

    let client3 = Client::builder(IdentityStrategy::new(inbox_id.clone(), ident, 0))
        .store(store.clone())
        .api_client(DefaultTestClientCreator::create().build().unwrap())
        .default_mls_store()
        .unwrap()
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
        .api_client(DefaultTestClientCreator::create().build().unwrap())
        .default_mls_store()
        .unwrap()
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
    let mut mock_api = MockBackendClient::new();
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

    let identity = IdentityStrategy::new("other_inbox_id".to_string(), ident, nonce);
    assert!(matches!(
        identity
            .initialize_identity(&wrapper, &SqlKeyStore::new(&store.db()), &scw_verifier)
            .await
            .unwrap_err(),
        IdentityError::NewIdentity(msg) if msg == "Inbox ID mismatch"
    ));
}

// Use the account_address associated inbox
#[xmtp_common::test]
async fn api_identity_happy_path() {
    let mut mock_api = MockBackendClient::new();
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

    let mut wrapper = ApiClientWrapper::new(mock_api, retry());
    wrapper
        .api_client
        .raw_mut_for_test()
        .unwrap()
        .expect_query()
        .returning({
            let ident = ident.clone();
            let inbox_id = inbox_id.clone();
            move |req| {
                let kind: IdentifierKind = (&ident).into();

                let update = IdentityUpdate {
                    actions: vec![IdentityAction {
                        kind: Some(IdentityActionKindProto::CreateInbox(CreateInboxProto {
                            initial_identifier: format!("{ident}"),
                            nonce,
                            initial_identifier_signature: Some(ProtoSignature {
                                signature: Some(SignatureEnum::Erc191(RecoverableEcdsaSignature {
                                    bytes: vec![1; 65], // dummy but structurally valid
                                })),
                            }),
                            initial_identifier_kind: kind as i32,
                            relying_party: None,
                        })),
                    }],
                    client_timestamp_ns: 0,
                    inbox_id: inbox_id.clone(),
                };

                Ok(xmtp_proto::backend_v1::QueryResponse {
                    envelopes: vec![xmtp_proto::backend_v1::ServerEnvelope {
                        meta: Some(xmtp_proto::backend_v1::EnvelopeMeta {
                            topic: req.queries[0].topic.clone(),
                            cursor: Some(xmtp_proto::backend_v1::Cursor { sequence_id: 1 }),
                            server_ns: 0,
                            message_hash: Some(xmtp_proto::backend_v1::MessageHash {
                                hash: Some(xmtp_proto::backend_v1::message_hash::Hash::Sha256(
                                    vec![1; 32],
                                )),
                            }),
                            ..Default::default()
                        }),
                        envelope: Some(xmtp_proto::backend_v1::ClientEnvelope {
                            payload: Some(
                                xmtp_proto::backend_v1::client_envelope::Payload::IdentityUpdate(
                                    update,
                                ),
                            ),
                        }),
                    }],
                    continuation: Some(xmtp_proto::backend_v1::Continuation { has_more: false }),
                })
            }
        });

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
    let identity = IdentityStrategy::new(inbox_id.clone(), ident, nonce);
    assert!(
        dbg!(
            identity
                .initialize_identity(&wrapper, &SqlKeyStore::new(&store.db()), &scw_verifier)
                .await
        )
        .is_ok()
    );
}

// Use a stored identity as long as the inbox_id matches the one provided.
#[xmtp_common::test]
async fn stored_identity_happy_path() {
    let mock_api = MockBackendClient::new();
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
    let identity = IdentityStrategy::new(inbox_id.clone(), ident, nonce);
    assert!(
        identity
            .initialize_identity(&wrapper, &SqlKeyStore::new(&store.db()), &scw_verifier)
            .await
            .is_ok()
    );
}

#[xmtp_common::test]
async fn stored_identity_mismatch() {
    let mock_api = MockBackendClient::new();
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
    let identity = IdentityStrategy::new(inbox_id.clone(), ident, nonce);
    let err = identity
        .initialize_identity(&wrapper, &SqlKeyStore::new(&store.db()), &scw_verifier)
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
    ))
    .api_client(DefaultTestClientCreator::create().build().unwrap())
    .store(store_a)
    .default_mls_store()
    .unwrap()
    .with_scw_verifier(MockSmartContractSignatureVerifier::new(true));
    let client_a = client_a.build().await.unwrap();

    register_client(&client_a, wallet).await;
    assert!(client_a.identity().is_ready());

    let keybytes_a = client_a.installation_public_key().to_vec();
    drop(client_a);

    // Reload the existing store and wallet
    let store_b = xmtp_db::TestDb::create_persistent_store(Some(tmpdb.clone())).await;

    let client_b = Client::builder(IdentityStrategy::new(inbox_id, wallet.identifier(), nonce))
        .api_client(DefaultTestClientCreator::create().build().unwrap())
        .store(store_b)
        .default_mls_store()
        .unwrap()
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
    // .api_client(DefaultTestClientCreator::create().build().await)
    // .store(store_c)
    // .build()
    // .await
    // .expect_err("Testing expected mismatch error");

    // Use cached only strategy
    let store_d = xmtp_db::TestDb::create_persistent_store(Some(tmpdb.clone())).await;
    let client_d = Client::builder(IdentityStrategy::CachedOnly)
        .api_client(DefaultTestClientCreator::create().build().unwrap())
        .store(store_d)
        .default_mls_store()
        .unwrap()
        .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
        .build()
        .await
        .unwrap();
    assert_eq!(client_d.installation_public_key().to_vec(), keybytes_a);
}

// A stored identity trusts any identifier of its inbox ID, so a build that
// requires it checks the identifier against the inbox's association state.
#[xmtp_common::test(unwrap_try = true)]
async fn stored_identity_opens_only_for_an_identifier_of_its_inbox() {
    let tmpdb = tmp_path();
    let owner = generate_local_wallet();
    let nonce = 1;
    let inbox_id = owner.identifier().inbox_id(nonce)?;
    let builder = async |identifier: Identifier| {
        Client::builder(IdentityStrategy::new(inbox_id.clone(), identifier, nonce))
            .api_client(DefaultTestClientCreator::create().build().unwrap())
            .store(xmtp_db::TestDb::create_persistent_store(Some(tmpdb.clone())).await)
            .default_mls_store()
            .unwrap()
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
    };
    let client = builder(owner.identifier()).await.build().await?;
    register_client(&client, &owner).await;
    drop(client);

    let stranger = generate_local_wallet().identifier();
    let rejected = builder(stranger)
        .await
        .require_identifier_in_inbox()
        .build()
        .await;
    assert!(matches!(
        rejected,
        Err(ClientBuilderError::Identity(IdentityError::IdentifierNotInInbox { inbox_id: stored }))
            if stored == inbox_id
    ));

    builder(owner.identifier())
        .await
        .require_identifier_in_inbox()
        .build()
        .await?;
}

// verifies: PROC-036
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn client_creation_logs_omit_full_id_with_retained_envelope() {
    use std::sync::Arc;
    use tracing::instrument::WithSubscriber;
    use xmtp_db::incoming_envelope::{
        IncomingLimits, NetworkEntityKind, NewIncomingEnvelope, PendingBudget,
        QueryIncomingEnvelope, StreamTopic,
    };
    use xmtp_logging::{Level, LogRecord, LogSinkTarget, SinkError, test_logging::LogCapture};
    use xmtp_proto::types::Cursor;

    struct Capture(parking_lot::Mutex<Vec<LogRecord>>);
    impl LogSinkTarget for Capture {
        fn on_record(&self, record: LogRecord) -> Result<(), SinkError> {
            self.0.lock().push(record);
            Ok(())
        }
    }
    let path = tmp_path();
    let wallet = generate_local_wallet();
    let identifier = wallet.identifier();
    let client = Client::builder(IdentityStrategy::new(
        identifier.inbox_id(0)?,
        identifier,
        0,
    ))
    .api_client(DefaultTestClientCreator::create().build()?)
    .store(xmtp_db::TestDb::create_persistent_store(Some(path.clone())).await)
    .default_mls_store()?
    .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
    .with_disable_workers(true)
    .build()
    .await?;
    register_client(&client, &wallet).await;
    let full_id = hex::encode(client.installation_public_key());
    let topic = StreamTopic {
        entity_id: vec![42; 32],
        kind: NetworkEntityKind::Group,
    };
    let retained = NewIncomingEnvelope {
        sequence_id: Cursor(1),
        envelope: vec![1, 2, 3],
    };
    let budget = PendingBudget {
        rows: 8,
        bytes: 1024,
    };
    client.context.db().admit_ordered_batch(
        &topic,
        Cursor(0),
        std::slice::from_ref(&retained),
        IncomingLimits {
            batch: budget,
            topic: budget,
            kind: budget,
        },
    )?;
    assert!(
        client
            .context
            .db()
            .pending_envelope(&topic, Cursor(1))?
            .is_some()
    );
    client.close().await?;
    drop(client);

    let sink = Arc::new(Capture(parking_lot::Mutex::new(Vec::new())));
    let capture = LogCapture::with_sink(Level::Debug, Some(sink.clone()));
    let identifier = wallet.identifier();
    let reopened = Client::builder(IdentityStrategy::new(
        identifier.inbox_id(0)?,
        identifier,
        0,
    ))
    .api_client(DefaultTestClientCreator::create().build()?)
    .store(xmtp_db::TestDb::create_persistent_store(Some(path)).await)
    .default_mls_store()?
    .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
    .with_disable_workers(true)
    .build()
    .with_subscriber(capture.dispatch())
    .await?;
    assert_eq!(hex::encode(reopened.installation_public_key()), full_id);
    assert_eq!(
        reopened
            .context
            .db()
            .pending_envelope(&topic, Cursor(1))?
            .unwrap()
            .envelope,
        retained.envelope
    );
    {
        let records = sink.0.lock();
        records
            .iter()
            .find(|record| {
                record
                    .message
                    .contains(xmtp_common::Event::ClientCreated.metadata().doc)
            })
            .expect("reopen emits the creation event");
        let json_has_full_id = capture.output().contains(&full_id);
        assert!(
            records
                .iter()
                .any(|record| record.message == "Found existing identity in store")
        );
        let app_has_full_id = records.iter().any(|record| {
            record.message.contains(&full_id)
                || record.fields.values().any(|value| value.contains(&full_id))
        });
        assert_eq!(
            (json_has_full_id, app_has_full_id),
            (false, false),
            "creation logs include the full ID while an envelope remains pending"
        );
    }
    reopened.close().await?;
}
