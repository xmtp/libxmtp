pub mod identity;

use crate::storage::identity_inbox::StoredIdentity;
use crate::{api::ApiClientWrapper, builder::ClientBuilderError, storage::EncryptedMessageStore};
use crate::{xmtp_openmls_provider::XmtpOpenMlsProvider, Fetch};
use ethers::etherscan::account;
pub use identity::Identity;
use log::debug;
use log::info;
use xmtp_id::InboxId;
use xmtp_proto::api_client::{XmtpIdentityClient, XmtpMlsClient};

pub enum LegacyIdentity {
    /// No legacy identity provided
    None,
    /// An encoded PrivateKeyBundle was provided
    FromKeys(Vec<u8>),
}

pub enum IdentityStrategy {
    /// Tries to get an identity from the disk store, if not found creates an identity.
    /// This strategy takes `inbox_id`(id), `wallet_address`(addr), `v2_identity`(v2) and ask a series of questions before deciding the behavior -
    /// `v2` matches the `addr`?
    ///     Y: `addr` has registered with an inbox(`registered_id`)?
    ///         Y: `id` matches `registered_id`?
    ///             Y: create an identity, if local installation key doesn't belong to `id` then generate a new installation key & signature request
    ///             N: create an identity that requires revoking the `addr` from `registered_id` and associate `addr`with `id`
    ///         N: create a new inbox with `addr` and nonce 0.
    ///     N: `addr` has registered with an inbox(`registered_id`)?
    ///         Y: `id` matches `registered_id`?
    ///             Y: create an identity, if local installation key doesn't belong to `id` then generate a new installation key & signature request
    ///             N: create an identity that requires revoking the `addr` from `registered_id` and associate `addr`with `id`
    ///         N: create a new inbox with `addr` and random nonce.
    CreateIfNotFound(InboxId, String, LegacyIdentity),
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
                // Q: do we just trust the cached identity without checking anything here?
                stored_identity.ok_or(ClientBuilderError::RequiredIdentityNotFound)
            }
            IdentityStrategy::CreateIfNotFound(inbox_id, account_address, legacy_identity) => {
                let v2_address = legacy_identity.into();
                if v2_address == account_address {
                    let registered_inbox_id =
                        api_client.get_inbox_ids(vec![account_addresses]).await;
                    if registered_inbox_id.is_not_empty() {
                        if registered_inbox_id == inbox_id {
                            // create an identity, if local installation key doesn't belong to `id` then generate a new installation key & signature request
                        } else {
                            // create an identity that requires revoking the `addr` from `registered_id` and associate `addr`with `id`
                        }
                    } else {
                        // create a new inbox with `addr` and nonce 0.
                    }
                } else {
                    let registered_inbox_id =
                        api_client.get_inbox_ids(vec![account_addresses]).await;
                    if registered_inbox_id.is_not_empty() {
                        if registered_inbox_id == inbox_id {
                            // create an identity, if local installation key doesn't belong to `id` then generate a new installation key & signature request
                        } else {
                            // create an identity that requires revoking the `addr` from `registered_id` and associate `addr`with `id`
                        }
                    } else {
                        // create a new inbox with `addr` and random nonce.
                    }
                }
            }
            #[cfg(test)]
            IdentityStrategy::ExternalIdentity(identity) => Ok(identity),
        }
    }

    async fn create_identity<ApiClient: XmtpMlsClient + XmtpIdentityClient>(
        api_client: &ApiClientWrapper<ApiClient>,
        inbox_id: InboxId,
        account_address: String,
        legacy_identity: LegacyIdentity,
    ) -> Result<Identity, ClientBuilderError> {
        info!("Creating identity");
        let identity = match legacy_identity {
            LegacyIdentity::None => {
                Identity::create_to_be_signed(inbox_id, account_address, api_client).await?
            }
            LegacyIdentity::FromKeys(key) => {
                Identity::create_from_legacy(inbox_id, account_address, key, api_client).await?
            }
        };
        Ok(identity)
    }
}
