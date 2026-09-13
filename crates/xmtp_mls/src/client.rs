use crate::{
    builder::DeviceSyncMode,
    context::XmtpSharedContext,
    groups::{
        ConversationListItem, GroupError, MlsGroup, group_permissions::PolicySet,
        welcome_sync::WelcomeService,
    },
    identity::{Identity, IdentityError, parse_credential},
    identity_updates::{IdentityUpdateError, IdentityUpdates, load_identity_updates},
    mls_store::{MlsStore, MlsStoreError},
    subscriptions::{LocalEventError, LocalEvents, SyncWorkerEvent},
    utils::VersionInfo,
    worker::device_sync::{
        DeviceSyncClient, preference_sync::PreferenceUpdate, worker::SyncMetric,
    },
    worker::{WorkerRunner, metrics::WorkerMetrics},
};
use crate::{
    groups::welcome_sync::GroupSyncSummary,
    identity_updates::{batch_get_association_state_with_verifier, get_creation_signature_kind},
    messages::{
        decoded_message::DecodedMessage,
        enrichment::{EnrichMessageError, enrich_messages},
    },
};
use itertools::Itertools;
use openmls::prelude::tls_codec::Error as TlsCodecError;
use std::{collections::HashMap, sync::Arc};
use thiserror::Error;
use tokio::sync::broadcast;
use xmtp_api::{ApiClientWrapper, XmtpApi};
use xmtp_common::{ErrorCode, Event, Retry, retry_async, retryable};
use xmtp_configuration::{CREATE_PQ_KEY_PACKAGE_EXTENSION, KEY_PACKAGE_ROTATION_INTERVAL_NS};
use xmtp_cryptography::signature::IdentifierValidationError;
use xmtp_db::TransactionOutcome::Continue;
use xmtp_db::{
    ConnectionExt, NotFound, StorageError, TransactionOutcome, XmtpDb,
    consent_record::{ConsentState, ConsentType, StoredConsentRecord},
    db_connection::DbConnection,
    encrypted_store::conversation_list::ConversationListItem as DbConversationListItem,
    group::{ConversationType, GroupMembershipState, GroupQueryArgs},
    group_message::StoredGroupMessage,
    identity::StoredIdentity,
    identity_cache::StoredIdentityKind,
};
use xmtp_db::{group::GroupQueryOrderBy, prelude::*};
use xmtp_id::key_package::{KeyPackageVerificationError, VerifiedKeyPackageV2};
use xmtp_id::{
    AsIdRef, InboxId, InboxIdRef,
    associations::{
        AssociationError, AssociationState, Identifier, MemberIdentifier, SignatureError,
        builder::{SignatureRequest, SignatureRequestError},
    },
    scw_verifier::SmartContractSignatureVerifier,
};
use xmtp_macro::log_event;
use xmtp_mls_common::{
    group::{DMMetadataOptions, GroupMetadataOptions},
    group_metadata::DmMembers,
    group_mutable_metadata::MessageDisappearingSettings,
};
use xmtp_proto::{
    ConversionError,
    api::HasStats,
    api_client::{ApiStats, IdentityStats},
};
use xmtp_proto::{types::InstallationId, xmtp::identity::associations::IdentifierKind};

