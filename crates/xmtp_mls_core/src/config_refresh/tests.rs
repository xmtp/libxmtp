//! The configuration worker's refresh turn: what it stores, what it blocks,
//! and what the running and next contexts see.

use crate::context::CoreContext;
use crate::core::config_refresh::ConfigurationWorker;
use crate::core::test_context::real_context_with;
use crate::server_configuration::{
    BlockedConnection, ServerConfigurationHandle, test_support::resolve,
};
use std::sync::Arc;
use xmtp_api::ApiClientWrapper;
use xmtp_api_backend::{BackendClient, TrackedStatsClient};
use xmtp_backend::test_support::TestServer;
use xmtp_common::Retry;
use xmtp_db::prelude::*;
use xmtp_db::{TestDb, XmtpTestDb};
use xmtp_proto::api::ToBoxedClient;

/// A deployment that publishes values no compiled default would produce.
const DISTINCT: &str = r#"
[server]
identifier = "org.example.distinct"

[limits]
max_query_topics = 17

[mls]
max_group_members = 23

[handles]
min_name_length = 5

[chains]
"eip155:1" = "http://127.0.0.1:8545"
"eip155:8453" = "http://127.0.0.1:8545"

[[application_components]]
component_id = 0xC001
name = "USER_PRONOUNS"
component_type = "tls_map_inbox_id_string"
insert_policy = "allow_if_self_or_non_member"
update_policy = "allow_if_self_or_non_member"
delete_policy = "allow_if_admin"
in_groups = true
in_dms = true

[[application_components]]
component_id = 0xC000
name = "topic"
component_type = "string"
insert_policy = "allow"
update_policy = "allow_if_admin"
delete_policy = "deny"
in_groups = true
in_dms = false
"#;

/// The catalogue `DISTINCT` publishes, as `(component_id, name)` in the
/// published order.
fn catalogue(configuration: &xmtp_configuration::ServerConfiguration) -> Vec<(u16, &str)> {
    configuration
        .application_components
        .iter()
        .map(|definition| (definition.component_id, definition.name.as_str()))
        .collect()
}

const DISTINCT_CATALOGUE: [(u16, &str); 2] = [(0xC000, "topic"), (0xC001, "USER_PRONOUNS")];

fn api_at(url: &str) -> xmtp_api_backend::TestClient {
    let transport = xmtp_api_grpc::GrpcClient::create(url.parse().unwrap())
        .unwrap()
        .arced();
    TrackedStatsClient::new(BackendClient::new(transport))
}

/// A wrapper pointed at an address nothing is listening on, so any network
/// read fails rather than quietly succeeding.
fn unreachable_api() -> xmtp_api::ApiClientWrapper<Arc<xmtp_api_backend::TestClient>> {
    xmtp_api::ApiClientWrapper::new(Arc::new(api_at("http://127.0.0.1:1")), Default::default())
}

// A refresh stores a changed catalogue, but the running client keeps the
// one it started with; the next client on the database gets the new one.
// verifies: CONF-020, CONF-040
#[xmtp_common::test(unwrap_try = true)]
async fn a_refreshed_catalogue_reaches_only_the_next_client() {
    let before = TestServer::from_toml(DISTINCT).await?;
    let after =
        TestServer::from_toml(&DISTINCT.replace(r#"name = "topic""#, r#"name = "mood""#)).await?;
    let store = TestDb::create_ephemeral_store().await;
    resolve(
        &ApiClientWrapper::new(Arc::new(api_at(&before.url)), Retry::default()),
        &store.db(),
        false,
    )
    .await?;
    before.stop().await?;
    // The deployment now at this URL has since renamed a field.
    let stored = store.db().server_configuration()?.unwrap();
    store.db().store_server_configuration(
        &stored.identifier,
        &after.url,
        &stored.response,
        stored.fetched_at_ns,
    )?;

    let api = Arc::new(api_at(&after.url));
    let handle = resolve(
        &ApiClientWrapper::new(api.clone(), Retry::default()),
        &store.db(),
        false,
    )
    .await?;
    let context = real_context_with(api, store.clone(), handle);
    assert_eq!(
        catalogue(context.server_configuration().configuration()),
        DISTINCT_CATALOGUE
    );
    ConfigurationWorker::new(context.clone()).tick().await;

    let renamed = [(0xC000, "mood"), (0xC001, "USER_PRONOUNS")];
    let stored = store.db().server_configuration()?.unwrap();
    let stored = xmtp_configuration::ServerConfiguration::from(
        <xmtp_proto::backend_v1::GetConfigurationResponse as prost::Message>::decode(
            stored.response.as_slice(),
        )?,
    );
    assert_eq!(catalogue(&stored), renamed);
    assert_eq!(
        catalogue(context.server_configuration().configuration()),
        DISTINCT_CATALOGUE
    );
    let handle = resolve(&unreachable_api(), &store.db(), true).await?;
    assert_eq!(catalogue(handle.configuration()), renamed);
    after.stop().await?;
}

// A refresh that meets a different deployment blocks the connection
// the client and records the conflict, rather than replacing the copy.
// verifies: CONF-030
#[xmtp_common::test(unwrap_try = true)]
async fn a_refresh_that_meets_another_deployment_blocks_the_connection() {
    let backend = TestServer::from_toml(DISTINCT).await?;

    let store = TestDb::create_ephemeral_store().await;
    let context = real_context_with(
        Arc::new(api_at(&backend.url)),
        store.clone(),
        ServerConfigurationHandle::new(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
            |_| {},
        ))),
    );

    // A copy this database was bound to by an earlier, different deployment.
    store.db().store_server_configuration(
        "org.example.elsewhere",
        &backend.url,
        &[],
        xmtp_common::time::now_ns(),
    )?;

    let mut worker = ConfigurationWorker::new(context.clone());
    worker.tick().await;

    // The conflict is recorded, and the bound identifier is kept.
    let stored = store.db().server_configuration()?.unwrap();
    assert_eq!(stored.identifier, "org.example.elsewhere");
    assert_eq!(
        stored.conflicting_identifier,
        Some("org.example.distinct".to_owned())
    );

    // The connection is blocked on the mismatch.
    let blocked_connection = context.server_configuration().blocked_connection().unwrap();
    assert!(
        matches!(
            blocked_connection,
            BlockedConnection::BackendMismatch { .. }
        ),
        "expected a mismatch block, got {blocked_connection:?}"
    );

    backend.stop().await?;
}
