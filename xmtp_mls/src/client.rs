use openmls::prelude::TlsSerializeTrait;
use thiserror::Error;
use tls_codec::Error as TlsSerializationError;
use xmtp_proto::api_client::{XmtpApiClient, XmtpMlsClient};

use crate::{
    api_client_wrapper::ApiClientWrapper,
    configuration::KEY_PACKAGE_TOP_UP_AMOUNT,
    identity::Identity,
    storage::{EncryptedMessageStore, StorageError},
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
    Ddd(#[from] diesel::result::Error),
    #[error("Query failed: {0}")]
    QueryError(#[from] xmtp_proto::api_client::Error),
    #[error("identity error: {0}")]
    Identity(#[from] crate::identity::IdentityError),
    #[error("serialization error: {0}")]
    Serialization(#[from] TlsSerializationError),
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

pub struct Client<ApiClient> {
    pub api_client: ApiClientWrapper<ApiClient>,
    pub(crate) _network: Network,
    pub(crate) _identity: Identity,
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
            _identity: identity,
            store,
        }
    }

    // TODO: Remove this and figure out the correct lifetimes to allow long lived provider
    fn mls_provider(&self) -> XmtpOpenMlsProvider {
        XmtpOpenMlsProvider::new(&self.store)
    }

    pub(crate) async fn register_identity(&self) -> Result<(), ClientError> {
        // TODO: Mark key package as last_resort in creation
        let last_resort_kp = self._identity.new_key_package(&self.mls_provider())?;
        let last_resort_kp_bytes = last_resort_kp.tls_serialize_detached()?;

        self.api_client
            .register_installation(last_resort_kp_bytes)
            .await?;

        Ok(())
    }

    pub async fn top_up_key_packages(&self) -> Result<(), ClientError> {
        let key_packages: Result<Vec<Vec<u8>>, ClientError> = (0..KEY_PACKAGE_TOP_UP_AMOUNT)
            .into_iter()
            .map(|_| -> Result<Vec<u8>, ClientError> {
                let kp = self._identity.new_key_package(&self.mls_provider())?;
                let kp_bytes = kp.tls_serialize_detached()?;

                Ok(kp_bytes)
            })
            .collect();

        self.api_client.upload_key_packages(key_packages?).await?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::builder::ClientBuilder;

    #[tokio::test]
    async fn test_register_installation() {
        let client = ClientBuilder::new_test().await.build().unwrap();
        let res = client.register_identity().await;
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn top_up_key_packages() {
        let client = ClientBuilder::new_test().await.build().unwrap();
        client.register_identity().await.unwrap();

        let res = client.top_up_key_packages().await;
        assert!(res.is_ok());
    }
}
