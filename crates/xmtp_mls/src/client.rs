use crate::{
    groups::welcome_sync::GroupSyncSummary,
    identity_updates::{batch_get_association_state_with_verifier, get_creation_signature_kind},
    messages::{
        decoded_message::DecodedMessage,
        enrichment::{EnrichMessageError, enrich_messages},
    },
};
use xmtp_configuration::{CREATE_PQ_KEY_PACKAGE_EXTENSION, KEY_PACKAGE_ROTATION_INTERVAL_NS};
use xmtp_macro::log_event;

use crate::{
    builder::SyncWorkerMode,
    context::XmtpSharedContext,
    groups::{
        ConversationListItem, GroupError, MlsGroup,
        device_sync::{DeviceSyncClient, preference_sync::PreferenceUpdate, worker::SyncMetric},
        group_permissions::PolicySet,
        welcome_sync::WelcomeService,
    },
    identity::{Identity, IdentityError, parse_credential},
    identity_updates::{IdentityUpdateError, IdentityUpdates, load_identity_updates},
    mls_store::{MlsStore, MlsStoreError},
    subscriptions::{LocalEventError, LocalEvents, SyncWorkerEvent},
    utils::VersionInfo,
    verified_key_package_v2::{KeyPackageVerificationError, VerifiedKeyPackageV2},
    worker::{WorkerRunner, metrics::WorkerMetrics},
};
use openmls::prelude::tls_codec::Error as TlsCodecError;
use std::{collections::HashMap, sync::Arc};
use thiserror::Error;
use tokio::sync::broadcast;
use xmtp_api::{ApiClientWrapper, XmtpApi};
use xmtp_common::{Event, Retry, fmt::ShortHex, retry_async, retryable};
use xmtp_cryptography::signature::IdentifierValidationError;
use xmtp_db::{
    ConnectionExt, NotFound, StorageError, XmtpDb,
    consent_record::{ConsentState, ConsentType, StoredConsentRecord},
    db_connection::DbConnection,
    encrypted_store::conversation_list::ConversationListItem as DbConversationListItem,
    group::{ConversationType, GroupMembershipState, GroupQueryArgs},
    group_message::StoredGroupMessage,
    identity::StoredIdentity,
};
use xmtp_db::{group::GroupQueryOrderBy, prelude::*};
use xmtp_id::{
    AsIdRef, InboxId, InboxIdRef,
    associations::{
        AssociationError, AssociationState, Identifier, MemberIdentifier, SignatureError,
        builder::{SignatureRequest, SignatureRequestError},
    },
    scw_verifier::SmartContractSignatureVerifier,
};
use xmtp_mls_common::{
    group::{DMMetadataOptions, GroupMetadataOptions},
    group_metadata::DmMembers,
    group_mutable_metadata::MessageDisappearingSettings,
};
use xmtp_proto::types::InstallationId;
use xmtp_proto::{
    api::HasStats,
    api_client::{ApiStats, IdentityStats},
};

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
    #[error("API error: {0}")]
    Api(#[from] xmtp_api::ApiError),
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
    #[error(transparent)]
    Db(#[from] xmtp_db::ConnectionError),
    #[error("generic:{0}")]
    Generic(String),
    #[error(transparent)]
    MlsStore(#[from] MlsStoreError),
    #[error(transparent)]
    EnrichMessage(#[from] EnrichMessageError),
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
            ClientError::Api(api_error) => retryable!(api_error),
            ClientError::Storage(storage_error) => retryable!(storage_error),
            ClientError::Db(db) => retryable!(db),
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
    pub(crate) local_events: broadcast::Sender<LocalEvents>,
    pub(crate) workers: Arc<WorkerRunner>,
}

#[derive(Clone)]
pub struct DeviceSync {
    pub(crate) server_url: Option<String>,
    pub(crate) mode: SyncWorkerMode,
}

// most of these things are `Arc`'s
impl<Context: Clone> Clone for Client<Context> {
    fn clone(&self) -> Self {
        Self {
            context: self.context.clone(),
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
        self.context.db().reconnect().map_err(StorageError::from)?;
        self.workers.spawn(self.context.clone());
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

    pub fn device_sync_server_url(&self) -> Option<&String> {
        self.context.device_sync_server_url()
    }

    pub fn device_sync_worker_enabled(&self) -> bool {
        self.context.device_sync_worker_enabled()
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
        let mut cached_inbox_ids = conn.fetch_cached_inbox_ids(identifiers)?;
        let mut new_inbox_ids = HashMap::default();

        let missing: Vec<_> = identifiers
            .iter()
            .filter(|ident| !cached_inbox_ids.contains_key(&format!("{ident}")))
            .collect();

        if !missing.is_empty() {
            let identifiers = identifiers.iter().map(Into::into).collect();
            new_inbox_ids = self.context.api().get_inbox_ids(identifiers).await?;
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
            group_id = group.group_id.short_hex()
        );

        // notify streams of our new group
        let _ = self
            .local_events
            .send(LocalEvents::NewGroup(group.group_id.clone()));

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
            group_id = group.group_id.short_hex(),
            target_inbox_id
        );
        group.add_members(&[target_inbox_id]).await?;

        // notify any streams of the new group
        let _ = self
            .local_events
            .send(LocalEvents::NewGroup(group.group_id.clone()));

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
    pub fn group(&self, group_id: &Vec<u8>) -> Result<MlsGroup<Context>, ClientError> {
        MlsStore::new(self.context.clone())
            .group(group_id)
            .map_err(Into::into)
    }

    /// Look up a group by its ID while stitching DMs
    ///
    /// Returns a [`MlsGroup`] if the group exists, or an error if it does not
    ///
    pub fn stitched_group(&self, group_id: &[u8]) -> Result<MlsGroup<Context>, ClientError> {
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
            .ok_or(NotFound::GroupById(group_id.to_vec()))
            .map_err(Into::into)
    }

    /// Find all the duplicate dms for this group
    pub fn find_duplicate_dms_for_group(
        &self,
        group_id: &[u8],
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
        group_id: &[u8],
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
    pub fn message_v2(&self, message_id: Vec<u8>) -> Result<DecodedMessage, ClientError> {
        let conn = self.context.db();
        let message = conn
            .get_group_message(&message_id)?
            .ok_or_else(|| NotFound::MessageById(message_id.clone()))?;

        let group_id = message.group_id.clone();

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
        let decoded_message = self.message_v2(message_id.clone()).ok();

        let num_deleted = conn.delete_message_by_id(&message_id)?;
        // Fire a local event if the message was successfully deleted
        if num_deleted > 0
            && let Some(message) = decoded_message
        {
            let _ = self.context.local_events().send(
                crate::subscriptions::LocalEvents::MessageDeleted(Box::new(message)),
            );
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
                        sequence_id: conversation_item.sequence_id?,
                        originator_id: conversation_item.originator_id?,
                        expire_at_ns: None, //Question: do we need to include this in conversation last message?
                        inserted_at_ns: 0, // Not used for conversation list display
                        should_push: true, // Not used for conversation list display
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

    /// Upload a Key Package to the network and publish the signed identity update
    /// from the provided SignatureRequest
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
        self.context
            .api()
            .upload_key_package(kp_bytes, true)
            .await?;

        // Step 4: Publish identity update (makes installation visible)
        self.context
            .api()
            .publish_identity_update(identity_update)
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

        // Clean up old key packages
        self.context.mls_storage().transaction(|conn| {
            conn.key_store()
                .db()
                .mark_key_package_before_id_to_be_deleted(history_id)?;
            Ok::<(), StorageError>(())
        })?;

        self.context
            .mls_storage()
            .db()
            .reset_key_package_rotation_queue(KEY_PACKAGE_ROTATION_INTERVAL_NS)?;

        // Mark identity as ready
        StoredIdentity::try_from(self.identity())?.store(&self.context.db())?;
        self.identity().set_ready();
        Ok(())
    }

    /// If no key rotation is scheduled, queue it to occur in the next 5 seconds.
    pub async fn queue_key_rotation(&self) -> Result<(), ClientError> {
        self.identity()
            .queue_key_rotation(&self.context.db())
            .await?;

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

        Ok(())
    }

    /// Fetches the current key package from the network for each of the `installation_id`s specified
    #[tracing::instrument(level = "trace", skip_all)]
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
    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn sync_welcomes(&self) -> Result<Vec<MlsGroup<Context>>, GroupError> {
        self.ensure_identity_ready()?;
        WelcomeService::new(self.context.clone())
            .sync_welcomes()
            .await
    }

    /// Sync all groups for the current installation and return the number of groups that were synced.
    /// Only active groups will be synced.
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
    pub async fn sync_all_welcomes_and_groups(
        &self,
        consent_states: Option<Vec<ConsentState>>,
    ) -> Result<GroupSyncSummary, GroupError> {
        self.ensure_identity_ready()?;
        WelcomeService::new(self.context.clone())
            .sync_all_welcomes_and_groups(consent_states)
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

        // Get the identities that are on the network, set those to true
        let mut can_message: HashMap<Identifier, bool> = self
            .context
            .api()
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
    use crate::context::XmtpSharedContext;
    use crate::groups::send_message_opts::SendMessageOpts;
    use crate::identity::IdentityError;
    use crate::subscriptions::StreamMessages;
    use crate::tester;
    use crate::utils::{LocalTester, LocalTesterBuilder, Tester};
    use crate::{builder::ClientBuilder, identity::serialize_key_package_hash_ref};
    use diesel::RunQueryDsl;
    use futures::TryStreamExt;
    use futures::stream::StreamExt;
    use prost::Message;
    use std::time::Duration;
    use xmtp_common::time::now_ns;
    use xmtp_common::{NS_IN_SEC, toxiproxy_test};
    use xmtp_content_types::ContentCodec;
    use xmtp_content_types::text::TextCodec;
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_db::consent_record::{ConsentType, StoredConsentRecord};
    use xmtp_db::identity::StoredIdentity;
    use xmtp_db::prelude::*;
    use xmtp_db::{
        ConnectionExt, Fetch, consent_record::ConsentState, group::GroupQueryArgs,
        group_message::MsgQueryArgs, schema::identity_updates,
    };
    use xmtp_id::associations::test_utils::WalletTestExt;

    #[xmtp_common::test]
    async fn test_group_member_recovery() {
        let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bola_wallet = generate_local_wallet();
        // Add two separate installations for Bola
        let bola_a = ClientBuilder::new_test_client(&bola_wallet).await;
        let bola_b = ClientBuilder::new_test_client(&bola_wallet).await;

        let group = amal.create_group(None, None).unwrap();

        // Add both of Bola's installations to the group
        group
            .add_members(&[bola_a.inbox_id(), bola_b.inbox_id()])
            .await
            .unwrap();

        let conn = amal.context.store().conn();
        conn.raw_query_write(|conn| diesel::delete(identity_updates::table).execute(conn))
            .unwrap();

        let members = group.members().await.unwrap();
        // The three installations should count as two members
        assert_eq!(members.len(), 2);
    }

    #[xmtp_common::test]
    async fn test_mls_error() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let result = client
            .context
            .api()
            .upload_key_package(vec![1, 2, 3], false)
            .await;

        assert!(result.is_err());
        let error_string = result.err().unwrap().to_string();
        assert!(error_string.contains("invalid identity") || error_string.contains("EndOfStream"));
    }

    #[xmtp_common::test]
    async fn test_register_installation() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;
        let client_2 = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        // Make sure the installation is actually on the network
        let association_state = client_2
            .identity_updates()
            .get_latest_association_state(&client_2.context.db(), client.inbox_id())
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
        let mut kp1 = client
            .get_key_packages_for_installation_ids(vec![installation_public_key.clone()])
            .await
            .unwrap();
        assert_eq!(kp1.len(), 1);
        let binding = kp1.remove(&installation_public_key).unwrap().unwrap();
        let init1 = binding.inner.hpke_init_key();
        let fetched_identity: StoredIdentity = client.context.db().fetch(&()).unwrap().unwrap();
        assert!(fetched_identity.next_key_package_rotation_ns.is_some());
        // Rotate and fetch again.
        client.queue_key_rotation().await.unwrap();
        //check the rotation value has been set
        let fetched_identity: StoredIdentity = client.context.db().fetch(&()).unwrap().unwrap();
        assert!(fetched_identity.next_key_package_rotation_ns.is_some());

        xmtp_common::time::sleep(std::time::Duration::from_secs(11)).await;

        let mut kp2 = client
            .get_key_packages_for_installation_ids(vec![installation_public_key.clone()])
            .await
            .unwrap();
        assert_eq!(kp2.len(), 1);
        let binding = kp2.remove(&installation_public_key).unwrap().unwrap();
        let init2 = binding.inner.hpke_init_key();

        assert_ne!(init1, init2);
    }

    #[xmtp_common::test]
    async fn test_find_groups() {
        let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let group_1 = client.create_group(None, None).unwrap();
        let group_2 = client.create_group(None, None).unwrap();

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
                .find_inbox_id_from_identifier(&client.context.db(), wallet.identifier())
                .await
                .unwrap(),
            Some(client.inbox_id().to_string())
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_double_dms() {
        tester!(alice);
        tester!(bob);

        let alice_dm = alice
            .create_dm_by_inbox_id(bob.inbox_id().to_string(), None)
            .await?;
        alice_dm
            .send_message(b"Welcome 1", SendMessageOpts::default())
            .await?;

        let bob_dm = bob
            .create_dm_by_inbox_id(alice.inbox_id().to_string(), None)
            .await?;

        tester!(alice2, from: alice);
        let alice_dm2 = alice
            .create_dm_by_inbox_id(bob.inbox_id().to_string(), None)
            .await?;
        alice_dm2
            .send_message(b"Welcome 2", SendMessageOpts::default())
            .await?;

        alice_dm.update_installations().await?;
        alice.sync_welcomes().await?;
        bob.sync_welcomes().await?;

        alice_dm
            .send_message(b"Welcome from 1", SendMessageOpts::default())
            .await?;

        // This message will set bob's dm as the primary DM for all clients
        bob_dm
            .send_message(b"Bob says hi 1", SendMessageOpts::default())
            .await?;
        // Alice will sync, pulling in Bob's DM message, which will cause
        // a database trigger to update `last_message_ns`, putting bob's DM to the top.
        alice_dm.sync().await?;

        alice2.sync_welcomes().await?;
        let mut groups = alice2.find_groups(GroupQueryArgs::default())?;

        assert_eq!(groups.len(), 1);
        let group = groups.pop()?;

        group.sync().await?;
        let messages = group.find_messages(&MsgQueryArgs::default())?;

        assert_eq!(messages.len(), 6);

        // Reload alice's DM. This will load the DM that Bob just created and sent a message on.
        let new_alice_dm = alice.stitched_group(&alice_dm.group_id)?;

        // The group_id should not be what we asked for because it was stitched
        assert_ne!(alice_dm.group_id, new_alice_dm.group_id);
        // They should be the same, due the the message that Bob sent above.
        assert_eq!(new_alice_dm.group_id, bob_dm.group_id);
    }

    #[rstest::rstest]
    #[xmtp_common::test(flavor = "multi_thread")]
    async fn only_test_sync_welcomes() {
        let alice = ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await;
        let bob = ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await;

        let alice_bob_group = alice.create_group(None, None).unwrap();
        alice_bob_group
            .add_members(&[bob.inbox_id()])
            .await
            .unwrap();

        let bob_received_groups = bob.sync_welcomes().await.unwrap();
        assert_eq!(bob_received_groups.len(), 1);
        assert_eq!(
            bob_received_groups.first().unwrap().group_id,
            alice_bob_group.group_id
        );

        let duplicate_received_groups = bob.sync_welcomes().await.unwrap();
        assert_eq!(duplicate_received_groups.len(), 0);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_common::test(flavor = "multi_thread")]
    async fn test_leaf_node_lifetime_validation_disabled() {
        use crate::utils::test_mocks_helpers::set_test_mode_limit_key_package_lifetime;

        // Create a client with default KP lifetime
        let alice = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        // Create a client with default KP lifetime
        set_test_mode_limit_key_package_lifetime(false, 0);
        let cat = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let alice_bob_group = alice.create_group(None, None).unwrap();
        alice_bob_group
            .add_members(&[cat.inbox_id()])
            .await
            .unwrap();

        let cat_received_groups = cat.sync_welcomes().await.unwrap();
        assert_eq!(cat_received_groups.len(), 1);
        assert_eq!(
            cat_received_groups.first().unwrap().group_id,
            alice_bob_group.group_id
        );

        // Create a client with a KP that expires in 5 seconds
        set_test_mode_limit_key_package_lifetime(true, 5);
        let bob = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        // Alice invites Bob with short living KP
        alice_bob_group
            .add_members(&[bob.inbox_id()])
            .await
            .unwrap();

        // Since Bob's KP is still valid, Bob should successfully process the Welcome
        let bob_received_groups = bob.sync_welcomes().await.unwrap();

        // Wait for Bob's KP and their leafnode's lifetime to expire
        xmtp_common::time::sleep(Duration::from_secs(7)).await;

        assert_eq!(bob_received_groups.len(), 1);
        assert_eq!(
            bob_received_groups.first().unwrap().group_id,
            alice_bob_group.group_id
        );

        let bob_duplicate_received_groups = bob.sync_welcomes().await.unwrap();
        let cat_duplicate_received_groups = cat.sync_welcomes().await.unwrap();
        assert_eq!(bob_duplicate_received_groups.len(), 0);
        assert_eq!(cat_duplicate_received_groups.len(), 0);

        set_test_mode_limit_key_package_lifetime(false, 0);
        let dave = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        alice_bob_group
            .add_members(&[dave.inbox_id()])
            .await
            .unwrap();
        // Dave should be okay receiving a welcome where members of the group are expired
        let dave_received_groups = dave.sync_welcomes().await.unwrap();
        assert_eq!(dave_received_groups.len(), 1);
        assert_eq!(
            dave_received_groups.first().unwrap().group_id,
            alice_bob_group.group_id
        );
        let dave_duplicate_received_groups = dave.sync_welcomes().await.unwrap();
        assert_eq!(dave_duplicate_received_groups.len(), 0);

        // Cat receives commits to add expired group members, they should pass validation and be added
        let cat_group = cat_received_groups.first().unwrap();
        cat_group.sync().await.unwrap();
        assert_eq!(cat_group.members().await.unwrap().len(), 4);
    }

    #[rstest::rstest]
    #[xmtp_common::test(flavor = "multi_thread", worker_threads = 10)]
    async fn test_sync_all_groups() {
        let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let alix_bo_group1 = alix.create_group(None, None).unwrap();
        let alix_bo_group2 = alix.create_group(None, None).unwrap();
        alix_bo_group1.add_members(&[bo.inbox_id()]).await.unwrap();
        alix_bo_group2.add_members(&[bo.inbox_id()]).await.unwrap();

        let bob_received_groups = bo.sync_welcomes().await.unwrap();
        assert_eq!(bob_received_groups.len(), 2);

        let bo_groups = bo.find_groups(GroupQueryArgs::default()).unwrap();
        let bo_group1 = bo.group(&alix_bo_group1.clone().group_id).unwrap();
        let bo_messages1 = bo_group1.find_messages(&MsgQueryArgs::default()).unwrap();
        assert_eq!(bo_messages1.len(), 1);
        let bo_group2 = bo.group(&alix_bo_group2.clone().group_id).unwrap();
        let bo_messages2 = bo_group2.find_messages(&MsgQueryArgs::default()).unwrap();
        assert_eq!(bo_messages2.len(), 1);
        alix_bo_group1
            .send_message(vec![1, 2, 3].as_slice(), SendMessageOpts::default())
            .await
            .unwrap();
        alix_bo_group2
            .send_message(vec![1, 2, 3].as_slice(), SendMessageOpts::default())
            .await
            .unwrap();

        let summary = bo.sync_all_groups(bo_groups).await.unwrap();
        assert_eq!(summary.num_synced, 2);

        let bo_messages1 = bo_group1.find_messages(&MsgQueryArgs::default()).unwrap();
        assert_eq!(bo_messages1.len(), 2);
        let bo_group2 = bo.group(&alix_bo_group2.clone().group_id).unwrap();
        let bo_messages2 = bo_group2.find_messages(&MsgQueryArgs::default()).unwrap();
        assert_eq!(bo_messages2.len(), 2);
    }

    #[xmtp_common::test(flavor = "multi_thread")]
    async fn test_sync_all_groups_and_welcomes() {
        tester!(alix);
        tester!(bo, passkey);

        // Create two groups and add Bob
        let alix_bo_group1 = alix.create_group(None, None).unwrap();
        let alix_bo_group2 = alix.create_group(None, None).unwrap();

        alix_bo_group1.add_members(&[bo.inbox_id()]).await.unwrap();
        alix_bo_group2.add_members(&[bo.inbox_id()]).await.unwrap();

        // Initial sync (None): Bob should fetch both groups
        let bob_received_groups = bo.sync_all_welcomes_and_groups(None).await.unwrap();
        assert_eq!(bob_received_groups.num_synced, 0);

        xmtp_common::time::sleep(Duration::from_millis(100)).await;

        // Verify Bo initially has no messages
        let bo_group1 = bo.group(&alix_bo_group1.group_id.clone()).unwrap();
        assert_eq!(
            bo_group1
                .find_messages(&MsgQueryArgs::default())
                .unwrap()
                .len(),
            1
        );
        let bo_group2 = bo.group(&alix_bo_group2.group_id.clone()).unwrap();
        assert_eq!(
            bo_group2
                .find_messages(&MsgQueryArgs::default())
                .unwrap()
                .len(),
            1
        );

        // Alix sends a message to both groups
        alix_bo_group1
            .send_message(vec![1, 2, 3].as_slice(), SendMessageOpts::default())
            .await
            .unwrap();
        alix_bo_group2
            .send_message(vec![4, 5, 6].as_slice(), SendMessageOpts::default())
            .await
            .unwrap();

        // Sync with `Unknown`: Bob should not fetch new messages
        let bob_received_groups_unknown = bo
            .sync_all_welcomes_and_groups(Some([ConsentState::Allowed].to_vec()))
            .await
            .unwrap();
        assert_eq!(bob_received_groups_unknown.num_synced, 0);

        // Verify Bob still has no messages
        assert_eq!(
            bo_group1
                .find_messages(&MsgQueryArgs::default())
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            bo_group2
                .find_messages(&MsgQueryArgs::default())
                .unwrap()
                .len(),
            1
        );

        // Alix sends another message to both groups
        alix_bo_group1
            .send_message(vec![7, 8, 9].as_slice(), SendMessageOpts::default())
            .await
            .unwrap();
        alix_bo_group2
            .send_message(vec![10, 11, 12].as_slice(), SendMessageOpts::default())
            .await
            .unwrap();

        // Sync with `None`: Bob should fetch all messages
        let bo_sync_summary = bo
            .sync_all_welcomes_and_groups(Some([ConsentState::Unknown].to_vec()))
            .await
            .unwrap();
        assert_eq!(bo_sync_summary.num_synced, 2);

        // Verify Bob now has all messages
        let bo_messages1 = bo_group1.find_messages(&MsgQueryArgs::default()).unwrap();
        assert_eq!(bo_messages1.len(), 3);

        let bo_messages2 = bo_group2.find_messages(&MsgQueryArgs::default()).unwrap();
        assert_eq!(bo_messages2.len(), 3);
    }

    #[cfg_attr(all(feature = "d14n", target_arch = "wasm32"), ignore)]
    #[xmtp_common::test]
    async fn test_sync_100_allowed_groups_performance() {
        tester!(alix);
        tester!(bo, passkey);

        let group_count = 100;
        let mut groups = Vec::with_capacity(group_count);

        for _ in 0..group_count {
            let group = alix.create_group(None, None).unwrap();
            group.add_members(&[bo.inbox_id()]).await.unwrap();
            groups.push(group);
        }

        xmtp_common::time::sleep(Duration::from_millis(100)).await;

        let start = xmtp_common::time::Instant::now();
        let _synced_count = bo.sync_all_welcomes_and_groups(None).await.unwrap();
        let elapsed = start.elapsed();

        let test_group = groups.first().unwrap();
        let bo_group = bo.group(&test_group.group_id).unwrap();
        assert_eq!(
            bo_group
                .find_messages(&MsgQueryArgs::default())
                .unwrap()
                .len(),
            1,
            "Expected 1 welcome message synced"
        );

        println!(
            "Synced {} groups in {:?} (avg per group: {:?})",
            group_count,
            elapsed,
            elapsed / group_count as u32
        );

        let start = xmtp_common::time::Instant::now();
        bo.sync_all_welcomes_and_groups(None).await.unwrap();
        let elapsed = start.elapsed();

        println!(
            "Synced {} groups in {:?} (avg per group: {:?})",
            group_count,
            elapsed,
            elapsed / group_count as u32
        );
    }

    #[rstest::rstest]
    #[xmtp_common::test]
    async fn test_add_remove_then_add_again() {
        let amal = Tester::new().await;
        let bola = Tester::new().await;

        // Create a group and invite bola
        let amal_group = amal.create_group(None, None).unwrap();
        amal_group.add_members(&[bola.inbox_id()]).await.unwrap();
        assert_eq!(amal_group.members().await.unwrap().len(), 2);

        // Now remove bola
        amal_group.remove_members(&[bola.inbox_id()]).await.unwrap();
        assert_eq!(amal_group.members().await.unwrap().len(), 1);

        // See if Bola can see that they were added to the group
        bola.sync_welcomes().await.unwrap();
        let bola_groups = bola.find_groups(Default::default()).unwrap();
        assert_eq!(bola_groups.len(), 1);
        let bola_group = bola_groups.first().unwrap();
        bola_group.sync().await.unwrap();

        assert!(!bola_group.is_active().unwrap());

        // Bola should have one readable message (them being added to the group)
        let mut bola_messages = bola_group.find_messages(&MsgQueryArgs::default()).unwrap();

        assert_eq!(bola_messages.len(), 2);

        // Add Bola back to the group
        amal_group.add_members(&[bola.inbox_id()]).await.unwrap();
        bola.sync_welcomes().await.unwrap();

        // Send a message from Amal, now that Bola is back in the group
        amal_group
            .send_message(vec![1, 2, 3].as_slice(), SendMessageOpts::default())
            .await
            .unwrap();

        // Sync Bola's state to get the latest
        if let Err(err) = bola_group.sync().await {
            panic!("Error syncing group: {:?}", err);
        }
        // Find Bola's updated list of messages
        bola_messages = bola_group.find_messages(&MsgQueryArgs::default()).unwrap();
        // Bola should have been able to decrypt the last message
        assert_eq!(bola_messages.len(), 4);
        assert_eq!(
            bola_messages.get(3).unwrap().decrypted_message_bytes,
            vec![1, 2, 3]
        )
    }

    async fn get_key_package_init_key<Context: XmtpSharedContext, Id: AsRef<[u8]>>(
        client: &Client<Context>,
        installation_id: Id,
    ) -> Result<Vec<u8>, IdentityError> {
        let mut kps_map = client
            .get_key_packages_for_installation_ids(vec![installation_id.as_ref().to_vec()])
            .await
            .map_err(|_| IdentityError::NewIdentity("Failed to fetch key packages".to_string()))?;

        let kp_result = kps_map.remove(installation_id.as_ref()).ok_or_else(|| {
            IdentityError::NewIdentity(format!(
                "Missing key package for {}",
                hex::encode(installation_id.as_ref())
            ))
        })??;

        serialize_key_package_hash_ref(&kp_result.inner, &client.context.mls_provider())
    }

    #[xmtp_common::test]
    async fn test_key_package_rotation() {
        let alix_wallet = generate_local_wallet();
        let bo_wallet = generate_local_wallet();
        let alix = ClientBuilder::new_test_client(&alix_wallet).await;
        let bo = ClientBuilder::new_test_client(&bo_wallet).await;

        let alix_original_init_key =
            get_key_package_init_key(&alix, alix.installation_public_key())
                .await
                .unwrap();
        let bo_original_init_key = get_key_package_init_key(&bo, bo.installation_public_key())
            .await
            .unwrap();

        let alix_fetched_identity: StoredIdentity = alix.context.db().fetch(&()).unwrap().unwrap();
        assert!(alix_fetched_identity.next_key_package_rotation_ns.is_some());
        let bo_fetched_identity: StoredIdentity = bo.context.db().fetch(&()).unwrap().unwrap();
        assert!(bo_fetched_identity.next_key_package_rotation_ns.is_some());
        // Bo's original key should be deleted
        let bo_original_from_db = bo
            .db()
            .find_key_package_history_entry_by_hash_ref(bo_original_init_key.clone());
        assert!(bo_original_from_db.is_ok());

        alix.create_group_with_identifiers(&[bo_wallet.identifier()], None, None)
            .await
            .unwrap();
        let bo_keys_queued_for_rotation = bo.context.db().is_identity_needs_rotation().unwrap();
        assert!(!bo_keys_queued_for_rotation);

        bo.sync_welcomes().await.unwrap();

        //check the rotation value has been set and less than Queue rotation interval
        let bo_fetched_identity: StoredIdentity = bo.context.db().fetch(&()).unwrap().unwrap();
        assert!(bo_fetched_identity.next_key_package_rotation_ns.is_some());
        let updated_at = bo
            .context
            .db()
            .key_package_rotation_history()
            .into_iter()
            .map(|(_, updated_at)| updated_at)
            .next_back()
            .unwrap();
        assert!(
            bo_fetched_identity.next_key_package_rotation_ns.unwrap() - updated_at < 5 * NS_IN_SEC
        );

        //check original keys must not be marked to be deleted
        let bo_keys = bo
            .context
            .db()
            .find_key_package_history_entry_by_hash_ref(bo_original_init_key.clone());
        assert!(bo_keys.unwrap().delete_at_ns.is_none());
        //wait for worker to rotate the keypackage
        xmtp_common::time::sleep(std::time::Duration::from_secs(11)).await;
        //check the rotation queue must be cleared
        let bo_keys_queued_for_rotation = bo.context.db().is_identity_needs_rotation().unwrap();
        assert!(!bo_keys_queued_for_rotation);

        let bo_fetched_identity: StoredIdentity = bo.context.db().fetch(&()).unwrap().unwrap();
        assert!(bo_fetched_identity.next_key_package_rotation_ns.unwrap() > 0);

        let bo_new_key = get_key_package_init_key(&bo, bo.installation_public_key())
            .await
            .unwrap();
        // Bo's key should have changed
        assert_ne!(bo_original_init_key, bo_new_key);

        // Depending on timing, old key should already be deleted, or marked to be deleted
        let bo_keys = bo
            .context
            .db()
            .find_key_package_history_entry_by_hash_ref(bo_original_init_key.clone())
            .ok();
        if let Some(key) = bo_keys {
            assert!(key.delete_at_ns.is_some());
        }

        xmtp_common::time::sleep(std::time::Duration::from_secs(10)).await;
        let bo_keys = bo
            .context
            .db()
            .find_key_package_history_entry_by_hash_ref(bo_original_init_key.clone());
        assert!(bo_keys.is_err());

        bo.sync_welcomes().await.unwrap();
        let bo_new_key_2 = get_key_package_init_key(&bo, bo.installation_public_key())
            .await
            .unwrap();
        // Bo's key should not have changed syncing the second time.
        assert_eq!(bo_new_key, bo_new_key_2);

        let alix_keys_queued_for_rotation = alix.context.db().is_identity_needs_rotation().unwrap();
        assert!(!alix_keys_queued_for_rotation);

        alix.sync_welcomes().await.unwrap();
        let alix_key_2 = get_key_package_init_key(&alix, alix.installation_public_key())
            .await
            .unwrap();

        // Alix's key should not have changed at all
        assert_eq!(alix_original_init_key, alix_key_2);

        alix.create_group_with_identifiers(&[bo_wallet.identifier()], None, None)
            .await
            .unwrap();
        bo.sync_welcomes().await.unwrap();

        // Bo should have two groups now
        let bo_groups = bo.find_groups(GroupQueryArgs::default()).unwrap();
        assert_eq!(bo_groups.len(), 2);

        // Bo's original key should be deleted
        let bo_original_after_delete = bo
            .db()
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
            .find_or_create_dm(client2.inbox_id().to_string(), None)
            .await
            .unwrap();

        // Verify DM was created with correct properties
        let metadata = dm1.metadata().await.unwrap();
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
            .find_or_create_dm(client2.inbox_id().to_string(), None)
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

    #[xmtp_common::test(unwrap_try = true)]
    async fn should_stream_consent() {
        let alix = Tester::builder().sync_worker().build().await;
        let bo = Tester::new().await;

        let receiver = alix.local_events.subscribe();
        let stream = receiver.stream_consent_updates();
        futures::pin_mut!(stream);

        let group = alix
            .create_group_with_members(&[bo.inbox_id().to_string()], None, None)
            .await
            .unwrap();
        xmtp_common::time::sleep(std::time::Duration::from_millis(500)).await;

        // first record is denied consent to the group.
        group.update_consent_state(ConsentState::Denied).unwrap();

        xmtp_common::time::sleep(std::time::Duration::from_millis(500)).await;

        // second is allowing consent for the group
        alix.set_consent_states(&[StoredConsentRecord {
            entity: hex::encode(&group.group_id),
            state: ConsentState::Allowed,
            entity_type: ConsentType::ConversationId,
            consented_at_ns: now_ns(),
        }])
        .await
        .unwrap();

        xmtp_common::time::sleep(std::time::Duration::from_millis(500)).await;

        // third allowing consent for bo inbox id
        alix.set_consent_states(&[StoredConsentRecord {
            entity: bo.inbox_id().to_string(),
            entity_type: ConsentType::InboxId,
            state: ConsentState::Allowed,
            consented_at_ns: now_ns(),
        }])
        .await
        .unwrap();

        // First consent update from creating the group
        let item = stream.next().await??;
        assert_eq!(item.len(), 1);
        assert_eq!(item[0].entity_type, ConsentType::ConversationId);
        assert_eq!(item[0].entity, hex::encode(&group.group_id));
        assert_eq!(item[0].state, ConsentState::Allowed);

        let item = stream.next().await??;
        assert_eq!(item.len(), 1);
        assert_eq!(item[0].entity_type, ConsentType::ConversationId);
        assert_eq!(item[0].entity, hex::encode(&group.group_id));
        assert_eq!(item[0].state, ConsentState::Denied);

        let item = stream.next().await??;
        assert_eq!(item.len(), 1);
        assert_eq!(item[0].entity_type, ConsentType::ConversationId);
        assert_eq!(item[0].entity, hex::encode(group.group_id));
        assert_eq!(item[0].state, ConsentState::Allowed);

        let item = stream.next().await??;
        assert_eq!(item.len(), 1);
        assert_eq!(item[0].entity_type, ConsentType::InboxId);
        assert_eq!(item[0].entity, bo.inbox_id());
        assert_eq!(item[0].state, ConsentState::Allowed);
    }

    #[rstest::rstest]
    #[xmtp_common::test(unwrap_try = true)]
    // Set to 50 seconds to safely account for the 16 second keepalive interval and 10 second timeout
    #[timeout(Duration::from_secs(50))]
    #[cfg_attr(any(target_arch = "wasm32"), ignore)]
    async fn should_reconnect() {
        toxiproxy_test(async || {
            let alix = Tester::builder().proxy().build().await;
            let bo = Tester::builder().build().await;

            let start_new_convo = || async {
                bo.create_group_with_members(&[alix.inbox_id().to_string()], None, None)
                    .await
                    .unwrap()
            };

            let stream = alix.client.stream_conversations(None, false).await.unwrap();
            futures::pin_mut!(stream);

            start_new_convo().await;

            let success_res = stream.try_next().await;
            assert!(success_res.is_ok());

            // Black hole the connection for a minute, then reconnect. The test will timeout without the keepalives.
            alix.for_each_proxy(async |p| {
                p.with_timeout("downstream".into(), 60_000, 1.0).await;
            })
            .await;

            start_new_convo().await;

            let should_fail = stream.try_next().await;
            assert!(should_fail.is_err());

            start_new_convo().await;

            alix.for_each_proxy(async |p| {
                p.delete_all_toxics().await.unwrap();
            })
            .await;
            xmtp_common::time::sleep(std::time::Duration::from_millis(500)).await;

            // stream closes after it gets the broken pipe b/c of blackhole & HTTP/2 KeepAlive
            futures_test::assert_stream_done!(stream);
            xmtp_common::time::sleep(std::time::Duration::from_millis(100)).await;
            let mut new_stream = alix.client.stream_conversations(None, false).await.unwrap();
            let new_res = new_stream.try_next().await;
            assert!(new_res.is_ok());
            assert!(new_res.unwrap().is_some());
        })
        .await
    }

    #[rstest::rstest]
    #[xmtp_common::test(unwrap_try = true)]
    async fn test_list_conversations_pagination() {
        use prost::Message;
        use xmtp_mls_common::group::GroupMetadataOptions;

        let alix = Tester::builder().build().await;
        let bo = Tester::builder().build().await;

        // Create 15 groups with small delays to ensure different created_at_ns values
        let mut all_group_ids = Vec::new();
        for i in 0..15 {
            let group = alix
                .create_group_with_members(
                    &[bo.inbox_id().to_string()],
                    None,
                    Some(GroupMetadataOptions {
                        name: Some(format!("Group {}", i + 1)),
                        ..Default::default()
                    }),
                )
                .await
                .unwrap();
            all_group_ids.push(group.group_id.clone());
            group
                .send_message(
                    TextCodec::encode("hello".to_string())
                        .unwrap()
                        .encode_to_vec()
                        .as_slice(),
                    SendMessageOpts::default(),
                )
                .await
                .unwrap();
            // Small delay to ensure different timestamps
            xmtp_common::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        let mut before_ns = None;
        let mut all_conversation_ids = Vec::new();
        loop {
            let results = alix
                .list_conversations(GroupQueryArgs {
                    limit: Some(5),
                    last_activity_before_ns: before_ns,
                    ..Default::default()
                })
                .unwrap();

            if results.is_empty() {
                break;
            }
            assert_eq!(results.len(), 5);

            all_conversation_ids.extend(results.iter().map(|item| item.group.group_id.clone()));

            before_ns = Some(
                results
                    .last()
                    .unwrap()
                    .last_message
                    .as_ref()
                    .unwrap()
                    .sent_at_ns,
            );
        }

        assert_eq!(
            all_conversation_ids.len(),
            15,
            "Should have 15 total conversations"
        );
        all_conversation_ids.dedup();

        // Check that we got all 15 unique groups
        assert_eq!(
            all_conversation_ids.len(),
            15,
            "Should have 15 total conversations after deduping"
        );
    }

    #[xmtp_common::test]
    async fn test_delete_message() {
        let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        // Create a group with both users
        let group = alix
            .create_group_with_members(&[bo.inbox_id().to_string()], None, None)
            .await
            .unwrap();

        // Send a message
        let message_id = group
            .send_message(
                TextCodec::encode("test message".to_string())
                    .unwrap()
                    .encode_to_vec()
                    .as_slice(),
                SendMessageOpts::default(),
            )
            .await
            .unwrap();

        // Verify the message exists
        let message = alix.message(message_id.clone()).unwrap();
        assert_eq!(message.id, message_id);

        // Delete the message
        let deleted_count = alix.delete_message(message_id.clone()).unwrap();
        assert_eq!(deleted_count, 1, "Should delete exactly 1 message");

        // Verify the message no longer exists
        let result = alix.message(message_id.clone());
        assert!(result.is_err(), "Message should not exist after deletion");

        // Test idempotency - deleting again should not error and return 0
        let deleted_count = alix.delete_message(message_id).unwrap();
        assert_eq!(
            deleted_count, 0,
            "Deleting non-existent message should return 0"
        );
    }
}
