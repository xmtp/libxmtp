//! Real contexts for core tests: a connection to the local backend, an
//! ephemeral database and new key material, with no client around them.

use crate::context::{
    LocalContext,
    test_support::{TestContextParts, mock_identity},
};
use crate::preflight::bind_preflight;
use std::sync::Arc;
use xmtp_api::ApiClientWrapper;
use xmtp_common::Retry;
use xmtp_db::group_message::{ContentType, DeliveryStatus, GroupMessageKind, StoredGroupMessage};
use xmtp_db::sql_key_store::SqlKeyStore;
use xmtp_db::{DefaultDbConnection, DefaultStore, TestDb, XmtpTestDb};
use xmtp_id::associations::test_utils::MockSmartContractSignatureVerifier;
use xmtp_mls_server_config::server_configuration::ServerConfigurationHandle;
use xmtp_proto::api_client::{ApiBuilder, XmtpBackendClient, XmtpTestClient};
use xmtp_proto::types::{Cursor, GroupId};

/// A core test's context. Its identity is only key material: nothing is
/// registered on the backend.
pub(crate) type RealContext = Arc<
    LocalContext<
        Arc<xmtp_api_backend::TestClient>,
        DefaultStore,
        SqlKeyStore<DefaultDbConnection>,
        (),
    >,
>;

/// A context on the local backend and a new ephemeral database, holding the
/// compiled configuration.
pub(crate) async fn real_context() -> RealContext {
    let api = xmtp_api_backend::TestClient::create()
        .build()
        .expect("a local backend client");
    real_context_with(
        Arc::new(api),
        TestDb::create_ephemeral_store().await,
        ServerConfigurationHandle::default(),
    )
}

/// A context over `api` and `store` that holds `handle`, as a client
/// build with its workers disabled opens it.
pub(crate) fn real_context_with(
    api: Arc<xmtp_api_backend::TestClient>,
    store: DefaultStore,
    handle: ServerConfigurationHandle,
) -> RealContext {
    let mut api = ApiClientWrapper::new(api, Retry::default());
    let snapshot = Arc::new(handle.configuration().clone());
    api.api_client.set_limits(Arc::new(snapshot.limits.clone()));
    api.set_configuration(snapshot);
    let mls_storage = SqlKeyStore::new(store.db());
    let context = Arc::new(
        TestContextParts {
            identity: mock_identity(),
            api_client: api,
            store,
            mls_storage,
            scw_verifier: Box::new(MockSmartContractSignatureVerifier::new(true)),
            server_configuration: handle,
            stream_policy: Default::default(),
        }
        .into_context(()),
    );
    bind_preflight(&context);
    context
}

/// A published application message in `group_id` at `cursor`, from another
/// sender.
pub(crate) fn stored_msg(cursor: Cursor, group_id: GroupId) -> StoredGroupMessage {
    StoredGroupMessage {
        id: xmtp_common::rand_vec::<32>(),
        group_id,
        decrypted_message_bytes: b"test message".to_vec(),
        sent_at_ns: 100,
        kind: GroupMessageKind::Application,
        sender_installation_id: xmtp_common::rand_vec::<32>(),
        sender_inbox_id: "test inbox".into(),
        delivery_status: DeliveryStatus::Published,
        content_type: ContentType::Text,
        version_major: 0,
        version_minor: 0,
        authority_id: "testauthority".to_string(),
        reference_id: None,
        sequence_id: cursor.0 as i64,
        envelope_hash: None,
        expiry_ns: None,
        expiry_unknown: false,
        deleted_by_inbox_id: None,
        expire_at_ns: None,
        inserted_at_ns: 0,
        should_push: true,
        idempotency_key: 100.to_string(),
    }
}
