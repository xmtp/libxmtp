use std::sync::{atomic::Ordering, Arc};

use thiserror::Error;
use tokio::sync::broadcast;
use tracing::debug;
use xmtp_db::events::{Event, Events, EVENTS_ENABLED};

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
    utils::{events::EventWorker, VersionInfo},
    worker::WorkerRunner,
    GroupCommitLock, StorageError, XmtpApi, XmtpOpenMlsProvider,
};
use xmtp_api::{ApiClientWrapper, ApiDebugWrapper};
use xmtp_common::Retry;
use xmtp_cryptography::signature::IdentifierValidationError;
use xmtp_id::scw_verifier::RemoteSignatureVerifier;
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;

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

pub struct ClientBuilder<ApiClient, Db = xmtp_db::DefaultStore> {
    api_client: Option<ApiClientWrapper<ApiClient>>,
    identity: Option<Identity>,
    store: Option<Db>,
    identity_strategy: IdentityStrategy,
    scw_verifier: Option<Arc<Box<dyn SmartContractSignatureVerifier>>>,
    device_sync_server_url: Option<String>,
    device_sync_worker_mode: SyncWorkerMode,
    version_info: VersionInfo,
    disable_local_telemetry: bool,
}

#[derive(Clone, Copy, Debug)]
pub enum SyncWorkerMode {
    Disabled,
    Enabled,
}

impl Client<()> {
    /// Get the builder for this [`Client`]
    pub fn builder(strategy: IdentityStrategy) -> ClientBuilder<()> {
        ClientBuilder::<()>::new(strategy)
    }
}

impl<ApiClient, Db> ClientBuilder<ApiClient, Db> {
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
            disable_local_telemetry: false,
        }
    }
}

#[cfg(test)]
impl<ApiClient, Db> ClientBuilder<ApiClient, Db>
where
    ApiClient: Clone,
    Db: Clone,
{
    pub fn from_client(client: Client<ApiClient, Db>) -> ClientBuilder<ApiClient, Db> {
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
            disable_local_telemetry: false,
        }
    }
}

impl<ApiClient, Db> ClientBuilder<ApiClient, Db> {
    pub async fn build(self) -> Result<Client<ApiClient, Db>, ClientBuilderError>
    where
        ApiClient: XmtpApi + 'static + Send + Sync,
        Db: xmtp_db::XmtpDb + 'static + Send + Sync,
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
            disable_local_telemetry: disable_events,
        } = self;

        if disable_events {
            EVENTS_ENABLED.store(false, Ordering::SeqCst);
        }

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

        let conn = store.conn();
        let provider = XmtpOpenMlsProvider::new(conn);
        let identity = if let Some(identity) = identity {
            identity
        } else {
            identity_strategy
                .initialize_identity(&api_client, &provider, &scw_verifier)
                .await?
        };

        debug!(
            inbox_id = identity.inbox_id(),
            installation_id = hex::encode(identity.installation_keys.public_bytes()),
            "Initialized identity"
        );
        // get sequence_id from identity updates and loaded into the DB
        load_identity_updates(
            &api_client,
            provider.db(),
            vec![identity.inbox_id.as_str()].as_slice(),
        )
        .await?;

        let (tx, _) = broadcast::channel(32);
        let context = Arc::new(XmtpMlsLocalContext {
            identity,
            store,
            api_client,
            version_info,
            scw_verifier,
            mutexes: MutexRegistry::new(),
            mls_commit_lock: Arc::new(GroupCommitLock::new()),
            local_events: tx.clone(),
            device_sync: DeviceSync {
                server_url: device_sync_server_url,
                mode: device_sync_worker_mode,
            },
            workers: Arc::new(parking_lot::Mutex::default()),
        });

        let client = Client {
            context,
            local_events: tx,
        };

        // register workers
        if client.device_sync_worker_enabled() {
            WorkerRunner::register_new_worker(&client.context, {
                let context = client.context.clone();
                move || SyncWorker::new(&context)
            });
        }
        WorkerRunner::register_new_worker(&client.context, {
            let context = client.context.clone();
            move || EventWorker::new(&context)
        });
        WorkerRunner::register_new_worker(&client.context, {
            let client = client.clone();
            move || KeyPackagesCleanerWorker::new(client.clone())
        });
        WorkerRunner::register_new_worker(&client.context, {
            let client = client.clone();
            move || DisappearingMessagesWorker::new(client.clone())
        });

        Events::track(provider.db(), None, Event::ClientBuild, ());

        Ok(client)
    }

    pub fn identity(self, identity: Identity) -> Self {
        Self {
            identity: Some(identity),
            ..self
        }
    }

    pub fn store<NewDb>(self, db: NewDb) -> ClientBuilder<ApiClient, NewDb> {
        ClientBuilder {
            store: Some(db),
            api_client: self.api_client,
            identity: self.identity,
            identity_strategy: self.identity_strategy,
            scw_verifier: self.scw_verifier,
            device_sync_server_url: self.device_sync_server_url,
            device_sync_worker_mode: self.device_sync_worker_mode,
            version_info: self.version_info,
            disable_local_telemetry: self.disable_local_telemetry,
        }
    }

    pub fn device_sync_server_url(self, url: &str) -> Self {
        Self {
            device_sync_server_url: Some(url.into()),
            ..self
        }
    }

    pub fn disable_local_telemetry(self) -> Self {
        Self {
            disable_local_telemetry: true,
            ..self
        }
    }

    pub fn device_sync_worker_mode(self, mode: SyncWorkerMode) -> Self {
        Self {
            device_sync_worker_mode: mode,
            ..self
        }
    }

    pub fn api_client<A>(self, api_client: A) -> ClientBuilder<A, Db> {
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
            disable_local_telemetry: self.disable_local_telemetry,
        }
    }

    pub fn version(self, version_info: VersionInfo) -> ClientBuilder<ApiClient, Db> {
        ClientBuilder {
            version_info,
            ..self
        }
    }

    /// Wrap the Api Client in a Debug Adapter which prints api stats on error.
    /// Requires the api client to be set in the builder.
    pub fn enable_api_debug_wrapper(
        self,
    ) -> Result<ClientBuilder<ApiDebugWrapper<ApiClient>, Db>, ClientBuilderError> {
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
            disable_local_telemetry: self.disable_local_telemetry,
        })
    }

    pub fn with_scw_verifier(
        self,
        verifier: impl SmartContractSignatureVerifier + 'static,
    ) -> ClientBuilder<ApiClient, Db> {
        ClientBuilder {
            api_client: self.api_client,
            identity: self.identity,
            identity_strategy: self.identity_strategy,
            scw_verifier: Some(Arc::new(Box::new(verifier))),
            store: self.store,

            device_sync_server_url: self.device_sync_server_url,
            device_sync_worker_mode: self.device_sync_worker_mode,
            version_info: self.version_info,
            disable_local_telemetry: self.disable_local_telemetry,
        }
    }

    /// Build the client with a default remote verifier
    /// requires the 'api' to be set.
    pub fn with_remote_verifier(self) -> Result<ClientBuilder<ApiClient, Db>, ClientBuilderError>
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
            disable_local_telemetry: self.disable_local_telemetry,
        })
    }
}
