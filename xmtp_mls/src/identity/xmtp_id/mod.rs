pub mod identity;

use crate::storage::identity_inbox::StoredIdentity;
use crate::{api::ApiClientWrapper, builder::ClientBuilderError, storage::EncryptedMessageStore};
use crate::{xmtp_openmls_provider::XmtpOpenMlsProvider, Fetch};
pub use identity::Identity;
use log::debug;
use log::info;
use xmtp_proto::api_client::{XmtpIdentityClient, XmtpMlsClient};

/// The member that the [Identity] is created from.
pub enum Member {
    /// user's Ethereum wallet address
    Address(String),
    /// address and corresponding legacy private key(same secp256k1 as Ethereum wallet)
    LegacyKey(String, Vec<u8>),
}

pub enum IdentityStrategy {
    /// Tries to get an identity from the disk store. If not found, then it checks whether the address has an associated inbox and create one if not.
    /// Finally use that inbox to create an identity.
    /// Note that if you want to associate the wallet with a different inbox_id than already associated, do that separately.
    CreateIfNotFound(Member),
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
            IdentityStrategy::CreateIfNotFound(member) => {
                let identity = match member {
                    Member::Address(address) => stored_identity
                        .unwrap_or(Identity::create_to_be_signed(address, api_client).await?),

                    Member::LegacyKey(address, key) => stored_identity
                        .unwrap_or(Identity::create_from_legacy(address, key, api_client).await?),
                };

                Ok(identity)
            }
            #[cfg(test)]
            IdentityStrategy::ExternalIdentity(identity) => Ok(identity),
        }
    }
}
