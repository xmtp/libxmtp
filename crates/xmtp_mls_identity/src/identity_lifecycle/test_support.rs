//! A registered installation on the local backend and a real database, built
//! as a test client's context is but with no client around it, for identity
//! tests that read private state.

use crate::identity_lifecycle::{generate_and_store_key_package, identity_setup};
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
use xmtp_mls_core::config::ForkRecoveryOpts;
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
}

impl PeerOptions {
    /// Use a deployment configuration that differs from the compiled defaults
    /// only in the fields `edit` touches, as a test client's `configured`
    /// option does.
    pub fn configure(&mut self, edit: impl FnOnce(&mut ServerConfiguration)) {
        self.config_provider = Some(Arc::new(StaticConfigProvider::edited(edit)));
    }
}

/// One installation of an inbox whose owner is `owner`.
pub struct TestPeer<F, A = PeerApi> {
    /// The inbox owner's signer.
    pub owner: PrivateKeySigner,
    /// The installation's context.
    pub context: PeerContext<F, A>,
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
        Self::register(
            generate_local_wallet(),
            xmtp_db::TestDb::create_persistent_store(None).await,
            PeerOptions::default(),
        )
        .await
    }

    /// Another registered installation of this peer's inbox.
    pub async fn new_installation(&self) -> Self {
        Self::register(
            self.owner.clone(),
            xmtp_db::TestDb::create_ephemeral_store().await,
            PeerOptions::default(),
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
            StreamPolicy::default(),
            Box::new(MockSmartContractSignatureVerifier::new(true)),
            Some(self.context.identity().clone()),
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
        let peer = Self::open(
            owner,
            store,
            ApiClientWrapper::new(Self::backend(), Retry::default()),
            StreamPolicy::default(),
            Box::new(MockSmartContractSignatureVerifier::new(true)),
            None,
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

    /// This installation with the stream policy that `edit` makes, over the
    /// same database and backend connection.
    pub async fn with_stream_policy(&self, edit: impl FnOnce(&mut StreamPolicy)) -> Self {
        let mut policy = self.context.stream_policy().clone();
        edit(&mut policy);
        self.with_api(
            self.context.api().api_client.raw_for_test().clone(),
            policy,
            None,
        )
        .await
    }
}

impl<F, A> TestPeer<F, A>
where
    A: XmtpApi + 'static,
    F: MaybeSend + MaybeSync + 'static,
{
    /// This installation over `api`, with `policy` and, when given,
    /// `verifier`, opened from the stored state as an offline client build
    /// opens it.
    pub async fn with_api<B, G>(
        &self,
        api: B,
        policy: StreamPolicy,
        verifier: Option<Box<dyn SmartContractSignatureVerifier>>,
    ) -> TestPeer<G, B>
    where
        B: XmtpApi + 'static,
        G: PeerFeatures<B>,
    {
        TestPeer::open(
            self.owner.clone(),
            self.context.store().clone(),
            ApiClientWrapper::new(api, Retry::default()),
            policy,
            verifier.unwrap_or_else(|| Box::new(MockSmartContractSignatureVerifier::new(true))),
            Some(self.context.identity().clone()),
            PeerOptions::default(),
        )
        .await
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
    /// `options` does: the deployment's configuration, then the identity,
    /// then this inbox's updates, unless `identity` is the stored one.
    async fn open(
        owner: PrivateKeySigner,
        store: DefaultStore,
        mut api: ApiClientWrapper<A>,
        policy: StreamPolicy,
        verifier: Box<dyn SmartContractSignatureVerifier>,
        identity: Option<Identity>,
        options: PeerOptions,
    ) -> Self {
        let conn = store.db();
        let mls_storage = SqlKeyStore::new(store.db());
        let offline = identity.is_some();
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
        let identity = match identity {
            Some(identity) => identity,
            None => {
                let identity = identity_setup(&owner)
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
        let features = F::new(&api, &policy, own_inbox_stored_through);
        let mut context = TestContextParts {
            identity,
            api_client: api,
            store,
            mls_storage,
            scw_verifier: verifier,
            server_configuration,
            stream_policy: policy,
        }
        .into_context(features);
        context.version_info = options.version_info;
        context.fork_recovery_opts = options.fork_recovery_opts;
        context.change_callbacks = options.change_callbacks;
        let context = Arc::new(context);
        xmtp_mls_core::preflight::bind_preflight(&context);
        Self { owner, context }
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
