use std::collections::HashSet;

use openmls::{
    framing::{MlsMessageIn, MlsMessageInBody},
    messages::Welcome,
    prelude::TlsSerializeTrait,
};
use thiserror::Error;
use tls_codec::{Deserialize, Error as TlsSerializationError};
use xmtp_proto::api_client::{XmtpApiClient, XmtpMlsClient};

use crate::{
    api_client_wrapper::{ApiClientWrapper, IdentityUpdate},
    groups::MlsGroup,
    identity::Identity,
    storage::{group::GroupMembershipState, DbConnection, EncryptedMessageStore, StorageError},
    types::Address,
    utils::topic::get_welcome_topic,
    verified_key_package::{KeyPackageVerificationError, VerifiedKeyPackage},
    xmtp_openmls_provider::XmtpOpenMlsProvider,
};

#[derive(Clone, Copy, Default, Debug)]
pub enum Network {
    Local(&'static str),
    #[default]
    Dev,
    Prod,
}

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("could not publish: {0}")]
    PublishError(String),
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("dieselError: {0}")]
    Diesel(#[from] diesel::result::Error),
    #[error("Query failed: {0}")]
    QueryError(#[from] xmtp_proto::api_client::Error),
    #[error("identity error: {0}")]
    Identity(#[from] crate::identity::IdentityError),
    #[error("serialization error: {0}")]
    Serialization(#[from] TlsSerializationError),
    #[error("key package verification: {0}")]
    KeyPackageVerification(#[from] KeyPackageVerificationError),
    #[error("message processing: {0}")]
    MessageProcessing(#[from] crate::groups::MessageProcessingError),
    #[error("generic:{0}")]
    Generic(String),
}

impl From<String> for ClientError {
    fn from(value: String) -> Self {
        Self::Generic(value)
    }
}

impl From<&str> for ClientError {
    fn from(value: &str) -> Self {
        Self::Generic(value.to_string())
    }
}

#[derive(Debug)]
pub struct Client<ApiClient> {
    pub(crate) api_client: ApiClientWrapper<ApiClient>,
    pub(crate) _network: Network,
    pub(crate) identity: Identity,
    pub(crate) store: EncryptedMessageStore,
}

impl<'a, ApiClient> Client<ApiClient>
where
    ApiClient: XmtpMlsClient + XmtpApiClient,
{
    pub fn new(
        api_client: ApiClient,
        network: Network,
        identity: Identity,
        store: EncryptedMessageStore,
    ) -> Self {
        Self {
            api_client: ApiClientWrapper::new(api_client),
            _network: network,
            identity,
            store,
        }
    }

    /// Build this struct
    /// # Arguments
    /// * `strat`: the [`IdentityStrategy`] for this client
    ///
    /// # Example
    ///
    ///  TODO: Fix this example
    ///  ```no_run
    ///  Client::builder()
    ///     .api_client(api_client)
    ///     .network(Network::Dev)
    ///     .build()
    ///  ```
    pub fn builder<Owner: crate::InboxOwner>(
        strat: IdentityStrategy<Owner>,
    ) -> ClientBuilder<ApiClient, Owner> {
        ClientBuilder::new(strat)
    }

    // TODO: Remove this and figure out the correct lifetimes to allow long lived provider
    pub fn mls_provider(&self, conn: &'a mut DbConnection) -> XmtpOpenMlsProvider<'a> {
        XmtpOpenMlsProvider::new(conn)
    }

    pub fn create_group(&self) -> Result<MlsGroup<ApiClient>, ClientError> {
        let group = MlsGroup::create_and_insert(self, GroupMembershipState::Allowed)
            .map_err(|e| ClientError::Generic(format!("group create error {}", e)))?;

        Ok(group)
    }

    pub fn find_groups(
        &self,
        allowed_states: Option<Vec<GroupMembershipState>>,
        created_after_ns: Option<i64>,
        created_before_ns: Option<i64>,
        limit: Option<i64>,
    ) -> Result<Vec<MlsGroup<ApiClient>>, ClientError> {
        Ok(EncryptedMessageStore::find_groups(
            &mut self.store.conn()?,
            allowed_states,
            created_after_ns,
            created_before_ns,
            limit,
        )?
        .into_iter()
        .map(|stored_group| MlsGroup::new(self, stored_group.id, stored_group.created_at_ns))
        .collect())
    }

    pub async fn register_identity(&self) -> Result<(), ClientError> {
        // TODO: Mark key package as last_resort in creation
        let mut connection = self.store.conn()?;
        let mls_provider = XmtpOpenMlsProvider::new(&mut connection);
        let last_resort_kp = self.identity.new_key_package(&mls_provider)?;
        let last_resort_kp_bytes = last_resort_kp.tls_serialize_detached()?;

        self.api_client
            .register_installation(last_resort_kp_bytes)
            .await?;

        Ok(())
    }

    async fn get_all_active_installation_ids(
        &self,
        wallet_addresses: Vec<String>,
    ) -> Result<Vec<Vec<u8>>, ClientError> {
        let update_mapping = self
            .api_client
            .get_identity_updates(0, wallet_addresses)
            .await?;

        let mut installation_ids: Vec<Vec<u8>> = vec![];

        for (_, updates) in update_mapping {
            let mut tmp: HashSet<Vec<u8>> = HashSet::new();
            for update in updates {
                match update {
                    IdentityUpdate::Invalid => {}
                    IdentityUpdate::NewInstallation(new_installation) => {
                        // TODO: Validate credential
                        tmp.insert(new_installation.installation_id);
                    }
                    IdentityUpdate::RevokeInstallation(revoke_installation) => {
                        tmp.remove(&revoke_installation.installation_id);
                    }
                }
            }
            installation_ids.extend(tmp);
        }

        Ok(installation_ids)
    }

    // Get a flat list of one key package per installation for all the wallet addresses provided.
    // Revoked installations will be omitted from the list
    pub async fn get_key_packages_for_wallet_addresses(
        &self,
        wallet_addresses: Vec<String>,
    ) -> Result<Vec<VerifiedKeyPackage>, ClientError> {
        let installation_ids = self
            .get_all_active_installation_ids(wallet_addresses)
            .await?;

        self.get_key_packages_for_installation_ids(installation_ids)
            .await
    }

    pub async fn get_key_packages_for_installation_ids(
        &self,
        installation_ids: Vec<Vec<u8>>,
    ) -> Result<Vec<VerifiedKeyPackage>, ClientError> {
        let key_package_results = self
            .api_client
            .consume_key_packages(installation_ids)
            .await?;

        let mut conn = self.store.conn()?;

        Ok(key_package_results
            .values()
            .map(|bytes| {
                VerifiedKeyPackage::from_bytes(&self.mls_provider(&mut conn), bytes.as_slice())
            })
            .collect::<Result<_, _>>()?)
    }

    // Download all unread welcome messages and convert to groups.
    // Returns any new groups created in the operation
    pub async fn sync_welcomes(&self) -> Result<Vec<MlsGroup<ApiClient>>, ClientError> {
        let welcome_topic = get_welcome_topic(&self.installation_public_key());
        let mut conn = self.store.conn()?;
        let provider = self.mls_provider();
        // TODO: Use the last_message_timestamp_ns field on the TopicRefreshState to only fetch new messages
        // Waiting for more atomic update methods
        let envelopes = self.api_client.read_topic(&welcome_topic, 0).await?;

        let groups: Vec<MlsGroup<ApiClient>> = envelopes
            .into_iter()
            .filter_map(|envelope| {
                // TODO: Wrap in a transaction
                let welcome = match extract_welcome(&envelope.message) {
                    Ok(welcome) => welcome,
                    Err(err) => {
                        log::error!("failed to extract welcome: {}", err);
                        return None;
                    }
                };

                // TODO: Update last_message_timestamp_ns on success or non-retryable error
                // TODO: Abort if error is retryable
                match MlsGroup::create_from_welcome(self, &mut conn, &provider, welcome) {
                    Ok(mls_group) => Some(mls_group),
                    Err(err) => {
                        log::error!("failed to create group from welcome: {}", err);
                        None
                    }
                }
            })
            .collect();

        Ok(groups)
    }

    pub fn account_address(&self) -> Address {
        self.identity.account_address.clone()
    }

    pub fn installation_public_key(&self) -> Vec<u8> {
        self.identity.installation_keys.to_public_vec()
    }
}

fn extract_welcome(welcome_bytes: &Vec<u8>) -> Result<Welcome, ClientError> {
    // let welcome_proto = WelcomeMessageProto::decode(&mut welcome_bytes.as_slice())?;
    let welcome = MlsMessageIn::tls_deserialize(&mut welcome_bytes.as_slice())?;
    match welcome.extract() {
        MlsMessageInBody::Welcome(welcome) => Ok(welcome),
        _ => Err(ClientError::Generic(
            "unexpected message type in welcome".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use xmtp_cryptography::utils::generate_local_wallet;

    use crate::{builder::ClientBuilder, InboxOwner};

    #[tokio::test]
    async fn test_mls_error() {
        let client = ClientBuilder::new_test_client(generate_local_wallet().into()).await;
        let result = client.api_client.register_installation(vec![1, 2, 3]).await;

        assert!(result.is_err());
        let error_string = result.err().unwrap().to_string();
        assert!(error_string.contains("invalid identity"));
    }

    #[tokio::test]
    async fn test_register_installation() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(wallet.clone().into()).await;
        client.register_identity().await.unwrap();

        // Make sure the installation is actually on the network
        let installation_ids = client
            .get_all_active_installation_ids(vec![wallet.get_address()])
            .await
            .unwrap();
        assert_eq!(installation_ids.len(), 1);
    }

    #[tokio::test]
    async fn test_find_groups() {
        let client = ClientBuilder::new_test_client(generate_local_wallet().into()).await;
        let group_1 = client.create_group().unwrap();
        let group_2 = client.create_group().unwrap();

        let groups = client.find_groups(None, None, None, None).unwrap();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].group_id, group_1.group_id);
        assert_eq!(groups[1].group_id, group_2.group_id);
    }

    #[tokio::test]
    async fn test_sync_welcomes() {
        let alice = ClientBuilder::new_test_client(generate_local_wallet().into()).await;
        alice.register_identity().await.unwrap();
        let bob = ClientBuilder::new_test_client(generate_local_wallet().into()).await;
        bob.register_identity().await.unwrap();

        let conn = &mut alice.store.conn().unwrap();
        let alice_bob_group = alice.create_group().unwrap();
        alice_bob_group
            .add_members_by_installation_id(vec![bob.installation_public_key()])
            .await
            .unwrap();

        // Manually mark as committed
        // TODO: Replace with working synchronization once we can add members end to end
        let intents = alice
            .store
            .find_group_intents(conn, alice_bob_group.group_id.clone(), None, None)
            .unwrap();
        let intent = intents.first().unwrap();
        // Set the intent to committed manually
        alice
            .store
            .set_group_intent_committed(conn, intent.id)
            .unwrap();

        alice_bob_group.post_commit(conn).await.unwrap();

        let bob_received_groups = bob.sync_welcomes().await.unwrap();
        assert_eq!(bob_received_groups.len(), 1);
        assert_eq!(
            bob_received_groups.first().unwrap().group_id,
            alice_bob_group.group_id
        );
    }
}
