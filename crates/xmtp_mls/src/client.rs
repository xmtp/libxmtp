pub use crate::core::config::DeviceSync;
use crate::subscriptions::catch_up::{CatchUpError, CatchUpSummary};
#[cfg(not(target_arch = "wasm32"))]
use crate::subscriptions::router_callbacks::{self, ApiClientIdentity};
#[cfg(any(test, feature = "test-utils"))]
use crate::subscriptions::stream_messages::stream_stats::StreamWithStats;
use crate::{
    attachments::Attachments,
    context::XmtpSharedContext,
    groups::{
        ConversationListItem, GroupError, MlsGroup, group_permissions::PolicySet,
        welcome_sync::WelcomeService,
    },
    identity::{Identity, IdentityError, parse_credential},
    identity_updates::{
        IdentityUpdateError, IdentityUpdates, load_identity_updates,
        load_identity_updates_for_client,
    },
    inbox_handles::{self, HandleError, IdentityTarget, account_targets, resolve_verified_in},
    mls_store::{KeyPackageResultsByInstallation, MlsStore, MlsStoreError},
    server_configuration::{BlockedConnection, ServerConfigError},
    subscriptions::{
        LocalEventError,
        internal::{emit_consent_changes, emit_deleted_messages},
    },
    utils::VersionInfo,
    worker::device_sync::{DeviceSyncClient, worker::SyncMetric},
    worker::{WorkerRunner, metrics::WorkerMetrics},
};
use crate::{
    core::events::PreferenceUpdate,
    subscriptions::{self, SubscribeError},
};
use crate::{
    groups::welcome_sync::GroupSyncSummary,
    identity_updates::{batch_get_association_state_with_verifier, get_creation_signature_kind},
    messages::{
        decoded_message::DecodedMessage,
        enrichment::{EnrichMessageError, enrich_messages},
    },
};
use futures::Stream;
use notifications::{NotificationConfig, NotificationError, NotificationState};
use openmls::prelude::tls_codec::Error as TlsCodecError;
use std::{
    collections::HashMap,
    sync::{Arc, atomic::Ordering},
};
use thiserror::Error;
use xmtp_api::{ApiClientWrapper, XmtpApi};
use xmtp_common::{ErrorCode, Event, Retry, retry_async, retryable};
use xmtp_common::{MaybeSend, StreamHandle};
use xmtp_configuration::{
    CONVERSATION_LIST_DEFAULT_LIMIT, CREATE_PQ_KEY_PACKAGE_EXTENSION,
    KEY_PACKAGE_ROTATION_INTERVAL_NS, LOCAL_PAGE_MAX_LIMIT,
};
use xmtp_cryptography::signature::IdentifierValidationError;
use xmtp_db::TransactionOutcome::Continue;
use xmtp_db::consent_record::StoredConsentRecord;
use xmtp_db::{
    ConnectionExt, NotFound, StorageError, TransactionOutcome, XmtpDb,
    consent_record::{
        ConsentActError, ConsentKey, ConsentState, ConsentType, ConsentWrite, Decision, Owed,
        conversation_consent, write_consent,
    },
    db_connection::DbConnection,
    encrypted_store::conversation_list::ConversationListItem as DbConversationListItem,
    group::{ConversationType, GroupQueryArgs},
    group_message::StoredGroupMessage,
    identity::StoredIdentity,
};
use xmtp_db::{group::GroupQueryOrderBy, prelude::*};
use xmtp_events::DeletionCause;
use xmtp_id::associations::handle::Handle;
use xmtp_id::key_package::{KeyPackageVerificationError, VerifiedKeyPackageV2};
use xmtp_id::{
    AsIdRef, InboxId, InboxIdRef, InboxOwner,
    associations::{
        AssociationError, AssociationState, Identifier, MemberIdentifier, SignatureError,
        builder::{SignatureRequest, SignatureRequestError},
    },
    scw_verifier::SmartContractSignatureVerifier,
};
use xmtp_macro::log_event;
use xmtp_mls_group_state::{
    group::{DMMetadataOptions, GroupMetadataOptions},
    group_metadata::DmMembers,
    group_mutable_metadata::MessageDisappearingSettings,
};
pub use xmtp_mls_notifications::client as notifications;
#[cfg(not(target_arch = "wasm32"))]
use xmtp_proto::api_client::XmtpMlsBidiStreams;
use xmtp_proto::api_client::XmtpMlsStreams;
use xmtp_proto::types::InstallationId;
use xmtp_proto::{
    api::HasStats,
    api_client::{ApiStats, IdentityStats, XmtpBackendClient},
};

