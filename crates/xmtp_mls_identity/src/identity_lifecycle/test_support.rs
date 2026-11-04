//! A registered installation on the local backend and a real database, built
//! as a test client's context is but with no client around it, for identity
//! tests that read private state.

use crate::identity_lifecycle::{IdentityStrategy, generate_and_store_key_package, identity_setup};
use crate::identity_updates::{
    IdentityFeatures, IdentityResolutionRegistry, IdentityUpdateError, IdentityUpdates,
    load_identity_updates, publish_with_conflict_retry,
};
use alloy::signers::local::PrivateKeySigner;
use std::sync::Arc;
use xmtp_api::{ApiClientWrapper, XmtpApi};
use xmtp_common::{MaybeSend, MaybeSync, Retry, retry_async};
use xmtp_configuration::{CREATE_PQ_KEY_PACKAGE_EXTENSION, KEY_PACKAGE_ROTATION_INTERVAL_NS};
use xmtp_configuration::{ConfigProvider, ServerConfiguration, StaticConfigProvider};
use xmtp_cryptography::utils::generate_local_wallet;
use xmtp_db::{
    DefaultStore, StorageError, Store, TransactionOutcome, XmtpTestDb, identity::StoredIdentity,
    prelude::*, sql_key_store::SqlKeyStore,
};
use xmtp_id::{
    InboxOwner, associations::test_utils::MockSmartContractSignatureVerifier,
    scw_verifier::SmartContractSignatureVerifier,
};
use xmtp_mls_core::change_callbacks::UnstableChangeCallbacks;
use xmtp_mls_core::config::{ForkRecoveryOpts, ForkRecoveryPolicy};
use xmtp_mls_core::context::{CoreContext, LocalContext, test_support::TestContextParts};
use xmtp_mls_core::identity::{Identity, IdentityError};
use xmtp_mls_core::policy::StreamPolicy;
use xmtp_mls_core::version::VersionInfo;
use xmtp_proto::api::HasStats;
use xmtp_proto::api_client::{ApiBuilder, ApiStats, XmtpBackendClient, XmtpTestClient};
use xmtp_proto::types::{Cursor, InstallationId, Topic};

/// A peer's backend client: one connection to the local backend.
pub type PeerApi = Arc<xmtp_api_backend::TestClient>;
/// A peer's MLS key store, on its database.
pub type PeerStorage = SqlKeyStore<xmtp_db::DefaultDbConnection>;
/// A peer's context with the feature set `F`, over the backend client `A`.
pub type PeerContext<F, A = PeerApi> = Arc<LocalContext<A, DefaultStore, PeerStorage, F>>;

/// A feature set built for a peer's context.
pub trait PeerFeatures<A>: MaybeSend + MaybeSync + Sized + 'static {
    /// The feature set of a context over `api` with `policy`, whose own
    /// inbox's identity updates are stored through `own_inbox_stored_through`.
    fn new(
        api: &ApiClientWrapper<A>,
        policy: &StreamPolicy,
        own_inbox_stored_through: Option<i64>,
    ) -> Self;

    /// As [`Self::new`], for a context whose backend client only queries:
    /// it opens no streams, as a build over `ClientBuilder::api_client`.
    fn new_query_only(
        api: &ApiClientWrapper<A>,
        policy: &StreamPolicy,
        own_inbox_stored_through: Option<i64>,
    ) -> Self {
        Self::new(api, policy, own_inbox_stored_through)
    }
}

/// Identity resolution only.
pub struct IdentityTestFeatures {
    identity_resolutions: Arc<IdentityResolutionRegistry>,
}

impl IdentityFeatures for IdentityTestFeatures {
    fn identity_resolutions(&self) -> &IdentityResolutionRegistry {
        &self.identity_resolutions
    }
}

impl<A> PeerFeatures<A> for IdentityTestFeatures {
    fn new(
        _: &ApiClientWrapper<A>,
        _: &StreamPolicy,
        own_inbox_stored_through: Option<i64>,
    ) -> Self {
        Self {
            identity_resolutions: Arc::new(IdentityResolutionRegistry::starting_after(
                own_inbox_stored_through,
            )),
        }
    }
}

