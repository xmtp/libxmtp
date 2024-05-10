pub mod identity;

use crate::storage::identity::StoredIdentity;
use crate::{api::ApiClientWrapper, builder::ClientBuilderError, storage::EncryptedMessageStore};
use crate::{xmtp_openmls_provider::XmtpOpenMlsProvider, Fetch};
pub use identity::Identity;
pub use identity::IdentityError;
use log::debug;
use log::info;
use xmtp_proto::api_client::{XmtpIdentityClient, XmtpMlsClient};

pub enum IdentityStrategy {
    /// Tries to get an identity from the disk store. If not found, getting one from backend.
    CreateIfNotFound(String, Option<Vec<u8>>), // (address, legacy_signed_private_key)
    /// Identity that is already in the disk store
    CachedOnly,
    /// An already-built Identity for testing purposes
    #[cfg(test)]
    ExternalIdentity(Identity),
}

#[allow(dead_code)]
impl IdentityStrategy {
    pub(crate) async fn initialize_identity<ApiClient: XmtpMlsClient + XmtpIdentityClient>(
        self,
        api_client: &ApiClientWrapper<ApiClient>,
        store: &EncryptedMessageStore,
    ) -> Result<Identity, ClientBuilderError> {
        info!("Initializing identity");
        let conn = store.conn()?;
        let provider = XmtpOpenMlsProvider::new(&conn);
        let stored_identity: Option<Identity> = provider
            .conn()
            .fetch(&())?
            .map(|i: StoredIdentity| i.into());
        debug!("Existing identity in store: {:?}", stored_identity);
        match self {
            IdentityStrategy::CachedOnly => {
                stored_identity.ok_or(ClientBuilderError::RequiredIdentityNotFound)
            }
            IdentityStrategy::CreateIfNotFound(address, legacy_signed_private_key) => {
                if let Some(identity) = stored_identity {
                    Ok(identity)
                } else {
                    Identity::new(address, legacy_signed_private_key, api_client)
                        .await
                        .map_err(ClientBuilderError::from)
                }
            }
            #[cfg(test)]
            IdentityStrategy::ExternalIdentity(identity) => Ok(identity),
        }
    }
}
