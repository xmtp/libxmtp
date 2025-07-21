use crate::{
    client::{Client, DeviceSync},
    context::XmtpMlsLocalContext,
    groups::{
        device_sync::worker::SyncWorker, disappearing_messages::DisappearingMessagesWorker,
        key_package_cleaner_worker::KeyPackagesCleanerWorker,
    },
    identity::{Identity, IdentityStrategy},
    identity_updates::load_identity_updates,
    mutex_registry::MutexRegistry,
    track,
    utils::{events::EventWorker, VersionInfo},
    worker::WorkerRunner,
    GroupCommitLock, StorageError, XmtpApi,
};
use std::sync::{atomic::Ordering, Arc};
use thiserror::Error;
use tokio::sync::broadcast;
use tracing::debug;
use xmtp_api::{ApiClientWrapper, ApiDebugWrapper};
use xmtp_common::Retry;
use xmtp_cryptography::signature::IdentifierValidationError;
use xmtp_db::XmtpMlsStorageProvider;
use xmtp_db::{
    events::{Events, EVENTS_ENABLED},
    sql_key_store::SqlKeyStore,
    XmtpDb,
};
use xmtp_id::scw_verifier::RemoteSignatureVerifier;
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;

type ContextParts<Api, S, Db> = Arc<XmtpMlsLocalContext<Api, Db, S>>;