/// The options a client build applies to a peer's context. Each starts as
/// a test client's build leaves it.
#[derive(Default)]
pub struct PeerOptions {
    /// The deployment's configuration in place of the published one, as a
    /// build's configuration provider supplies it.
    pub config_provider: Option<Arc<dyn ConfigProvider>>,
    /// The client's version.
    pub version_info: VersionInfo,
    /// Fork recovery settings.
    pub fork_recovery_opts: ForkRecoveryOpts,
    /// SDK-registered notifications for group-state changes.
    pub change_callbacks: UnstableChangeCallbacks,
    /// The receiver's limits and waits.
    pub stream_policy: StreamPolicy,
    /// Count intent state changes in the database, as a build with SQLite
    /// triggers enabled does.
    pub triggers: bool,
    /// Open no streams over the backend client, as a build over
    /// `ClientBuilder::api_client` does.
    pub query_only: bool,
}

impl PeerOptions {
    /// Use a deployment configuration that differs from the compiled defaults
    /// only in the fields `edit` touches, as a test client's `configured`
    /// option does.
    pub fn configure(&mut self, edit: impl FnOnce(&mut ServerConfiguration)) {
        self.config_provider = Some(Arc::new(StaticConfigProvider::edited(edit)));
    }

    /// Request fork recovery for every group, as a test client's
    /// `enable_fork_recovery_requests` option does.
    pub fn enable_fork_recovery_requests(&mut self) {
        self.fork_recovery_opts = ForkRecoveryOpts {
            enable_recovery_requests: ForkRecoveryPolicy::All,
            ..ForkRecoveryOpts::default()
        };
    }

    /// Request fork recovery for `groups` only, as a test client's
    /// `enable_fork_recovery_requests_for` option does.
    pub fn enable_fork_recovery_requests_for(&mut self, groups: Vec<String>) {
        self.fork_recovery_opts = ForkRecoveryOpts {
            enable_recovery_requests: ForkRecoveryPolicy::AllowlistedGroups,
            groups_to_request_recovery: groups,
            ..ForkRecoveryOpts::default()
        };
    }
}

/// Where [`TestPeer::open`] finds the installation's identity.
enum Opening {
    /// The identity that `strategy` initializes, as a client built with it
    /// opens it.
    Strategy(IdentityStrategy),
    /// This identity, as a client rebuilt from another one opens it, with
    /// the stored state read offline when `offline`.
    Rebuilt { identity: Identity, offline: bool },
}

/// One installation of an inbox whose owner is `owner`.
pub struct TestPeer<F, A = PeerApi> {
    /// The inbox owner's signer.
    pub owner: PrivateKeySigner,
    /// The installation's context.
    pub context: PeerContext<F, A>,
    config_provider: Option<Arc<dyn ConfigProvider>>,
    query_only: bool,
}

/// A peer with identity resolution only.
pub type IdentityTestPeer = TestPeer<IdentityTestFeatures>;

impl<F: IdentityFeatures + PeerFeatures<PeerApi>> TestPeer<F> {
    /// A registered installation of a new inbox, on an ephemeral database.
    pub async fn new() -> Self {
        Self::new_with(|_| {}).await
    }

    /// As [`Self::new`], with the build options that `edit` sets before the
    /// context opens.
    pub async fn new_with(edit: impl FnOnce(&mut PeerOptions)) -> Self {
        let mut options = PeerOptions::default();
        edit(&mut options);
        Self::register(
            generate_local_wallet(),
            xmtp_db::TestDb::create_ephemeral_store().await,
            options,
        )
        .await
    }

    /// As [`Self::new`], on a database that keeps its rows when the
    /// connection is released and reopened.
    pub async fn persistent() -> Self {
        Self::persistent_with(|_| {}).await
    }

