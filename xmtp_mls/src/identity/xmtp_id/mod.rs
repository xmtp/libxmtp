pub mod identity;

use crate::storage::identity_inbox::StoredIdentity;
use crate::{api::ApiClientWrapper, builder::ClientBuilderError, storage::EncryptedMessageStore};
use crate::{xmtp_openmls_provider::XmtpOpenMlsProvider, Fetch};
pub use identity::Identity;
use log::debug;
use log::info;
use xmtp_id::InboxId;
use xmtp_proto::api_client::{XmtpIdentityClient, XmtpMlsClient};

/// The member that the [Identity] is created from.
pub enum Member {
    /// user's Ethereum wallet address
    Address(String),
    /// address and corresponding legacy private key(same secp256k1 as Ethereum wallet)
    LegacyKey(String, Vec<u8>),
}

pub enum IdentityStrategy {
    /// Tries to get an identity from the disk store, if not found creates an identity.
    ///
    /// Platform SDK could have following logic before calling `libxmtp.create_client`:
    ///
    /// inbox_id = libxmtp.get_inbox_id(wallet_address)
    /// if !inbox_id {
    ///     inbox_id = libxmtp.generate_inbox_id(wallet_address, 0)
    ///     if use_legacy_key {
    ///         pass // We will create inbox & add association in one call in `libxmtp.create_client(inbox_id, member_address)`
    ///     } else {
    ///         inbox_id = libxmtp.generate_inbox_id(wallet_address, random_nonce)
    ///         libxmtp.create_new_inbox_with_user_signature(inbox_id, wallet_address)
    ///     }
    /// } else if user_wants_additional_inbox() {
    ///     inbox_id = libxmtp.generate_inbox_id(wallet_address, random_nonce)
    ///     libxmtp.create_new_inbox_with_user_signature(inbox_id, wallet_address)
    /// }
    ///
    /// libxmtp.create_client(inbox_id, member_address)
    CreateIfNotFound(InboxId, Member),
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
                stored_identity.ok_or(ClientBuilderError::RequiredIdentityNotFound)
            }
            IdentityStrategy::CreateIfNotFound(inbox_id, member) => {
                // sanity check if member is associated with inbox_id and then create identity
                let identity = match member {
                    Member::Address(address) => {
                        let inbox_ids = api_client.get_inbox_ids(vec![address.clone()]).await?;
                        let inbox_id = inbox_ids.get(&address).unwrap().as_ref().unwrap();
                        Identity::create_to_be_signed(inbox_id.to_string(), address, api_client)
                            .await?
                    }
                    Member::LegacyKey(account_address, key) => {
                        Identity::create_from_legacy(inbox_id, account_address, key, api_client)
                            .await?
                    }
                };

                Ok(identity)
            }
            #[cfg(test)]
            IdentityStrategy::ExternalIdentity(identity) => Ok(identity),
        }
    }
}
