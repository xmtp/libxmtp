#[cfg(test)]
mod tests;

use crate::{
    context::ClientMode,
    groups::welcome_sync::GroupSyncSummary,
    identity_updates::{batch_get_association_state_with_verifier, get_creation_signature_kind},
    messages::{
        decoded_message::DecodedMessage,
        enrichment::{EnrichMessageError, enrich_messages},
    },
};
use xmtp_configuration::{CREATE_PQ_KEY_PACKAGE_EXTENSION, KEY_PACKAGE_ROTATION_INTERVAL_NS};

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
    track,
    utils::VersionInfo,
    verified_key_package_v2::{KeyPackageVerificationError, VerifiedKeyPackageV2},
    worker::{WorkerRunner, metrics::WorkerMetrics},
};
use openmls::prelude::tls_codec::Error as TlsCodecError;
use std::{collections::HashMap, sync::Arc};
use thiserror::Error;
use tokio::sync::broadcast;
use xmtp_api::{ApiClientWrapper, XmtpApi};
use xmtp_common::{Retry, retry_async, retryable};
use xmtp_cryptography::signature::IdentifierValidationError;
use xmtp_db::{
    ConnectionExt, NotFound, StorageError, XmtpDb,
    consent_record::{ConsentState, ConsentType, StoredConsentRecord},
    db_connection::DbConnection,
    encrypted_store::conversation_list::ConversationListItem as DbConversationListItem,
    events::EventLevel,
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
        tracing::info!("creating group");

        let group: MlsGroup<Context> = MlsGroup::create_and_insert(
            self.context.clone(),
            ConversationType::Group,
            permissions_policy_set.unwrap_or_default(),
            opts.unwrap_or_default(),
            None,
        )?;

        // notify streams of our new group
        let _ = self
            .local_events
            .send(LocalEvents::NewGroup(group.group_id.clone()));

        track!(
            &self.context,
            "Group Create",
            {
                "conversation_type": ConversationType::Group
            },
            group: &group.group_id,
            level: EventLevel::None
        );

        Ok(group)
    }

    /// Create a group with an initial set of members added
    pub async fn create_group_with_members(
        &self,
        account_identifiers: &[Identifier],
        permissions_policy_set: Option<PolicySet>,
        opts: Option<GroupMetadataOptions>,
    ) -> Result<MlsGroup<Context>, ClientError> {
        tracing::info!("creating group");
        let group = self.create_group(permissions_policy_set, opts)?;

        group.add_members(account_identifiers).await?;

        Ok(group)
    }

    pub async fn create_group_with_inbox_ids(
        &self,
        inbox_ids: &[impl AsIdRef],
        permissions_policy_set: Option<PolicySet>,
        opts: Option<GroupMetadataOptions>,
    ) -> Result<MlsGroup<Context>, ClientError> {
        tracing::info!("creating group");
        let group = self.create_group(permissions_policy_set, opts)?;

        group.add_members_by_inbox_id(inbox_ids).await?;

        Ok(group)
    }

    /// Create a new Direct Message with the default settings
    async fn create_dm_by_inbox_id(
        &self,
        dm_target_inbox_id: InboxId,
        opts: Option<DMMetadataOptions>,
    ) -> Result<MlsGroup<Context>, ClientError> {
        tracing::info!("creating dm with {}", dm_target_inbox_id);
        let group: MlsGroup<Context> = MlsGroup::create_dm_and_insert(
            &self.context,
            GroupMembershipState::Allowed,
            dm_target_inbox_id.clone(),
            opts.unwrap_or_default(),
        )?;

        group.add_members_by_inbox_id(&[dm_target_inbox_id]).await?;

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

        self.find_or_create_dm_by_inbox_id(inbox_id, opts).await
    }

    /// Find or create a Direct Message by inbox_id with the default settings
    pub async fn find_or_create_dm_by_inbox_id(
        &self,
        inbox_id: impl AsIdRef,
        opts: Option<DMMetadataOptions>,
    ) -> Result<MlsGroup<Context>, ClientError> {
        self.ensure_identity_ready()?;
        let inbox_id = inbox_id.as_ref();
        tracing::info!("finding or creating dm with inbox_id: {}", inbox_id);
        let db = self.context.db();
        let group = db.find_dm_group(&DmMembers {
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
            .find_dm_group(&DmMembers {
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
        let num_deleted = conn.delete_message_by_id(&message_id)?;
        // Fire a local event if the message was successfully deleted
        if num_deleted > 0 {
            let _ = self.context.local_events().send(
                crate::subscriptions::LocalEvents::MessageDeleted(message_id),
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

        if self.context.mode() == ClientMode::Notification {
            return Err(ClientError::Generic(
                "Notification clients cannot register on the network.".to_string(),
            ));
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
        if self.context.readonly_mode() {
            tracing::info!("Skipping rotate keypackage. Client is in read-only mode.");
            return Ok(());
        }

        self.identity().queue_key_rotation(&self.context.db())?;

        Ok(())
    }

    /// Upload a new key package to the network replacing an existing key package
    /// This is expected to be run any time the client receives new Welcome messages
    pub async fn rotate_and_upload_key_package(&self) -> Result<(), ClientError> {
        if self.context.readonly_mode() {
            tracing::info!("Skipping rotate keypackage. Client is in read-only mode.");
            return Ok(());
        }

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

    pub async fn sync_all_welcomes_and_history_sync_groups(
        &self,
    ) -> Result<GroupSyncSummary, ClientError> {
        self.sync_welcomes().await?;
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