    /// As [`Self::persistent`], with the build options that `edit` sets
    /// before the context opens.
    pub async fn persistent_with(edit: impl FnOnce(&mut PeerOptions)) -> Self {
        let mut options = PeerOptions::default();
        edit(&mut options);
        Self::register(
            generate_local_wallet(),
            xmtp_db::TestDb::create_persistent_store(None).await,
            options,
        )
        .await
    }

    /// The installation that the persistent database at `database` stores,
    /// over a new connection to that file and a query-only backend client,
    /// with the build options that `edit` sets. It opens as a client built
    /// with `strategy` over `ClientBuilder::api_client` with its workers
    /// disabled does: it resolves the deployment's configuration from the
    /// backend, initializes the identity with `strategy`, then loads this
    /// inbox's updates. `owner` is the inbox owner's signer.
    pub async fn open_persistent(
        owner: PrivateKeySigner,
        database: &str,
        strategy: IdentityStrategy,
        edit: impl FnOnce(&mut PeerOptions),
    ) -> Self {
        let mut options = PeerOptions {
            query_only: true,
            ..PeerOptions::default()
        };
        edit(&mut options);
        Self::open(
            owner,
            xmtp_db::TestDb::create_persistent_store(Some(database.into())).await,
            ApiClientWrapper::new(Self::backend(), Retry::default()),
            Box::new(MockSmartContractSignatureVerifier::new(true)),
            Opening::Strategy(strategy),
            options,
        )
        .await
    }

    /// Another registered installation of this peer's inbox.
    pub async fn new_installation(&self) -> Self {
        self.new_installation_with(|_| {}).await
    }

    /// As [`Self::new_installation`], with the build options that `edit`
    /// sets before the context opens.
    pub async fn new_installation_with(&self, edit: impl FnOnce(&mut PeerOptions)) -> Self {
        let mut options = PeerOptions::default();
        edit(&mut options);
        Self::register(
            self.owner.clone(),
            xmtp_db::TestDb::create_ephemeral_store().await,
            options,
        )
        .await
    }

    /// This installation opened from `snapshot`, a copy of its database, with
    /// the build options that `edit` sets. As a test client built from a
    /// snapshot does, it opens offline, publishes the stored identity
    /// updates, drops its identity and network state except the delivery
    /// allocator, rotates its key package and loads its inbox's updates.
    pub async fn from_snapshot(
        &self,
        snapshot: &[u8],
        edit: impl FnOnce(&mut PeerOptions),
    ) -> Self {
        let mut options = PeerOptions::default();
        edit(&mut options);
        let peer = Self::open(
            self.owner.clone(),
            xmtp_db::TestDb::create_ephemeral_store_from_snapshot(snapshot, None::<&str>).await,
            ApiClientWrapper::new(Self::backend(), Retry::default()),
            Box::new(MockSmartContractSignatureVerifier::new(true)),
            Opening::Rebuilt {
                identity: self.context.identity().clone(),
                offline: true,
            },
            options,
        )
        .await;
        peer.refresh_restored_state().await;
        peer
    }

    fn backend() -> PeerApi {
        Arc::new(
            xmtp_api_backend::TestClient::create()
                .build()
                .expect("a local backend client"),
        )
    }

    async fn register(owner: PrivateKeySigner, store: DefaultStore, options: PeerOptions) -> Self {
        let strategy = identity_setup(&owner);
        let peer = Self::open(
            owner,
            store,
            ApiClientWrapper::new(Self::backend(), Retry::default()),
            Box::new(MockSmartContractSignatureVerifier::new(true)),
            Opening::Strategy(strategy),
            options,
        )
        .await;
        peer.register_with_owner().await;
        peer
    }

