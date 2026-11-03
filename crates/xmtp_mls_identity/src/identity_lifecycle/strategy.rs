//! How a client finds or creates its identity on startup.

use crate::XmtpApi;
use crate::identity::{Identity, IdentityError};
use tracing::{debug, info};
use xmtp_api::ApiClientWrapper;
use xmtp_common::snippet::Snippet;
use xmtp_db::identity::StoredIdentity;
use xmtp_db::{Fetch, XmtpMlsStorageProvider};
use xmtp_id::associations::Identifier;
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_id::{InboxId, InboxIdRef};

/**
 * The identity strategy determines how the [`ClientBuilder`](crate::builder::ClientBuilder) constructs an identity on startup.
 *
 * [`IdentityStrategy::CreateIfNotFound`] will attempt to create a new identity if one isn't found in the store.
 * This is the default behavior.
 *
 * [`IdentityStrategy::CreateForIdentifier`] acts as `CreateIfNotFound` for the inbox of an identifier,
 * which the builder asks the backend for only after it has checked the backend's deployment.
 *
 * [`IdentityStrategy::CachedOnly`] will attempt to get an identity from the store. If not found, it will
 * return an error. This is useful if you don't want to create a new identity on startup because the caller
 * does not have access to a signer.
 *
 * `IdentityStrategy::ExternalIdentity` allows you to provide an already-constructed identity to the
 * client. This is useful for testing and not expected to be used in production.
 */
#[derive(Debug, Clone)]
pub enum IdentityStrategy {
    /// Tries to get an identity from the disk store. If not found, getting one from backend.
    CreateIfNotFound {
        inbox_id: InboxId,
        identifier: Identifier,
        nonce: u64,
    },
    /// `CreateIfNotFound` for the inbox the backend associates with the
    /// identifier, or else the inbox its registration at the nonce creates.
    CreateForIdentifier { identifier: Identifier, nonce: u64 },
    /// Identity that is already in the disk store
    CachedOnly,
    /// An already-built Identity for testing purposes
    #[cfg(any(test, feature = "test-utils"))]
    ExternalIdentity(Identity),
}

impl IdentityStrategy {
    pub fn inbox_id(&self) -> Option<InboxIdRef<'_>> {
        use IdentityStrategy::*;
        match self {
            CreateIfNotFound { inbox_id, .. } => Some(inbox_id),
            _ => None,
        }
    }

    /// Create a new Identity Strategy, with [`IdentityStrategy::CreateIfNotFound`].
    /// If an Identity is not found in the local store, creates a new one.
    #[tracing::instrument(level = "trace", skip_all)]
    pub fn new(inbox_id: InboxId, identifier: Identifier, nonce: u64) -> Self {
        Self::CreateIfNotFound {
            inbox_id,
            identifier,
            nonce,
        }
    }

    /// Create an [`IdentityStrategy::CreateForIdentifier`], for a caller that
    /// does not know the identifier's inbox. The build takes the inbox the
    /// backend names, or the one the identifier creates at `nonce`, and fails
    /// with [`IdentityError::IdentifierNotInInbox`] unless the identifier
    /// belongs to it, as
    /// [`ClientBuilder::build`](crate::builder::ClientBuilder::build)
    /// describes. An offline build checks the newest state the database
    /// stores, which can predate a revocation.
    pub fn for_identifier(identifier: Identifier, nonce: u64) -> Self {
        Self::CreateForIdentifier { identifier, nonce }
    }

    /// Replace [`IdentityStrategy::CreateForIdentifier`] with
    /// [`IdentityStrategy::CreateIfNotFound`] for the identifier's inbox. Call
    /// it only after the backend's deployment is checked: the request carries
    /// the identifier.
    pub(crate) async fn with_inbox<ApiClient: XmtpApi>(
        self,
        api_client: &ApiClientWrapper<ApiClient>,
    ) -> Result<Self, IdentityError> {
        let Self::CreateForIdentifier { identifier, nonce } = self else {
            return Ok(self);
        };
        let found = api_client
            .get_inbox_ids(vec![identifier.clone().into()])
            .await?;
        let inbox_id = match found.into_iter().next().flatten() {
            Some(inbox_id) => inbox_id,
            None => identifier.inbox_id(nonce)?,
        };
        Ok(Self::new(inbox_id, identifier, nonce))
    }
}

impl IdentityStrategy {
    /**
     * Initialize an identity from the given strategy. If a stored identity is found in the database,
     * it will return that identity.
     *
     * If a stored identity is found, it will validate that the inbox_id of the stored identity matches
     * the inbox_id configured on the strategy.
     *
     **/
    #[tracing::instrument(level = "trace", skip_all)]
    pub(crate) async fn initialize_identity<ApiClient: XmtpApi, S: XmtpMlsStorageProvider>(
        self,
        api_client: &ApiClientWrapper<ApiClient>,
        mls_storage: &S,
        scw_signature_verifier: impl SmartContractSignatureVerifier,
    ) -> Result<Identity, IdentityError> {
        use IdentityStrategy::*;

        info!("Initializing identity");
        let stored_identity: Option<Identity> = mls_storage
            .db()
            .fetch(&())?
            .map(|i: StoredIdentity| i.try_into())
            .transpose()?;

        let strategy = match &self {
            CreateIfNotFound { .. } => "create_if_not_found",
            CreateForIdentifier { .. } => "create_for_identifier",
            CachedOnly => "cached_only",
            #[cfg(any(test, feature = "test-utils"))]
            ExternalIdentity(_) => "external_identity",
        };
        debug!(
            strategy,
            stored_identity_present = stored_identity.is_some(),
            "identity strategy"
        );
        match self.with_inbox(api_client).await? {
            CachedOnly => stored_identity.ok_or(IdentityError::RequiredIdentityNotFound),
            CreateForIdentifier { .. } => unreachable!("with_inbox replaces the strategy"),
            CreateIfNotFound {
                inbox_id,
                identifier,
                nonce,
            } => {
                if let Some(stored_identity) = stored_identity {
                    tracing::debug!(
                        installation_id =
                            stored_identity.installation_keys.public_bytes().snippet(),
                        inbox_id = stored_identity.inbox_id,
                        "Found existing identity in store"
                    );
                    if inbox_id != stored_identity.inbox_id {
                        return Err(IdentityError::InboxIdMismatch {
                            id: inbox_id.clone(),
                            stored: stored_identity.inbox_id,
                        });
                    }

                    Ok(stored_identity)
                } else {
                    crate::identity_lifecycle::new_identity(
                        inbox_id,
                        identifier,
                        nonce,
                        api_client,
                        mls_storage,
                        &scw_signature_verifier,
                    )
                    .await
                }
            }
            #[cfg(any(test, feature = "test-utils"))]
            ExternalIdentity(identity) => Ok(identity),
        }
    }
}
