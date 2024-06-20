use log::debug;
use thiserror::Error;

use xmtp_cryptography::signature::AddressValidationError;

use crate::{
    api::ApiClientWrapper,
    client::Client,
    identity::{Identity, IdentityStrategy},
    identity_updates::load_identity_updates,
    retry::Retry,
    storage::EncryptedMessageStore,
    StorageError, XmtpApi,
};

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
}

pub struct ClientBuilder<ApiClient> {
    api_client: Option<ApiClient>,
    identity: Option<Identity>,
    store: Option<EncryptedMessageStore>,
    identity_strategy: IdentityStrategy,
    history_sync_url: Option<String>,
}

impl<ApiClient> ClientBuilder<ApiClient>
where
    ApiClient: XmtpApi,
{
    pub fn new(strategy: IdentityStrategy) -> Self {
        Self {
            api_client: None,
            identity: None,
            store: None,
            identity_strategy: strategy,
            history_sync_url: None,
        }
    }

    pub fn api_client(mut self, api_client: ApiClient) -> Self {
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

    pub fn history_sync_url(mut self, url: String) -> Self {
        self.history_sync_url = Some(url);
        self
    }

    pub async fn build(mut self) -> Result<Client<ApiClient>, ClientBuilderError> {
        debug!("Building client");
        let api_client = self
            .api_client
            .take()
            .ok_or(ClientBuilderError::MissingParameter {
                parameter: "api_client",
            })?;
        let api_client_wrapper = ApiClientWrapper::new(api_client, Retry::default());
        let store = self
            .store
            .take()
            .ok_or(ClientBuilderError::MissingParameter { parameter: "store" })?;
        debug!("Initializing identity");
        let identity = self
            .identity_strategy
            .initialize_identity(&api_client_wrapper, &store)
            .await?;

        // get sequence_id from identity updates and loaded into the DB
        load_identity_updates(
            &api_client_wrapper,
            &store.conn()?,
            vec![identity.clone().inbox_id],
        )
        .await?;

        let client = Client::new(api_client_wrapper, identity, store, self.history_sync_url);
        
         Ok(client)
    }
}

#[cfg(test)]
mod tests {
    use crate::api::ApiClientWrapper;
    use crate::builder::ClientBuilderError;
    use crate::identity::IdentityError;
    use crate::retry::Retry;
    use crate::{
        api::test_utils::*, identity::Identity, storage::identity::StoredIdentity,
        utils::test::rand_vec, Store,
    };
    use ethers::signers::Signer;
    use ethers_core::k256;
    use openmls::credentials::{Credential, CredentialType};
    use openmls_basic_credential::SignatureKeyPair;
    use openmls_traits::types::SignatureScheme;
    use prost::Message;
    use xmtp_api_grpc::grpc_api_helper::Client as GrpcClient;
    use xmtp_cryptography::signature::h160addr_to_string;
    use xmtp_cryptography::utils::{generate_local_wallet, rng};
    use xmtp_id::associations::ValidatedLegacySignedPublicKey;
    use xmtp_id::associations::{
        generate_inbox_id, test_utils::rand_u64, RecoverableEcdsaSignature,
    };
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

    async fn register_client(client: &Client<GrpcClient>, owner: &impl InboxOwner) {
        let mut signature_request = client.context.signature_request().unwrap();
        let signature_text = signature_request.signature_text();
        signature_request
            .add_signature(Box::new(RecoverableEcdsaSignature::new(
                signature_text.clone(),
                owner.sign(&signature_text).unwrap().into(),
            )))
            .await
            .unwrap();

        client.register_identity(signature_request).await.unwrap();
    }

    /// Generate a random legacy key proto bytes and corresponding account address.
    async fn generate_random_legacy_key() -> (Vec<u8>, String) {
        let wallet = generate_local_wallet();
        let address = h160addr_to_string(wallet.address());
        let created_ns = rand_u64();
        let secret_key = k256::ecdsa::SigningKey::random(&mut rng());
        let public_key = k256::ecdsa::VerifyingKey::from(&secret_key);
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
        let signed_public_key = wallet.sign_message(message).await.unwrap().to_vec();
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

    #[tokio::test]
    async fn builder_test() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;
        assert!(!client.installation_public_key().is_empty());
    }

    // Test client creation using various identity strategies that creates new inboxes
    #[tokio::test]
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
                .local_grpc()
                .await
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
    #[tokio::test]
    async fn test_2nd_time_client_creation() {
        let (legacy_key, legacy_account_address) = generate_random_legacy_key().await;
        let identity_strategy = IdentityStrategy::CreateIfNotFound(
            generate_inbox_id(&legacy_account_address, &0),
            legacy_account_address.to_string(),
            0,
            Some(legacy_key.clone()),
        );
        let store =
            EncryptedMessageStore::new_unencrypted(StorageOption::Persistent(tmp_path())).unwrap();

        let client1: Client<GrpcClient> = ClientBuilder::new(identity_strategy.clone())
            .store(store.clone())
            .local_grpc()
            .await
            .build()
            .await
            .unwrap();
        assert!(client1.context.signature_request().is_none());

        let client2: Client<GrpcClient> = ClientBuilder::new(IdentityStrategy::CachedOnly)
            .store(store.clone())
            .local_grpc()
            .await
            .build()
            .await
            .unwrap();
        assert!(client2.context.signature_request().is_none());
        assert!(client1.inbox_id() == client2.inbox_id());
        assert!(client1.installation_public_key() == client2.installation_public_key());

        let client3: Client<GrpcClient> = ClientBuilder::new(IdentityStrategy::CreateIfNotFound(
            generate_inbox_id(&legacy_account_address, &0),
            legacy_account_address.to_string(),
            0,
            None,
        ))
        .store(store.clone())
        .local_grpc()
        .await
        .build()
        .await
        .unwrap();
        assert!(client3.context.signature_request().is_none());
        assert!(client1.inbox_id() == client3.inbox_id());
        assert!(client1.installation_public_key() == client3.installation_public_key());

        let client4: Client<GrpcClient> = ClientBuilder::new(IdentityStrategy::CreateIfNotFound(
            generate_inbox_id(&legacy_account_address, &0),
            legacy_account_address.to_string(),
            0,
            Some(legacy_key),
        ))
        .temp_store()
        .local_grpc()
        .await
        .build()
        .await
        .unwrap();
        assert!(client4.context.signature_request().is_some());
        assert!(client1.inbox_id() == client4.inbox_id());
        assert!(client1.installation_public_key() != client4.installation_public_key());
    }

    // Should return error if inbox associated with given account_address doesn't match the provided one.
    #[tokio::test]
    async fn api_identity_mismatch() {
        let mut mock_api = MockApiClient::new();
        let tmpdb = tmp_path();

        let store =
            EncryptedMessageStore::new_unencrypted(StorageOption::Persistent(tmpdb)).unwrap();
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
                .initialize_identity(&wrapper, &store)
                .await
                .unwrap_err(),
            IdentityError::NewIdentity(msg) if msg == "Inbox ID mismatch"
        ));
    }

    // Use the account_address associated inbox
    #[tokio::test]
    async fn api_identity_happy_path() {
        let mut mock_api = MockApiClient::new();
        let tmpdb = tmp_path();

        let store =
            EncryptedMessageStore::new_unencrypted(StorageOption::Persistent(tmpdb)).unwrap();
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
        assert!(dbg!(identity.initialize_identity(&wrapper, &store).await).is_ok());
    }

    // Use a stored identity as long as the inbox_id matches the one provided.
    #[tokio::test]
    async fn stored_identity_happy_path() {
        let mock_api = MockApiClient::new();
        let tmpdb = tmp_path();

        let store =
            EncryptedMessageStore::new_unencrypted(StorageOption::Persistent(tmpdb)).unwrap();
        let nonce = 0;
        let address = generate_local_wallet().get_address();
        let inbox_id = generate_inbox_id(&address, &nonce);

        let stored: StoredIdentity = (&Identity {
            inbox_id: inbox_id.clone(),
            installation_keys: SignatureKeyPair::new(SignatureScheme::ED25519).unwrap(),
            credential: Credential::new(CredentialType::Basic, rand_vec()),
            signature_request: None,
        })
            .into();

        stored.store(&store.conn().unwrap()).unwrap();
        let wrapper = ApiClientWrapper::new(mock_api, Retry::default());
        let identity = IdentityStrategy::CreateIfNotFound(inbox_id.clone(), address, nonce, None);
        assert!(identity.initialize_identity(&wrapper, &store).await.is_ok());
    }

    #[tokio::test]
    async fn stored_identity_mismatch() {
        let mock_api = MockApiClient::new();

        let nonce = 0;
        let address = generate_local_wallet().get_address();
        let stored_inbox_id = generate_inbox_id(&address, &nonce);

        let tmpdb = tmp_path();
        let store =
            EncryptedMessageStore::new_unencrypted(StorageOption::Persistent(tmpdb)).unwrap();

        let stored: StoredIdentity = (&Identity {
            inbox_id: stored_inbox_id.clone(),
            installation_keys: SignatureKeyPair::new(SignatureScheme::ED25519).unwrap(),
            credential: Credential::new(CredentialType::Basic, rand_vec()),
            signature_request: None,
        })
            .into();

        stored.store(&store.conn().unwrap()).unwrap();

        let wrapper = ApiClientWrapper::new(mock_api, Retry::default());

        let inbox_id = "inbox_id".to_string();
        let identity =
            IdentityStrategy::CreateIfNotFound(inbox_id.clone(), address.clone(), nonce, None);
        let err = identity
            .initialize_identity(&wrapper, &store)
            .await
            .unwrap_err();

        assert!(
            matches!(err, IdentityError::InboxIdMismatch { id, stored } if id == inbox_id && stored == stored_inbox_id)
        );
    }

    #[tokio::test]
    async fn identity_persistence_test() {
        let tmpdb = tmp_path();
        let wallet = &generate_local_wallet();

        // Generate a new Wallet + Store
        let store_a =
            EncryptedMessageStore::new_unencrypted(StorageOption::Persistent(tmpdb.clone()))
                .unwrap();

        let nonce = 1;
        let inbox_id = generate_inbox_id(&wallet.get_address(), &nonce);
        let client_a = ClientBuilder::new(IdentityStrategy::CreateIfNotFound(
            inbox_id.clone(),
            wallet.get_address(),
            nonce,
            None,
        ))
        .local_grpc()
        .await
        .store(store_a)
        .build()
        .await
        .unwrap();

        register_client(&client_a, wallet).await;

        let keybytes_a = client_a.installation_public_key();
        drop(client_a);

        // Reload the existing store and wallet
        let store_b =
            EncryptedMessageStore::new_unencrypted(StorageOption::Persistent(tmpdb.clone()))
                .unwrap();

        let client_b = ClientBuilder::new(IdentityStrategy::CreateIfNotFound(
            inbox_id,
            wallet.get_address(),
            nonce,
            None,
        ))
        .local_grpc()
        .await
        .store(store_b)
        .build()
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
        // .local_grpc()
        // .await
        // .store(store_c)
        // .build()
        // .await
        // .expect_err("Testing expected mismatch error");

        // Use cached only strategy
        let store_d =
            EncryptedMessageStore::new_unencrypted(StorageOption::Persistent(tmpdb.clone()))
                .unwrap();
        let client_d = ClientBuilder::new(IdentityStrategy::CachedOnly)
            .local_grpc()
            .await
            .store(store_d)
            .build()
            .await
            .unwrap();
        assert_eq!(client_d.installation_public_key(), keybytes_a);
    }
}