use xmtp_proto::types::GroupId;
/// Enum representing the network the Client is connected to
#[derive(Clone, Copy, Default, Debug)]
pub enum Network {
    Local(&'static str),
    #[default]
    Dev,
    Prod,
}

/// Timeout for waiting until a registration publish can be read.
#[derive(Debug, Clone)]
pub struct VisibilityConfirmationOptions {
    pub timeout_ms: u64,
}

const REGISTRATION_INITIAL_BACKOFF: std::time::Duration = std::time::Duration::from_millis(50);
const REGISTRATION_MAX_BACKOFF: std::time::Duration = std::time::Duration::from_secs(1);

impl Default for VisibilityConfirmationOptions {
    fn default() -> Self {
        Self { timeout_ms: 30_000 }
    }
}

#[derive(Debug, Error, ErrorCode)]
pub enum ClientError {
    #[error(transparent)]
    #[error_code(inherit)]
    AddressValidation(#[from] IdentifierValidationError),
    /// Could not publish.
    ///
    /// Failed to publish messages to the network. May be retryable.
    #[error("could not publish: {0}")]
    PublishError(String),
    /// Storage error.
    ///
    /// Database operation failed. May be retryable.
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    /// API error.
    ///
    /// Network request to XMTP backend failed. Retryable.
    #[error("API error: {0}")]
    Api(#[from] xmtp_api::ApiError),
    /// Identity error.
    ///
    /// Problem with identity operations. Not retryable.
    #[error("identity error: {0}")]
    Identity(#[from] crate::identity::IdentityError),
    /// TLS Codec error.
    ///
    /// Encoding/decoding MLS TLS structures failed. Not retryable.
    #[error("TLS Codec error: {0}")]
    TlsError(#[from] TlsCodecError),
    /// Key package verification failed.
    ///
    /// Invalid key package received from network. Not retryable.
    #[error("key package verification: {0}")]
    KeyPackageVerification(#[from] KeyPackageVerificationError),
    /// Stream inconsistency.
    ///
    /// Message stream state became inconsistent. Not retryable.
    #[error("Stream inconsistency error: {0}")]
    StreamInconsistency(String),
    /// Association error.
    ///
    /// Identity association operation failed. Not retryable.
    #[error("Association error: {0}")]
    Association(#[from] AssociationError),
    /// Signature validation error.
    ///
    /// A signature failed verification. Not retryable.
    #[error("signature validation error: {0}")]
    SignatureValidation(#[from] SignatureError),
    /// Identity update error.
    ///
    /// Failed to process identity update. Not retryable.
    #[error(transparent)]
    IdentityUpdate(#[from] IdentityUpdateError),
    /// Signature request error.
    ///
    /// Failed to create/process signature request. Not retryable.
    #[error(transparent)]
    SignatureRequest(#[from] SignatureRequestError),
    /// Group error.
    ///
    /// Group operation failed. May be retryable.
    // the box is to prevent infinite cycle between client and group errors
    #[error(transparent)]
    Group(Box<GroupError>),
    /// Local event error.
    ///
    /// Failed to process local event. Not retryable.
    #[error(transparent)]
    LocalEvent(#[from] LocalEventError),
    /// Database connection error.
    ///
    /// Connection to database failed. Retryable.
    #[error(transparent)]
    Db(#[from] xmtp_db::ConnectionError),
    /// Generic error.
    ///
    /// Unclassified error. May be retryable.
    #[error("generic:{0}")]
    Generic(String),
    /// MLS store error.
    ///
    /// OpenMLS key store operation failed. Not retryable.
    #[error(transparent)]
    MlsStore(#[from] MlsStoreError),
    /// Message enrichment error.
    ///
    /// Failed to enrich message content. Not retryable.
    #[error(transparent)]
    EnrichMessage(#[from] EnrichMessageError),
    /// Conversion Error
    ///
    /// Data type failed to convert. Not retryable.
    #[error(transparent)]
    Conversion(#[from] xmtp_proto::ConversionError),
    /// Registration not visible.
    ///
    /// Registration has no publish cursor or is not visible before the timeout. Not retryable.
    #[error("Registration is not visible")]
    RegistrationNotVisible,
    /// Client is closed.
    ///
    /// Operation was attempted on a client that has been shut down via
    /// `Client::close`. Not retryable — build a new client instead.
    #[error("client is closed")]
    AlreadyClosed,
}

impl ClientError {
    pub fn db_needs_connection(&self) -> bool {
        match self {
            Self::Storage(s) => s.db_needs_connection(),
            Self::Db(c) => c.db_needs_connection(),
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
            ClientError::Api(api_error) => retryable!(api_error),
            ClientError::Storage(storage_error) => retryable!(storage_error),
            ClientError::Db(db) => retryable!(db),
            // SCW verification errors carry retryability through SignatureError;
            // transient RPC provider failures must not advance the welcome cursor.
            // See xmtp/libxmtp#3394.
            ClientError::SignatureValidation(e) => retryable!(e),
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
pub struct Client<Context> {
    pub context: Context,
    pub installation_id: InstallationId,
    pub(crate) local_events: broadcast::Sender<LocalEvents>,
    pub(crate) workers: Arc<WorkerRunner>,
}

impl<Context> Drop for Client<Context> {
    fn drop(&mut self) {
        log_event!(Event::ClientDropped, self.installation_id);
    }
}

#[derive(Clone)]
pub struct DeviceSync {
    pub(crate) mode: DeviceSyncMode,
}

// most of these things are `Arc`'s
impl<Context: Clone> Clone for Client<Context> {
    fn clone(&self) -> Self {
        Self {
            context: self.context.clone(),
            installation_id: self.installation_id,
            local_events: self.local_events.clone(),
            workers: self.workers.clone(),
        }
    }
}

impl<Context> Client<Context>
where
    Context: XmtpSharedContext,
{
    pub fn identity_updates(&self) -> IdentityUpdates<&Context> {
        IdentityUpdates::new(&self.context)
    }

    pub fn mls_store(&self) -> MlsStore<Context> {
        MlsStore::new(self.context.clone())
    }

    pub fn scw_verifier(&self) -> Arc<Box<dyn SmartContractSignatureVerifier>> {
        self.context.scw_verifier()
    }

    pub fn version_info(&self) -> &VersionInfo {
        self.context.version_info()
    }
}

impl<Context> Client<Context>
where
    Context: XmtpSharedContext,
    Context::ApiClient: HasStats,
{
    pub fn api_stats(&self) -> ApiStats {
        self.context.api().api_client.mls_stats()
    }

    pub fn identity_api_stats(&self) -> IdentityStats {
        self.context.api().api_client.identity_stats()
    }

    pub fn clear_stats(&self) {
        self.context.api().api_client.mls_stats().clear();
        self.context.api().api_client.identity_stats().clear();
    }
}

/// Get the [`AssociationState`] for each `inbox_id`
pub async fn inbox_addresses_with_verifier<ApiClient: XmtpApi>(
    api_client: &ApiClientWrapper<ApiClient>,
    conn: &impl DbQuery,
    inbox_ids: Vec<InboxIdRef<'_>>,
    scw_verifier: &impl SmartContractSignatureVerifier,
) -> Result<Vec<AssociationState>, ClientError> {
    load_identity_updates(api_client, conn, &inbox_ids).await?;
    let state = batch_get_association_state_with_verifier(
        conn,
        &inbox_ids.into_iter().map(|i| (i, None)).collect::<Vec<_>>(),
        scw_verifier,
    )
    .await?;
    Ok(state)
}

impl<Context> Client<Context>
where
    Context: XmtpSharedContext + 'static,
{
    /// Reconnect to the client's database if it has previously been released
    pub fn reconnect_db(&self) -> Result<(), ClientError> {
        if self.context.is_closed() {
            return Err(ClientError::AlreadyClosed);
        }
        self.context.db().reconnect().map_err(StorageError::from)?;
        self.workers.spawn(self.context.clone());
        Ok(())
    }

    /// Cleanly shut down this client: cancel in-flight workers and streams,
    /// then release the DB connection. Idempotent — a second call is `Ok(())`.
    ///
    /// Callers (notably the Node binding consumers) should `await` this before
    /// deleting the SQLite file or dropping the client wrapper, to avoid late
    /// log spew from detached workers/streams firing against a dead DB.
    pub async fn close(&self) -> Result<(), ClientError> {
        // `shutdown_complete` is distinct from `is_closed()` (which reflects
        // cancellation): only set after the DB actually disconnects. If
        // `disconnect()` errors below, callers can retry `close()` and we'll
        // attempt disconnect again rather than silently short-circuiting.
        if self.context.shutdown_complete() {
            return Ok(());
        }
        self.context.cancellation_token().cancel();
        self.context.close_message_delivery()?;
        self.workers.shutdown().await;
        self.context
            .db()
            .disconnect()
            .map_err(xmtp_db::StorageError::from)?;
        self.context.mark_shutdown_complete();
        log_event!(Event::ClientClosed, self.installation_id);
        Ok(())
    }

    /// yields until the sync worker notifies that it is initialized and running.
    pub async fn wait_for_sync_worker_init(&self) {
        self.workers.wait_for_sync_worker_init().await;
    }

    pub fn sync_metrics(&self) -> Option<Arc<WorkerMetrics<SyncMetric>>> {
        self.workers.sync_metrics()
    }
}

impl<Context> Client<Context>
where
    Context: XmtpSharedContext,
{
    /// Retrieves the client's installation public key, sometimes also called `installation_id`
    pub fn installation_public_key(&self) -> InstallationId {
        self.context.installation_id()
    }
    /// Retrieves the client's inbox ID
    pub fn inbox_id(&self) -> InboxIdRef<'_> {
        self.context.identity().inbox_id()
    }

    /// get a reference to the monolithic Database object where
    /// higher-level queries are defined
    pub fn db(&self) -> <Context::Db as XmtpDb>::DbQuery {
        self.context.db()
    }

    /// This associates an installation_id with a human-readable
    /// name and makes the logs a little easier to read.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn set_name(&self, name: &str) {
        log_event!(Event::AssociateName, self.context.installation_id(), name);
    }

    pub fn device_sync_client(&self) -> DeviceSyncClient<Context> {
        let metrics = self.context.sync_metrics();
        DeviceSyncClient::new(
            self.context.clone(),
            metrics.unwrap_or(Arc::new(WorkerMetrics::new(self.context.installation_id()))),
        )
    }

    /// Calls the server to look up the `inbox_id` associated with a given identifier
    pub async fn find_inbox_id_from_identifier(
        &self,
        conn: &impl DbQuery,
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
        conn: &impl DbQuery,
        identifiers: &[Identifier],
    ) -> Result<Vec<Option<String>>, ClientError> {
        let ids: Vec<(String, StoredIdentityKind)> = identifiers
            .iter()
            .map(|i| {
                Ok::<_, ConversionError>((
                    i.clone().to_string(),
                    StoredIdentityKind::try_from(IdentifierKind::from(i))?,
                ))
            })
            .try_collect()?;
        let cached_inbox_ids = conn.fetch_cached_inbox_ids(&ids)?;
        let mut new_inbox_ids: HashMap<&Identifier, Option<String>> = HashMap::new();

        let missing: Vec<_> = identifiers
            .iter()
            .filter(|ident| !cached_inbox_ids.contains_key(&format!("{ident}")))
            .collect();

        if !missing.is_empty() {
            let requests = missing
                .iter()
                .map(|identifier| (*identifier).into())
                .collect();
            let results = self.context.api().get_inbox_ids(requests).await?;
            new_inbox_ids = missing.into_iter().zip(results).collect();
        }

        let inbox_ids = identifiers
            .iter()
            .map(|ident| {
                let cache_key = format!("{ident}");
                if let Some(inbox_id) = cached_inbox_ids.get(&cache_key) {
                    return Some(inbox_id.clone());
                }
                new_inbox_ids.get(ident).cloned().flatten()
            })
            .collect();
        Ok(inbox_ids)
    }

    /// Get the highest `sequence_id` from the local database for the client's `inbox_id`.
    /// This may not be consistent with the latest state on the backend.
    pub fn inbox_sequence_id(
        &self,
        conn: &DbConnection<<Context::Db as XmtpDb>::Connection>,
    ) -> Result<i64, StorageError> {
        self.context
            .identity()
            .sequence_id(conn)
            .map_err(Into::into)
    }

    /// Get the [`AssociationState`] for the client's `inbox_id`
    pub async fn inbox_state(
        &self,
        refresh_from_network: bool,
    ) -> Result<AssociationState, ClientError> {
        let conn = self.context.db();
        let inbox_id = self.inbox_id();
        if refresh_from_network {
            load_identity_updates(self.context.api(), &conn, &[inbox_id]).await?;
        }
        let identity_service = IdentityUpdates::new(&self.context);
        let state = identity_service
            .get_association_state(&conn, inbox_id, None)
            .await?;
        Ok(state)
    }

    /// Get the [`AssociationState`] for each `inbox_id`
    pub async fn inbox_addresses(
        &self,
        refresh_from_network: bool,
        inbox_ids: Vec<InboxIdRef<'_>>,
    ) -> Result<Vec<AssociationState>, ClientError> {
        let conn = self.context.db();
        if refresh_from_network {
            load_identity_updates(self.context.api(), &conn, &inbox_ids).await?;
        }
        let identity_service = IdentityUpdates::new(&self.context);
        let state = identity_service
            .batch_get_association_state(
                &conn,
                &inbox_ids.into_iter().map(|i| (i, None)).collect::<Vec<_>>(),
            )
            .await?;
        Ok(state)
    }

    /// Get the total number of inbox updates for `inbox_ids`. `refresh_from_network` will force
    /// a network refresh. May still access network if an inbox_id does not yet exist in the local
    /// cache.
    pub async fn fetch_inbox_updates_count(
        &self,
        refresh_from_network: bool,
        inbox_ids: Vec<InboxIdRef<'_>>,
    ) -> Result<HashMap<InboxId, u32>, ClientError> {
        let conn = self.context.db();
        if refresh_from_network {
            load_identity_updates(self.context.api(), &conn, &inbox_ids).await?;
        }
        let inbox_id_strs = inbox_ids.to_vec();
        let counts = conn.count_inbox_updates(&inbox_id_strs)?;
        Ok(counts.into_iter().map(|(k, v)| (k, v as u32)).collect())
    }

    /// Get the total number of inbox updates for the client's inbox_id.
    /// Setting `refresh_from_network` forces a network refresh, otherwise
    /// this operation is offline.
    pub async fn fetch_own_inbox_updates_count(
        &self,
        refresh_from_network: bool,
    ) -> Result<u32, ClientError> {
        let inbox_id = self.inbox_id();
        Ok(self
            .fetch_inbox_updates_count(refresh_from_network, vec![inbox_id])
            .await?
            .get(inbox_id)
            .copied()
            .unwrap_or(0))
    }

    /// Get the signature kind used to create an inbox.
    ///
    /// # Arguments
    /// * `inbox_id` - The inbox ID to check
    /// * `refresh_from_network` - Whether to fetch updates from the network first
    ///
    /// # Returns
    /// * `Some(SignatureKind)` - The signature kind used to create the inbox
    /// * `None` - Inbox doesn't exist or creation info is unavailable
    pub async fn inbox_creation_signature_kind(
        &self,
        inbox_id: InboxIdRef<'_>,
        refresh_from_network: bool,
    ) -> Result<Option<xmtp_id::associations::SignatureKind>, ClientError> {
        let conn = self.context.db();

        // Load the first identity update (creation update) for this inbox if requested
        if refresh_from_network {
            load_identity_updates(self.context.api(), &conn, &[inbox_id]).await?;
        }

        let verifier = self.context.scw_verifier();

        let signature_kind = get_creation_signature_kind(&conn, verifier, inbox_id).await?;

        Ok(signature_kind)
    }

    /// Set a consent record in the local database.
    /// If the consent record is an address set the consent state for both the address and `inbox_id`
    pub async fn set_consent_states(
        &self,
        records: &[StoredConsentRecord],
    ) -> Result<(), ClientError> {
        let conn = self.context.db();
        let changed_records = conn.insert_or_replace_consent_records(records)?;

        if !changed_records.is_empty() {
            let updates: Vec<_> = changed_records
                .into_iter()
                .map(PreferenceUpdate::Consent)
                .collect();

            // Broadcast the consent update changes
            let _ = self
                .local_events
                .send(LocalEvents::PreferencesChanged(updates.clone()));
            let _ = self
                .context
                .worker_events()
                .send(SyncWorkerEvent::SyncPreferences(updates));
        }

        Ok(())
    }

    /// Get the consent state for a given entity
    pub async fn get_consent_state(
        &self,
        entity_type: ConsentType,
        entity: String,
    ) -> Result<ConsentState, ClientError> {
        let conn = self.context.db();
        let record = conn.get_consent_record(entity, entity_type)?;

        match record {
            Some(rec) => Ok(rec.state),
            None => Ok(ConsentState::Unknown),
        }
    }

    /// Release the client's database connection
    pub fn release_db_connection(&self) -> Result<(), ClientError> {
        self.context
            .db()
            .disconnect()
            .map_err(xmtp_db::StorageError::from)?;
        Ok(())
    }

    /// Get a reference to the client's identity struct
    pub fn identity(&self) -> &Identity {
        self.context.identity()
    }

    /// Ensures identity is ready before performing operations.
    /// Call `register_identity()` first if this fails.
    fn ensure_identity_ready(&self) -> Result<(), ClientError> {
        if !self.identity().is_ready() {
            tracing::warn!(
                inbox_id = %self.inbox_id(),
                "Operation attempted before register_identity() was called"
            );
            return Err(IdentityError::UninitializedIdentity.into());
        }
        Ok(())
    }

    /// Create a new group with the default settings
    /// Applies a custom [`PolicySet`] to the group if one is specified
    pub fn create_group(
        &self,
        permissions_policy_set: Option<PolicySet>,
        opts: Option<GroupMetadataOptions>,
    ) -> Result<MlsGroup<Context>, ClientError> {
        self.ensure_identity_ready()?;

        let group: MlsGroup<Context> = MlsGroup::create_and_insert(
            self.context.clone(),
            ConversationType::Group,
            permissions_policy_set.unwrap_or_default(),
            opts.unwrap_or_default(),
            None,
        )?;

        log_event!(
            Event::CreatedGroup,
            self.context.installation_id(),
            group_id = group.group_id
        );

        // notify streams of our new group
        let _ = self
            .local_events
            .send(LocalEvents::NewGroup(group.group_id));

        Ok(group)
    }

    /// Create a group with an initial set of members added
    pub async fn create_group_with_identifiers(
        &self,
        account_identifiers: &[Identifier],
        permissions_policy_set: Option<PolicySet>,
        opts: Option<GroupMetadataOptions>,
    ) -> Result<MlsGroup<Context>, ClientError> {
        let group = self.create_group(permissions_policy_set, opts)?;

        group.add_members_by_identity(account_identifiers).await?;

        Ok(group)
    }

    #[tracing::instrument(level = "debug", skip_all, fields(size = inbox_ids.len()))]
    pub async fn create_group_with_members(
        &self,
        inbox_ids: &[impl AsIdRef],
        permissions_policy_set: Option<PolicySet>,
        opts: Option<GroupMetadataOptions>,
    ) -> Result<MlsGroup<Context>, ClientError> {
        tracing::info!("creating group");
        let group = self.create_group(permissions_policy_set, opts)?;

        group.add_members(inbox_ids).await?;

        Ok(group)
    }

    /// Create a new Direct Message with the default settings
    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_dm_by_inbox_id(
        &self,
        target_inbox_id: InboxId,
        opts: Option<DMMetadataOptions>,
    ) -> Result<MlsGroup<Context>, ClientError> {
        let group: MlsGroup<Context> = MlsGroup::create_dm_and_insert(
            &self.context,
            GroupMembershipState::Allowed,
            target_inbox_id.clone(),
            opts.unwrap_or_default(),
            None,
        )?;

        log_event!(
            Event::CreatedDM,
            self.context.installation_id(),
            group_id = group.group_id,
            target_inbox = target_inbox_id
        );
        // notify any streams of the new group
        let _ = self
            .local_events
            .send(LocalEvents::NewGroup(group.group_id));

        group.add_members(&[target_inbox_id]).await?;

        Ok(group)
    }

    /// Find or create a Direct Message with the default settings
    pub async fn find_or_create_dm_by_identity(
        &self,
        target_identity: Identifier,
        opts: Option<DMMetadataOptions>,
    ) -> Result<MlsGroup<Context>, ClientError> {
        self.ensure_identity_ready()?;
        tracing::info!("finding or creating dm with address: {target_identity}");
        let inbox_id = match self
            .find_inbox_id_from_identifier(&self.context.db(), target_identity.clone())
            .await?
        {
            Some(id) => id,
            None => {
                return Err(NotFound::InboxIdForAddress(target_identity.to_string()).into());
            }
        };

        self.find_or_create_dm(inbox_id, opts).await
    }

    /// Find or create a Direct Message by inbox_id with the default settings
    pub async fn find_or_create_dm(
        &self,
        inbox_id: impl AsIdRef,
        opts: Option<DMMetadataOptions>,
    ) -> Result<MlsGroup<Context>, ClientError> {
        self.ensure_identity_ready()?;
        let inbox_id = inbox_id.as_ref();
        tracing::info!("finding or creating dm with inbox_id: {}", inbox_id);
        let db = self.context.db();
        let group = db.find_active_dm_group(&DmMembers {
            member_one_inbox_id: self.inbox_id(),
            member_two_inbox_id: inbox_id,
        })?;

        if let Some(group) = group {
            return Ok(MlsGroup::new(
                self.context.clone(),
                group.id,
                group.dm_id,
                group.conversation_type,
                group.created_at_ns,
            ));
        }
        self.create_dm_by_inbox_id(inbox_id.to_string(), opts).await
    }

    /// Look up a group by its ID
    ///
    /// Returns a [`MlsGroup`] if the group exists, or an error if it does not
    ///
    pub fn group(&self, group_id: &GroupId) -> Result<MlsGroup<Context>, ClientError> {
        MlsStore::new(self.context.clone())
            .group(group_id)
            .map_err(Into::into)
    }

    /// Look up a group by its ID while stitching DMs
    ///
    /// Returns a [`MlsGroup`] if the group exists, or an error if it does not
    ///
    pub fn stitched_group(&self, group_id: &GroupId) -> Result<MlsGroup<Context>, ClientError> {
        let conn = self.context.db();
        let stored_group = conn.fetch_stitched(group_id)?;
        stored_group
            .map(|g| {
                MlsGroup::new(
                    self.context.clone(),
                    g.id,
                    g.dm_id,
                    g.conversation_type,
                    g.created_at_ns,
                )
            })
            .ok_or(NotFound::GroupById(*group_id))
            .map_err(Into::into)
    }

    /// Find all the duplicate dms for this group
    pub fn find_duplicate_dms_for_group(
        &self,
        group_id: &GroupId,
    ) -> Result<Vec<MlsGroup<Context>>, ClientError> {
        let (group, _) = MlsGroup::new_cached(self.context.clone(), group_id)?;
        group.find_duplicate_dms()
    }

    /// Fetches the message disappearing settings for a given group ID.
    ///
    /// Returns `Some(MessageDisappearingSettings)` if the group exists and has valid settings,
    /// `None` if the group or settings are missing, or `Err(ClientError)` on a database error.
    pub fn group_disappearing_settings(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<MessageDisappearingSettings>, ClientError> {
        let (group, _) = MlsGroup::new_cached(self.context.clone(), group_id)?;
        Ok(group.disappearing_settings()?)
    }

    /**
     * Look up a DM group by the target's inbox_id.
     *
     * Returns a [`MlsGroup`] if the group exists, or an error if it does not
     */
    pub fn dm_group_from_target_inbox(
        &self,
        target_inbox_id: String,
    ) -> Result<MlsGroup<Context>, ClientError> {
        let conn = self.context.db();

        let group = conn
            .find_active_dm_group(&DmMembers {
                member_one_inbox_id: self.inbox_id(),
                member_two_inbox_id: &target_inbox_id,
            })?
            .ok_or(NotFound::DmByInbox(target_inbox_id))?;
        Ok(MlsGroup::new(
            self.context.clone(),
            group.id,
            group.dm_id,
            group.conversation_type,
            group.created_at_ns,
        ))
    }

    /// Look up a message by its ID
    /// Returns a [`StoredGroupMessage`] if the message exists, or an error if it does not
    pub fn message(&self, message_id: Vec<u8>) -> Result<StoredGroupMessage, ClientError> {
        let conn = &mut self.context.db();
        let message = conn.get_group_message(&message_id)?;
        Ok(message.ok_or(NotFound::MessageById(message_id))?)
    }

    /// Look up and enrich a message by its ID, returning a [`DecodedMessage`]
    /// Returns an error if the message is not found or if it cannot be decoded/enriched
    #[xmtp_common::mls_span]
    pub fn message_v2(&self, message_id: Vec<u8>) -> Result<DecodedMessage, ClientError> {
        let conn = self.context.db();
        let message = conn
            .get_group_message(&message_id)?
            .ok_or_else(|| NotFound::MessageById(message_id.clone()))?;

        let group_id = message.group_id;

        let enriched = enrich_messages(conn, &group_id, vec![message])?;

        // Since enrich_messages returns a Vec<DecodedMessage>, we can use .into_iter().next().ok_or(...) to take ownership without cloning.
        enriched
            .into_iter()
            .next()
            // In practice `enrich_messages` should always return an array of the same length as the input
            .ok_or_else(|| ClientError::Generic("Failed to decode message".to_string()))
    }

    /// Delete a message by its ID
    /// This method is idempotent and will not error if the message is not found
    /// Returns the number of messages deleted (0 or 1)
    pub fn delete_message(&self, message_id: Vec<u8>) -> Result<usize, ClientError> {
        let conn = self.context.db();

        // Fetch the message before deleting so we can emit the decoded message in the event
        let msg = conn.get_group_message(&message_id)?;

        let num_deleted = conn.delete_message_by_id(&message_id)?;
        // Fire a local event if the message was successfully deleted
        if num_deleted > 0
            && let Some(message) = msg
        {
            let _ =
                self.context
                    .local_events()
                    .send(crate::subscriptions::LocalEvents::MsgsDeleted(vec![
                        message,
                    ]));
        }

        Ok(num_deleted)
    }

    /// Query for groups with optional filters
    ///
    /// Filters:
    /// - allowed_states: only return groups with the given membership states
    /// - created_after_ns: only return groups created after the given timestamp (in nanoseconds)
    /// - created_before_ns: only return groups created before the given timestamp (in nanoseconds)
    /// - limit: only return the first `limit` groups
    pub fn find_groups(&self, args: GroupQueryArgs) -> Result<Vec<MlsGroup<Context>>, ClientError> {
        MlsStore::new(self.context.clone())
            .find_groups(args)
            .map_err(Into::into)
    }

    pub fn list_conversations(
        &self,
        args: GroupQueryArgs,
    ) -> Result<Vec<ConversationListItem<Context>>, ClientError> {
        let mut args = args.clone();
        // Default to last activity order by for this endpoint
        if args.order_by.is_none() {
            args.order_by = Some(GroupQueryOrderBy::LastActivity);
        }
        Ok(self
            .context
            .db()
            .fetch_conversation_list(args)?
            .into_iter()
            .map(|conversation_item: DbConversationListItem| {
                let message = conversation_item.message_id.and_then(|message_id| {
                    // Only construct StoredGroupMessage if all fields are Some
                    let msg: Option<StoredGroupMessage> = Some(StoredGroupMessage {
                        id: message_id,
                        group_id: conversation_item.id,
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
                        sequence_id: conversation_item.sequence_id?,
                        envelope_hash: None,
                        expiry_ns: None,
                        expire_at_ns: None, //Question: do we need to include this in conversation last message?
                        inserted_at_ns: 0, // Not used for conversation list display
                        should_push: true, // Not used for conversation list display
                        // The conversation_list view does not carry the key; use
                        // the timestamp proxy (display-only, never republished).
                        idempotency_key: conversation_item.sent_at_ns.unwrap_or_default().to_string(),
                    });
                    if msg.is_none() {
                        tracing::warn!("tried listing message, but message had missing fields so it was skipped");
                    }
                    msg
                });

                ConversationListItem {
                    group: MlsGroup::new(
                        self.context.clone(),
                        conversation_item.id,
                        conversation_item.dm_id,
                        conversation_item.conversation_type,
                        conversation_item.created_at_ns,
                    ),
                    last_message: message,
                    is_commit_log_forked: conversation_item.is_commit_log_forked,
                }
            })
            .collect())
    }

    /// Upload the key package before the identity update exposes this installation.
    /// Record its receipt for key retirement and retain the registration cursor.
    #[xmtp_common::mls_span]
    pub async fn register_identity(
        &self,
        signature_request: SignatureRequest,
    ) -> Result<(), ClientError> {
        tracing::info!("registering identity");

        // Handle crash recovery - if already registered, just mark ready and return
        let stored_identity: Option<StoredIdentity> = self.context.db().fetch(&())?;
        if stored_identity.is_some() {
            tracing::info!("Identity already registered, skipping");
            self.identity().set_ready();
            return Ok(());
        }

        // Step 1: Generate key package and store locally (not uploaded yet)
        let (kp_bytes, history_id) = self.identity().generate_and_store_key_package(
            self.context.mls_storage(),
            CREATE_PQ_KEY_PACKAGE_EXTENSION,
        )?;

        // Step 2: Validate signatures (fails here if invalid - no network pollution)
        let identity_update = signature_request
            .build_identity_update()
            .map_err(IdentityUpdateError::from)?;
        identity_update
            .to_verified(&self.context.scw_verifier())
            .await?;

        // Step 3: Upload key package first (prevents race condition)
        let key_package_meta = self.context.api().upload_key_package(kp_bytes).await?;
        let key_package_cursor = key_package_meta
            .cursor
            .filter(|cursor| cursor.sequence_id > 0)
            .ok_or(xmtp_api::ApiError::InvalidResponse(
                "key package publish cursor",
            ))?;

        // Step 4: Publish identity update (makes installation visible)
        let registration_cursor = crate::identity_updates::publish_with_conflict_retry(
            self.context.api(),
            &self.context.db(),
            identity_update,
            &self.context.scw_verifier(),
        )
        .await?;

        // Step 5: Fetch and store in local DB (needed for group operations)
        let inbox_id = self.inbox_id().to_string();
        retry_async!(
            Retry::default(),
            (async {
                load_identity_updates(self.context.api(), &self.context.db(), &[inbox_id.as_str()])
                    .await
            })
        )?;

        // Backend publication order can differ from local generation order.
        crate::state_tx::state_write(self.context.mls_storage(), |tx| {
            let storage = tx.storage();
            storage.db().record_key_package_publication(
                history_id,
                xmtp_proto::types::Cursor(key_package_cursor.sequence_id),
            )?;
            storage
                .db()
                .reset_key_package_rotation_queue(KEY_PACKAGE_ROTATION_INTERVAL_NS)?;
            Ok::<_, StorageError>(Continue(()))
        })
        .map(TransactionOutcome::into_continued)?;

        // Mark identity as ready
        let mut stored_identity = StoredIdentity::try_from(self.identity())?;
        stored_identity.registration_cursor_sequence_id = Some(registration_cursor.0 as i64);
        stored_identity.store(&self.context.db())?;
        self.identity().set_ready();
        Ok(())
    }

    /// Wait until the serving database exposes the registration identity-topic head.
    /// A missing or older head is polled. The timeout also bounds each request.
    pub async fn wait_for_registration_visible(
        &self,
        options: VisibilityConfirmationOptions,
    ) -> Result<(), ClientError> {
        use xmtp_common::time::{Duration, sleep, timeout};
        if !self.identity().is_ready() {
            return Err(ClientError::RegistrationNotVisible);
        }
        let stored: Option<StoredIdentity> = self.context.db().fetch(&())?;
        let sequence_id = stored
            .and_then(|identity| identity.registration_cursor_sequence_id)
            .and_then(|sequence_id| u64::try_from(sequence_id).ok())
            .filter(|sequence_id| *sequence_id != 0)
            .ok_or(ClientError::RegistrationNotVisible)?;
        timeout(Duration::from_millis(options.timeout_ms), async {
            let mut delay = REGISTRATION_INITIAL_BACKOFF;
            let inbox = hex::decode(self.inbox_id())
                .map_err(|_| xmtp_api::ApiError::InvalidRequest("registration inbox id"))?;
            let topic = xmtp_proto::types::Topic::new_identity_update(inbox);
            xmtp_proto::types::Topic::parse(&topic)?;
            loop {
                let heads = self
                    .context
                    .api()
                    .newest_topic_cursors(vec![topic.clone()])
                    .await?;
                let head = heads
                    .get(&topic)
                    .ok_or(xmtp_api::ApiError::InvalidResponse(
                        "registration identity head",
                    ))?;
                if head.0 >= sequence_id {
                    return Ok(());
                }
                sleep(delay).await;
                delay = (delay * 2).min(REGISTRATION_MAX_BACKOFF);
            }
        })
        .await
        .map_err(|_| ClientError::RegistrationNotVisible)?
    }

    /// If no key rotation is scheduled, queue it to occur in the next 5 seconds.
    pub fn queue_key_rotation(&self) -> Result<(), ClientError> {
        crate::worker::key_package_maintenance::queue_key_rotation(&self.context)?;
        Ok(())
    }

    /// Upload a new key package to the network replacing an existing key package
    /// This is expected to be run any time the client receives new Welcome messages
    pub async fn rotate_and_upload_key_package(&self) -> Result<(), ClientError> {
        self.identity()
            .rotate_and_upload_key_package(
                self.context.api(),
                self.context.mls_storage(),
                CREATE_PQ_KEY_PACKAGE_EXTENSION,
            )
            .await?;
        // The rotation marked superseded KPs delete_at=now+grace; without this
        // the parked KpDeletion task would sweep them up to ~30d late.
        crate::worker::key_package_maintenance::nudge_deletion(&self.context)?;

        Ok(())
    }

    /// Fetches the current key package from the network for each of the `installation_id`s specified
    #[tracing::instrument(skip_all)]
    pub async fn get_key_packages_for_installation_ids(
        &self,
        installation_ids: Vec<Vec<u8>>,
    ) -> Result<
        HashMap<Vec<u8>, Result<VerifiedKeyPackageV2, KeyPackageVerificationError>>,
        ClientError,
    > {
        MlsStore::new(self.context.clone())
            .get_key_packages_for_installation_ids(installation_ids)
            .await
            .map_err(Into::into)
    }

    /// Download all unread welcome messages and converts to a group struct, ignoring malformed messages.
    /// Returns any new groups created in the operation
    #[tracing::instrument(skip_all)]
    pub async fn sync_welcomes(&self) -> Result<Vec<MlsGroup<Context>>, GroupError> {
        self.ensure_identity_ready()?;
        WelcomeService::new(self.context.clone())
            .sync_welcomes()
            .await
    }

    /// Sync all groups for the current installation and return the number of groups that were synced.
    /// Only active groups will be synced.
    #[tracing::instrument(err, skip_all, fields(operation = "sync_all_groups"))]
    pub async fn sync_all_groups(
        &self,
        groups: Vec<MlsGroup<Context>>,
    ) -> Result<GroupSyncSummary, GroupError> {
        self.ensure_identity_ready()?;
        WelcomeService::new(self.context.clone())
            .sync_all_groups(groups)
            .await
    }

    /// Sync all unread welcome messages and then sync all groups.
    /// Returns the total number of active groups synced.
    #[xmtp_common::mls_span]
    pub async fn sync_all_welcomes_and_groups(
        &self,
        consent_states: Option<Vec<ConsentState>>,
    ) -> Result<GroupSyncSummary, GroupError> {
        self.ensure_identity_ready()?;
        WelcomeService::new(self.context.clone())
            .sync_all_welcomes_and_groups(consent_states)
            .await
    }

    /// Sweep every group flagged `paused_for_version` and clear the
    /// pause flag for any whose floor is now satisfied by this
    /// client's `pkg_version`. Pure local-state operation — no
    /// network calls. Returns the count of groups unstuck.
    ///
    /// `sync_all_welcomes_and_groups` already runs this sweep as a
    /// preamble; the standalone entry point is for SDKs that want a
    /// cheap "post-upgrade recovery" hook independent of the normal
    /// sync flow.
    pub async fn unstick_paused_groups(&self) -> Result<usize, GroupError> {
        self.ensure_identity_ready()?;
        WelcomeService::new(self.context.clone())
            .unstick_paused_groups()
            .await
    }

    pub async fn sync_all_welcomes_and_device_sync_groups(
        &self,
    ) -> Result<GroupSyncSummary, ClientError> {
        self.sync_welcomes().await?;
        self.sync_all_device_sync_groups().await
    }

    pub async fn sync_all_device_sync_groups(&self) -> Result<GroupSyncSummary, ClientError> {
        let groups = self
            .context
            .db()
            .all_sync_groups()?
            .into_iter()
            .map(|g| {
                MlsGroup::new(
                    self.context.clone(),
                    g.id,
                    g.dm_id,
                    g.conversation_type,
                    g.created_at_ns,
                )
            })
            .collect();

        Ok(self.sync_all_groups(groups).await?)
    }

    /**
     * Validates a credential against the given installation public key
     *
     * This will go to the network and get the latest association state for the inbox.
     * It ensures that the installation_pub_key is in that association state
     */
    pub async fn validate_credential_against_network(
        &self,
        conn: &DbConnection<<Context::Db as XmtpDb>::Connection>,
        credential: &[u8],
        installation_pub_key: Vec<u8>,
    ) -> Result<InboxId, ClientError> {
        let inbox_id = parse_credential(credential)?;
        let association_state = IdentityUpdates::new(&self.context)
            .get_latest_association_state(conn, &inbox_id)
            .await?;
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

        let results = self.context.api().get_inbox_ids(requests).await?;
        Ok(account_identifiers
            .iter()
            .cloned()
            .zip(results.into_iter().map(|inbox_id| inbox_id.is_some()))
            .collect())
    }
}

#[cfg(test)]
pub(crate) mod tests;
