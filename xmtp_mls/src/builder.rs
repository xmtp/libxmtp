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
}

pub struct ClientBuilder<ApiClient> {
    api_client: Option<ApiClient>,
    identity: Option<Identity>,
    store: Option<EncryptedMessageStore>,
    identity_strategy: IdentityStrategy,
}

impl<ApiClient> ClientBuilder<ApiClient>
where
    ApiClient: XmtpApi,
{
    pub fn new(strat: IdentityStrategy) -> Self {
        Self {
            api_client: None,
            identity: None,
            store: None,
            identity_strategy: strat,
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

        Ok(Client::new(api_client_wrapper, identity, store))
    }
}

#[cfg(test)]
mod tests {
    use crate::api::ApiClientWrapper;
    use crate::builder::ClientBuilderError;
    use crate::identity::IdentityError;
    use crate::retry::Retry;
    use crate::{
        api::test_utils::*,
        identity::Identity,
        storage::identity::StoredIdentity,
        utils::test::{rand_string, rand_vec},
        Store,
    };
    use openmls::credentials::{Credential, CredentialType};
    use openmls_basic_credential::SignatureKeyPair;
    use openmls_traits::types::SignatureScheme;
    use xmtp_api_grpc::grpc_api_helper::Client as GrpcClient;
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_id::associations::{
        generate_inbox_id, test_utils::rand_u64, RecoverableEcdsaSignature,
    };
    use xmtp_proto::xmtp::identity::api::v1::{
        get_inbox_ids_response::Response as GetInboxIdsResponseItem, GetInboxIdsResponse,
    };

    use super::{ClientBuilder, IdentityStrategy};
    use crate::{
        storage::{EncryptedMessageStore, StorageOption},
        utils::test::tmp_path,
        Client, InboxOwner,
    };

    async fn get_local_grpc_client() -> GrpcClient {
        GrpcClient::create("http://localhost:5556".to_string(), false)
            .await
            .unwrap()
    }

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

    impl ClientBuilder<GrpcClient> {
        pub async fn local_grpc(self) -> Self {
            self.api_client(get_local_grpc_client().await)
        }

        fn temp_store(self) -> Self {
            let tmpdb = tmp_path();
            self.store(
                EncryptedMessageStore::new_unencrypted(StorageOption::Persistent(tmpdb)).unwrap(),
            )
        }

        pub async fn new_test_client(owner: &impl InboxOwner) -> Client<GrpcClient> {
            let nonce = 1;
            let inbox_id = generate_inbox_id(&owner.get_address(), &nonce);
            let client = Self::new(IdentityStrategy::CreateIfNotFound(
                inbox_id,
                owner.get_address(),
                nonce,
                None,
            ))
            .temp_store()
            .local_grpc()
            .await
            .build()
            .await
            .unwrap();

            register_client(&client, owner).await;

            client
        }
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
        // test cases where new inbox are created
        let legacy_account_address = "0x0bd00b21af9a2d538103c3aaf95cb507f8af1b28";
        let legacy_key = hex::decode("0880bdb7a8b3f6ede81712220a20ad528ea38ce005268c4fb13832cfed13c2b2219a378e9099e48a38a30d66ef991a96010a4c08aaa8e6f5f9311a430a41047fd90688ca39237c2899281cdf2756f9648f93767f91c0e0f74aed7e3d3a8425e9eaa9fa161341c64aa1c782d004ff37ffedc887549ead4a40f18d1179df9dff124612440a403c2cb2338fb98bfe5f6850af11f6a7e97a04350fc9d37877060f8d18e8f66de31c77b3504c93cf6a47017ea700a48625c4159e3f7e75b52ff4ea23bc13db77371001").unwrap();
        let non_legacy_account_address = generate_local_wallet().get_address();
        let nonce_for_legacy = 0;
        let nonce_for_non_legacy = rand_u64();

        struct IdentityStrategyTestCase {
            strategy: IdentityStrategy,
            err: Option<String>,
        }

        // Given that the identity in db will hijack the test cases, we put the happy case for an inbox_id at the end.
        let identity_strategies_test_cases = vec![
            // legacy cases
            IdentityStrategyTestCase {
                strategy: IdentityStrategy::CreateIfNotFound(
                    generate_inbox_id(legacy_account_address, &111),
                    legacy_account_address.to_string(),
                    111,
                    Some(legacy_key.clone()),
                ),
                err: Some("Nonce must be 0 if legacy key is provided".to_string()),
            },
            IdentityStrategyTestCase {
                strategy: IdentityStrategy::CreateIfNotFound(
                    generate_inbox_id(legacy_account_address, &111),
                    legacy_account_address.to_string(),
                    nonce_for_legacy,
                    Some(legacy_key.clone()),
                ),
                err: Some("Inbox ID doesn't match nonce & address".to_string()),
            },
            IdentityStrategyTestCase {
                strategy: IdentityStrategy::CreateIfNotFound(
                    generate_inbox_id(legacy_account_address, &nonce_for_legacy),
                    legacy_account_address.to_string(),
                    nonce_for_legacy,
                    Some(legacy_key.clone()),
                ),
                err: None,
            },
            // non-legacy cases
            IdentityStrategyTestCase {
                strategy: IdentityStrategy::CreateIfNotFound(
                    generate_inbox_id(&non_legacy_account_address, &0),
                    non_legacy_account_address.clone(),
                    0,
                    None,
                ),
                err: Some("Nonce must be non-zero if legacy key is not provided".to_string()),
            },
            IdentityStrategyTestCase {
                strategy: IdentityStrategy::CreateIfNotFound(
                    generate_inbox_id(&non_legacy_account_address, &0),
                    non_legacy_account_address.clone(),
                    nonce_for_non_legacy,
                    None,
                ),
                err: Some("Inbox ID doesn't match nonce & address".to_string()),
            },
            IdentityStrategyTestCase {
                strategy: IdentityStrategy::CreateIfNotFound(
                    generate_inbox_id(&non_legacy_account_address, &nonce_for_non_legacy),
                    non_legacy_account_address.clone(),
                    nonce_for_non_legacy,
                    None,
                ),
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
                // println!("expected {}, result {:?}", err_string, result);
                assert!(matches!(
                    result,
                    Err(ClientBuilderError::Identity(IdentityError::NewIdentity(err))) if err == err_string
                ));
            } else {
                assert!(result.is_ok());
            }
        }
    }

    // Create two clients sequencially with the same inbox id & legacy key
    //
    #[tokio::test]
    async fn test_twice_client_creation() {
        let legacy_account_address = "0x0bd00b21af9a2d538103c3aaf95cb507f8af1b28";
        let legacy_key = hex::decode("0880bdb7a8b3f6ede81712220a20ad528ea38ce005268c4fb13832cfed13c2b2219a378e9099e48a38a30d66ef991a96010a4c08aaa8e6f5f9311a430a41047fd90688ca39237c2899281cdf2756f9648f93767f91c0e0f74aed7e3d3a8425e9eaa9fa161341c64aa1c782d004ff37ffedc887549ead4a40f18d1179df9dff124612440a403c2cb2338fb98bfe5f6850af11f6a7e97a04350fc9d37877060f8d18e8f66de31c77b3504c93cf6a47017ea700a48625c4159e3f7e75b52ff4ea23bc13db77371001").unwrap();
        let identity_strategy = IdentityStrategy::CreateIfNotFound(
            generate_inbox_id(&legacy_account_address, &0),
            legacy_account_address.to_string(),
            0,
            Some(legacy_key),
        );

        let client1 = ClientBuilder::new(identity_strategy.clone())
            .temp_store()
            .local_grpc()
            .await
            .build()
            .await
            .unwrap();

        let client2 = ClientBuilder::new(identity_strategy)
            .temp_store()
            .local_grpc()
            .await
            .build()
            .await
            .unwrap();

        assert!(client1.inbox_id() == client2.inbox_id());
    }

    // Should return error if inbox associated with given account_address doesn't match the provided one.
    #[tokio::test]
    async fn api_identity_mismatch() {
        let mut mock_api = MockApiClient::new();
        let tmpdb = tmp_path();

        let store =
            EncryptedMessageStore::new_unencrypted(StorageOption::Persistent(tmpdb)).unwrap();
        let nonce = 0;
        let address = rand_string();
        let inbox_id = "inbox_id".to_string();

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
        let address = rand_string();
        let inbox_id = "inbox_id".to_string();

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
        assert!(identity.initialize_identity(&wrapper, &store).await.is_ok());
    }

    // Use a stored identity as long as the inbox_id matches the one provided.
    #[tokio::test]
    async fn stored_identity_happy_path() {
        let mock_api = MockApiClient::new();
        let tmpdb = tmp_path();

        let store =
            EncryptedMessageStore::new_unencrypted(StorageOption::Persistent(tmpdb)).unwrap();
        let nonce = 0;
        let address = rand_string();
        let inbox_id = "inbox_id".to_string();

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

        let network_address = rand_string();
        let stored_inbox_id = "stored_inbox_id".to_string();

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
            IdentityStrategy::CreateIfNotFound(inbox_id.clone(), network_address.clone(), 0, None);
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