#[derive(Error, Debug)]
pub enum ClientBuilderError {
    #[error(transparent)]
    AddressValidation(#[from] IdentifierValidationError),
    #[error("Missing parameter: {parameter}")]
    MissingParameter { parameter: &'static str },
    #[error(transparent)]
    ClientError(#[from] crate::client::ClientError),
    #[error("Storage Error")]
    StorageError(#[from] StorageError),
    #[error(transparent)]
    Identity(#[from] crate::identity::IdentityError),
    #[error(transparent)]
    WrappedApiError(#[from] xmtp_api::ApiError),
    #[error(transparent)]
    GroupError(#[from] Box<crate::groups::GroupError>),
    #[error(transparent)]
    DeviceSync(#[from] Box<crate::groups::device_sync::DeviceSyncError>),
}

impl From<crate::groups::device_sync::DeviceSyncError> for ClientBuilderError {
    fn from(value: crate::groups::device_sync::DeviceSyncError) -> Self {
        ClientBuilderError::DeviceSync(Box::new(value))
    }
}

impl From<crate::groups::GroupError> for ClientBuilderError {
    fn from(value: crate::groups::GroupError) -> Self {
        ClientBuilderError::GroupError(Box::new(value))
    }
}

pub struct ClientBuilder<ApiClient, S, Db = xmtp_db::DefaultStore> {
    api_client: Option<ApiClientWrapper<ApiClient>>,
    identity: Option<Identity>,
    store: Option<Db>,
    identity_strategy: IdentityStrategy,
    scw_verifier: Option<Arc<Box<dyn SmartContractSignatureVerifier>>>,
    device_sync_server_url: Option<String>,
    device_sync_worker_mode: SyncWorkerMode,
    version_info: VersionInfo,
    allow_offline: bool,
    disable_events: bool,
    mls_storage: Option<S>,
}

#[derive(Clone, Copy, Debug)]
pub enum SyncWorkerMode {
    Disabled,
    Enabled,
}

impl Client<()> {
    /// Get the builder for this [`Client`]
    pub fn builder(strategy: IdentityStrategy) -> ClientBuilder<(), ()> {
        ClientBuilder::<(), ()>::new(strategy)
    }
}

impl<ApiClient, S, Db> ClientBuilder<ApiClient, S, Db> {
    #[tracing::instrument(level = "trace", skip_all)]
    pub fn new(identity_strategy: IdentityStrategy) -> Self {
        Self {
            identity_strategy,
            api_client: None,
            identity: None,
            store: None,
            scw_verifier: None,
            device_sync_server_url: None,
            device_sync_worker_mode: SyncWorkerMode::Enabled,
            version_info: VersionInfo::default(),
            allow_offline: false,
            #[cfg(not(test))]
            disable_events: false,
            #[cfg(test)]
            disable_events: true,
            mls_storage: None,
        }
    }
}

#[cfg(test)]
impl<ApiClient, S, Db> ClientBuilder<ApiClient, S, Db>
where
    ApiClient: Clone,
    Db: Clone,
{
    pub fn from_client(
        client: Client<ContextParts<ApiClient, S, Db>>,
    ) -> ClientBuilder<ApiClient, Arc<S>, Db> {
        let cloned_api = client.context.api_client.clone();
        ClientBuilder {
            api_client: Some(cloned_api),
            identity: Some(client.context.identity.clone()),
            store: Some(client.context.store.clone()),
            identity_strategy: IdentityStrategy::CachedOnly,
            scw_verifier: Some(client.context.scw_verifier.clone()),
            device_sync_server_url: client.context.device_sync.server_url.clone(),
            device_sync_worker_mode: client.context.device_sync.mode,
            version_info: client.context.version_info.clone(),
            allow_offline: false,
            #[cfg(test)]
            disable_events: true,
            #[cfg(not(test))]
            disable_events: false,
            mls_storage: Some(client.context.mls_storage.clone()),
        }
    }
}

impl<ApiClient, S, Db> ClientBuilder<ApiClient, S, Db> {
    pub async fn build(self) -> Result<Client<ContextParts<ApiClient, S, Db>>, ClientBuilderError>
    where
        ApiClient: XmtpApi + 'static + Send + Sync,
        Db: xmtp_db::XmtpDb + 'static + Send + Sync,
        S: XmtpMlsStorageProvider + 'static + Send + Sync,
    {
        let ClientBuilder {
            mut api_client,
            identity,
            mut store,
            identity_strategy,
            mut scw_verifier,

            device_sync_server_url,
            device_sync_worker_mode,
            version_info,
            allow_offline,
            disable_events,
            mut mls_storage,
            ..
        } = self;

        let api_client = api_client
            .take()
            .ok_or(ClientBuilderError::MissingParameter {
                parameter: "api_client",
            })?;

        let scw_verifier = scw_verifier
            .take()
            .ok_or(ClientBuilderError::MissingParameter {
                parameter: "scw_verifier",
            })?;

        let store = store
            .take()
            .ok_or(ClientBuilderError::MissingParameter { parameter: "store" })?;

        let mls_storage = mls_storage
            .take()
            .ok_or(ClientBuilderError::MissingParameter {
                parameter: "mls_storage",
            })?;

        let conn = store.db();
        let identity = if let Some(identity) = identity {
            identity
        } else {
            identity_strategy
                .initialize_identity(&api_client, &mls_storage, &scw_verifier)
                .await?
        };

        debug!(
            inbox_id = identity.inbox_id(),
            installation_id = hex::encode(identity.installation_keys.public_bytes()),
            "Initialized identity"
        );
        if !allow_offline {
            // get sequence_id from identity updates and loaded into the DB
            load_identity_updates(
                &api_client,
                &conn,
                vec![identity.inbox_id.as_str()].as_slice(),
            )
            .await?;
        }

        let (tx, _) = broadcast::channel(32);
        let (worker_tx, _) = broadcast::channel(32);
        let mut workers = WorkerRunner::new();
        let context = Arc::new(XmtpMlsLocalContext {
            identity,
            mls_storage: Arc::new(mls_storage),
            store,
            api_client,
            version_info,
            scw_verifier,
            mutexes: MutexRegistry::new(),
            mls_commit_lock: Arc::new(GroupCommitLock::new()),
            local_events: tx.clone(),
            worker_events: worker_tx.clone(),
            device_sync: DeviceSync {
                server_url: device_sync_server_url,
                mode: device_sync_worker_mode,
            },
            workers: workers.clone(),
        });

        // register workers
        if context.device_sync_worker_enabled() {
            workers.register_new_worker::<SyncWorker<ContextParts<ApiClient, S, Db>>, _>(
                context.clone(),
            );
        }
        if !disable_events {
            EVENTS_ENABLED.store(true, Ordering::SeqCst);
            workers.register_new_worker::<EventWorker<ContextParts<ApiClient, S, Db>>, _>(
                context.clone(),
            );
        }
        workers.register_new_worker::<KeyPackagesCleanerWorker<ContextParts<ApiClient, S, Db>>, _>(
            context.clone(),
        );
        workers
            .register_new_worker::<DisappearingMessagesWorker<ContextParts<ApiClient, S, Db>>, _>(
                context.clone(),
            );
        workers.spawn();
        let client = Client {
            context,
            local_events: tx,
            workers,
        };

        // Clear old events
        if let Err(err) = Events::clear_old_events(&client.db()) {
            tracing::warn!("ClientEvents clear old events: {err:?}");
        }
        track!("Client Build");

        Ok(client)
    }

    pub fn identity(self, identity: Identity) -> Self {
        Self {
            identity: Some(identity),
            ..self
        }
    }

    pub fn store<NewDb>(self, db: NewDb) -> ClientBuilder<ApiClient, S, NewDb> {
        ClientBuilder {
            store: Some(db),
            api_client: self.api_client,
            identity: self.identity,
            identity_strategy: self.identity_strategy,
            scw_verifier: self.scw_verifier,
            device_sync_server_url: self.device_sync_server_url,
            device_sync_worker_mode: self.device_sync_worker_mode,
            version_info: self.version_info,
            allow_offline: self.allow_offline,
            disable_events: self.disable_events,
            mls_storage: self.mls_storage,
        }
    }

    /// Use the default SQlite MLS Key-Value Store
    pub fn default_mls_store(
        self,
    ) -> Result<
        ClientBuilder<ApiClient, SqlKeyStore<<Db as XmtpDb>::DbQuery>, Db>,
        ClientBuilderError,
    >
    where
        Db: XmtpDb,
    {
        Ok(ClientBuilder {
            api_client: self.api_client,
            identity: self.identity,
            identity_strategy: self.identity_strategy,
            scw_verifier: self.scw_verifier,
            device_sync_server_url: self.device_sync_server_url,
            device_sync_worker_mode: self.device_sync_worker_mode,
            version_info: self.version_info,
            allow_offline: self.allow_offline,
            disable_events: self.disable_events,
            mls_storage: Some(SqlKeyStore::new(
                self.store
                    .as_ref()
                    .ok_or(ClientBuilderError::MissingParameter {
                        parameter: "encrypted store",
                    })?
                    .db(),
            )),
            store: self.store,
        })
    }

    pub fn mls_storage<NewS>(self, mls_storage: NewS) -> ClientBuilder<ApiClient, NewS, Db> {
        ClientBuilder {
            store: self.store,
            api_client: self.api_client,
            identity: self.identity,
            identity_strategy: self.identity_strategy,
            scw_verifier: self.scw_verifier,
            device_sync_server_url: self.device_sync_server_url,
            device_sync_worker_mode: self.device_sync_worker_mode,
            version_info: self.version_info,
            allow_offline: self.allow_offline,
            disable_events: self.disable_events,
            mls_storage: Some(mls_storage),
        }
    }

    pub fn with_device_sync_server_url(self, url: Option<String>) -> Self {
        Self {
            device_sync_server_url: url,
            ..self
        }
    }

    pub fn device_sync_server_url(self, url: &str) -> Self {
        Self {
            device_sync_server_url: Some(url.into()),
            ..self
        }
    }

    pub fn with_device_sync_worker_mode(self, mode: Option<SyncWorkerMode>) -> Self {
        Self {
            device_sync_worker_mode: mode.unwrap_or(SyncWorkerMode::Enabled),
            ..self
        }
    }

    pub fn device_sync_worker_mode(self, mode: SyncWorkerMode) -> Self {
        Self {
            device_sync_worker_mode: mode,
            ..self
        }
    }

    pub fn api_client<A>(self, api_client: A) -> ClientBuilder<A, S, Db> {
        let api_retry = Retry::builder().build();
        let api_client = ApiClientWrapper::new(api_client, api_retry);
        ClientBuilder {
            api_client: Some(api_client),
            identity: self.identity,
            identity_strategy: self.identity_strategy,
            scw_verifier: self.scw_verifier,
            store: self.store,
            device_sync_server_url: self.device_sync_server_url,
            device_sync_worker_mode: self.device_sync_worker_mode,
            version_info: self.version_info,
            allow_offline: self.allow_offline,
            disable_events: self.disable_events,
            mls_storage: self.mls_storage,
        }
    }

    pub fn maybe_version(
        mut self,
        version: Option<VersionInfo>,
    ) -> ClientBuilder<ApiClient, S, Db> {
        if let Some(v) = version {
            self.version_info = v;
        }
        self
    }

    pub fn version(self, version_info: VersionInfo) -> ClientBuilder<ApiClient, S, Db> {
        Self {
            version_info,
            ..self
        }
    }

    /// Skip network calls when building a client
    pub fn with_allow_offline(
        self,
        allow_offline: Option<bool>,
    ) -> ClientBuilder<ApiClient, S, Db> {
        Self {
            allow_offline: allow_offline.unwrap_or(false),
            ..self
        }
    }

    #[cfg(not(test))]
    pub fn with_disable_events(
        self,
        disable_events: Option<bool>,
    ) -> ClientBuilder<ApiClient, S, Db> {
        Self {
            disable_events: disable_events.unwrap_or(false),
            ..self
        }
    }

    #[cfg(all(test, not(target_arch = "wasm32")))]
    pub fn with_disable_events(
        self,
        disable_events: Option<bool>,
    ) -> ClientBuilder<ApiClient, S, Db> {
        Self {
            disable_events: disable_events.unwrap_or(true),
            ..self
        }
    }

    #[cfg(all(test, target_arch = "wasm32"))]
    pub fn with_disable_events(
        self,
        _disable_events: Option<bool>,
    ) -> ClientBuilder<ApiClient, S, Db> {
        Self {
            disable_events: true,
            ..self
        }
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn enable_sqlite_triggers(self) -> Self
    where
        Db: XmtpDb,
    {
        let db = self.store.as_ref().unwrap().db();
        db.register_triggers();
        db.disable_memory_security();
        self
    }

    /// Wrap the Api Client in a Debug Adapter which prints api stats on error.
    /// Requires the api client to be set in the builder.
    pub fn enable_api_debug_wrapper(
        self,
    ) -> Result<ClientBuilder<ApiDebugWrapper<ApiClient>, S, Db>, ClientBuilderError> {
        if self.api_client.is_none() {
            return Err(ClientBuilderError::MissingParameter {
                parameter: "api_client",
            });
        }

        Ok(ClientBuilder {
            api_client: Some(
                self.api_client
                    .expect("checked for none")
                    .attach_debug_wrapper(),
            ),
            identity: self.identity,
            identity_strategy: self.identity_strategy,
            scw_verifier: self.scw_verifier,
            store: self.store,

            device_sync_server_url: self.device_sync_server_url,
            device_sync_worker_mode: self.device_sync_worker_mode,
            version_info: self.version_info,
            allow_offline: self.allow_offline,
            disable_events: self.disable_events,
            mls_storage: self.mls_storage,
        })
    }

    pub fn with_scw_verifier(
        self,
        verifier: impl SmartContractSignatureVerifier + 'static,
    ) -> ClientBuilder<ApiClient, S, Db> {
        ClientBuilder {
            api_client: self.api_client,
            identity: self.identity,
            identity_strategy: self.identity_strategy,
            scw_verifier: Some(Arc::new(Box::new(verifier))),
            store: self.store,

            device_sync_server_url: self.device_sync_server_url,
            device_sync_worker_mode: self.device_sync_worker_mode,
            version_info: self.version_info,
            allow_offline: self.allow_offline,
            disable_events: self.disable_events,
            mls_storage: self.mls_storage,
        }
    }

    /// Build the client with a default remote verifier
    /// requires the 'api' to be set.
    pub fn with_remote_verifier(self) -> Result<ClientBuilder<ApiClient, S, Db>, ClientBuilderError>
    where
        ApiClient: Clone + XmtpApi + Send + Sync + 'static,
    {
        let api = self
            .api_client
            .clone()
            .ok_or(ClientBuilderError::MissingParameter {
                parameter: "api_client",
            })?;

        #[allow(clippy::arc_with_non_send_sync)]
        Ok(ClientBuilder {
            api_client: self.api_client,
            identity: self.identity,
            identity_strategy: self.identity_strategy,
            scw_verifier: Some(Arc::new(Box::new(RemoteSignatureVerifier::new(api))
                as Box<dyn SmartContractSignatureVerifier>)),
            store: self.store,

            device_sync_server_url: self.device_sync_server_url,
            device_sync_worker_mode: self.device_sync_worker_mode,
            version_info: self.version_info,
            allow_offline: self.allow_offline,
            disable_events: self.disable_events,
            mls_storage: self.mls_storage,
        })
    }
}
