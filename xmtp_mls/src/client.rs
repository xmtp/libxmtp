#[cfg(any(test, feature = "test-utils"))]
use crate::groups::device_sync::WorkerHandle;
use crate::groups::group_mutable_metadata::MessageDisappearingSettings;
use crate::groups::{ConversationListItem, DMMetadataOptions};
use crate::storage::consent_record::ConsentType;
use crate::utils::VersionInfo;
use crate::GroupCommitLock;
use crate::{
    groups::{
        device_sync::preference_sync::UserPreferenceUpdate, group_metadata::DmMembers,
        group_permissions::PolicySet, GroupError, GroupMetadataOptions, MlsGroup,
    },
    identity::{parse_credential, Identity, IdentityError},
    identity_updates::{load_identity_updates, IdentityUpdateError},
    mutex_registry::MutexRegistry,
    storage::{
        consent_record::{ConsentState, StoredConsentRecord},
        db_connection::DbConnection,
        group::{GroupMembershipState, GroupQueryArgs, StoredGroup},
        group_message::StoredGroupMessage,
        refresh_state::EntityKind,
        xmtp_openmls_provider::XmtpOpenMlsProvider,
        EncryptedMessageStore, NotFound, StorageError,
    },
    subscriptions::{LocalEventError, LocalEvents},
    types::InstallationId,
    verified_key_package_v2::{KeyPackageVerificationError, VerifiedKeyPackageV2},
    Fetch, XmtpApi,
};
use futures::stream::{self, FuturesUnordered, StreamExt};
use openmls::prelude::tls_codec::Error as TlsCodecError;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use thiserror::Error;
use tokio::sync::broadcast;
use xmtp_api::ApiClientWrapper;
use xmtp_common::{retry_async, retryable, Retry};
use xmtp_cryptography::signature::IdentifierValidationError;
use xmtp_id::{
    associations::{
        builder::{SignatureRequest, SignatureRequestError},
        AssociationError, AssociationState, Identifier, MemberIdentifier, SignatureError,
    },
    scw_verifier::{RemoteSignatureVerifier, SmartContractSignatureVerifier},
    InboxId, InboxIdRef,
};
use xmtp_proto::api_client::{ApiStats, IdentityStats};
use xmtp_proto::xmtp::mls::api::v1::{welcome_message, GroupMessage, WelcomeMessage};

