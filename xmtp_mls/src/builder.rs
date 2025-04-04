use std::sync::Arc;

use thiserror::Error;
use tracing::debug;

use xmtp_cryptography::signature::IdentifierValidationError;
use xmtp_id::scw_verifier::{RemoteSignatureVerifier, SmartContractSignatureVerifier};

use crate::{
    client::Client,
    identity::{Identity, IdentityStrategy},
    identity_updates::load_identity_updates,
    StorageError, XmtpApi, XmtpOpenMlsProvider,
};
use xmtp_db::EncryptedMessageStore;

use xmtp_api::ApiClientWrapper;
use xmtp_common::Retry;

#[derive(Error, Debug)]
pub enum ClientBuilderError {
    #[error(transparent)]
    AddressValidation(#[from] IdentifierValidationError),

    #[error("Missing parameter: {parameter}")]
    MissingParameter { parameter: &'static str },
    #[error(transparent)]
    ClientError(#[from] crate::client::ClientError),

    // #[error("Failed to serialize/deserialize state for persistence: {source}")]
    // SerializationError { source: serde_json::Error },
    #[error("Database was configured with a different wallet")]
    StoredIdentityMismatch,

    #[error("Uncovered Case")]
    UncoveredCase,
    #[error("Storage Error")]
    StorageError(#[from] StorageError),
    #[error(transparent)]
    Identity(#[from] crate::identity::IdentityError),
    #[error(transparent)]
    WrappedApiError(#[from] xmtp_api::Error),
    #[error(transparent)]
    GroupError(#[from] crate::groups::GroupError),
    #[error(transparent)]
    Api(#[from] xmtp_proto::ApiError),
    #[error(transparent)]
    DeviceSync(#[from] crate::groups::device_sync::DeviceSyncError),
}

pub struct ClientBuilder<ApiClient, V> {
    api_client: Option<ApiClientWrapper<ApiClient>>,
    identity: Option<Identity>,
    store: Option<EncryptedMessageStore>,
    identity_strategy: IdentityStrategy,
    scw_verifier: Option<V>,

    device_sync_server_url: Option<String>,
    device_sync_worker_mode: SyncWorkerMode,
}

#[derive(Clone)]
pub enum SyncWorkerMode {
    Disabled,
    Enabled,
}

impl Client<(), ()> {
    /// Get the builder for this [`Client`]
    pub fn builder(strategy: IdentityStrategy) -> ClientBuilder<(), ()> {
        ClientBuilder::<(), ()>::new(strategy)
    }
}

impl<ApiClient, V> ClientBuilder<ApiClient, V> {
    #[tracing::instrument(level = "trace", skip_all)]
    pub fn new(strategy: IdentityStrategy) -> Self {
        Self {
            api_client: None,
            identity: None,
            store: None,
            identity_strategy: strategy,
            scw_verifier: None,
            device_sync_server_url: None,
            device_sync_worker_mode: SyncWorkerMode::Enabled,
        }
    }
}
impl<ApiClient, V> ClientBuilder<ApiClient, V> {
    pub async fn build(self) -> Result<Client<ApiClient, V>, ClientBuilderError>
    where
        ApiClient: XmtpApi + 'static + Send + Sync,
        V: SmartContractSignatureVerifier + 'static + Send + Sync,
    {
        let ClientBuilder {
            mut api_client,
            identity,
            mut store,
            identity_strategy,
            mut scw_verifier,

            device_sync_server_url,
            device_sync_worker_mode,
        } = self;

        let api = api_client
            .take()
            .ok_or(ClientBuilderError::MissingParameter {
                parameter: "api_client",
            })?;

        let scw_verifier = scw_verifier
            .take()
            .ok_or(ClientBuilderError::MissingParameter {
                parameter: "scw_verifier",
            })?;

        let store = store
            .take()
            .ok_or(ClientBuilderError::MissingParameter { parameter: "store" })?;

        let conn = store.conn()?;
        let provider = XmtpOpenMlsProvider::new(conn);
        let identity = if let Some(identity) = identity {
            identity
        } else {
            identity_strategy
                .initialize_identity(&api, &provider, &scw_verifier)
                .await?
        };

        debug!(
            inbox_id = identity.inbox_id(),
            installation_id = hex::encode(identity.installation_keys.public_bytes()),
            "Initialized identity"
        );
        // get sequence_id from identity updates and loaded into the DB
        load_identity_updates(
            &api,
            provider.conn_ref(),
            vec![identity.inbox_id.as_str()].as_slice(),
        )
        .await?;

        let client = Client::new(
            api,
            identity,
            store,
            scw_verifier,
            device_sync_server_url.clone(),
            device_sync_worker_mode.clone(),
        );

        // start workers
        if !matches!(device_sync_worker_mode, SyncWorkerMode::Disabled) {
            client.start_sync_worker();
        }
        client.start_disappearing_messages_cleaner_worker();

        Ok(client)
    }

    pub fn identity(self, identity: Identity) -> Self {
        Self {
            identity: Some(identity),
            ..self
        }
    }

    pub fn store(self, store: EncryptedMessageStore) -> Self {
        Self {
            store: Some(store),
            ..self
        }
    }

    pub fn device_sync_server_url(self, url: &str) -> Self {
        Self {
            device_sync_server_url: Some(url.into()),
            ..self
        }
    }

    pub fn device_sync_worker_mode(self, mode: SyncWorkerMode) -> Self {
        Self {
            device_sync_worker_mode: mode,
            ..self
        }
    }

    pub fn api_client<A>(self, api_client: A) -> ClientBuilder<A, V> {
        let cooldown = xmtp_common::ExponentialBackoff::builder()
            .duration(std::time::Duration::from_secs(3))
            .multiplier(3)
            .max_jitter(std::time::Duration::from_millis(100))
            .total_wait_max(std::time::Duration::from_secs(120))
            .build();

        let api_retry = Retry::builder().with_cooldown(cooldown).build();
        let wrapper = ApiClientWrapper::new(Arc::new(api_client), api_retry);
        ClientBuilder {
            api_client: Some(wrapper),
            identity: self.identity,
            identity_strategy: self.identity_strategy,
            scw_verifier: self.scw_verifier,
            store: self.store,

            device_sync_server_url: self.device_sync_server_url,
            device_sync_worker_mode: self.device_sync_worker_mode,
        }
    }

    pub fn with_scw_verifier<V2>(self, verifier: V2) -> ClientBuilder<ApiClient, V2> {
        ClientBuilder {
            api_client: self.api_client,
            identity: self.identity,
            identity_strategy: self.identity_strategy,
            scw_verifier: Some(verifier),
            store: self.store,

            device_sync_server_url: self.device_sync_server_url,
            device_sync_worker_mode: self.device_sync_worker_mode,
        }
    }

    /// Build the client with a default remote verifier
    /// requires the 'api' to be set.
    pub fn with_remote_verifier(
        self,
    ) -> Result<ClientBuilder<ApiClient, RemoteSignatureVerifier<ApiClient>>, ClientBuilderError>
    where
        ApiClient: Clone,
    {
        let api = self
            .api_client
            .clone()
            .ok_or(ClientBuilderError::MissingParameter {
                parameter: "api_client",
            })?;
        let remote_verifier = RemoteSignatureVerifier::new(api);

        Ok(ClientBuilder {
            api_client: self.api_client,
            identity: self.identity,
            identity_strategy: self.identity_strategy,
            scw_verifier: Some(remote_verifier),
            store: self.store,

            device_sync_server_url: self.device_sync_server_url,
            device_sync_worker_mode: self.device_sync_worker_mode,
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
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
    use xmtp_db::{identity::StoredIdentity, Store};

    use openmls::credentials::{Credential, CredentialType};
    use prost::Message;
    use xmtp_common::rand_u64;
    use xmtp_cryptography::utils::{generate_local_wallet, rng};
    use xmtp_cryptography::XmtpInstallationCredential;
    use xmtp_id::associations::test_utils::{MockSmartContractSignatureVerifier, WalletTestExt};
    use xmtp_id::associations::unverified::UnverifiedSignature;
    use xmtp_id::associations::{Identifier, ValidatedLegacySignedPublicKey};
    use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
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

    use super::{ClientBuilder, IdentityStrategy};
    use crate::{Client, InboxOwner};
    use xmtp_db::{EncryptedMessageStore, StorageOption};

    async fn register_client<C: XmtpApi, V: SmartContractSignatureVerifier>(
        client: &Client<C, V>,
        owner: &impl InboxOwner,
    ) {
        let mut signature_request = client.context.signature_request().unwrap();
        let signature_text = signature_request.signature_text();
        let scw_verifier = MockSmartContractSignatureVerifier::new(true);
        signature_request
            .add_signature(owner.sign(&signature_text).unwrap(), &scw_verifier)
            .await
            .unwrap();

        client.register_identity(signature_request).await.unwrap();
    }

    fn retry() -> Retry<ExponentialBackoff, ExponentialBackoff> {
        let strategy = ExponentialBackoff::default();
        Retry::builder().with_cooldown(strategy).build()
    }

    /// Generate a random legacy key proto bytes and corresponding account address.
    async fn generate_random_legacy_key() -> (Vec<u8>, String) {
        let wallet = generate_local_wallet();
        let ident = wallet.get_identifier().unwrap();
        let address = format!("{ident}");
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
                assert!(result.is_ok());
            }
        }
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
        let store = EncryptedMessageStore::new(
            StorageOption::Persistent(tmp_path()),
            EncryptedMessageStore::generate_enc_key(),
        )
        .await
        .unwrap();

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
        let tmpdb = tmp_path();
        let scw_verifier = MockSmartContractSignatureVerifier::new(true);

        let store = EncryptedMessageStore::new(
            StorageOption::Persistent(tmpdb),
            EncryptedMessageStore::generate_enc_key(),
        )
        .await
        .unwrap();
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

        let wrapper = ApiClientWrapper::new(mock_api.into(), retry());

        let identity = IdentityStrategy::new("other_inbox_id".to_string(), ident, nonce, None);
        assert!(matches!(
            identity
                .initialize_identity(&wrapper, &store.mls_provider().unwrap(), &scw_verifier)
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

        let store = EncryptedMessageStore::new(
            StorageOption::Persistent(tmpdb),
            EncryptedMessageStore::generate_enc_key(),
        )
        .await
        .unwrap();
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

        let wrapper = ApiClientWrapper::new(mock_api.into(), retry());

        let identity = IdentityStrategy::new(inbox_id.clone(), ident, nonce, None);
        assert!(dbg!(
            identity
                .initialize_identity(&wrapper, &store.mls_provider().unwrap(), &scw_verifier)
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

        let store = EncryptedMessageStore::new(
            StorageOption::Persistent(tmpdb),
            EncryptedMessageStore::generate_enc_key(),
        )
        .await
        .unwrap();
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

        stored.store(&store.conn().unwrap()).unwrap();
        let wrapper = ApiClientWrapper::new(mock_api.into(), retry());
        let identity = IdentityStrategy::new(inbox_id.clone(), ident, nonce, None);
        assert!(identity
            .initialize_identity(&wrapper, &store.mls_provider().unwrap(), &scw_verifier)
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
        let store = EncryptedMessageStore::new(
            StorageOption::Persistent(tmpdb),
            EncryptedMessageStore::generate_enc_key(),
        )
        .await
        .unwrap();

        let stored: StoredIdentity = (&Identity {
            inbox_id: stored_inbox_id.clone(),
            installation_keys: Default::default(),
            credential: Credential::new(CredentialType::Basic, rand_vec::<24>()),
            signature_request: None,
            is_ready: AtomicBool::new(true),
        })
            .try_into()
            .unwrap();

        stored.store(&store.conn().unwrap()).unwrap();

        let wrapper = ApiClientWrapper::new(mock_api.into(), retry());

        let inbox_id = "inbox_id".to_string();
        let identity = IdentityStrategy::new(inbox_id.clone(), ident, nonce, None);
        let err = identity
            .initialize_identity(&wrapper, &store.mls_provider().unwrap(), &scw_verifier)
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
        let db_key = EncryptedMessageStore::generate_enc_key();

        // Generate a new Wallet + Store
        let store_a = EncryptedMessageStore::new(StorageOption::Persistent(tmpdb.clone()), db_key)
            .await
            .unwrap();

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
        .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
        .build()
        .await
        .unwrap();

        register_client(&client_a, wallet).await;
        assert!(client_a.identity().is_ready());

        let keybytes_a = client_a.installation_public_key().to_vec();
        drop(client_a);

        // Reload the existing store and wallet
        let store_b = EncryptedMessageStore::new(StorageOption::Persistent(tmpdb.clone()), db_key)
            .await
            .unwrap();

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
        let store_d = EncryptedMessageStore::new(StorageOption::Persistent(tmpdb.clone()), db_key)
            .await
            .unwrap();
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
                let account_address = format!("{scw_addr:?}");
                let ident = Identifier::eth(&account_address).unwrap();
                let account_id = AccountId::new_evm(anvil_meta.chain_id, account_address.clone());

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
                    .with_remote_verifier()
                    .unwrap()
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
