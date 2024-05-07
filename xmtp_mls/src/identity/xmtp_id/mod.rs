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
    /// Tries to get an identity from the disk store, if not found creates an identity, will create an inbox if address isn't associated with one already.
    ///
    /// Platform SDK could have following logic before `IdentityStrategy.initialize_identity(api_client, store)`:
    ///
    /// inbox_id = libxmtp.get_inbox_id(wallet_address)
    /// if !inbox_id {
    ///     inbox_id = libxmtp.generate_inbox_id(wallet_address, 0)
    ///     if use_legacy_key() {
    ///         pass // We will create inbox & add association in one call in `IdentityStrategy.initialize_identity(api_client, store)`.
    ///     } else {
    ///         inbox_id = libxmtp.generate_inbox_id(wallet_address, random_nonce)
    ///         libxmtp.create_new_inbox_with_user_signature(inbox_id, wallet_address)
    ///         TODO: Similar to use_legacy_key(), we can include this in one call so that user doesn't have to sign twice.
    ///     }
    /// } else {
    ///     // rare case
    ///     if user_wants_additional_inbox() {
    ///         inbox_id = libxmtp.generate_inbox_id(wallet_address, random_nonce)
    ///         libxmtp.create_new_inbox_with_user_signature(inbox_id, wallet_address)
    ///     }
    /// }
    ///
    /// identity = IdentityStrategy.initialize_identity(api_client, store)
    /// client = create_client(identity, ...)
    ///
    /// Note that if you want to associate the wallet with a different inbox_id than already associated, do that as a independent step before creating a client.
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
                    Member::Address(address) => {
                        let inbox_ids = api_client.get_inbox_ids(vec![address.clone()]).await?;
                        let inbox_id = inbox_ids
                            .get(&address)
                            .ok_or(ClientBuilderError::UncoveredCase)?;
                        if let Some(stored_identity) = stored_identity {
                            if &stored_identity.clone().inbox_id != inbox_id {
                                return Err(ClientBuilderError::StoredIdentityMismatch);
                            } else {
                                return Ok(stored_identity);
                            }
                        } else {
                            Identity::create_to_be_signed(inbox_id.to_string(), address, api_client)
                                .await?
                        }
                    }

                    Member::LegacyKey(address, key) => {
                        let inbox_ids = api_client.get_inbox_ids(vec![address.clone()]).await?;
                        let inbox_id = inbox_ids
                            .get(&address)
                            .ok_or(ClientBuilderError::UncoveredCase)?;

                        if let Some(stored_identity) = stored_identity {
                            return Ok(stored_identity);
                        } else {
                            Identity::create_from_legacy(inbox_id.clone(), address, key, api_client)
                                .await?
                        }
                    }
                };

                Ok(identity)
            }
            #[cfg(test)]
            IdentityStrategy::ExternalIdentity(identity) => Ok(identity),
        }
    }
}
