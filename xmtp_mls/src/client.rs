use std::collections::HashSet;

use openmls::prelude::TlsSerializeTrait;
use thiserror::Error;
use tls_codec::Error as TlsSerializationError;
use xmtp_proto::api_client::{XmtpApiClient, XmtpMlsClient};

use crate::{
    api_client_wrapper::{ApiClientWrapper, IdentityUpdate},
    configuration::KEY_PACKAGE_TOP_UP_AMOUNT,
    groups::MlsGroup,
    identity::Identity,
    storage::{group::GroupMembershipState, EncryptedMessageStore, StorageError},
    types::Address,
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
    pub api_client: ApiClientWrapper<ApiClient>,
    pub(crate) _network: Network,
    pub(crate) identity: Identity,
    pub store: EncryptedMessageStore, // Temporarily exposed outside crate for CLI client
}

impl<ApiClient> Client<ApiClient>
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

    // TODO: Remove this and figure out the correct lifetimes to allow long lived provider
    pub fn mls_provider(&self) -> XmtpOpenMlsProvider {
        XmtpOpenMlsProvider::new(&self.store)
    }

    pub fn create_group(&self) -> Result<MlsGroup<ApiClient>, ClientError> {
        let group = MlsGroup::create_and_insert(self, GroupMembershipState::Allowed)
            .map_err(|e| ClientError::Generic(format!("group create error {}", e)))?;

        Ok(group)
    }

    pub fn find_groups(
        &self,
        allowed_states: Option<Vec<GroupMembershipState>>,
        created_at_ns_gt: Option<i64>,
        limit: Option<i64>,
    ) -> Result<Vec<MlsGroup<ApiClient>>, ClientError> {
        Ok(self
            .store
            .find_groups(
                &mut self.store.conn()?,
                allowed_states,
                created_at_ns_gt,
                limit,
            )?
            .into_iter()
            .map(|stored_group| MlsGroup::new(self, stored_group.id, stored_group.created_at_ns))
            .collect())
    }

    pub async fn register_identity(&self) -> Result<(), ClientError> {
        // TODO: Mark key package as last_resort in creation
        let last_resort_kp = self.identity.new_key_package(&self.mls_provider())?;
        let last_resort_kp_bytes = last_resort_kp.tls_serialize_detached()?;

        self.api_client
            .register_installation(last_resort_kp_bytes)
            .await?;

        Ok(())
    }

    pub async fn top_up_key_packages(&self) -> Result<(), ClientError> {
        let key_packages: Result<Vec<Vec<u8>>, ClientError> = (0..KEY_PACKAGE_TOP_UP_AMOUNT)
            .map(|_| -> Result<Vec<u8>, ClientError> {
                let kp = self.identity.new_key_package(&self.mls_provider())?;
                let kp_bytes = kp.tls_serialize_detached()?;

                Ok(kp_bytes)
            })
            .collect();

        self.api_client.upload_key_packages(key_packages?).await?;

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

        let mls_provider = self.mls_provider();

        Ok(key_package_results
            .values()
            .map(|bytes| VerifiedKeyPackage::from_bytes(&mls_provider, bytes.as_slice()))
            .collect::<Result<_, _>>()?)
    }

    pub fn account_address(&self) -> Address {
        self.identity.account_address.clone()
    }

    pub fn installation_public_key(&self) -> Vec<u8> {
        self.identity.installation_keys.to_public_vec()
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
    async fn test_top_up_key_packages() {
        let wallet = generate_local_wallet();
        let wallet_address = wallet.get_address();
        let client = ClientBuilder::new_test_client(wallet.clone().into()).await;

        client.register_identity().await.unwrap();
        client.top_up_key_packages().await.unwrap();

        let key_packages = client
            .get_key_packages_for_wallet_addresses(vec![wallet_address.clone()])
            .await
            .unwrap();

        assert_eq!(key_packages.len(), 1);

        let key_package = key_packages.first().unwrap();
        assert_eq!(key_package.wallet_address, wallet_address);

        let key_packages_2 = client
            .get_key_packages_for_wallet_addresses(vec![wallet_address.clone()])
            .await
            .unwrap();

        assert_eq!(key_packages_2.len(), 1);

        // Ensure we got back different key packages
        let key_package_2 = key_packages_2.first().unwrap();
        assert_eq!(key_package_2.wallet_address, wallet_address);
        assert!(!(key_package_2.eq(key_package)));
    }

    #[tokio::test]
    async fn test_find_groups() {
        let client = ClientBuilder::new_test_client(generate_local_wallet().into()).await;
        let group_1 = client.create_group().unwrap();
        let group_2 = client.create_group().unwrap();

        let groups = client.find_groups(None, None, None).unwrap();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].group_id, group_1.group_id);
        assert_eq!(groups[1].group_id, group_2.group_id);
    }
}
