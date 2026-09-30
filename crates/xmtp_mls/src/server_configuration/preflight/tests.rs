//! Real builder and SQLite tests with a counted, pausable transport.
use super::*;
use crate::{Client, identity::IdentityStrategy};
use parking_lot::Mutex;
use prost::Message;
use std::collections::VecDeque;
use tokio::sync::oneshot;
use xmtp_common::RetryableError;
use xmtp_db::{ConnectionExt, TestDb, XmtpDb, XmtpTestDb, prelude::*};
use xmtp_id::associations::test_utils::MockSmartContractSignatureVerifier;
use xmtp_proto::{api::ApiClientError, api_client::XmtpBackendClient, backend_v1 as wire};

mod transport;
use transport::*;

type TestClient = Client<
    Arc<
        XmtpMlsLocalContext<
            Arc<ScriptedApi>,
            TestDb,
            xmtp_db::sql_key_store::SqlKeyStore<<TestDb as XmtpDb>::DbQuery>,
        >,
    >,
>;
const OLD: &str = "http://old.example";
const NEW: &str = "http://new.example";
const IDENTIFIER: &str = "org.example.same";

fn response(identifier: &str) -> wire::GetConfigurationResponse {
    wire::GetConfigurationResponse {
        identifier: identifier.into(),
        ..Default::default()
    }
}

async fn fixture() -> (TestClient, Arc<Script>) {
    fixture_options(false, false).await
}

async fn fixture_options(streams: bool, remote: bool) -> (TestClient, Arc<Script>) {
    let store = TestDb::create_ephemeral_store().await;
    let prior = response(IDENTIFIER);
    store
        .db()
        .store_server_configuration(IDENTIFIER, OLD, &prior.encode_to_vec(), 1)
        .unwrap();
    let script = Arc::new(Script {
        db: store.clone(),
        calls: Mutex::default(),
        responses: Mutex::default(),
        pause: Mutex::default(),
        wire: Mutex::default(),
    });
    let mut builder = Client::builder(IdentityStrategy::ExternalIdentity(
        crate::identity::Identity::mock_identity(),
    ))
    .store(store)
    .api_client(Arc::new(ScriptedApi(script.clone())))
    .with_allow_offline(Some(true))
    .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
    .with_disable_workers(true)
    .default_mls_store()
    .unwrap();
    if streams {
        builder.incoming_factory = Some(Arc::new(
            crate::subscriptions::incoming::BidiSubscriptionFactory {
                api: builder.api_client.as_ref().unwrap().clone(),
            },
        ));
    }
    if remote {
        builder = builder.with_remote_verifier().unwrap();
    }
    let client = builder.build().await.unwrap();
    assert!(
        script.calls.lock().is_empty(),
        "offline build must not dispatch"
    );
    (client, script)
}