use xmtp_proto::types::GroupId;

mod conversations;
mod error;
mod identity;
mod registration;
mod streams;
mod sync;

pub use error::{ClientError, stream_failure_details};

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

impl Default for VisibilityConfirmationOptions {
    fn default() -> Self {
        Self { timeout_ms: 30_000 }
    }
}

/// An app's consent decision about one entity. It carries no
/// time: the client times it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsentAct {
    pub entity_type: ConsentType,
    pub entity: String,
    pub state: ConsentState,
}

impl ConsentAct {
    pub fn new(entity_type: ConsentType, state: ConsentState, entity: String) -> Self {
        Self {
            entity_type,
            entity,
            state,
        }
    }
}

/// Clients manage access to the network, identity, and data store
pub struct Client<Context> {
    pub context: Context,
    pub installation_id: InstallationId,
    pub(crate) workers: Arc<WorkerRunner>,
    pub(crate) app_lifetime: Arc<AppLifetime>,
}

pub(crate) struct AppLifetime {
    pub(crate) events: xmtp_events::EventBus<crate::subscriptions::internal::InternalEvent>,
}

impl Drop for AppLifetime {
    fn drop(&mut self) {
        self.events.close_app_subscriptions();
    }
}

impl<Context> Drop for Client<Context> {
    fn drop(&mut self) {
        log_event!(Event::ClientDropped, self.installation_id);
    }
}