    async fn refresh_restored_state(&self) {
        use xmtp_db::ConnectionExt;
        let context = &self.context;
        let db = context.db();
        let updates = db
            .get_identity_updates(context.inbox_id(), None, None)
            .expect("the stored identity updates");
        for update in updates {
            let update = update.payload.try_into().expect("a stored identity update");
            if let Err(error) =
                publish_with_conflict_retry(context.api(), &db, update, &context.scw_verifier())
                    .await
            {
                tracing::warn!("{error:?}");
            }
        }
        db.raw_query(|c| {
            use xmtp_db::diesel::{ExpressionMethods, QueryDsl, RunQueryDsl};
            use xmtp_db::schema::{
                association_state, identity_cache, identity_updates, incoming_envelopes,
                refresh_state,
            };
            xmtp_db::diesel::delete(association_state::table).execute(c)?;
            xmtp_db::diesel::delete(identity_cache::table).execute(c)?;
            xmtp_db::diesel::delete(identity_updates::table).execute(c)?;
            xmtp_db::diesel::delete(incoming_envelopes::table).execute(c)?;
            xmtp_db::diesel::delete(
                refresh_state::table.filter(
                    refresh_state::entity_kind
                        .ne(xmtp_db::refresh_state::EntityKind::DeliveryAllocator),
                ),
            )
            .execute(c)?;
            Ok(())
        })
        .expect("the dropped identity and network state");
        super::rotate_and_upload_key_package(
            context.identity(),
            context.api(),
            context.mls_storage(),
            CREATE_PQ_KEY_PACKAGE_EXTENSION,
        )
        .await
        .expect("a rotated key package");
        crate::key_package_maintenance::nudge_deletion(context)
            .expect("the key package deletion task");
        load_identity_updates(context.api(), &db, &[context.inbox_id()])
            .await
            .expect("the inbox's identity updates");
    }

    /// The backend calls this installation's connection made.
    pub fn api_stats(&self) -> ApiStats {
        self.context.api().api_client.mls_stats()
    }

    /// This installation with the build options that `edit` sets, starting
    /// from the ones its context holds, over the same database and backend
    /// connection, as a client rebuilt from this one opens.
    pub async fn with_options(&self, edit: impl FnOnce(&mut PeerOptions)) -> Self {
        self.rebuild(
            self.context.api().api_client.raw_for_test().clone(),
            None,
            false,
            edit,
        )
        .await
    }

    /// This installation with the stream policy that `edit` makes, over the
    /// same database and backend connection, opened from the stored state as
    /// an offline client build opens it.
    pub async fn with_stream_policy(&self, edit: impl FnOnce(&mut StreamPolicy)) -> Self {
        self.rebuild(
            self.context.api().api_client.raw_for_test().clone(),
            None,
            true,
            |options| edit(&mut options.stream_policy),
        )
        .await
    }
}

impl<F, A> TestPeer<F, A>
where
    A: XmtpApi + 'static,
    F: MaybeSend + MaybeSync + 'static,
{
    /// This installation over `api`, with `verifier` when given and the
    /// build options that `edit` sets, starting from the ones it was opened
    /// with, opened from the stored state as an offline client build opens it.
    pub async fn with_api<B, G>(
        &self,
        api: B,
        verifier: Option<Box<dyn SmartContractSignatureVerifier>>,
        edit: impl FnOnce(&mut PeerOptions),
    ) -> TestPeer<G, B>
    where
        B: XmtpApi + 'static,
        G: PeerFeatures<B>,
    {
        self.rebuild(api, verifier, true, edit).await
    }

    /// This installation over `api` and the same database, as a client
    /// rebuilt from this one opens it: with the stored identity and the build
    /// options that `edit` sets, starting from the ones it was opened with.
    /// Unless `offline`, it then loads this inbox's updates.
    async fn rebuild<B, G>(
        &self,
        api: B,
        verifier: Option<Box<dyn SmartContractSignatureVerifier>>,
        offline: bool,
        edit: impl FnOnce(&mut PeerOptions),
    ) -> TestPeer<G, B>
    where
        B: XmtpApi + 'static,
        G: PeerFeatures<B>,
    {
        let mut options = self.options();
        edit(&mut options);
        TestPeer::open(
            self.owner.clone(),
            self.context.store().clone(),
            ApiClientWrapper::new(api, Retry::default()),
            verifier.unwrap_or_else(|| Box::new(MockSmartContractSignatureVerifier::new(true))),
            Opening::Rebuilt {
                identity: self.context.identity().clone(),
                offline,
            },
            options,
        )
        .await
    }

    /// The build options this installation's context holds.
    fn options(&self) -> PeerOptions {
        PeerOptions {
            config_provider: self.config_provider.clone(),
            version_info: self.context.version_info.clone(),
            fork_recovery_opts: self.context.fork_recovery_opts.clone(),
            change_callbacks: self.context.change_callbacks.clone(),
            stream_policy: self.context.stream_policy().clone(),
            triggers: false,
            query_only: self.query_only,
        }
    }

    /// This installation's inbox ID.
    pub fn inbox_id(&self) -> &str {
        self.context.inbox_id()
    }

    /// This installation's public key.
    pub fn installation_public_key(&self) -> InstallationId {
        self.context.installation_id()
    }
}