/// Enum representing the network the Client is connected to
#[derive(Clone, Copy, Default, Debug)]
pub enum Network {
    Local(&'static str),
    #[default]
    Dev,
    Prod,
}

#[derive(Debug, Error)]
pub enum ClientError {
    #[error(transparent)]
    AddressValidation(#[from] IdentifierValidationError),
    #[error("could not publish: {0}")]
    PublishError(String),
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("dieselError: {0}")]
    Diesel(#[from] diesel::result::Error),
    #[error("API error: {0}")]
    Api(#[from] xmtp_api::Error),
    #[error("identity error: {0}")]
    Identity(#[from] crate::identity::IdentityError),
    #[error("TLS Codec error: {0}")]
    TlsError(#[from] TlsCodecError),
    #[error("key package verification: {0}")]
    KeyPackageVerification(#[from] KeyPackageVerificationError),
    #[error("Stream inconsistency error: {0}")]
    StreamInconsistency(String),
    #[error("Association error: {0}")]
    Association(#[from] AssociationError),
    #[error("signature validation error: {0}")]
    SignatureValidation(#[from] SignatureError),
    #[error(transparent)]
    IdentityUpdate(#[from] IdentityUpdateError),
    #[error(transparent)]
    SignatureRequest(#[from] SignatureRequestError),
    // the box is to prevent infinite cycle between client and group errors
    #[error(transparent)]
    Group(Box<GroupError>),
    #[error(transparent)]
    LocalEvent(#[from] LocalEventError),
    #[error("generic:{0}")]
    Generic(String),
}

impl ClientError {
    pub fn db_needs_connection(&self) -> bool {
        match self {
            Self::Storage(s) => s.db_needs_connection(),
            _ => false,
        }
    }
}

impl From<NotFound> for ClientError {
    fn from(value: NotFound) -> Self {
        ClientError::Storage(StorageError::NotFound(value))
    }
}

impl From<GroupError> for ClientError {
    fn from(err: GroupError) -> ClientError {
        ClientError::Group(Box::new(err))
    }
}

impl xmtp_common::RetryableError for ClientError {
    fn is_retryable(&self) -> bool {
        match self {
            ClientError::Group(group_error) => retryable!(group_error),
            ClientError::Diesel(diesel_error) => retryable!(diesel_error),
            ClientError::Api(api_error) => retryable!(api_error),
            ClientError::Storage(storage_error) => retryable!(storage_error),
            ClientError::Generic(err) => err.contains("database is locked"),
            _ => false,
        }
    }
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

/// Clients manage access to the network, identity, and data store
pub struct Client<ApiClient, V = RemoteSignatureVerifier<ApiClient>> {
    pub(crate) api_client: Arc<ApiClientWrapper<ApiClient>>,
    pub(crate) context: Arc<XmtpMlsLocalContext>,
    pub(crate) history_sync_url: Option<String>,
    pub(crate) local_events: broadcast::Sender<LocalEvents>,
    /// The method of verifying smart contract wallet signatures for this Client
    pub(crate) scw_verifier: Arc<V>,
    pub(crate) version_info: Arc<VersionInfo>,

    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) sync_worker_handle: Arc<parking_lot::Mutex<Option<Arc<WorkerHandle>>>>,
}

// most of these things are `Arc`'s
impl<ApiClient, V> Clone for Client<ApiClient, V> {
    fn clone(&self) -> Self {
        Self {
            api_client: self.api_client.clone(),
            context: self.context.clone(),
            history_sync_url: self.history_sync_url.clone(),
            local_events: self.local_events.clone(),
            scw_verifier: self.scw_verifier.clone(),
            version_info: self.version_info.clone(),

            #[cfg(any(test, feature = "test-utils"))]
            sync_worker_handle: self.sync_worker_handle.clone(),
        }
    }
}

/// The local context a XMTP MLS needs to function:
/// - Sqlite Database
/// - Identity for the User
pub struct XmtpMlsLocalContext {
    /// XMTP Identity
    pub(crate) identity: Identity,
    /// XMTP Local Storage
    store: EncryptedMessageStore,
    pub(crate) mutexes: MutexRegistry,
    pub(crate) mls_commit_lock: std::sync::Arc<GroupCommitLock>,
}

impl XmtpMlsLocalContext {
    /// The installation public key is the primary identifier for an installation
    pub fn installation_public_key(&self) -> InstallationId {
        (*self.identity.installation_keys.public_bytes()).into()
    }

    /// Get the account address of the blockchain account associated with this client
    pub fn inbox_id(&self) -> InboxIdRef<'_> {
        self.identity.inbox_id()
    }

    /// Get sequence id, may not be consistent with the backend
    pub fn inbox_sequence_id(&self, conn: &DbConnection) -> Result<i64, StorageError> {
        self.identity.sequence_id(conn)
    }

    pub fn store(&self) -> &EncryptedMessageStore {
        &self.store
    }

    /// Pulls a new database connection and creates a new provider
    pub fn mls_provider(&self) -> Result<XmtpOpenMlsProvider, StorageError> {
        Ok(self.store.conn()?.into())
    }

    /// Integrators should always check the `signature_request` return value of this function before calling [`register_identity`](Self::register_identity).
    /// If `signature_request` returns `None`, then the wallet signature is not required and [`register_identity`](Self::register_identity) can be called with None as an argument.
    pub fn signature_request(&self) -> Option<SignatureRequest> {
        self.identity.signature_request()
    }

    pub fn sign_with_public_context(
        &self,
        text: impl AsRef<str>,
    ) -> Result<Vec<u8>, IdentityError> {
        self.identity.sign_with_public_context(text)
    }

    pub fn mls_commit_lock(&self) -> &Arc<GroupCommitLock> {
        &self.mls_commit_lock
    }
}

impl<ApiClient, V> Client<ApiClient, V>
where
    ApiClient: XmtpApi,
    V: SmartContractSignatureVerifier,
{
    // Test only function to update the version of the client
    #[cfg(test)]
    pub fn test_update_version(&mut self, version: &str) {
        Arc::make_mut(&mut self.version_info).test_update_version(version);
    }

    pub fn api_stats(&self) -> ApiStats {
        self.api_client.api_client.stats()
    }

    pub fn identity_api_stats(&self) -> IdentityStats {
        self.api_client.api_client.identity_stats()
    }
}

impl<ApiClient, V> Client<ApiClient, V>
where
    ApiClient: XmtpApi,
    V: SmartContractSignatureVerifier,
{
    /// Create a new client with the given network, identity, and store.
    /// It is expected that most users will use the [`ClientBuilder`](crate::builder::ClientBuilder) instead of instantiating
    /// a client directly.
    pub fn new(
        mut api_client: ApiClientWrapper<ApiClient>,
        identity: Identity,
        store: EncryptedMessageStore,
        scw_verifier: V,
        history_sync_url: Option<String>,
    ) -> Self
    where
        V: SmartContractSignatureVerifier,
    {
        api_client.attach_inbox_id(Some(identity.inbox_id().to_string()));
        let context = Arc::new(XmtpMlsLocalContext {
            identity,
            store,
            mutexes: MutexRegistry::new(),
            mls_commit_lock: Arc::new(GroupCommitLock::new()),
        });
        let (tx, _) = broadcast::channel(32);

        Self {
            api_client: api_client.into(),
            context,
            history_sync_url,
            local_events: tx,
            #[cfg(any(test, feature = "test-utils"))]
            sync_worker_handle: Arc::new(parking_lot::Mutex::default()),
            scw_verifier: scw_verifier.into(),
            version_info: Arc::new(VersionInfo::default()),
        }
    }

    pub fn scw_verifier(&self) -> &Arc<V> {
        &self.scw_verifier
    }

    pub fn version_info(&self) -> &Arc<VersionInfo> {
        &self.version_info
    }
}

impl<ApiClient, V> Client<ApiClient, V>
where
    ApiClient: XmtpApi + Send + Sync + 'static,
    V: SmartContractSignatureVerifier + Send + Sync + 'static,
{
    /// Reconnect to the client's database if it has previously been released
    pub fn reconnect_db(&self) -> Result<(), ClientError> {
        self.context.store.reconnect()?;
        // restart all the workers
        // TODO: The only worker we have right now are the
        // sync workers. if we have other workers we
        // should create a better way to track them.
        if self.history_sync_url.is_some() {
            self.start_sync_worker();
        }

        self.start_disappearing_messages_cleaner_worker();

        Ok(())
    }
}

impl<ApiClient, V> Client<ApiClient, V>
where
    ApiClient: XmtpApi,
    V: SmartContractSignatureVerifier,
{
    /// Retrieves the client's installation public key, sometimes also called `installation_id`
    pub fn installation_public_key(&self) -> InstallationId {
        self.context.installation_public_key()
    }
    /// Retrieves the client's inbox ID
    pub fn inbox_id(&self) -> InboxIdRef<'_> {
        self.context.inbox_id()
    }

    /// Pulls a connection and creates a new MLS Provider
    pub fn mls_provider(&self) -> Result<XmtpOpenMlsProvider, StorageError> {
        self.context.mls_provider()
    }

    pub fn history_sync_url(&self) -> Option<&String> {
        self.history_sync_url.as_ref()
    }

    /// Calls the server to look up the `inbox_id` associated with a given identifier
    pub async fn find_inbox_id_from_identifier(
        &self,
        conn: &DbConnection,
        identifier: Identifier,
    ) -> Result<Option<String>, ClientError> {
        let results = self
            .find_inbox_ids_from_identifiers(conn, &[identifier])
            .await?;
        Ok(results.into_iter().next().flatten())
    }

    /// Calls the server to look up the `inbox_id`s` associated with a list of identifiers.
    /// If no `inbox_id` is found, returns None.
    pub(crate) async fn find_inbox_ids_from_identifiers(
        &self,
        conn: &DbConnection,
        identifiers: &[Identifier],
    ) -> Result<Vec<Option<String>>, ClientError> {
        let mut cached_inbox_ids = conn.fetch_cached_inbox_ids(identifiers)?;
        let mut new_inbox_ids = HashMap::default();

        let missing: Vec<_> = identifiers
            .iter()
            .filter(|ident| !cached_inbox_ids.contains_key(&format!("{ident}")))
            .collect();

        if !missing.is_empty() {
            let identifiers = identifiers.iter().map(Into::into).collect();
            new_inbox_ids = self.api_client.get_inbox_ids(identifiers).await?;
        }

        let inbox_ids = identifiers
            .iter()
            .map(|ident| {
                let cache_key = format!("{ident}");
                if let Some(inbox_id) = cached_inbox_ids.remove(&cache_key) {
                    return Some(inbox_id);
                }
                if let Some(inbox_id) = new_inbox_ids.remove(&ident.into()) {
                    return Some(inbox_id);
                }
                None
            })
            .collect();
        Ok(inbox_ids)
    }

    /// Get the highest `sequence_id` from the local database for the client's `inbox_id`.
    /// This may not be consistent with the latest state on the backend.
    pub fn inbox_sequence_id(&self, conn: &DbConnection) -> Result<i64, StorageError> {
        self.context.inbox_sequence_id(conn)
    }

    /// Get the [`AssociationState`] for the client's `inbox_id`
    pub async fn inbox_state(
        &self,
        refresh_from_network: bool,
    ) -> Result<AssociationState, ClientError> {
        let conn = self.store().conn()?;
        let inbox_id = self.inbox_id();
        if refresh_from_network {
            load_identity_updates(&self.api_client, &conn, &[inbox_id]).await?;
        }
        let state = self.get_association_state(&conn, inbox_id, None).await?;
        Ok(state)
    }

    /// Get the [`AssociationState`] for each `inbox_id`
    pub async fn inbox_addresses(
        &self,
        refresh_from_network: bool,
        inbox_ids: Vec<InboxIdRef<'_>>,
    ) -> Result<Vec<AssociationState>, ClientError> {
        let conn = self.store().conn()?;
        if refresh_from_network {
            load_identity_updates(&self.api_client, &conn, &inbox_ids).await?;
        }
        let state = self
            .batch_get_association_state(
                &conn,
                &inbox_ids.into_iter().map(|i| (i, None)).collect::<Vec<_>>(),
            )
            .await?;
        Ok(state)
    }

    /// Set a consent record in the local database.
    /// If the consent record is an address set the consent state for both the address and `inbox_id`
    pub async fn set_consent_states(
        &self,
        records: &[StoredConsentRecord],
    ) -> Result<(), ClientError> {
        let conn = self.store().conn()?;
        let changed_records = conn.insert_or_replace_consent_records(records)?;

        if self.history_sync_url.is_some() && !changed_records.is_empty() {
            let records = changed_records
                .into_iter()
                .map(UserPreferenceUpdate::ConsentUpdate)
                .collect();
            let _ = self
                .local_events
                .send(LocalEvents::OutgoingPreferenceUpdates(records));
        }

        Ok(())
    }

    /// Get the consent state for a given entity
    pub async fn get_consent_state(
        &self,
        entity_type: ConsentType,
        entity: String,
    ) -> Result<ConsentState, ClientError> {
        let conn = self.store().conn()?;
        let record = conn.get_consent_record(entity, entity_type)?;

        match record {
            Some(rec) => Ok(rec.state),
            None => Ok(ConsentState::Unknown),
        }
    }

    /// Gets a reference to the client's store
    pub fn store(&self) -> &EncryptedMessageStore {
        &self.context.store
    }

    /// Release the client's database connection
    pub fn release_db_connection(&self) -> Result<(), ClientError> {
        let store = &self.context.store;
        store.release_connection()?;
        Ok(())
    }

    /// Get a reference to the client's identity struct
    pub fn identity(&self) -> &Identity {
        &self.context.identity
    }

    /// Get a reference (in an Arc) to the client's local context
    pub fn context(&self) -> &Arc<XmtpMlsLocalContext> {
        &self.context
    }

    /// Create a new group with the default settings
    /// Applies a custom [`PolicySet`] to the group if one is specified
    pub fn create_group(
        &self,
        permissions_policy_set: Option<PolicySet>,
        opts: GroupMetadataOptions,
    ) -> Result<MlsGroup<Self>, ClientError> {
        tracing::info!("creating group");
        let provider = self.mls_provider()?;
        let group: MlsGroup<Client<ApiClient, V>> = MlsGroup::create_and_insert(
            Arc::new(self.clone()),
            &provider,
            GroupMembershipState::Allowed,
            permissions_policy_set.unwrap_or_default(),
            opts,
        )?;

        // notify streams of our new group
        let _ = self
            .local_events
            .send(LocalEvents::NewGroup(group.group_id.clone()));

        Ok(group)
    }

    /// Create a group with an initial set of members added
    pub async fn create_group_with_members(
        &self,
        account_identifiers: &[Identifier],
        permissions_policy_set: Option<PolicySet>,
        opts: GroupMetadataOptions,
    ) -> Result<MlsGroup<Self>, ClientError> {
        tracing::info!("creating group");
        let group = self.create_group(permissions_policy_set, opts)?;

        group.add_members(account_identifiers).await?;

        Ok(group)
    }

    pub async fn create_group_with_inbox_ids(
        &self,
        inbox_ids: &[InboxId],
        permissions_policy_set: Option<PolicySet>,
        opts: GroupMetadataOptions,
    ) -> Result<MlsGroup<Self>, ClientError> {
        tracing::info!("creating group");
        let group = self.create_group(permissions_policy_set, opts)?;

        group.add_members_by_inbox_id(inbox_ids).await?;

        Ok(group)
    }

    /// Create a new Direct Message with the default settings
    async fn create_dm_by_inbox_id(
        &self,
        dm_target_inbox_id: InboxId,
        opts: DMMetadataOptions,
    ) -> Result<MlsGroup<Self>, ClientError> {
        tracing::info!("creating dm with {}", dm_target_inbox_id);
        let provider = self.mls_provider()?;

        let group: MlsGroup<Client<ApiClient, V>> = MlsGroup::create_dm_and_insert(
            &provider,
            Arc::new(self.clone()),
            GroupMembershipState::Allowed,
            dm_target_inbox_id.clone(),
            opts,
        )?;

        group
            .add_members_by_inbox_id_with_provider(&provider, &[dm_target_inbox_id])
            .await?;

        // notify any streams of the new group
        let _ = self
            .local_events
            .send(LocalEvents::NewGroup(group.group_id.clone()));

        Ok(group)
    }

    /// Find or create a Direct Message with the default settings
    pub async fn find_or_create_dm(
        &self,
        target_identity: Identifier,
        opts: DMMetadataOptions,
    ) -> Result<MlsGroup<Self>, ClientError> {
        tracing::info!("finding or creating dm with address: {target_identity}");
        let provider = self.mls_provider()?;
        let inbox_id = match self
            .find_inbox_id_from_identifier(provider.conn_ref(), target_identity.clone())
            .await?
        {
            Some(id) => id,
            None => {
                return Err(NotFound::InboxIdForAddress(target_identity.to_string()).into());
            }
        };

        self.find_or_create_dm_by_inbox_id(inbox_id, opts).await
    }

    /// Find or create a Direct Message by inbox_id with the default settings
    pub async fn find_or_create_dm_by_inbox_id(
        &self,
        inbox_id: InboxId,
        opts: DMMetadataOptions,
    ) -> Result<MlsGroup<Self>, ClientError> {
        tracing::info!("finding or creating dm with inbox_id: {}", inbox_id);
        let provider = self.mls_provider()?;
        let group = provider.conn_ref().find_dm_group(&DmMembers {
            member_one_inbox_id: self.inbox_id(),
            member_two_inbox_id: &inbox_id,
        })?;
        if let Some(group) = group {
            return Ok(MlsGroup::new(self.clone(), group.id, group.created_at_ns));
        }
        self.create_dm_by_inbox_id(inbox_id, opts).await
    }

    pub(crate) fn create_sync_group(
        &self,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<MlsGroup<Self>, ClientError> {
        tracing::info!("creating sync group");
        let sync_group = MlsGroup::create_and_insert_sync_group(Arc::new(self.clone()), provider)?;

        Ok(sync_group)
    }

    /// Look up a group by its ID
    ///
    /// Returns a [`MlsGroup`] if the group exists, or an error if it does not
    ///
    pub fn group_with_conn(
        &self,
        conn: &DbConnection,
        group_id: &Vec<u8>,
    ) -> Result<MlsGroup<Self>, ClientError> {
        let stored_group: Option<StoredGroup> = conn.fetch(group_id)?;
        stored_group
            .map(|g| MlsGroup::new(self.clone(), g.id, g.created_at_ns))
            .ok_or(NotFound::GroupById(group_id.clone()))
            .map_err(Into::into)
    }

    /// Look up a group by its ID
    ///
    /// Returns a [`MlsGroup`] if the group exists, or an error if it does not
    ///
    pub fn group(&self, group_id: Vec<u8>) -> Result<MlsGroup<Self>, ClientError> {
        let conn = &self.store().conn()?;
        self.group_with_conn(conn, &group_id)
    }

    /// Look up a group by its ID while stitching DMs
    ///
    /// Returns a [`MlsGroup`] if the group exists, or an error if it does not
    ///
    pub fn stitched_group(&self, group_id: &[u8]) -> Result<MlsGroup<Self>, ClientError> {
        let conn = &mut self.store().conn()?;
        self.stitched_group_with_conn(conn, group_id)
    }

    /// Look up a group by its ID while stitching DMs
    ///
    /// Returns a [`MlsGroup`] if the group exists, or an error if it does not
    ///
    pub fn stitched_group_with_conn(
        &self,
        conn: &DbConnection,
        group_id: &[u8],
    ) -> Result<MlsGroup<Self>, ClientError> {
        let stored_group = conn.fetch_stitched(group_id)?;
        stored_group
            .map(|g| MlsGroup::new(self.clone(), g.id, g.created_at_ns))
            .ok_or(NotFound::GroupById(group_id.to_vec()))
            .map_err(Into::into)
    }

    /// Fetches the message disappearing settings for a given group ID.
    ///
    /// Returns `Some(MessageDisappearingSettings)` if the group exists and has valid settings,
    /// `None` if the group or settings are missing, or `Err(ClientError)` on a database error.
    pub fn group_disappearing_settings(
        &self,
        group_id: Vec<u8>,
    ) -> Result<Option<MessageDisappearingSettings>, ClientError> {
        let conn = &mut self.store().conn()?;
        let stored_group: Option<StoredGroup> = conn.fetch(&group_id)?;

        let settings = stored_group.and_then(|group| {
            let from_ns = group.message_disappear_from_ns?;
            let in_ns = group.message_disappear_in_ns?;

            Some(MessageDisappearingSettings { from_ns, in_ns })
        });

        Ok(settings)
    }

    /**
     * Look up a DM group by the target's inbox_id.
     *
     * Returns a [`MlsGroup`] if the group exists, or an error if it does not
     */
    pub fn dm_group_from_target_inbox(
        &self,
        target_inbox_id: String,
    ) -> Result<MlsGroup<Self>, ClientError> {
        let conn = self.store().conn()?;

        let group = conn
            .find_dm_group(&DmMembers {
                member_one_inbox_id: self.inbox_id(),
                member_two_inbox_id: &target_inbox_id,
            })?
            .ok_or(NotFound::DmByInbox(target_inbox_id))?;
        Ok(MlsGroup::new(self.clone(), group.id, group.created_at_ns))
    }

    /// Look up a message by its ID
    /// Returns a [`StoredGroupMessage`] if the message exists, or an error if it does not
    pub fn message(&self, message_id: Vec<u8>) -> Result<StoredGroupMessage, ClientError> {
        let conn = &mut self.store().conn()?;
        let message = conn.get_group_message(&message_id)?;
        Ok(message.ok_or(NotFound::MessageById(message_id))?)
    }

    /// Query for groups with optional filters
    ///
    /// Filters:
    /// - allowed_states: only return groups with the given membership states
    /// - created_after_ns: only return groups created after the given timestamp (in nanoseconds)
    /// - created_before_ns: only return groups created before the given timestamp (in nanoseconds)
    /// - limit: only return the first `limit` groups
    pub fn find_groups(&self, args: GroupQueryArgs) -> Result<Vec<MlsGroup<Self>>, ClientError> {
        Ok(self
            .store()
            .conn()?
            .find_groups(args)?
            .into_iter()
            .map(|stored_group| {
                MlsGroup::new(self.clone(), stored_group.id, stored_group.created_at_ns)
            })
            .collect())
    }

    pub fn list_conversations(
        &self,
        args: GroupQueryArgs,
    ) -> Result<Vec<ConversationListItem<Self>>, ClientError> {
        Ok(self
            .store()
            .conn()?
            .fetch_conversation_list(args)?
            .into_iter()
            .map(|conversation_item| {
                let message = conversation_item.message_id.and_then(|message_id| {
                    // Only construct StoredGroupMessage if all fields are Some
                    Some(StoredGroupMessage {
                        id: message_id,
                        group_id: conversation_item.id.clone(),
                        decrypted_message_bytes: conversation_item.decrypted_message_bytes?,
                        sent_at_ns: conversation_item.sent_at_ns?,
                        sender_installation_id: conversation_item.sender_installation_id?,
                        sender_inbox_id: conversation_item.sender_inbox_id?,
                        kind: conversation_item.kind?,
                        delivery_status: conversation_item.delivery_status?,
                        content_type: conversation_item.content_type?,
                        version_major: conversation_item.version_major?,
                        version_minor: conversation_item.version_minor?,
                        authority_id: conversation_item.authority_id?,
                        reference_id: None, // conversation_item does not use message reference_id
                    })
                });

                ConversationListItem {
                    group: MlsGroup::new(
                        self.clone(),
                        conversation_item.id,
                        conversation_item.created_at_ns,
                    ),
                    last_message: message,
                }
            })
            .collect())
    }

    /// Upload a Key Package to the network and publish the signed identity update
    /// from the provided SignatureRequest
    pub async fn register_identity(
        &self,
        signature_request: SignatureRequest,
    ) -> Result<(), ClientError> {
        tracing::info!("registering identity");
        // Register the identity before applying the signature request
        let provider: XmtpOpenMlsProvider = self.store().conn()?.into();

        self.identity()
            .register(&provider, &self.api_client)
            .await?;

        self.apply_signature_request(signature_request).await?;
        self.identity().set_ready();
        Ok(())
    }

    /// Upload a new key package to the network replacing an existing key package
    /// This is expected to be run any time the client receives new Welcome messages
    pub async fn rotate_and_upload_key_package(
        &self,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<(), ClientError> {
        self.identity()
            .rotate_and_upload_key_package(provider, &self.api_client)
            .await?;

        Ok(())
    }

    /// Query for group messages that have a `sequence_id` > than the highest cursor
    /// found in the local database
    pub(crate) async fn query_group_messages(
        &self,
        group_id: &[u8],
        conn: &DbConnection,
    ) -> Result<Vec<GroupMessage>, ClientError> {
        let id_cursor = conn.get_last_cursor_for_id(group_id, EntityKind::Group)?;

        let messages = self
            .api_client
            .query_group_messages(group_id.to_vec(), Some(id_cursor as u64))
            .await?;

        Ok(messages)
    }

    /// Query for welcome messages that have a `sequence_id` > than the highest cursor
    /// found in the local database
    pub(crate) async fn query_welcome_messages(
        &self,
        conn: &DbConnection,
    ) -> Result<Vec<WelcomeMessage>, ClientError> {
        let installation_id = self.installation_public_key();
        let id_cursor = conn.get_last_cursor_for_id(installation_id, EntityKind::Welcome)?;

        let welcomes = self
            .api_client
            .query_welcome_messages(installation_id.as_ref(), Some(id_cursor as u64))
            .await?;

        Ok(welcomes)
    }

    /// Fetches the current key package from the network for each of the `installation_id`s specified
    #[tracing::instrument(level = "trace", skip_all)]
    pub(crate) async fn get_key_packages_for_installation_ids(
        &self,
        installation_ids: Vec<Vec<u8>>,
    ) -> Result<
        HashMap<Vec<u8>, Result<VerifiedKeyPackageV2, KeyPackageVerificationError>>,
        ClientError,
    > {
        let key_package_results = self
            .api_client
            .fetch_key_packages(installation_ids.clone())
            .await?;

        let crypto_provider = XmtpOpenMlsProvider::new_crypto();

        let results: HashMap<Vec<u8>, Result<VerifiedKeyPackageV2, KeyPackageVerificationError>> =
            installation_ids
                .iter()
                .zip(key_package_results.values())
                .map(|(id, bytes)| {
                    (
                        id.clone(),
                        VerifiedKeyPackageV2::from_bytes(&crypto_provider, bytes),
                    )
                })
                .collect();

        Ok(results)
    }

    /// Download all unread welcome messages and converts to a group struct, ignoring malformed messages.
    /// Returns any new groups created in the operation
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn sync_welcomes(
        &self,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<Vec<MlsGroup<Self>>, GroupError> {
        let envelopes = self.query_welcome_messages(provider.conn_ref()).await?;
        let num_envelopes = envelopes.len();

        let groups: Vec<MlsGroup<Self>> = stream::iter(envelopes.into_iter())
            .filter_map(|envelope: WelcomeMessage| async {
                let welcome_v1 = match envelope.version {
                    Some(welcome_message::Version::V1(v1)) => v1,
                    _ => {
                        tracing::error!(
                            "failed to extract welcome message, invalid payload only v1 supported."
                        );
                        return None;
                    }
                };
                retry_async!(
                    Retry::default(),
                    (async { self.process_new_welcome(provider, &welcome_v1).await })
                )
                .ok()
            })
            .collect()
            .await;

        // If any welcomes were found, rotate your key package
        if num_envelopes > 0 {
            self.rotate_and_upload_key_package(provider).await?;
        }

        Ok(groups)
    }

    /// Internal API to process a unread welcome message and convert to a group.
    /// In a database transaction, increments the cursor for a given installation and
    /// applies the update after the welcome processed successfully.
    async fn process_new_welcome(
        &self,
        provider: &XmtpOpenMlsProvider,
        welcome: &welcome_message::V1,
    ) -> Result<MlsGroup<Self>, GroupError> {
        let result = MlsGroup::create_from_welcome(self, provider, welcome).await;

        match result {
            Ok(mls_group) => Ok(mls_group),
            Err(err) => {
                use crate::DuplicateItem::*;
                use crate::StorageError::*;

                if matches!(err, GroupError::Storage(Duplicate(WelcomeId(_)))) {
                    tracing::warn!(
                        "failed to create group from welcome due to duplicate welcome ID: {}",
                        err
                    );
                } else {
                    tracing::error!("failed to create group from welcome: {}", err);
                }

                Err(err)
            }
        }
    }

    /// Sync all groups for the current installation and return the number of groups that were synced.
    /// Only active groups will be synced.
    pub async fn sync_all_groups(
        &self,
        groups: Vec<MlsGroup<Self>>,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<usize, GroupError> {
        let active_group_count = Arc::new(AtomicUsize::new(0));

        let sync_futures = groups
            .into_iter()
            .map(|group| {
                let active_group_count = Arc::clone(&active_group_count);
                async move {
                    tracing::info!(
                        inbox_id = self.inbox_id(),
                        "[{}] syncing group",
                        self.inbox_id()
                    );
                    tracing::info!(
                        inbox_id = self.inbox_id(),
                        "[{}] syncing group",
                        self.inbox_id()
                    );
                    let is_active = group
                        .load_mls_group_with_lock_async(provider, |mls_group| async move {
                            Ok::<bool, GroupError>(mls_group.is_active())
                        })
                        .await?;
                    if is_active {
                        group.maybe_update_installations(provider, None).await?;

                        group.sync_with_conn(provider).await?;
                        active_group_count.fetch_add(1, Ordering::SeqCst);
                    }

                    Ok::<(), GroupError>(())
                }
            })
            .collect::<FuturesUnordered<_>>();

        sync_futures
            .collect::<Vec<Result<_, _>>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;

        Ok(active_group_count.load(Ordering::SeqCst))
    }

    /// Sync all unread welcome messages and then sync all groups.
    /// Returns the total number of active groups synced.
    pub async fn sync_all_welcomes_and_groups(
        &self,
        provider: &XmtpOpenMlsProvider,
        consent_states: Option<Vec<ConsentState>>,
    ) -> Result<usize, ClientError> {
        self.sync_welcomes(provider).await?;
        let query_args = GroupQueryArgs {
            consent_states,
            include_sync_groups: true,
            include_duplicate_dms: true,
            ..GroupQueryArgs::default()
        };
        let groups = provider
            .conn_ref()
            .find_groups(query_args)?
            .into_iter()
            .map(|g| MlsGroup::new(self.clone(), g.id, g.created_at_ns))
            .collect();
        let active_groups_count = self.sync_all_groups(groups, provider).await?;

        Ok(active_groups_count)
    }

    /**
     * Validates a credential against the given installation public key
     *
     * This will go to the network and get the latest association state for the inbox.
     * It ensures that the installation_pub_key is in that association state
     */
    pub async fn validate_credential_against_network(
        &self,
        conn: &DbConnection,
        credential: &[u8],
        installation_pub_key: Vec<u8>,
    ) -> Result<InboxId, ClientError> {
        let inbox_id = parse_credential(credential)?;
        let association_state = self.get_latest_association_state(conn, &inbox_id).await?;
        let ident = MemberIdentifier::installation(installation_pub_key);

        match association_state.get(&ident) {
            Some(_) => Ok(inbox_id),
            None => Err(IdentityError::InstallationIdNotFound(inbox_id).into()),
        }
    }

    /// Check whether an account_identifier has a key package registered on the network
    ///
    /// Arguments:
    /// - account_identifier: a list of account identifiers to check
    ///
    /// Returns:
    /// A Vec of booleans indicating whether each account address has a key package registered on the network
    pub async fn can_message(
        &self,
        account_identifiers: &[Identifier],
    ) -> Result<HashMap<Identifier, bool>, ClientError> {
        let requests = account_identifiers.iter().map(Into::into).collect();

        // Get the identities that are on the network, set those to true
        let mut can_message: HashMap<Identifier, bool> = self
            .api_client
            .get_inbox_ids(requests)
            .await?
            .into_iter()
            .filter_map(|(ident, _)| Some((ident.try_into().ok()?, true)))
            .collect();

        // Fill in the rest with false
        for ident in account_identifiers {
            if !can_message.contains_key(ident) {
                can_message.insert(ident.clone(), false);
            }
        }

        Ok(can_message)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use super::Client;
    use crate::storage::consent_record::{ConsentType, StoredConsentRecord};
    use crate::subscriptions::StreamMessages;
    use diesel::RunQueryDsl;
    use futures::stream::StreamExt;
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_id::associations::test_utils::WalletTestExt;
    use xmtp_id::scw_verifier::SmartContractSignatureVerifier;

    use crate::groups::DMMetadataOptions;
    use crate::identity::IdentityError;
    use crate::{
        builder::ClientBuilder,
        groups::GroupMetadataOptions,
        hpke::{decrypt_welcome, encrypt_welcome},
        identity::serialize_key_package_hash_ref,
        storage::{
            consent_record::ConsentState, group::GroupQueryArgs, group_message::MsgQueryArgs,
            schema::identity_updates,
        },
        utils::test::HISTORY_SYNC_URL,
        XmtpApi,
    };

    #[xmtp_common::test]
    async fn test_group_member_recovery() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola_wallet = generate_local_wallet();
        // Add two separate installations for Bola
        let bola_a = ClientBuilder::new_test_client(&bola_wallet).await;
        let bola_b = ClientBuilder::new_test_client(&bola_wallet).await;

        let group = amal
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();

        // Add both of Bola's installations to the group
        group
            .add_members_by_inbox_id(&[bola_a.inbox_id(), bola_b.inbox_id()])
            .await
            .unwrap();

        let conn = amal.store().conn().unwrap();
        conn.raw_query_write(|conn| diesel::delete(identity_updates::table).execute(conn))
            .unwrap();

        let members = group.members().await.unwrap();
        // // The three installations should count as two members
        assert_eq!(members.len(), 2);
    }

    #[xmtp_common::test]
    async fn test_mls_error() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let result = client
            .api_client
            .upload_key_package(vec![1, 2, 3], false)
            .await;

        assert!(result.is_err());
        let error_string = result.err().unwrap().to_string();
        assert!(error_string.contains("invalid identity"));
    }

    #[xmtp_common::test]
    async fn test_register_installation() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;
        let client_2 = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        // Make sure the installation is actually on the network
        let association_state = client_2
            .get_latest_association_state(&client_2.store().conn().unwrap(), client.inbox_id())
            .await
            .unwrap();

        assert_eq!(association_state.installation_ids().len(), 1);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(
        not(target_arch = "wasm32"),
        tokio::test(flavor = "multi_thread", worker_threads = 1)
    )]
    async fn test_rotate_key_package() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;

        let installation_public_key = client.installation_public_key().to_vec();
        // Get original KeyPackage.
        let kp1 = client
            .get_key_packages_for_installation_ids(vec![installation_public_key.clone()])
            .await
            .unwrap();
        assert_eq!(kp1.len(), 1);
        let binding = kp1[&installation_public_key].clone().unwrap();
        let init1 = binding.inner.hpke_init_key();

        // Rotate and fetch again.
        client
            .rotate_and_upload_key_package(&client.mls_provider().unwrap())
            .await
            .unwrap();

        let kp2 = client
            .get_key_packages_for_installation_ids(vec![installation_public_key.clone()])
            .await
            .unwrap();
        assert_eq!(kp2.len(), 1);
        let binding = kp2[&installation_public_key].clone().unwrap();
        let init2 = binding.inner.hpke_init_key();

        assert_ne!(init1, init2);
    }

    #[xmtp_common::test]
    async fn test_find_groups() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let group_1 = client
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        let group_2 = client
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();

        let groups = client.find_groups(GroupQueryArgs::default()).unwrap();
        assert_eq!(groups.len(), 2);
        assert!(groups.iter().any(|g| g.group_id == group_1.group_id));
        assert!(groups.iter().any(|g| g.group_id == group_2.group_id));
    }

    #[xmtp_common::test]
    async fn test_find_inbox_id() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;
        assert_eq!(
            client
                .find_inbox_id_from_identifier(&client.store().conn().unwrap(), wallet.identifier())
                .await
                .unwrap(),
            Some(client.inbox_id().to_string())
        );
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(
        not(target_arch = "wasm32"),
        tokio::test(flavor = "multi_thread", worker_threads = 2)
    )]
    async fn double_dms() {
        let alice_wallet = generate_local_wallet();
        let alice = ClientBuilder::new_test_client(&alice_wallet).await;
        let alice_provider = alice.mls_provider().unwrap();

        let bob = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bob_provider = bob.mls_provider().unwrap();

        let alice_dm = alice
            .create_dm_by_inbox_id(bob.inbox_id().to_string(), DMMetadataOptions::default())
            .await
            .unwrap();
        alice_dm.send_message(b"Welcome 1").await.unwrap();

        let bob_dm = bob
            .create_dm_by_inbox_id(alice.inbox_id().to_string(), DMMetadataOptions::default())
            .await
            .unwrap();

        let alice2 = ClientBuilder::new_test_client(&alice_wallet).await;
        let alice2_provider = alice2.mls_provider().unwrap();
        let alice_dm2 = alice
            .create_dm_by_inbox_id(bob.inbox_id().to_string(), DMMetadataOptions::default())
            .await
            .unwrap();
        alice_dm2.send_message(b"Welcome 2").await.unwrap();

        alice_dm.update_installations().await.unwrap();
        alice.sync_welcomes(&alice_provider).await.unwrap();
        bob.sync_welcomes(&bob_provider).await.unwrap();

        alice_dm.send_message(b"Welcome from 1").await.unwrap();

        // This message will set bob's dm as the primary DM for all clients
        bob_dm.send_message(b"Bob says hi 1").await.unwrap();
        // Alice will sync, pulling in Bob's DM message, which will cause
        // a database trigger to update `last_message_ns`, putting bob's DM to the top.
        alice_dm.sync().await.unwrap();

        alice2.sync_welcomes(&alice2_provider).await.unwrap();
        let groups = alice2
            .find_groups(GroupQueryArgs {
                ..Default::default()
            })
            .unwrap();

        assert_eq!(groups.len(), 1);

        groups[0].sync().await.unwrap();
        let messages = groups[0]
            .find_messages(&MsgQueryArgs {
                ..Default::default()
            })
            .unwrap();

        assert_eq!(messages.len(), 3);

        // Reload alice's DM. This will load the DM that Bob just created and sent a message on.
        let new_alice_dm = alice.stitched_group(&alice_dm.group_id).unwrap();

        // The group_id should not be what we asked for because it was stitched
        assert_ne!(alice_dm.group_id, new_alice_dm.group_id);
        // They should be the same, due the the message that Bob sent above.
        assert_eq!(new_alice_dm.group_id, bob_dm.group_id);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(
        not(target_arch = "wasm32"),
        tokio::test(flavor = "multi_thread", worker_threads = 2)
    )]
    async fn test_sync_welcomes() {
        let alice = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bob = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let alice_bob_group = alice
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        alice_bob_group
            .add_members_by_inbox_id(&[bob.inbox_id()])
            .await
            .unwrap();

        let bob_received_groups = bob
            .sync_welcomes(&bob.mls_provider().unwrap())
            .await
            .unwrap();
        assert_eq!(bob_received_groups.len(), 1);
        assert_eq!(
            bob_received_groups.first().unwrap().group_id,
            alice_bob_group.group_id
        );

        let duplicate_received_groups = bob
            .sync_welcomes(&bob.mls_provider().unwrap())
            .await
            .unwrap();
        assert_eq!(duplicate_received_groups.len(), 0);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(
        not(target_arch = "wasm32"),
        tokio::test(flavor = "multi_thread", worker_threads = 2)
    )]
    async fn test_sync_all_groups() {
        let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let alix_bo_group1 = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        let alix_bo_group2 = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        alix_bo_group1
            .add_members_by_inbox_id(&[bo.inbox_id()])
            .await
            .unwrap();
        alix_bo_group2
            .add_members_by_inbox_id(&[bo.inbox_id()])
            .await
            .unwrap();

        let bob_received_groups = bo.sync_welcomes(&bo.mls_provider().unwrap()).await.unwrap();
        assert_eq!(bob_received_groups.len(), 2);

        let bo_groups = bo.find_groups(GroupQueryArgs::default()).unwrap();
        let bo_group1 = bo.group(alix_bo_group1.clone().group_id).unwrap();
        let bo_messages1 = bo_group1.find_messages(&MsgQueryArgs::default()).unwrap();
        assert_eq!(bo_messages1.len(), 0);
        let bo_group2 = bo.group(alix_bo_group2.clone().group_id).unwrap();
        let bo_messages2 = bo_group2.find_messages(&MsgQueryArgs::default()).unwrap();
        assert_eq!(bo_messages2.len(), 0);
        alix_bo_group1
            .send_message(vec![1, 2, 3].as_slice())
            .await
            .unwrap();
        alix_bo_group2
            .send_message(vec![1, 2, 3].as_slice())
            .await
            .unwrap();

        bo.sync_all_groups(bo_groups, &bo.mls_provider().unwrap())
            .await
            .unwrap();

        let bo_messages1 = bo_group1.find_messages(&MsgQueryArgs::default()).unwrap();
        assert_eq!(bo_messages1.len(), 1);
        let bo_group2 = bo.group(alix_bo_group2.clone().group_id).unwrap();
        let bo_messages2 = bo_group2.find_messages(&MsgQueryArgs::default()).unwrap();
        assert_eq!(bo_messages2.len(), 1);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(
        not(target_arch = "wasm32"),
        tokio::test(flavor = "multi_thread", worker_threads = 2)
    )]
    async fn test_sync_all_groups_and_welcomes() {
        let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        // Create two groups and add Bob
        let alix_bo_group1 = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        let alix_bo_group2 = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();

        alix_bo_group1
            .add_members_by_inbox_id(&[bo.inbox_id()])
            .await
            .unwrap();
        alix_bo_group2
            .add_members_by_inbox_id(&[bo.inbox_id()])
            .await
            .unwrap();

        // Initial sync (None): Bob should fetch both groups
        let bob_received_groups = bo
            .sync_all_welcomes_and_groups(&bo.mls_provider().unwrap(), None)
            .await
            .unwrap();
        assert_eq!(bob_received_groups, 2);

        // Verify Bob initially has no messages
        let bo_group1 = bo.group(alix_bo_group1.group_id.clone()).unwrap();
        assert_eq!(
            bo_group1
                .find_messages(&MsgQueryArgs::default())
                .unwrap()
                .len(),
            0
        );
        let bo_group2 = bo.group(alix_bo_group2.group_id.clone()).unwrap();
        assert_eq!(
            bo_group2
                .find_messages(&MsgQueryArgs::default())
                .unwrap()
                .len(),
            0
        );

        // Alix sends a message to both groups
        alix_bo_group1
            .send_message(vec![1, 2, 3].as_slice())
            .await
            .unwrap();
        alix_bo_group2
            .send_message(vec![4, 5, 6].as_slice())
            .await
            .unwrap();

        // Sync with `Unknown`: Bob should not fetch new messages
        let bob_received_groups_unknown = bo
            .sync_all_welcomes_and_groups(
                &bo.mls_provider().unwrap(),
                Some([ConsentState::Allowed].to_vec()),
            )
            .await
            .unwrap();
        assert_eq!(bob_received_groups_unknown, 0);

        // Verify Bob still has no messages
        assert_eq!(
            bo_group1
                .find_messages(&MsgQueryArgs::default())
                .unwrap()
                .len(),
            0
        );
        assert_eq!(
            bo_group2
                .find_messages(&MsgQueryArgs::default())
                .unwrap()
                .len(),
            0
        );

        // Alix sends another message to both groups
        alix_bo_group1
            .send_message(vec![7, 8, 9].as_slice())
            .await
            .unwrap();
        alix_bo_group2
            .send_message(vec![10, 11, 12].as_slice())
            .await
            .unwrap();

        // Sync with `None`: Bob should fetch all messages
        let bob_received_groups_all = bo
            .sync_all_welcomes_and_groups(
                &bo.mls_provider().unwrap(),
                Some([ConsentState::Unknown].to_vec()),
            )
            .await
            .unwrap();
        assert_eq!(bob_received_groups_all, 2);

        // Verify Bob now has all messages
        let bo_messages1 = bo_group1.find_messages(&MsgQueryArgs::default()).unwrap();
        assert_eq!(bo_messages1.len(), 2);

        let bo_messages2 = bo_group2.find_messages(&MsgQueryArgs::default()).unwrap();
        assert_eq!(bo_messages2.len(), 2);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(
        not(target_arch = "wasm32"),
        tokio::test(flavor = "multi_thread", worker_threads = 1)
    )]
    async fn test_welcome_encryption() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let provider = client.mls_provider().unwrap();

        let kp = client.identity().new_key_package(&provider).unwrap();
        let hpke_public_key = kp.hpke_init_key().as_slice();
        let to_encrypt = vec![1, 2, 3];

        // Encryption doesn't require any details about the sender, so we can test using one client
        let encrypted = encrypt_welcome(to_encrypt.as_slice(), hpke_public_key).unwrap();

        let decrypted = decrypt_welcome(&provider, hpke_public_key, encrypted.as_slice()).unwrap();

        assert_eq!(decrypted, to_encrypt);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(
        not(target_arch = "wasm32"),
        tokio::test(flavor = "multi_thread", worker_threads = 1)
    )]
    async fn test_add_remove_then_add_again() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        // Create a group and invite bola
        let amal_group = amal
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        amal_group
            .add_members_by_inbox_id(&[bola.inbox_id()])
            .await
            .unwrap();
        assert_eq!(amal_group.members().await.unwrap().len(), 2);

        // Now remove bola
        amal_group
            .remove_members_by_inbox_id(&[bola.inbox_id()])
            .await
            .unwrap();
        assert_eq!(amal_group.members().await.unwrap().len(), 1);

        // See if Bola can see that they were added to the group
        bola.sync_welcomes(&bola.mls_provider().unwrap())
            .await
            .unwrap();
        let bola_groups = bola.find_groups(Default::default()).unwrap();
        assert_eq!(bola_groups.len(), 1);
        let bola_group = bola_groups.first().unwrap();
        bola_group.sync().await.unwrap();
        // TODO: figure out why Bola's status is not updating to be inactive
        // assert!(!bola_group.is_active().unwrap());

        // Bola should have one readable message (them being added to the group)
        let mut bola_messages = bola_group.find_messages(&MsgQueryArgs::default()).unwrap();

        assert_eq!(bola_messages.len(), 1);

        // Add Bola back to the group
        amal_group
            .add_members_by_inbox_id(&[bola.inbox_id()])
            .await
            .unwrap();
        bola.sync_welcomes(&bola.mls_provider().unwrap())
            .await
            .unwrap();

        // Send a message from Amal, now that Bola is back in the group
        amal_group
            .send_message(vec![1, 2, 3].as_slice())
            .await
            .unwrap();

        // Sync Bola's state to get the latest
        bola_group.sync().await.unwrap();
        // Find Bola's updated list of messages
        bola_messages = bola_group.find_messages(&MsgQueryArgs::default()).unwrap();
        // Bola should have been able to decrypt the last message
        assert_eq!(bola_messages.len(), 2);
        assert_eq!(
            bola_messages.get(1).unwrap().decrypted_message_bytes,
            vec![1, 2, 3]
        )
    }

    async fn get_key_package_init_key<
        ApiClient: XmtpApi,
        Verifier: SmartContractSignatureVerifier,
        Id: AsRef<[u8]>,
    >(
        client: &Client<ApiClient, Verifier>,
        installation_id: Id,
    ) -> Result<Vec<u8>, IdentityError> {
        let kps_map = client
            .get_key_packages_for_installation_ids(vec![installation_id.as_ref().to_vec()])
            .await
            .map_err(|_| IdentityError::NewIdentity("Failed to fetch key packages".to_string()))?;

        let kp_result = kps_map
            .get(installation_id.as_ref())
            .ok_or_else(|| {
                IdentityError::NewIdentity(format!(
                    "Missing key package for {}",
                    hex::encode(installation_id.as_ref())
                ))
            })?
            .clone()?;

        serialize_key_package_hash_ref(&kp_result.inner, &client.mls_provider()?)
    }

    #[xmtp_common::test]
    async fn test_key_package_rotation() {
        let alix_wallet = generate_local_wallet();
        let bo_wallet = generate_local_wallet();
        let alix = ClientBuilder::new_test_client(&alix_wallet).await;
        let bo = ClientBuilder::new_test_client(&bo_wallet).await;
        let bo_store = bo.store();

        let alix_original_init_key =
            get_key_package_init_key(&alix, alix.installation_public_key())
                .await
                .unwrap();
        let bo_original_init_key = get_key_package_init_key(&bo, bo.installation_public_key())
            .await
            .unwrap();

        // Bo's original key should be deleted
        let bo_original_from_db = bo_store
            .conn()
            .unwrap()
            .find_key_package_history_entry_by_hash_ref(bo_original_init_key.clone());
        assert!(bo_original_from_db.is_ok());

        alix.create_group_with_members(
            &[bo_wallet.identifier()],
            None,
            GroupMetadataOptions::default(),
        )
        .await
        .unwrap();

        bo.sync_welcomes(&bo.mls_provider().unwrap()).await.unwrap();

        let bo_new_key = get_key_package_init_key(&bo, bo.installation_public_key())
            .await
            .unwrap();
        // Bo's key should have changed
        assert_ne!(bo_original_init_key, bo_new_key);

        bo.sync_welcomes(&bo.mls_provider().unwrap()).await.unwrap();
        let bo_new_key_2 = get_key_package_init_key(&bo, bo.installation_public_key())
            .await
            .unwrap();
        // Bo's key should not have changed syncing the second time.
        assert_eq!(bo_new_key, bo_new_key_2);

        alix.sync_welcomes(&alix.mls_provider().unwrap())
            .await
            .unwrap();
        let alix_key_2 = get_key_package_init_key(&alix, alix.installation_public_key())
            .await
            .unwrap();
        // Alix's key should not have changed at all
        assert_eq!(alix_original_init_key, alix_key_2);

        alix.create_group_with_members(
            &[bo_wallet.identifier()],
            None,
            GroupMetadataOptions::default(),
        )
        .await
        .unwrap();
        bo.sync_welcomes(&bo.mls_provider().unwrap()).await.unwrap();

        // Bo should have two groups now
        let bo_groups = bo.find_groups(GroupQueryArgs::default()).unwrap();
        assert_eq!(bo_groups.len(), 2);

        // Bo's original key should be deleted
        let bo_original_after_delete = bo_store
            .conn()
            .unwrap()
            .find_key_package_history_entry_by_hash_ref(bo_original_init_key);
        assert!(bo_original_after_delete.is_err());
    }

    #[xmtp_common::test]
    async fn test_find_or_create_dm_by_inbox_id() {
        let user1 = generate_local_wallet();
        let user2 = generate_local_wallet();
        let client1 = ClientBuilder::new_test_client(&user1).await;
        let client2 = ClientBuilder::new_test_client(&user2).await;

        // First call should create a new DM
        let dm1 = client1
            .find_or_create_dm_by_inbox_id(
                client2.inbox_id().to_string(),
                DMMetadataOptions::default(),
            )
            .await
            .unwrap();

        // Verify DM was created with correct properties
        let metadata = dm1
            .metadata(&client1.mls_provider().unwrap())
            .await
            .unwrap();
        assert_eq!(
            metadata.dm_members.clone().unwrap().member_one_inbox_id,
            client1.inbox_id()
        );
        assert_eq!(
            metadata.dm_members.unwrap().member_two_inbox_id,
            client2.inbox_id()
        );

        // Second call should find the existing DM
        let dm2 = client1
            .find_or_create_dm_by_inbox_id(
                client2.inbox_id().to_string(),
                DMMetadataOptions::default(),
            )
            .await
            .unwrap();

        // Verify we got back the same DM
        assert_eq!(dm1.group_id, dm2.group_id);
        assert_eq!(dm1.created_at_ns, dm2.created_at_ns);

        // Verify the DM appears in conversations list
        let conversations = client1.find_groups(GroupQueryArgs::default()).unwrap();
        assert_eq!(conversations.len(), 1);
        assert_eq!(conversations[0].group_id, dm1.group_id);
    }

    #[xmtp_common::test]
    async fn should_stream_consent() {
        let alix_wallet = generate_local_wallet();
        let bo_wallet = generate_local_wallet();
        let alix =
            ClientBuilder::new_test_client_with_history(&alix_wallet, HISTORY_SYNC_URL).await;
        let bo = ClientBuilder::new_test_client_with_history(&bo_wallet, HISTORY_SYNC_URL).await;

        let group = alix
            .create_group_with_inbox_ids(
                &[bo.inbox_id().to_string()],
                None,
                GroupMetadataOptions::default(),
            )
            .await
            .unwrap();

        let receiver = alix.local_events.subscribe();
        let stream = receiver.stream_consent_updates();
        futures::pin_mut!(stream);

        // first record is denied consent to the group.
        group.update_consent_state(ConsentState::Denied).unwrap();

        xmtp_common::time::sleep(std::time::Duration::from_millis(1000)).await;

        // second is allowing consent for the group
        alix.set_consent_states(&[StoredConsentRecord {
            entity: hex::encode(&group.group_id),
            state: ConsentState::Allowed,
            entity_type: ConsentType::ConversationId,
        }])
        .await
        .unwrap();

        xmtp_common::time::sleep(std::time::Duration::from_millis(1000)).await;

        // third denying consent for bo address, and allowing consent for bo inbox id
        alix.set_consent_states(&[StoredConsentRecord {
            entity: bo.inbox_id().to_string(),
            entity_type: ConsentType::InboxId,
            state: ConsentState::Allowed,
        }])
        .await
        .unwrap();

        let item = stream.next().await.unwrap().unwrap();
        assert_eq!(item.len(), 1);
        assert_eq!(item[0].entity_type, ConsentType::ConversationId);
        assert_eq!(item[0].entity, hex::encode(&group.group_id));
        assert_eq!(item[0].state, ConsentState::Denied);
        let item = stream.next().await.unwrap().unwrap();
        assert_eq!(item.len(), 1);
        assert_eq!(item[0].entity_type, ConsentType::ConversationId);
        assert_eq!(item[0].entity, hex::encode(group.group_id));
        assert_eq!(item[0].state, ConsentState::Allowed);
        let item = stream.next().await.unwrap().unwrap();
        assert_eq!(item.len(), 1);
        assert_eq!(item[0].entity_type, ConsentType::InboxId);
        assert_eq!(item[0].entity, bo.inbox_id());
        assert_eq!(item[0].state, ConsentState::Allowed);
    }
}
