//! Deferred configuration admission through a built client's public API:
//! the first request on a pending client runs the configuration check, and
//! its outcome decides whether the request proceeds.

mod admission;
mod native_wire;

use tokio::sync::oneshot;
use xmtp_common::RetryableError;
use xmtp_db::{ConnectionExt, prelude::*};
use xmtp_mls::server_configuration::test_support::scripted_api::*;
use xmtp_mls::server_configuration::{BlockedConnection, ServerConfigError};
use xmtp_mls::utils::test::preflight::*;
use xmtp_proto::api::ApiClientError;

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
    assert!(matches!(cause(&error), ServerConfigError::Unavailable(_)));
    assert!(error.is_retryable());
    assert_eq!(*script.calls.lock(), vec!["configuration"]);
    script
        .responses
        .lock()
        .push_back(Ok(response("invalid identifier")));
    let error = query(&client).await.unwrap_err();
    assert!(matches!(cause(&error), ServerConfigError::Invalid(_)));
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
        matches!(cause(&error), ServerConfigError::Blocked(BlockedConnection::BackendMismatch { stored, received }) if stored == IDENTIFIER && received == "org.example.other")
    );
    let error = query(&client).await.unwrap_err();
    assert!(matches!(
        cause(&error),
        ServerConfigError::Blocked(BlockedConnection::BackendMismatch { .. })
    ));
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
    assert!(matches!(cause(&error), ServerConfigError::Unavailable(_)));
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