fn cause<'a>(error: &'a (dyn std::error::Error + 'static)) -> &'a ClientError {
    use std::error::Error;
    xmtp_api::preflight::failure(error)
        .unwrap()
        .source()
        .unwrap()
        .source()
        .unwrap()
        .downcast_ref()
        .unwrap()
}

async fn query(client: &TestClient) -> Result<wire::QueryResponse, xmtp_api::ApiError> {
    client
        .context
        .api()
        .api_client
        .query(Default::default())
        .await
}

// verifies: CONF-077, CONF-020, CONF-029
#[xmtp_common::test(unwrap_try = true)]
async fn deferred_preflight_commits_before_concurrent_targets_and_keeps_snapshot() {
    let (client, script) = fixture().await;
    let mut fetched = response(IDENTIFIER);
    fetched.server_version = "changed".into();
    script.responses.lock().push_back(Ok(fetched));
    let (send, receive) = oneshot::channel();
    *script.pause.lock() = Some(receive);
    let first = query(&client);
    let second = query(&client);
    futures::pin_mut!(first, second);
    assert!(futures::poll!(&mut first).is_pending());
    assert!(futures::poll!(&mut second).is_pending());
    assert_eq!(*script.calls.lock(), vec!["configuration"]);
    assert_eq!(
        script.db.db().server_configuration()?.unwrap().backend_url,
        OLD
    );
    send.send(()).unwrap();
    let (a, b) = futures::join!(first, second);
    a?;
    b?;
    assert_eq!(
        *script.calls.lock(),
        vec!["configuration", "query", "query"]
    );
    assert!(client.server_configuration().server_version.is_empty());
    query(&client).await?;
    assert_eq!(
        script
            .calls
            .lock()
            .iter()
            .filter(|&&s| s == "configuration")
            .count(),
        1
    );
    client.close().await?;
}

// verifies: CONF-077, CONF-071
#[xmtp_common::test(unwrap_try = true)]
async fn deferred_preflight_failure_reaches_the_first_call_and_new_call_can_retry() {
    let (client, script) = fixture().await;
    script.responses.lock().push_back(Err(ApiClientError::Auth(
        xmtp_proto::api::AuthError::CallbackFailed { retryable: true },
    )));
    let error = query(&client).await.unwrap_err();
    assert!(matches!(
        cause(&error),
        ClientError::ConfigurationUnavailable(_)
    ));
    assert!(error.is_retryable());
    assert_eq!(*script.calls.lock(), vec!["configuration"]);
    script
        .responses
        .lock()
        .push_back(Ok(response("invalid identifier")));
    let error = query(&client).await.unwrap_err();
    assert!(matches!(
        cause(&error),
        ClientError::ConfigurationInvalid(_)
    ));
    assert!(!error.is_retryable());
    assert_eq!(
        script.db.db().server_configuration()?.unwrap().backend_url,
        OLD
    );
    query(&client).await?;
    assert_eq!(
        *script.calls.lock(),
        vec!["configuration", "configuration", "configuration", "query"]
    );
    client.close().await?;
}

// verifies: CONF-077, CONF-030
#[xmtp_common::test(unwrap_try = true)]
async fn deferred_preflight_mismatch_blocks_even_when_conflict_write_fails() {
    use xmtp_db::diesel::RunQueryDsl;
    let (client, script) = fixture().await;
    script.db.db().raw_query(|conn| xmtp_db::diesel::sql_query("CREATE TRIGGER reject_conflict BEFORE UPDATE OF conflicting_identifier ON server_configuration BEGIN SELECT RAISE(FAIL, 'conflict test'); END").execute(conn))?;
    script
        .responses
        .lock()
        .push_back(Ok(response("org.example.other")));
    let error = query(&client).await.unwrap_err();
    assert!(
        matches!(cause(&error), ClientError::BackendMismatch { stored, received } if stored == IDENTIFIER && received == "org.example.other")
    );
    let error = query(&client).await.unwrap_err();
    assert!(matches!(cause(&error), ClientError::BackendMismatch { .. }));
    assert_eq!(*script.calls.lock(), vec!["configuration"]);
    let stored = script.db.db().server_configuration()?.unwrap();
    assert_eq!(stored.backend_url, OLD);
    assert_eq!(stored.identifier, IDENTIFIER);
    client.close().await?;
}

// verifies: CONF-077
#[xmtp_common::test(unwrap_try = true)]
async fn deferred_preflight_storage_failure_keeps_pending_until_a_later_call() {
    use xmtp_db::diesel::RunQueryDsl;
    let (client, script) = fixture().await;
    script.db.db().raw_query(|conn| xmtp_db::diesel::sql_query("CREATE TRIGGER reject_config BEFORE UPDATE OF response ON server_configuration BEGIN SELECT RAISE(FAIL, 'storage test'); END").execute(conn))?;
    let error = query(&client).await.unwrap_err();
    assert!(matches!(
        cause(&error),
        ClientError::ConfigurationUnavailable(_)
    ));
    assert_eq!(*script.calls.lock(), vec!["configuration"]);
    script
        .db
        .db()
        .raw_query(|conn| xmtp_db::diesel::sql_query("DROP TRIGGER reject_config").execute(conn))?;
    query(&client).await?;
    assert_eq!(
        *script.calls.lock(),
        vec!["configuration", "configuration", "query"]
    );
    client.close().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn deferred_preflight_cancelled_leader_releases_the_gate() {
    let (client, script) = fixture().await;
    let (_send, receive) = oneshot::channel();
    *script.pause.lock() = Some(receive);
    let mut first = Box::pin(query(&client));
    assert!(futures::poll!(&mut first).is_pending());
    drop(first);
    query(&client).await?;
    assert_eq!(
        *script.calls.lock(),
        vec!["configuration", "configuration", "query"]
    );
    let weak = Arc::downgrade(&client.context);
    client.close().await?;
    drop(client);
    assert!(
        weak.upgrade().is_none(),
        "the admission hook must not retain its client"
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn deferred_preflight_close_cancels_fetch_and_lock_wait_without_a_late_write() {
    let (client, script) = fixture().await;
    let (_send, receive) = oneshot::channel();
    *script.pause.lock() = Some(receive);
    let first = query(&client);
    let second = query(&client);
    futures::pin_mut!(first, second);
    assert!(futures::poll!(&mut first).is_pending());
    assert!(futures::poll!(&mut second).is_pending());
    let (a, b, closed) =
        xmtp_common::time::timeout(xmtp_common::time::Duration::from_secs(5), async {
            futures::join!(first, second, client.close())
        })
        .await?;
    assert!(matches!(cause(&a.unwrap_err()), ClientError::AlreadyClosed));
    assert!(matches!(cause(&b.unwrap_err()), ClientError::AlreadyClosed));
    closed?;
    assert_eq!(*script.calls.lock(), vec!["configuration"]);
}

// verifies: CONF-077, CONF-074, CONF-036
#[xmtp_common::test(unwrap_try = true)]
async fn deferred_preflight_refresh_serializes_and_a_raised_minimum_blocks_target() {
    let (client, script) = fixture().await;
    let mut answer = response(IDENTIFIER);
    answer.min_libxmtp_version = "9999.0.0".into();
    script.responses.lock().push_back(Ok(answer));
    let (send, receive) = oneshot::channel();
    *script.pause.lock() = Some(receive);
    let refresh = client.refresh_server_configuration();
    let target = query(&client);
    futures::pin_mut!(refresh, target);
    assert!(futures::poll!(&mut refresh).is_pending());
    assert!(futures::poll!(&mut target).is_pending());
    send.send(()).unwrap();
    let (refresh, target) = futures::join!(refresh, target);
    assert!(matches!(
        refresh.unwrap_err(),
        ClientError::ClientVersionTooOld { .. }
    ));
    assert!(matches!(
        cause(&target.unwrap_err()),
        ClientError::ClientVersionTooOld { .. }
    ));
    assert_eq!(*script.calls.lock(), vec!["configuration"]);
    assert_eq!(
        script.db.db().server_configuration()?.unwrap().backend_url,
        NEW
    );
    client.close().await?;
}

mod admission;

mod native_wire;

mod controls;

mod sync_barrier;
