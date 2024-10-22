use thiserror::Error;
use tracing::debug;

use crate::{
    api::ApiClientWrapper,
    client::Client,
    identity::{Identity, IdentityStrategy},
    identity_updates::load_identity_updates,
    retry::Retry,
    storage::EncryptedMessageStore,
    StorageError, XmtpApi,
};
use xmtp_cryptography::signature::AddressValidationError;
use xmtp_id::scw_verifier::{RemoteSignatureVerifier, SmartContractSignatureVerifier};
use xmtp_proto::api_client::BoxedApiClient;

#[derive(Error, Debug)]
pub enum ClientBuilderError {
    #[error(transparent)]
    AddressValidation(#[from] AddressValidationError),

    #[error("Missing parameter: {parameter}")]
    MissingParameter { parameter: &'static str },
    #[error(transparent)]
    ClientError(#[from] crate::client::ClientError),

    // #[error("Failed to serialize/deserialize state for persistence: {source}")]
    // SerializationError { source: serde_json::Error },
    #[error("Database was configured with a different wallet")]
    StoredIdentityMismatch,

    #[error("Inbox ID mismatch with address")]
    InboxIdMismatch,
    #[error("Uncovered Case")]
    UncoveredCase,
    #[error("Storage Error")]
    StorageError(#[from] StorageError),
    #[error(transparent)]
    Identity(#[from] crate::identity::IdentityError),
    #[error(transparent)]
    WrappedApiError(#[from] crate::api::WrappedApiError),
    #[error(transparent)]
    GroupError(#[from] crate::groups::GroupError),
    #[error(transparent)]
    ApiError(#[from] xmtp_proto::api_client::Error),
}

pub struct ClientBuilder<V = RemoteSignatureVerifier> {
    api_client: Option<BoxedApiClient>,
    identity: Option<Identity>,
    store: Option<EncryptedMessageStore>,
    identity_strategy: IdentityStrategy,
    history_sync_url: Option<String>,
    app_version: Option<String>,
    scw_verifier: Option<V>,
}

impl<V> Client<V> {
    /// Ge tthe builder for this [`Client`]
    pub fn builder(strategy: IdentityStrategy) -> ClientBuilder<V> {
        ClientBuilder::<V>::new(strategy)
    }
}

impl<V> ClientBuilder<V> {
    pub fn new(strategy: IdentityStrategy) -> Self {
        Self {
            api_client: None,
            identity: None,
            store: None,
            identity_strategy: strategy,
            history_sync_url: None,
            app_version: None,
            scw_verifier: None,
        }
    }

    pub fn api_client(mut self, api_client: BoxedApiClient) -> Self {
        self.api_client = Some(api_client);
        self
    }

    pub fn identity(mut self, identity: Identity) -> Self {
        self.identity = Some(identity);
        self
    }

    pub fn store(mut self, store: EncryptedMessageStore) -> Self {
        self.store = Some(store);
        self
    }

    pub fn history_sync_url(mut self, url: &str) -> Self {
        self.history_sync_url = Some(url.into());
        self
    }

    pub fn app_version(mut self, version: String) -> Self {
        self.app_version = Some(version);
        self
    }

    pub fn scw_signature_verifier(mut self, verifier: V) -> Self {
        self.scw_verifier = Some(verifier);
        self
    }
}

impl<V> ClientBuilder<V>
where
    V: SmartContractSignatureVerifier + Clone,
{
    /// Build with a custom smart contract wallet verifier
    pub async fn build_with_verifier(self) -> Result<Client<V>, ClientBuilderError> {
        inner_build(self).await
    }
}

impl ClientBuilder<RemoteSignatureVerifier> {
    /// Build with the default [`RemoteSignatureVerifier`]
    pub async fn build(mut self) -> Result<Client, ClientBuilderError> {
        let api_client =
            self.api_client
                .clone()
                .take()
                .ok_or(ClientBuilderError::MissingParameter {
                    parameter: "api_client",
                })?;
        self = self.scw_signature_verifier(RemoteSignatureVerifier::new(api_client));
        inner_build::<RemoteSignatureVerifier>(self).await
    }
}

async fn inner_build<V>(client: ClientBuilder<V>) -> Result<Client<V>, ClientBuilderError>
where
    V: SmartContractSignatureVerifier + Clone,
{
    let ClientBuilder {
        mut api_client,
        mut store,
        identity_strategy,
        #[cfg(feature = "message-history")]
        history_sync_url,
        app_version,
        mut scw_verifier,
        ..
    } = client;

    debug!("Building client");
    let mut api_client = api_client
        .take()
        .ok_or(ClientBuilderError::MissingParameter {
            parameter: "api_client",
        })?;

    api_client.set_libxmtp_version(env!("CARGO_PKG_VERSION").to_string())?;
    if let Some(app_version) = app_version {
        api_client.set_app_version(app_version)?;
    }

    let scw_verifier = scw_verifier
        .take()
        .ok_or(ClientBuilderError::MissingParameter {
            parameter: "scw_verifier",
        })?;

    let api_client_wrapper = ApiClientWrapper::new(api_client, Retry::default());
    let store = store
        .take()
        .ok_or(ClientBuilderError::MissingParameter { parameter: "store" })?;
    debug!("Initializing identity");

    let identity = identity_strategy
        .initialize_identity(&api_client_wrapper, &store, &scw_verifier)
        .await?;

    // get sequence_id from identity updates and loaded into the DB
    load_identity_updates(
        &api_client_wrapper,
        &store.conn()?,
        vec![identity.clone().inbox_id],
    )
    .await?;

    #[cfg(feature = "message-history")]
    let client = Client::new(
        api_client_wrapper,
        identity,
        store,
        scw_verifier,
        history_sync_url,
    );

    #[cfg(not(feature = "message-history"))]
    let client = Client::new(api_client_wrapper, identity, store, scw_verifier);

    Ok(client)
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);
    use std::sync::atomic::AtomicBool;

    use crate::api::ApiClientWrapper;
    use crate::builder::ClientBuilderError;
    use crate::identity::IdentityError;
    use crate::retry::Retry;
    use crate::utils::test::TestClient;
    use crate::XmtpApi;
    use crate::{
        api::test_utils::*, identity::Identity, storage::identity::StoredIdentity,
        utils::test::rand_vec, Store,
    };

    use openmls::credentials::{Credential, CredentialType};
    use openmls_basic_credential::SignatureKeyPair;
    use openmls_traits::types::SignatureScheme;
    use prost::Message;
    use xmtp_cryptography::utils::{generate_local_wallet, rng};
    use xmtp_id::associations::test_utils::MockSmartContractSignatureVerifier;
    use xmtp_id::associations::unverified::{
        UnverifiedRecoverableEcdsaSignature, UnverifiedSignature,
    };
    use xmtp_id::associations::ValidatedLegacySignedPublicKey;
    use xmtp_id::associations::{generate_inbox_id, test_utils::rand_u64};
    use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
    use xmtp_proto::xmtp::identity::api::v1::{
        get_inbox_ids_response::Response as GetInboxIdsResponseItem, GetInboxIdsResponse,
    };
    use xmtp_proto::xmtp::message_contents::signature::WalletEcdsaCompact;
    use xmtp_proto::xmtp::message_contents::signed_private_key::{Secp256k1, Union};
    use xmtp_proto::xmtp::message_contents::unsigned_public_key::{self, Secp256k1Uncompressed};
    use xmtp_proto::xmtp::message_contents::{
        signature, Signature, SignedPrivateKey, SignedPublicKey, UnsignedPublicKey,
    };

    use super::{ClientBuilder, IdentityStrategy};
    use crate::{
        storage::{EncryptedMessageStore, StorageOption},
        utils::test::tmp_path,
        Client, InboxOwner,
    };

    async fn register_client<V: SmartContractSignatureVerifier + Clone>(
        client: &Client<V>,
        owner: &impl InboxOwner,
    ) {
        let mut signature_request = client.context.signature_request().unwrap();
        let signature_text = signature_request.signature_text();
        let scw_verifier = MockSmartContractSignatureVerifier::new(true);
        signature_request
            .add_signature(
                UnverifiedSignature::RecoverableEcdsa(UnverifiedRecoverableEcdsaSignature::new(
                    owner.sign(&signature_text).unwrap().into(),
                )),
                &scw_verifier,
            )
            .await
            .unwrap();

        client.register_identity(signature_request).await.unwrap();
    }

    /// Generate a random legacy key proto bytes and corresponding account address.
    async fn generate_random_legacy_key() -> (Vec<u8>, String) {
        let wallet = generate_local_wallet();
        let address = wallet.get_address();
        let created_ns = rand_u64();
        let secret_key = ethers::core::k256::ecdsa::SigningKey::random(&mut rng());
        let public_key = ethers::core::k256::ecdsa::VerifyingKey::from(&secret_key);
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
        let signed_public_key: Vec<u8> = wallet.sign(&message).unwrap().into();
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

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn builder_test() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;
        assert!(!client.installation_public_key().is_empty());
    }

    // Test client creation using various identity strategies that creates new inboxes
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
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
                    IdentityStrategy::CreateIfNotFound(
                        generate_inbox_id(&legacy_account_address, &1),
                        legacy_account_address.clone(),
                        1,
                        Some(legacy_key),
                    )
                },
                err: Some("Nonce must be 0 if legacy key is provided".to_string()),
            },
            IdentityStrategyTestCase {
                strategy: {
                    let (legacy_key, legacy_account_address) = generate_random_legacy_key().await;
                    IdentityStrategy::CreateIfNotFound(
                        generate_inbox_id(&legacy_account_address, &1),
                        legacy_account_address.clone(),
                        0,
                        Some(legacy_key),
                    )
                },
                err: Some("Inbox ID doesn't match nonce & address".to_string()),
            },
            IdentityStrategyTestCase {
                strategy: {
                    let (legacy_key, legacy_account_address) = generate_random_legacy_key().await;
                    IdentityStrategy::CreateIfNotFound(
                        generate_inbox_id(&legacy_account_address, &0),
                        legacy_account_address.clone(),
                        0,
                        Some(legacy_key),
                    )
                },
                err: None,
            },
            // non-legacy cases
            IdentityStrategyTestCase {
                strategy: {
                    let account_address = generate_local_wallet().get_address();
                    IdentityStrategy::CreateIfNotFound(
                        generate_inbox_id(&account_address, &1),
                        account_address.clone(),
                        0,
                        None,
                    )
                },
                err: Some("Inbox ID doesn't match nonce & address".to_string()),
            },
            IdentityStrategyTestCase {
                strategy: {
                    let nonce = 1;
                    let account_address = generate_local_wallet().get_address();
                    IdentityStrategy::CreateIfNotFound(
                        generate_inbox_id(&account_address, &nonce),
                        account_address.clone(),
                        nonce,
                        None,
                    )
                },
                err: None,
            },
            IdentityStrategyTestCase {
                strategy: {
                    let nonce = 0;
                    let account_address = generate_local_wallet().get_address();
                    IdentityStrategy::CreateIfNotFound(
                        generate_inbox_id(&account_address, &nonce),
                        account_address.clone(),
                        nonce,
                        None,
                    )
                },
                err: None,
            },
        ];

        for test_case in identity_strategies_test_cases {
            let result = ClientBuilder::new(test_case.strategy)
                .temp_store()
                .await
                .api_client(Box::new(TestClient::create_local().await))
                .scw_signature_verifier(MockSmartContractSignatureVerifier::new(true))
                .build_with_verifier()
                .await;

            if let Some(err_string) = test_case.err {
                assert!(result.is_err());
                assert!(matches!(
                    result,
                    Err(ClientBuilderError::Identity(IdentityError::NewIdentity(err))) if err == err_string
                ));
            } else {
                assert!(result.is_ok());
            }
        }
    }

    // First, create a client1 using legacy key and then test following cases:
    // - create client2 from same db with [IdentityStrategy::CachedOnly]
    // - create client3 from same db with [IdentityStrategy::CreateIfNotFound]
    // - create client4 with different db.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_2nd_time_client_creation() {
        let (legacy_key, legacy_account_address) = generate_random_legacy_key().await;
        let identity_strategy = IdentityStrategy::CreateIfNotFound(
            generate_inbox_id(&legacy_account_address, &0),
            legacy_account_address.clone(),
            0,
            Some(legacy_key.clone()),
        );
        let store = EncryptedMessageStore::new(
            StorageOption::Persistent(tmp_path()),
            EncryptedMessageStore::generate_enc_key(),
        )
        .await
        .unwrap();

        let client1 = ClientBuilder::new(identity_strategy.clone())
            .store(store.clone())
            .api_client(Box::new(TestClient::create_local().await))
            .scw_signature_verifier(MockSmartContractSignatureVerifier::new(true))
            .build_with_verifier()
            .await
            .unwrap();
        assert!(client1.context.signature_request().is_none());

        let client2 = ClientBuilder::new(IdentityStrategy::CachedOnly)
            .store(store.clone())
            .api_client(Box::new(TestClient::create_local().await))
            .scw_signature_verifier(MockSmartContractSignatureVerifier::new(true))
            .build_with_verifier()
            .await
            .unwrap();
        assert!(client2.context.signature_request().is_none());
        assert!(client1.inbox_id() == client2.inbox_id());
        assert!(client1.installation_public_key() == client2.installation_public_key());

        let client3 = ClientBuilder::new(IdentityStrategy::CreateIfNotFound(
            generate_inbox_id(&legacy_account_address, &0),
            legacy_account_address.to_string(),
            0,
            None,
        ))
        .store(store.clone())
        .api_client(Box::new(TestClient::create_local().await))
        .scw_signature_verifier(MockSmartContractSignatureVerifier::new(true))
        .build_with_verifier()
        .await
        .unwrap();
        assert!(client3.context.signature_request().is_none());
        assert!(client1.inbox_id() == client3.inbox_id());
        assert!(client1.installation_public_key() == client3.installation_public_key());

        let client4 = ClientBuilder::new(IdentityStrategy::CreateIfNotFound(
            generate_inbox_id(&legacy_account_address, &0),
            legacy_account_address.to_string(),
            0,
            Some(legacy_key),
        ))
        .temp_store()
        .await
        .api_client(Box::new(TestClient::create_local().await))
        .scw_signature_verifier(MockSmartContractSignatureVerifier::new(true))
        .build_with_verifier()
        .await
        .unwrap();
        assert!(client4.context.signature_request().is_some());
        assert!(client1.inbox_id() == client4.inbox_id());
        assert!(client1.installation_public_key() != client4.installation_public_key());
    }

    // Should return error if inbox associated with given account_address doesn't match the provided one.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn api_identity_mismatch() {
        let mut mock_api = Box::new(MockApiClient::new());
        let tmpdb = tmp_path();
        let scw_verifier = MockSmartContractSignatureVerifier::new(true);

        let store = EncryptedMessageStore::new(
            StorageOption::Persistent(tmpdb),
            EncryptedMessageStore::generate_enc_key(),
        )
        .await
        .unwrap();
        let nonce = 0;
        let address = generate_local_wallet().get_address();
        let inbox_id = generate_inbox_id(&address, &nonce);

        let address_cloned = address.clone();
        let inbox_id_cloned = inbox_id.clone();
        mock_api.expect_get_inbox_ids().returning(move |_| {
            Ok(GetInboxIdsResponse {
                responses: vec![GetInboxIdsResponseItem {
                    address: address_cloned.clone(),
                    inbox_id: Some(inbox_id_cloned.clone()),
                }],
            })
        });

        let wrapper = ApiClientWrapper::new(mock_api, Retry::default());

        let identity =
            IdentityStrategy::CreateIfNotFound("other_inbox_id".to_string(), address, nonce, None);
        assert!(matches!(
            identity
                .initialize_identity(&wrapper, &store, &scw_verifier)
                .await
                .unwrap_err(),
            IdentityError::NewIdentity(msg) if msg == "Inbox ID mismatch"
        ));
    }

    // Use the account_address associated inbox
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn api_identity_happy_path() {
        let mut mock_api = Box::new(MockApiClient::new());
        let tmpdb = tmp_path();
        let scw_verifier = MockSmartContractSignatureVerifier::new(true);

        let store = EncryptedMessageStore::new(
            StorageOption::Persistent(tmpdb),
            EncryptedMessageStore::generate_enc_key(),
        )
        .await
        .unwrap();
        let nonce = 0;
        let address = generate_local_wallet().get_address();
        let inbox_id = generate_inbox_id(&address, &nonce);

        let address_cloned = address.clone();
        let inbox_id_cloned = inbox_id.clone();
        mock_api.expect_get_inbox_ids().returning(move |_| {
            Ok(GetInboxIdsResponse {
                responses: vec![GetInboxIdsResponseItem {
                    address: address_cloned.clone(),
                    inbox_id: Some(inbox_id_cloned.clone()),
                }],
            })
        });

        let wrapper = ApiClientWrapper::new(mock_api, Retry::default());

        let identity = IdentityStrategy::CreateIfNotFound(inbox_id.clone(), address, nonce, None);
        assert!(dbg!(
            identity
                .initialize_identity(&wrapper, &store, &scw_verifier)
                .await
        )
        .is_ok());
    }

    // Use a stored identity as long as the inbox_id matches the one provided.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn stored_identity_happy_path() {
        let mock_api = Box::new(MockApiClient::new());
        let tmpdb = tmp_path();
        let scw_verifier = MockSmartContractSignatureVerifier::new(true);

        let store = EncryptedMessageStore::new(
            StorageOption::Persistent(tmpdb),
            EncryptedMessageStore::generate_enc_key(),
        )
        .await
        .unwrap();
        let nonce = 0;
        let address = generate_local_wallet().get_address();
        let inbox_id = generate_inbox_id(&address, &nonce);

        let stored: StoredIdentity = (&Identity {
            inbox_id: inbox_id.clone(),
            installation_keys: SignatureKeyPair::new(SignatureScheme::ED25519).unwrap(),
            credential: Credential::new(CredentialType::Basic, rand_vec()),
            signature_request: None,
            is_ready: AtomicBool::new(true),
        })
            .try_into()
            .unwrap();

        stored.store(&store.conn().unwrap()).unwrap();
        let wrapper = ApiClientWrapper::new(mock_api, Retry::default());
        let identity = IdentityStrategy::CreateIfNotFound(inbox_id.clone(), address, nonce, None);
        assert!(identity
            .initialize_identity(&wrapper, &store, &scw_verifier)
            .await
            .is_ok());
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn stored_identity_mismatch() {
        let mock_api = Box::new(MockApiClient::new());
        let scw_verifier = MockSmartContractSignatureVerifier::new(true);

        let nonce = 0;
        let address = generate_local_wallet().get_address();
        let stored_inbox_id = generate_inbox_id(&address, &nonce);

        let tmpdb = tmp_path();
        let store = EncryptedMessageStore::new(
            StorageOption::Persistent(tmpdb),
            EncryptedMessageStore::generate_enc_key(),
        )
        .await
        .unwrap();

        let stored: StoredIdentity = (&Identity {
            inbox_id: stored_inbox_id.clone(),
            installation_keys: SignatureKeyPair::new(SignatureScheme::ED25519).unwrap(),
            credential: Credential::new(CredentialType::Basic, rand_vec()),
            signature_request: None,
            is_ready: AtomicBool::new(true),
        })
            .try_into()
            .unwrap();

        stored.store(&store.conn().unwrap()).unwrap();

        let wrapper = ApiClientWrapper::new(mock_api, Retry::default());

        let inbox_id = "inbox_id".to_string();
        let identity =
            IdentityStrategy::CreateIfNotFound(inbox_id.clone(), address.clone(), nonce, None);
        let err = identity
            .initialize_identity(&wrapper, &store, &scw_verifier)
            .await
            .unwrap_err();

        assert!(
            matches!(err, IdentityError::InboxIdMismatch { id, stored } if id == inbox_id && stored == stored_inbox_id)
        );
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn identity_persistence_test() {
        let tmpdb = tmp_path();
        let wallet = &generate_local_wallet();
        let db_key = EncryptedMessageStore::generate_enc_key();

        // Generate a new Wallet + Store
        let store_a = EncryptedMessageStore::new(StorageOption::Persistent(tmpdb.clone()), db_key)
            .await
            .unwrap();

        let nonce = 1;
        let inbox_id = generate_inbox_id(&wallet.get_address(), &nonce);
        let client_a = Client::builder(IdentityStrategy::CreateIfNotFound(
            inbox_id.clone(),
            wallet.get_address(),
            nonce,
            None,
        ))
        .api_client(Box::new(TestClient::create_local().await))
        .store(store_a)
        .scw_signature_verifier(MockSmartContractSignatureVerifier::new(true))
        .build_with_verifier()
        .await
        .unwrap();

        register_client(&client_a, wallet).await;
        assert!(client_a.identity().is_ready());

        let keybytes_a = client_a.installation_public_key();
        drop(client_a);

        // Reload the existing store and wallet
        let store_b = EncryptedMessageStore::new(StorageOption::Persistent(tmpdb.clone()), db_key)
            .await
            .unwrap();

        let client_b = Client::builder(IdentityStrategy::CreateIfNotFound(
            inbox_id,
            wallet.get_address(),
            nonce,
            None,
        ))
        .api_client(Box::new(TestClient::create_local().await))
        .store(store_b)
        .scw_signature_verifier(MockSmartContractSignatureVerifier::new(true))
        .build_with_verifier()
        .await
        .unwrap();
        let keybytes_b = client_b.installation_public_key();
        drop(client_b);

        // Ensure the persistence was used to store the generated keys
        assert_eq!(keybytes_a, keybytes_b);

        // Create a new wallet and store
        // TODO: Need to return error if the found identity doesn't match the provided arguments
        // let store_c =
        //     EncryptedMessageStore::new_unencrypted(StorageOption::Persistent(tmpdb.clone()))
        //         .unwrap();

        // ClientBuilder::new(IdentityStrategy::CreateIfNotFound(
        //     generate_local_wallet().get_address(),
        //     None,
        // ))
        // .api_client(TestClient::create_local().await)
        // .store(store_c)
        // .build()
        // .await
        // .expect_err("Testing expected mismatch error");

        // Use cached only strategy
        let store_d = EncryptedMessageStore::new(StorageOption::Persistent(tmpdb.clone()), db_key)
            .await
            .unwrap();
        let client_d = Client::builder(IdentityStrategy::CachedOnly)
            .api_client(Box::new(TestClient::create_local().await))
            .store(store_d)
            .scw_signature_verifier(MockSmartContractSignatureVerifier::new(true))
            .build_with_verifier()
            .await
            .unwrap();
        assert_eq!(client_d.installation_public_key(), keybytes_a);
    }

    /// anvil cannot be used in WebAssembly
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    #[cfg(not(target_arch = "wasm32"))]
    async fn test_remote_is_valid_signature() {
        use ethers::{
            abi::Token,
            signers::{LocalWallet, Signer},
            types::{Bytes, H256, U256},
            utils::hash_message,
        };
        use std::sync::Arc;
        use xmtp_id::associations::AccountId;
        use xmtp_id::utils::test::CoinbaseSmartWallet;
        use xmtp_id::{
            associations::unverified::NewUnverifiedSmartContractWalletSignature,
            utils::test::with_docker_smart_contracts,
        };

        with_docker_smart_contracts(
            |anvil_meta, _provider, client, smart_contracts| async move {
                let wallet: LocalWallet = anvil_meta.keys[0].clone().into();

                let owners = vec![Bytes::from(H256::from(wallet.address()).0.to_vec())];

                let scw_factory = smart_contracts.coinbase_smart_wallet_factory();
                let nonce = U256::from(0);

                let scw_addr = scw_factory
                    .get_address(owners.clone(), nonce)
                    .await
                    .unwrap();

                let contract_call = scw_factory.create_account(owners.clone(), nonce);

                contract_call.send().await.unwrap().await.unwrap();
                let account_id = AccountId::new_evm(anvil_meta.chain_id, format!("{scw_addr:?}"));
                let account_id_string: String = account_id.clone().into();

                let identity_strategy = IdentityStrategy::CreateIfNotFound(
                    generate_inbox_id(&account_id_string, &0),
                    account_id_string,
                    0,
                    None,
                );

                let xmtp_client = Client::<TestClient>::builder(identity_strategy)
                    .temp_store()
                    .await
                    .local_client()
                    .await
                    .build()
                    .await
                    .unwrap();

                let smart_wallet = CoinbaseSmartWallet::new(
                    scw_addr,
                    Arc::new(client.with_signer(wallet.clone().with_chain_id(anvil_meta.chain_id))),
                );

                let mut signature_request = xmtp_client.context.signature_request().unwrap();
                let signature_text = signature_request.signature_text();
                let hash_to_sign = hash_message(signature_text);
                let replay_safe_hash = smart_wallet
                    .replay_safe_hash(hash_to_sign.into())
                    .call()
                    .await
                    .unwrap();
                let signature_bytes: Bytes = ethers::abi::encode(&[Token::Tuple(vec![
                    Token::Uint(U256::from(0)),
                    Token::Bytes(wallet.sign_hash(replay_safe_hash.into()).unwrap().to_vec()),
                ])])
                .into();

                signature_request
                    .add_new_unverified_smart_contract_signature(
                        NewUnverifiedSmartContractWalletSignature::new(
                            signature_bytes.to_vec(),
                            account_id,
                            None,
                        ),
                        &xmtp_client.scw_verifier,
                    )
                    .await
                    .unwrap();

                xmtp_client
                    .register_identity(signature_request)
                    .await
                    .unwrap();
            },
        )
        .await;
    }
}
