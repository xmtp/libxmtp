//! A registered installation on the local backend and a real database, built
//! as a test client's context is but with no client around it, for identity
//! tests that read private state.

use crate::context::{CoreContext, LocalContext, test_support::TestContextParts};
use crate::core::policy::StreamPolicy;
use crate::identity::Identity;
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
use xmtp_cryptography::utils::generate_local_wallet;
use xmtp_db::{
    DefaultStore, StorageError, Store, TransactionOutcome, XmtpTestDb, identity::StoredIdentity,
    prelude::*, sql_key_store::SqlKeyStore,
};
use xmtp_id::{
    InboxOwner, associations::test_utils::MockSmartContractSignatureVerifier,
    scw_verifier::SmartContractSignatureVerifier,
};
use xmtp_proto::api::HasStats;
use xmtp_proto::api_client::{ApiBuilder, ApiStats, XmtpBackendClient, XmtpTestClient};
use xmtp_proto::types::{Cursor, InstallationId, Topic};

/// A peer's backend client: one connection to the local backend.
pub(crate) type PeerApi = Arc<xmtp_api_backend::TestClient>;
pub(crate) type PeerStorage = SqlKeyStore<xmtp_db::DefaultDbConnection>;
pub(crate) type PeerContext<F, A = PeerApi> = Arc<LocalContext<A, DefaultStore, PeerStorage, F>>;

/// A feature set built for a peer's context.
pub(crate) trait PeerFeatures<A>: MaybeSend + MaybeSync + Sized + 'static {
    fn new(
        api: &ApiClientWrapper<A>,
        policy: &StreamPolicy,
        own_inbox_stored_through: Option<i64>,
    ) -> Self;
}

/// Identity resolution only.
pub(crate) struct IdentityTestFeatures {
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

/// One installation of an inbox whose owner is `owner`.
pub(crate) struct TestPeer<F, A = PeerApi> {
    pub(crate) owner: PrivateKeySigner,
    pub(crate) context: PeerContext<F, A>,
}

pub(crate) type IdentityTestPeer = TestPeer<IdentityTestFeatures>;

impl<F: IdentityFeatures + PeerFeatures<PeerApi>> TestPeer<F> {
    /// A registered installation of a new inbox, on an ephemeral database.
    pub(crate) async fn new() -> Self {
        Self::register(
            generate_local_wallet(),
            xmtp_db::TestDb::create_ephemeral_store().await,
        )
        .await
    }

    /// As [`Self::new`], on a database that keeps its rows when the
    /// connection is released and reopened.
    pub(crate) async fn persistent() -> Self {
        Self::register(
            generate_local_wallet(),
            xmtp_db::TestDb::create_persistent_store(None).await,
        )
        .await
    }

    /// Another registered installation of this peer's inbox.
    pub(crate) async fn new_installation(&self) -> Self {
        Self::register(
            self.owner.clone(),
            xmtp_db::TestDb::create_ephemeral_store().await,
        )
        .await
    }

    async fn register(owner: PrivateKeySigner, store: DefaultStore) -> Self {
        let api = Arc::new(
            xmtp_api_backend::TestClient::create()
                .build()
                .expect("a local backend client"),
        );
        let peer = Self::open(
            owner,
            store,
            ApiClientWrapper::new(api, Retry::default()),
            StreamPolicy::default(),
            Box::new(MockSmartContractSignatureVerifier::new(true)),
            None,
        )
        .await;
        peer.register_with_owner().await;
        peer
    }

    pub(crate) fn api_stats(&self) -> ApiStats {
        self.context.api().api_client.mls_stats()
    }

    /// This installation with the stream policy that `edit` makes, over the
    /// same database and backend connection.
    pub(crate) async fn with_stream_policy(&self, edit: impl FnOnce(&mut StreamPolicy)) -> Self {
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
    pub(crate) async fn with_api<B, G>(
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
        )
        .await
    }

    pub(crate) fn inbox_id(&self) -> &str {
        self.context.inbox_id()
    }

    pub(crate) fn installation_public_key(&self) -> InstallationId {
        self.context.installation_id()
    }
}

impl<F, A> TestPeer<F, A>
where
    A: XmtpApi + 'static,
    F: PeerFeatures<A>,
{
    /// Open a context as a client build with its workers disabled does: the
    /// deployment's configuration, then the identity, then this inbox's
    /// updates, unless `identity` is the stored one.
    async fn open(
        owner: PrivateKeySigner,
        store: DefaultStore,
        mut api: ApiClientWrapper<A>,
        policy: StreamPolicy,
        verifier: Box<dyn SmartContractSignatureVerifier>,
        identity: Option<Identity>,
    ) -> Self {
        let conn = store.db();
        let mls_storage = SqlKeyStore::new(store.db());
        let offline = identity.is_some();
        let server_configuration = crate::server_configuration::resolve(&api, &conn, offline)
            .await
            .expect("the deployment's configuration");
        let snapshot = Arc::new(server_configuration.configuration().clone());
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
        let context = Arc::new(
            TestContextParts {
                identity,
                api_client: api,
                store,
                mls_storage,
                scw_verifier: verifier,
                server_configuration,
                stream_policy: policy,
            }
            .into_context(features),
        );
        crate::core::preflight::bind_preflight(&context);
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
        crate::state_tx::state_write(context.mls_storage(), |tx| {
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
