use log::info;
use xmtp_id::InboxId;
use xmtp_proto::api_client::{XmtpIdentityClient, XmtpMlsClient};

use crate::{api::ApiClientWrapper, builder::ClientBuilderError, storage::EncryptedMessageStore};

pub mod identity;
pub use identity::Identity;

pub enum LegacyIdentity {
    /// No legacy identity provided
    None,
    /// An encoded PrivateKeyBundle was provided
    FromKeys(Vec<u8>),
}

/**
 * Identity strategy cases
 * 1. Has usable V2 identity. First time using xmtp_id.
 * 2. Has usable V2 identity. Wallet is registered with the desired xmtp_id and has installation keys in the database
 * 3. Has usable V2 identity. Wallet is registered with a different xmtp_id. No installation keys in the database
 * 4. No V2 identity or V2 identity already used. First time using xmtp_id
 * 5. No V2 identity or V2 identity already used. Wallet is registered with xmtp_id, but no installation keys in the database.
 * 6. No V2 identity or V2 identity already used. Wallet is registered with xmtp_id and has installation keys in the database
 */

pub enum IdentityStrategy {
    /// Tries to get an identity from the disk store, if not found creates an identity.
    /// If a `LegacyIdentity` is provided it will be converted to a `v3` identity.
    CreateIfNotFound(InboxId, String, u64, LegacyIdentity),
    /// Identity that is already in the disk store
    CachedOnly,
    /// An already-built Identity for testing purposes
    #[cfg(test)]
    ExternalIdentity(Identity),
}

impl IdentityStrategy {
    pub(crate) async fn initialize_identity<ApiClient: XmtpMlsClient + XmtpIdentityClient>(
        self,
        api_client: &ApiClientWrapper<ApiClient>,
        store: &EncryptedMessageStore,
    ) -> Result<Identity, ClientBuilderError> {
    }

    async fn create_identity<ApiClient: XmtpMlsClient + XmtpIdentityClient>(
        api_client: &ApiClientWrapper<ApiClient>,
        account_address: String,
        legacy_identity: LegacyIdentity,
    ) -> Result<Identity, ClientBuilderError> {
        info!("Creating identity");
        let identity = match legacy_identity {};
        Ok(identity)
    }
}