impl<F, A> TestPeer<F, A>
where
    A: XmtpApi + 'static,
    F: PeerFeatures<A>,
{
    /// Open a context as a client build with its workers disabled and
    /// `options` does: the deployment's configuration, then the identity
    /// that `opening` names, then this inbox's updates unless the build
    /// is offline.
    async fn open(
        owner: PrivateKeySigner,
        store: DefaultStore,
        mut api: ApiClientWrapper<A>,
        verifier: Box<dyn SmartContractSignatureVerifier>,
        opening: Opening,
        options: PeerOptions,
    ) -> Self {
        if options.triggers {
            let db = xmtp_db::DbConnection::new(store.conn());
            db.register_triggers();
            db.disable_memory_security();
        }
        let conn = store.db();
        let mls_storage = SqlKeyStore::new(store.db());
        let offline = matches!(opening, Opening::Rebuilt { offline: true, .. });
        let config_provider = options.config_provider.clone();
        let server_configuration = match options.config_provider {
            Some(provider) => {
                xmtp_mls_server_config::server_configuration::ServerConfigurationHandle::new(
                    provider,
                )
            }
            None => xmtp_mls_server_config::server_configuration::resolve(&api, &conn, offline)
                .await
                .expect("the deployment's configuration"),
        };
        let configuration = server_configuration.configuration();
        xmtp_mls_server_config::server_configuration::check_minimum_version(
            configuration,
            options.version_info.pkg_semver().semver(),
        )
        .expect("a deployment that admits this client");
        assert!(
            !configuration.auth.enabled || api.has_credential_source(),
            "a deployment that needs no credential"
        );
        let snapshot = Arc::new(configuration.clone());
        api.api_client.set_limits(Arc::new(snapshot.limits.clone()));
        api.set_configuration(snapshot);
        let identity = match opening {
            Opening::Rebuilt { identity, offline } => {
                if !offline {
                    load_identity_updates(&api, &conn, &[identity.inbox_id.as_str()])
                        .await
                        .expect("the inbox's identity updates");
                }
                identity
            }
            Opening::Strategy(strategy) => {
                let identity = strategy
                    .initialize_identity(&api, &mls_storage, &verifier)
                    .await
                    .expect("an identity");
                load_identity_updates(&api, &conn, &[identity.inbox_id.as_str()])
                    .await
                    .expect("the inbox's identity updates");
                identity
            }
        };
        let own_inbox_stored_through = conn
            .get_latest_sequence_id(&[identity.inbox_id.as_str()])
            .expect("the inbox's stored updates")
            .get(identity.inbox_id.as_str())
            .copied();
        let features = if options.query_only {
            F::new_query_only(&api, &options.stream_policy, own_inbox_stored_through)
        } else {
            F::new(&api, &options.stream_policy, own_inbox_stored_through)
        };
        let mut context = TestContextParts {
            identity,
            api_client: api,
            store,
            mls_storage,
            scw_verifier: verifier,
            server_configuration,
            stream_policy: options.stream_policy,
        }
        .into_context(features);
        context.version_info = options.version_info;
        context.fork_recovery_opts = options.fork_recovery_opts;
        context.change_callbacks = options.change_callbacks;
        let context = Arc::new(context);
        xmtp_mls_core::preflight::bind_preflight(&context);
        Self {
            owner,
            context,
            config_provider,
            query_only: options.query_only,
        }
    }
}

