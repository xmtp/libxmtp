//! Deferred configuration admission as a client sees it: one check per
//! client, closed by the client's close, refreshed under the same lock.

use crate::server_configuration::test_support::scripted_api::*;
use crate::server_configuration::{BlockedConnection, ServerConfigError};
use crate::utils::test::preflight::*;
use std::sync::Arc;
use tokio::sync::oneshot;
use xmtp_db::prelude::*;

mod native_wire;
mod sync_barrier;

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
    assert!(matches!(cause(&a.unwrap_err()), ServerConfigError::Closed));
    assert!(matches!(cause(&b.unwrap_err()), ServerConfigError::Closed));
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
        crate::client::ClientError::ClientVersionTooOld { .. }
    ));
    assert!(matches!(
        cause(&target.unwrap_err()),
        ServerConfigError::Blocked(BlockedConnection::ClientVersionTooOld { .. })
    ));
    assert_eq!(*script.calls.lock(), vec!["configuration"]);
    assert_eq!(
        script.db.db().server_configuration()?.unwrap().backend_url,
        NEW
    );
    client.close().await?;
}