// most of these things are `Arc`'s
impl<Context: Clone> Clone for Client<Context> {
    fn clone(&self) -> Self {
        Self {
            context: self.context.clone(),
            installation_id: self.installation_id,
            workers: self.workers.clone(),
            app_lifetime: self.app_lifetime.clone(),
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
        // Hold the call gate until the workers start. `close` waits for this
        // call before it stops the workers, so it also stops the ones started
        // here.
        let _call = self
            .context
            .foreground_calls()
            .enter()
            .ok_or(ClientError::AlreadyClosed)?;
        if self.context.is_closed() {
            return Err(ClientError::AlreadyClosed);
        }
        #[cfg(all(test, not(target_arch = "wasm32")))]
        test_hooks::pause_reconnect(&self.context.installation_id().to_vec());
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
        self.context
            .api()
            .api_client
            .unregister_client_event_writer(self.context.public_event_writer());
        self.context.events().close_app_subscriptions();
        self.context.cancellation_token().cancel();
        // A running host call can hold a pooled connection, and a running
        // reconnect can start workers. Wait for both before the workers stop
        // and the database disconnects.
        self.context.foreground_calls().close_and_wait().await;
        let delivery_result = self.context.close_message_delivery();
        self.workers.shutdown().await;
        self.context.events().close_internal_subscriptions();
        // Keep the database connected when lease release fails so a later
        // close call can retry it.
        delivery_result?;
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
    /// What this deployment published about itself, as resolved at build.
    /// A refresh rewrites the stored copy; it never changes
    /// this value.
    pub fn server_configuration(&self) -> &xmtp_configuration::ServerConfiguration {
        self.context.server_configuration().configuration()
    }

    /// Fetch the deployment configuration now and rewrite the stored copy.
    ///
    /// Applies the same validation, storage, and identifier
    /// binding the refresh worker applies. The snapshot this client is
    /// holding is unchanged: a new value takes effect at the next build.
    pub async fn refresh_server_configuration(
        &self,
    ) -> Result<xmtp_configuration::ServerConfiguration, ClientError> {
        Ok(crate::core::preflight::refresh(&self.context).await?)
    }

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

    /// Store the app's consent decisions in one transaction. An
    /// act on a group id the client does not hold fails the whole call, and
    /// nothing is stored.
    pub async fn set_consent_states(&self, acts: &[ConsentAct]) -> Result<(), ClientError> {
        let own = self.context.inbox_id();
        let keys = acts
            .iter()
            .map(|act| ConsentKey::for_app(&self.context.db(), own, act.entity_type, &act.entity))
            .collect::<Result<Vec<_>, _>>()?;
        crate::state_tx::state_write_with_events(
            self.context.mls_storage(),
            self.context.events(),
            |tx, events| {
                let storage = tx.storage();
                let db = storage.db();
                let mut changes = vec![];
                for (key, act) in keys.iter().zip(acts) {
                    let decision = ConsentWrite::Decide(Decision::Set(act.state));
                    changes.extend(write_consent(&db, key, decision, Owed::Mark)?);
                }
                emit_consent_changes(&self.context, events, &db, &changes)?;
                Ok::<_, StorageError>(Continue(()))
            },
        )?
        .into_continued();

        Ok(())
    }

    /// The consent an app reads for an entity: a conversation's under
    /// its key, held or not, or an inbox's record.
    // implements: CONS-001
    pub async fn get_consent_state(
        &self,
        entity_type: ConsentType,
        entity: String,
    ) -> Result<ConsentState, ClientError> {
        let db = self.context.db();
        let Some(key) = ConsentKey::parse(entity_type, &entity) else {
            return Ok(ConsentState::Unknown);
        };
        Ok(match key.group_id() {
            Some(id) => conversation_consent(&db, self.context.inbox_id(), &id)?,
            None => db
                .consent_row(&key)?
                .map_or(ConsentState::Unknown, |r| r.state),
        })
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
    pub(crate) fn ensure_identity_ready<E: From<BlockedConnection> + From<IdentityError>>(
        &self,
    ) -> Result<(), E> {
        // Once the connection is blocked, every later call fails with the
        // reason. This is the gate every client-level operation already passes
        // through, so the check costs nothing extra.
        self.context.server_configuration().check()?;
        if !self.identity().is_ready() {
            tracing::warn!(
                inbox_id = %self.inbox_id(),
                "Operation attempted before register_identity() was called"
            );
            return Err(IdentityError::UninitializedIdentity.into());
        }
        Ok(())
    }
}

impl<Context: XmtpSharedContext> Client<Context> {
    pub fn attachments(&self) -> Attachments<Context> {
        Attachments::new(self.context.clone())
    }

    /// The kinds of the workers this client registered.
    #[cfg(any(test, feature = "test-utils"))]
    #[doc(hidden)]
    pub fn registered_worker_kinds(&self) -> Vec<crate::worker::WorkerKind> {
        self.workers.registered_kinds()
    }
}

impl<Context: XmtpSharedContext> Client<Context> {
    /// Store configuration, then register inline. The task retries transient errors.
    pub async fn enable_notifications(
        &self,
        config: NotificationConfig,
    ) -> Result<NotificationState, NotificationError> {
        notifications::enable_notifications(&self.context, config).await
    }

    /// Disable locally before unregistering. The recipient identity and overrides stay.
    /// A failed unregister leaves the client disabled with its recipient reset owed.
    pub async fn disable_notifications(&self) -> Result<(), NotificationError> {
        notifications::disable_notifications(&self.context).await
    }

    /// Read the locally stored state without a backend call.
    pub fn notification_state(&self) -> Result<NotificationState, StorageError> {
        notifications::notification_state(&self.context)
    }
}

#[cfg(test)]
pub(crate) mod tests;

#[cfg(all(test, not(target_arch = "wasm32")))]
pub(crate) mod test_hooks {
    use std::sync::Arc;
    use tokio::sync::Barrier;

    type PauseHook = (Vec<u8>, Arc<Barrier>, Arc<Barrier>);
    /// Stops the next `reconnect_db` of one installation after its closed
    /// check. The test runs this synchronous call on a blocking thread.
    pub(crate) static PAUSE_RECONNECT: parking_lot::Mutex<Option<PauseHook>> =
        parking_lot::Mutex::new(None);

    pub(super) fn pause_reconnect(installation_id: &[u8]) {
        let paused = {
            let mut hook = PAUSE_RECONNECT.lock();
            if hook
                .as_ref()
                .is_some_and(|(installation, _, _)| installation.as_slice() == installation_id)
            {
                hook.take()
            } else {
                None
            }
        };
        if let Some((_, entered, release)) = paused {
            tokio::runtime::Handle::current().block_on(async {
                entered.wait().await;
                release.wait().await;
            });
        }
    }
}