impl<F> TestPeer<F>
where
    F: IdentityFeatures + PeerFeatures<PeerApi>,
{
    /// The steps of `Client::register_with_owner`: sign the registration,
    /// upload the key package, publish the identity update with the
    /// own-installation event baseline, record the receipt, then wait until
    /// the backend serves it.
    async fn register_with_owner(&self) {
        let context = &self.context;
        let mut request = context.signature_request().expect("a registration to sign");
        let signature = self
            .owner
            .sign(&request.signature_text())
            .expect("the owner's signature");
        request
            .add_signature(signature, context.scw_verifier())
            .await
            .expect("an accepted signature");
        let (key_package, history_id) = generate_and_store_key_package(
            context.identity(),
            context.mls_storage(),
            CREATE_PQ_KEY_PACKAGE_EXTENSION,
        )
        .expect("a stored key package");
        let update = request.build_identity_update().expect("an identity update");
        update
            .to_verified(&context.scw_verifier())
            .await
            .expect("a valid identity update");
        let key_package_cursor = context
            .api()
            .upload_key_package(key_package)
            .await
            .expect("an uploaded key package")
            .cursor
            .filter(|cursor| cursor.sequence_id > 0)
            .expect("the key package's cursor");
        let inbox_id = context.inbox_id().to_string();
        let registration = IdentityUpdates::new(context)
            .with_own_installation_events(
                &context.db(),
                Box::pin(async {
                    let registration = publish_with_conflict_retry(
                        context.api(),
                        &context.db(),
                        update,
                        &context.scw_verifier(),
                    )
                    .await
                    .map_err(|error| IdentityUpdateError::Publish(Box::new(error)))?;
                    retry_async!(
                        Retry::default(),
                        (async {
                            load_identity_updates(
                                context.api(),
                                &context.db(),
                                &[inbox_id.as_str()],
                            )
                            .await
                        })
                    )?;
                    Ok(registration)
                }),
            )
            .await
            .expect("a published registration");
        xmtp_mls_core::state_tx::state_write(context.mls_storage(), |tx| {
            let storage = tx.storage();
            storage.db().record_key_package_publication(
                history_id,
                Cursor(key_package_cursor.sequence_id),
            )?;
            storage
                .db()
                .reset_key_package_rotation_queue(KEY_PACKAGE_ROTATION_INTERVAL_NS)?;
            Ok::<_, StorageError>(TransactionOutcome::Continue(()))
        })
        .expect("the key package's receipt");
        let mut stored = StoredIdentity::try_from(context.identity()).expect("a stored identity");
        stored.registration_cursor_sequence_id = Some(registration.0 as i64);
        stored.store(&context.db()).expect("the stored identity");
        context.identity().set_ready();
        let topic = Topic::new_identity_update(hex::decode(&inbox_id).expect("a hex inbox id"));
        xmtp_common::wait_for_ge(
            || async {
                context
                    .api()
                    .newest_topic_cursors(vec![topic.clone()])
                    .await
                    .ok()
                    .and_then(|heads| heads.get(&topic).map(|head| head.0))
                    .unwrap_or_default()
            },
            registration.0,
        )
        .await
        .expect("the registration to become visible");
        context
            .db()
            .clear_registration_cursor(registration.0 as i64)
            .expect("the cleared receipt");
    }
}

/// Rotates and uploads an installation's key package as the client does,
/// with the post-quantum wrapper key forced on or off instead of taken from
/// the build's default.
pub async fn rotate_and_upload_key_package<A: XmtpApi, S: XmtpMlsStorageProvider>(
    identity: &Identity,
    api: &ApiClientWrapper<A>,
    storage: &S,
    include_post_quantum: bool,
) -> Result<(), IdentityError> {
    super::rotate_and_upload_key_package(identity, api, storage, include_post_quantum).await
}
