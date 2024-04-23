#[cfg(test)]
use std::println as debug;

#[cfg(not(test))]
use log::debug;
use log::info;

use xmtp_proto::api_client::{XmtpIdentityClient, XmtpMlsClient};
use xmtp_cryptography::signature::sanitize_evm_addresses;

use crate::{
    api::ApiClientWrapper,
    builder::ClientBuilderError,
    storage::{identity::StoredIdentity, EncryptedMessageStore},
    xmtp_openmls_provider::XmtpOpenMlsProvider,
    Fetch, InboxOwner,
};

pub mod legacy;
pub use legacy::*;

/// Describes how the legacy v2 identity key was obtained, if applicable.
///
/// XMTP SDK's may embed libxmtp (v3) alongside existing v2 protocol logic
/// for backwards-compatibility purposes. In this case, the client may already
/// have a wallet-signed v2 key. Depending on the source of this key,
/// libxmtp may choose to bootstrap v3 installation keys using the existing
/// legacy key.
///
/// If the client supports v2, then the serialized bytes of the legacy
/// SignedPrivateKey proto for the v2 identity key should be provided.
pub enum LegacyIdentity {
    // A client with no support for v2 messages
    None,
    // A cached v2 key was provided on client initialization
    Static(Vec<u8>),
    // A private bundle exists on the network from which the v2 key will be fetched
    Network(Vec<u8>),
    // A new v2 key was generated on client initialization
    KeyGenerator(Vec<u8>),
}

/// Describes whether the v3 identity should be created
/// If CreateIfNotFound is chosen, the wallet account address and legacy
/// v2 identity should be specified, or set to LegacyIdentity::None if not applicable.
pub enum IdentityStrategy {
    /// Tries to get an identity from the disk store, if not found creates an identity.
    /// If a `LegacyIdentity` is provided it will be converted to a `v3` identity.
    CreateIfNotFound(String, LegacyIdentity),
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
        let identity_option: Option<Identity> = provider
            .conn()
            .fetch(&())?
            .map(|i: StoredIdentity| i.into());
        debug!("Existing identity in store: {:?}", identity_option);
        match self {
            IdentityStrategy::CachedOnly => {
                identity_option.ok_or(ClientBuilderError::RequiredIdentityNotFound)
            }
            IdentityStrategy::CreateIfNotFound(account_address, legacy_identity) => {
                let account_address = sanitize_evm_addresses(vec![account_address])?[0].clone();
                match identity_option {
                    Some(identity) => {
                        if identity.account_address != account_address {
                            return Err(ClientBuilderError::StoredIdentityMismatch);
                        }
                        Ok(identity)
                    }
                    None => Ok(
                        Self::create_identity(api_client, account_address, legacy_identity).await?,
                    ),
                }
            }
            #[cfg(test)]
            IdentityStrategy::ExternalIdentity(identity) => Ok(identity),
        }
    }

    async fn create_identity<ApiClient: XmtpMlsClient + XmtpIdentityClient>(
        api_client: &ApiClientWrapper<ApiClient>,
        account_address: String,
        legacy_identity: LegacyIdentity,
    ) -> Result<Identity, ClientBuilderError> {
        info!("Creating identity");
        let identity = match legacy_identity {
            // This is a fresh install, and at most one v2 signature (enable_identity)
            // has been requested so far, so it's fine to request another one (grant_messaging_access).
            LegacyIdentity::None | LegacyIdentity::Network(_) => {
                Identity::create_to_be_signed(account_address)?
            }
            // This is a new XMTP user and two v2 signatures (create_identity and enable_identity)
            // have just been requested, don't request a third.
            LegacyIdentity::KeyGenerator(legacy_signed_private_key) => {
                Identity::create_from_legacy(account_address, legacy_signed_private_key)?
            }
            // This is an existing v2 install being upgraded to v3, not a fresh install.
            // Don't request a signature out of the blue if possible.
            LegacyIdentity::Static(legacy_signed_private_key) => {
                if Identity::has_existing_legacy_credential(api_client, &account_address).await? {
                    // Another installation has already derived a v3 key from this v2 key.
                    // Don't reuse the same v2 key - make a new key altogether.
                    Identity::create_to_be_signed(account_address)?
                } else {
                    Identity::create_from_legacy(account_address, legacy_signed_private_key)?
                }
            }
        };
        Ok(identity)
    }
}

// Deprecated
impl<Owner> From<&Owner> for IdentityStrategy
where
    Owner: InboxOwner,
{
    fn from(value: &Owner) -> Self {
        IdentityStrategy::CreateIfNotFound(value.get_address(), LegacyIdentity::None)
    }
}
